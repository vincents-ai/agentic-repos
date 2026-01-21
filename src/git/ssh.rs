use crate::git::{auth::AuthorizedKeysManager, error::Result, GitConfig, GitService};
use rand_core::OsRng;
use russh::server::{Msg, Server as _, Session};
use russh::*;
use russh_keys::ssh_key::PublicKey;
use russh_keys::{Algorithm, PrivateKey};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

pub struct SshServer {
    config: GitConfig,
    git_service: Arc<GitService>,
    auth_manager: Arc<Mutex<AuthorizedKeysManager>>,
}

impl SshServer {
    pub fn new(git_service: Arc<GitService>, config: Option<GitConfig>) -> Self {
        let config = config.unwrap_or_default();
        let auth_manager = AuthorizedKeysManager::new(config.clone());

        Self {
            config,
            git_service,
            auth_manager: Arc::new(Mutex::new(auth_manager)),
        }
    }

    pub async fn run(&self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.config.ssh_port);
        let socket = TcpListener::bind(&addr).await?;
        info!("SSH server listening on {}", addr);

        {
            let mut auth_manager = self.auth_manager.lock().await;
            if let Err(e) = auth_manager.load_authorized_keys().await {
                error!("Failed to load authorized keys: {}", e);
                warn!("SSH server will reject all authentication attempts");
            } else if !auth_manager.has_authorized_users() {
                warn!("No authorized users configured - SSH server will reject all authentication attempts");
                warn!(
                    "Add users to the authorized keys file: {:?}",
                    self.config.authorized_keys_path
                );
            } else {
                info!(
                    "Loaded authorized keys for {} users",
                    auth_manager.get_authorized_users().len()
                );
            }
        }

        let keys = vec![PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?];

        let config = Arc::new(russh::server::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(
                self.config.connection_timeout_secs,
            )),
            auth_rejection_time: std::time::Duration::from_secs(3),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys,
            ..Default::default()
        });

        let mut server = SshServerImpl {
            git_service: self.git_service.clone(),
            auth_manager: self.auth_manager.clone(),
            clients: Arc::new(Mutex::new(HashMap::new())),
            id: 0,
        };

        let mut shutdown_signal = Box::pin(signal::ctrl_c());

        tokio::select! {
            result = server.run_on_socket(config, &socket) => {
                if let Err(e) = result {
                    error!("SSH server error: {}", e);
                }
            }
            _ = shutdown_signal.as_mut() => {
                info!("Shutting down SSH server...");
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
struct SshServerImpl {
    git_service: Arc<GitService>,
    auth_manager: Arc<Mutex<AuthorizedKeysManager>>,
    clients: Arc<Mutex<HashMap<usize, (ChannelId, russh::server::Handle)>>>,
    id: usize,
}

impl russh::server::Server for SshServerImpl {
    type Handler = SshHandler;

    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        debug!("New SSH client from {:?}", peer_addr);
        self.id += 1;
        SshHandler {
            git_service: self.git_service.clone(),
            auth_manager: self.auth_manager.clone(),
            client_id: self.id,
            clients: self.clients.clone(),
        }
    }
}

#[derive(Clone)]
struct SshHandler {
    git_service: Arc<GitService>,
    auth_manager: Arc<Mutex<AuthorizedKeysManager>>,
    client_id: usize,
    clients: Arc<Mutex<HashMap<usize, (ChannelId, russh::server::Handle)>>>,
}

#[async_trait::async_trait]
impl russh::server::Handler for SshHandler {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> std::result::Result<russh::server::Auth, Self::Error> {
        debug!("Public key authentication for user: {}", user);

        let auth_manager = self.auth_manager.lock().await;
        if auth_manager.is_key_authorized(user, public_key) {
            info!("Authentication successful for user: {}", user);
            Ok(russh::server::Auth::Accept)
        } else {
            warn!("Authentication rejected for user: {}", user);
            Ok(russh::server::Auth::Reject {
                proceed_with_methods: None,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> std::result::Result<bool, Self::Error> {
        debug!("Opening session channel: {}", channel.id());
        let mut clients = self.clients.lock().await;
        clients.insert(self.client_id, (channel.id(), session.handle()));
        Ok(true)
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> std::result::Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data);
        debug!("Executing command: {}", command);

        match command.trim() {
            cmd if cmd.starts_with("git-upload-pack") => {
                let response = "git-upload-pack operation completed\n";
                session.data(channel, response.as_bytes().to_vec().into())?;
                let _ = session.exit_status_request(channel, 0);
            }
            cmd if cmd.starts_with("git-receive-pack") => {
                let response = "git-receive-pack operation completed\n";
                session.data(channel, response.as_bytes().to_vec().into())?;
                let _ = session.exit_status_request(channel, 0);
            }
            _ => {
                let error_msg = format!("Unknown command: {}\n", command);
                session.data(channel, error_msg.into_bytes().into())?;
                let _ = session.exit_status_request(channel, 1);
            }
        }

        session.close(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> std::result::Result<(), Self::Error> {
        debug!("Received data on channel {}: {} bytes", channel, data.len());
        let response = format!("Received: {}\r\n", String::from_utf8_lossy(data));
        session.data(channel, response.into_bytes().into())?;
        Ok(())
    }
}

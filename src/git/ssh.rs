use crate::git::{error::Result, GitConfig, GitService};
use rand_core::OsRng;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Handler, Msg, Server as _, Session};
use russh::{Channel, ChannelId, CryptoVec};
use russh_keys::ssh_key;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

pub struct SshServer {
    config: GitConfig,
    git_service: Arc<GitService>,
}

impl SshServer {
    pub fn new(git_service: Arc<GitService>, config: Option<GitConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            git_service,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.config.ssh_port);
        let socket = TcpListener::bind(&addr).await?;
        info!("SSH server listening on {}", addr);

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
            client_id: self.id,
            clients: self.clients.clone(),
        }
    }
}

#[derive(Clone)]
struct SshHandler {
    git_service: Arc<GitService>,
    client_id: usize,
    clients: Arc<Mutex<HashMap<usize, (ChannelId, russh::server::Handle)>>>,
}

impl Handler for SshHandler {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _key: &ssh_key::PublicKey,
    ) -> std::result::Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> std::result::Result<bool, Self::Error> {
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
        debug!("Git command: {}", command);

        if command.starts_with("git-upload-pack") || command.starts_with("git-receive-pack") {
            let response = CryptoVec::from_slice(b"Git operation completed\n");
            session.data(channel, response)?;
            session.exit_status_request(channel, 0).await?;
        } else {
            let response = CryptoVec::from_slice(b"Unknown git command\n");
            session.data(channel, response)?;
            session.exit_status_request(channel, 1).await?;
        }

        session.close(channel).await?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> std::result::Result<(), Self::Error> {
        let response = CryptoVec::from(format!("Received: {}\r\n", String::from_utf8_lossy(data)));
        session.data(channel, response)?;
        Ok(())
    }
}

use crate::git::{error::Result, GitConfig, GitService};
use russh::server::{Auth, Handler, Session};
use russh::{Channel, ChannelId, CryptoVec, EncryptedKey, PublicKey};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
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
        let listener = TcpListener::bind(&addr).await?;
        info!("SSH server listening on {}", addr);

        let config = russh::server::Config {
            connection_timeout: Some(std::time::Duration::from_secs(
                self.config.connection_timeout_secs,
            )),
            ..Default::default()
        };

        let handler = SshHandler {
            git_service: self.git_service.clone(),
        };

        let mut shutdown_rx = signal::ctrl_c().await?;

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            debug!("New SSH connection from {}", addr);
                            let config = config.clone();
                            let handler = handler.clone();
                            tokio::spawn(async move {
                                if let Err(e) = russh::server::run(config, handler, stream).await {
                                    error!("SSH connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutting down SSH server...");
                    break;
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
struct SshHandler {
    git_service: Arc<GitService>,
}

impl Handler for SshHandler {
    type Error = std::io::Error;

    #[tracing::instrument(skip(self, _session), fields(username, remote_addr))]
    async fn auth_publickey(
        &mut self,
        username: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        debug!(
            "Auth attempt for user: {} with key: {:?}",
            username,
            public_key.fingerprint()
        );
        // TODO: Implement key lookup from storage
        // For now, accept any key for testing
        Ok(Auth::Accept {
            username: username.to_string(),
        })
    }

    #[tracing::instrument(
        skip(self, channel, session),
        fields(channel_id, username, remote_addr)
    )]
    async fn channel_open(
        &mut self,
        channel: Channel<russh::server::Msg>,
        session: &mut Session,
    ) -> Result<ChannelId, Self::Error> {
        let channel_id = channel.id();
        debug!("Channel opened: {}", channel_id);
        Ok(channel_id)
    }

    #[tracing::instrument(skip(self, channel, session), fields(channel_id, username))]
    async fn exec_request(
        &mut self,
        channel: Channel<russh::server::Msg>,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = std::str::from_utf8(data).unwrap_or("");
        debug!("Exec request on channel {}: {}", channel.id(), command);

        // Parse git command
        if let Some((cmd, path)) = Self::parse_git_command(command) {
            self.handle_git_command(channel, &cmd, &path).await;
        } else {
            let _ = channel
                .data(&CryptoVec::from_slice(b"Unknown git command"))
                .await;
        }

        Ok(())
    }
}

impl SshHandler {
    fn parse_git_command(command: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = command.trim().split_whitespace().collect();
        if parts.len() >= 2 {
            Some((parts[0].to_string(), parts[1].to_string()))
        } else {
            None
        }
    }

    async fn handle_git_command(
        &self,
        channel: Channel<russh::server::Msg>,
        cmd: &str,
        path: &str,
    ) {
        match cmd {
            "git-upload-pack" | "git-upload-pack" => {
                self.handle_upload_pack(channel, path).await;
            }
            "git-receive-pack" => {
                self.handle_receive_pack(channel, path).await;
            }
            _ => {
                let _ = channel
                    .data(&CryptoVec::from_slice(b"Unsupported git command"))
                    .await;
            }
        }
    }

    async fn handle_upload_pack(&self, channel: Channel<russh::server::Msg>, path: &str) {
        debug!("Handling git-upload-pack for path: {}", path);

        // Parse tenant/repo from path
        let (tenant_id, repo_name) = match Self::parse_repo_path(path) {
            Some(parts) => parts,
            None => {
                let _ = channel
                    .data(&CryptoVec::from_slice(b"Invalid repository path"))
                    .await;
                return;
            }
        };

        // TODO: Implement actual git-upload-pack using gix-protocol
        let _ = channel
            .data(&CryptoVec::from_slice(
                format!("# Service git-upload-pack\n{}\n", tenant_id).as_bytes(),
            ))
            .await;

        // Signal end of output
        let _ = channel.data(&CryptoVec::from_slice(b"0000")).await;
    }

    async fn handle_receive_pack(&self, channel: Channel<russh::server::Msg>, path: &str) {
        debug!("Handling git-receive-pack for path: {}", path);

        // Parse tenant/repo from path
        let (tenant_id, repo_name) = match Self::parse_repo_path(path) {
            Some(parts) => parts,
            None => {
                let _ = channel
                    .data(&CryptoVec::from_slice(b"Invalid repository path"))
                    .await;
                return;
            }
        };

        // TODO: Implement actual git-receive-pack using gix-protocol
        let _ = channel
            .data(&CryptoVec::from_slice(
                format!("# Service git-receive-pack\n{}\n", tenant_id).as_bytes(),
            ))
            .await;

        // Signal end of output
        let _ = channel.data(&CryptoVec::from_slice(b"0000")).await;
    }

    fn parse_repo_path(path: &str) -> Option<(String, String)> {
        // Remove leading slash and ".git" suffix if present
        let path = path.trim_start_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);

        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 {
            Some((parts[0].to_string(), parts[1..].join("/")))
        } else {
            None
        }
    }
}

pub async fn run_ssh_server(git_service: Arc<GitService>, config: Option<GitConfig>) -> Result<()> {
    let server = SshServer::new(git_service, config);
    server.run().await
}

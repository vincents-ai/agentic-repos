use crate::git::{
    auth::AuthorizedKeysManager,
    command::CommandValidator,
    error::Result,
    rate_limit::{RateLimitContext, RateLimitError, RateLimiter},
    GitConfig, GitService,
};
use rand_core::OsRng;
use russh::server::{Msg, Server as _, Session};
use russh::*;
use russh_keys::ssh_key::{LineEnding, PublicKey};
use russh_keys::{Algorithm, PrivateKey};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

pub struct SshServer {
    config: GitConfig,
    git_service: Arc<GitService>,
    auth_manager: Arc<Mutex<AuthorizedKeysManager>>,
    rate_limiter: RateLimiter,
    connection_count: Arc<AtomicUsize>,
    ip_connections: Arc<Mutex<HashMap<String, usize>>>,
    channel_senders: Arc<Mutex<HashMap<ChannelId, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>>,
    lfs_config: crate::config::LfsConfig,
}

impl SshServer {
    pub fn new(
        git_service: Arc<GitService>,
        config: Option<GitConfig>,
        lfs_config: Option<crate::config::LfsConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let lfs_config = lfs_config.unwrap_or_default();
        let auth_manager = AuthorizedKeysManager::new(config.clone());
        let rate_limiter = RateLimiter::new(Some(config.rate_limit.clone()));

        Self {
            config,
            git_service,
            auth_manager: Arc::new(Mutex::new(auth_manager)),
            rate_limiter,
            connection_count: Arc::new(AtomicUsize::new(0)),
            ip_connections: Arc::new(Mutex::new(HashMap::new())),
            channel_senders: Arc::new(Mutex::new(HashMap::new())),
            lfs_config,
        }
    }

    /// Load host key from file, or generate and save a new one if it doesn't exist
    async fn load_or_generate_host_key(&self) -> Result<PrivateKey> {
        use tokio::fs;

        let key_path = &self.config.host_key_path;

        // Try to load existing host key
        if key_path.exists() {
            debug!("Loading host key from: {:?}", key_path);
            match fs::read(key_path).await {
                Ok(key_data) => match PrivateKey::from_openssh(&key_data) {
                    Ok(key) => {
                        info!("Successfully loaded host key from: {:?}", key_path);
                        return Ok(key);
                    }
                    Err(e) => {
                        warn!("Failed to parse existing host key: {}. Regenerating...", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to read host key file: {}. Regenerating...", e);
                }
            }
        } else {
            info!(
                "Host key file not found at: {:?}. Generating new key...",
                key_path
            );
        }

        // Generate new host key
        let new_key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Save the new key
        if let Err(e) = self.save_host_key(&new_key, key_path).await {
            warn!(
                "Failed to save host key to file: {}. Server will use temporary key.",
                e
            );
        } else {
            info!("Saved new host key to: {:?}", key_path);
        }

        Ok(new_key)
    }

    /// Save host key to file in OpenSSH format
    async fn save_host_key(&self, key: &PrivateKey, key_path: &Path) -> Result<()> {
        use tokio::fs;

        // Create parent directory if it doesn't exist
        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Convert key to OpenSSH format
        let openssh_data = key
            .to_openssh(LineEnding::LF)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Write to file with restrictive permissions
        fs::write(key_path, openssh_data.as_bytes()).await?;

        // Set file permissions to 600 (owner read/write only) on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            fs::set_permissions(key_path, perms).await?;
        }

        Ok(())
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

        let keys = vec![self.load_or_generate_host_key().await?];

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
            rate_limiter: self.rate_limiter.clone(),
            connection_count: self.connection_count.clone(),
            ip_connections: self.ip_connections.clone(),
            max_connections: self.config.max_connections,
            clients: Arc::new(Mutex::new(HashMap::new())),
            channel_senders: Arc::new(Mutex::new(HashMap::new())),
            id: 0,
            lfs_config: self.lfs_config.clone(),
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

#[cfg(test)]
impl SshServer {
    /// Get current connection count for testing
    pub fn get_connection_count(&self) -> usize {
        self.connection_count.load(Ordering::SeqCst)
    }

    /// Get the configuration for testing
    pub fn get_config(&self) -> &GitConfig {
        &self.config
    }

    /// Load host key for testing (public version of the private method)
    pub async fn test_load_or_generate_host_key(&self) -> Result<PrivateKey> {
        self.load_or_generate_host_key().await
    }

    /// Increment connection count for testing
    pub fn test_increment_connection_count(&self) -> usize {
        self.connection_count.fetch_add(1, Ordering::SeqCst)
    }

    /// Decrement connection count for testing  
    pub fn test_decrement_connection_count(&self) -> usize {
        self.connection_count.fetch_sub(1, Ordering::SeqCst)
    }
}

#[derive(Clone)]
struct SshServerImpl {
    git_service: Arc<GitService>,
    auth_manager: Arc<Mutex<AuthorizedKeysManager>>,
    rate_limiter: RateLimiter,
    connection_count: Arc<AtomicUsize>,
    ip_connections: Arc<Mutex<HashMap<String, usize>>>,
    max_connections: usize,
    clients: Arc<Mutex<HashMap<usize, (ChannelId, russh::server::Handle)>>>,
    channel_senders: Arc<Mutex<HashMap<ChannelId, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>>,
    id: usize,
    lfs_config: crate::config::LfsConfig,
}

impl russh::server::Server for SshServerImpl {
    type Handler = SshHandler;

    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        let peer_ip = peer_addr
            .map(|addr| addr.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        debug!("New SSH client from {:?}", peer_addr);

        // Check global connection limit
        let current_connections = self.connection_count.load(Ordering::SeqCst);
        if current_connections >= self.max_connections {
            warn!(
                "Rejecting connection from {}: global connection limit reached ({}/{})",
                peer_ip, current_connections, self.max_connections
            );
            // Return a handler that will immediately reject
            self.id += 1;
            return SshHandler {
                git_service: self.git_service.clone(),
                auth_manager: self.auth_manager.clone(),
                rate_limiter: self.rate_limiter.clone(),
                client_id: self.id,
                clients: self.clients.clone(),
                channel_senders: self.channel_senders.clone(),
                peer_ip,
                connection_count: self.connection_count.clone(),
                ip_connections: self.ip_connections.clone(),
                rejected: true,
                lfs_config: self.lfs_config.clone(),
            };
        }

        // Increment connection count
        self.connection_count.fetch_add(1, Ordering::SeqCst);

        // Track per-IP connections (we'll implement detailed limiting in the handler)
        self.id += 1;

        info!(
            "Accepted SSH connection from {} (total: {})",
            peer_ip,
            current_connections + 1
        );

        SshHandler {
            git_service: self.git_service.clone(),
            auth_manager: self.auth_manager.clone(),
            rate_limiter: self.rate_limiter.clone(),
            client_id: self.id,
            clients: self.clients.clone(),
            channel_senders: self.channel_senders.clone(),
            peer_ip,
            connection_count: self.connection_count.clone(),
            ip_connections: self.ip_connections.clone(),
            rejected: false,
            lfs_config: self.lfs_config.clone(),
        }
    }
}

#[derive(Clone)]
struct SshHandler {
    #[allow(dead_code)]
    git_service: Arc<GitService>,
    auth_manager: Arc<Mutex<AuthorizedKeysManager>>,
    rate_limiter: RateLimiter,
    client_id: usize,
    clients: Arc<Mutex<HashMap<usize, (ChannelId, russh::server::Handle)>>>,
    channel_senders: Arc<Mutex<HashMap<ChannelId, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>>,
    peer_ip: String,
    connection_count: Arc<AtomicUsize>,
    #[allow(dead_code)]
    ip_connections: Arc<Mutex<HashMap<String, usize>>>,
    rejected: bool,
    lfs_config: crate::config::LfsConfig,
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

        // Reject if connection was already marked as rejected
        if self.rejected {
            warn!(
                "Rejecting auth for user {} from {}: connection rejected due to limits",
                user, self.peer_ip
            );
            return Ok(russh::server::Auth::Reject {
                proceed_with_methods: None,
            });
        }

        // Check rate limits for authentication attempts
        let context = RateLimitContext {
            tenant_id: user.to_string(),
            ip_address: self.peer_ip.clone(),
            user_agent: "ssh-client".to_string(),
            request_id: format!("ssh-auth-{}", self.client_id),
            path: "/ssh/auth".to_string(),
            timestamp: std::time::Instant::now(),
        };

        if let Err(e) = self.rate_limiter.check_rate_limit(&context, "free").await {
            match e {
                RateLimitError::RateLimitExceeded {
                    limit_type,
                    retry_after,
                    ..
                } => {
                    warn!(
                        "Rate limit exceeded for SSH auth from {}: {} (retry after {}s)",
                        self.peer_ip, limit_type, retry_after
                    );
                    return Ok(russh::server::Auth::Reject {
                        proceed_with_methods: None,
                    });
                }
                _ => {
                    error!("Rate limit check failed: {}", e);
                }
            }
        }

        let auth_manager = self.auth_manager.lock().await;
        if auth_manager.is_key_authorized(user, public_key) {
            info!(
                "Authentication successful for user: {} from {}",
                user, self.peer_ip
            );
            Ok(russh::server::Auth::Accept)
        } else {
            warn!(
                "Authentication rejected for user: {} from {}",
                user, self.peer_ip
            );
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
        let command_str = String::from_utf8_lossy(data);
        debug!(
            "Received SSH command: {} from {}",
            command_str, self.peer_ip
        );

        // Rate limit command executions
        let context = RateLimitContext {
            tenant_id: format!("ssh-{}", self.client_id),
            ip_address: self.peer_ip.clone(),
            user_agent: "ssh-client".to_string(),
            request_id: format!("ssh-cmd-{}", self.client_id),
            path: "/ssh/exec".to_string(),
            timestamp: std::time::Instant::now(),
        };

        if let Err(e) = self.rate_limiter.check_rate_limit(&context, "free").await {
            match e {
                RateLimitError::RateLimitExceeded {
                    limit_type,
                    retry_after,
                    ..
                } => {
                    warn!(
                        "Rate limit exceeded for SSH command from {}: {} (retry after {}s)",
                        self.peer_ip, limit_type, retry_after
                    );
                    let error_msg = format!(
                        "ERROR: Rate limit exceeded. Try again in {}s.\n",
                        retry_after
                    );
                    session.data(channel, error_msg.into_bytes().into())?;
                    let _ = session.exit_status_request(channel, 1);
                    session.close(channel)?;
                    return Ok(());
                }
                _ => {
                    error!("Rate limit check failed: {}", e);
                }
            }
        }

        match CommandValidator::validate_command(&command_str) {
            Ok(git_command) => {
                info!("Validated command: {:?} from {}", git_command, self.peer_ip);

                match git_command {
                    crate::git::command::GitCommand::UploadPack { repository } => {
                        let (tenant, repo_name) =
                            match CommandValidator::parse_repository_path(&repository) {
                                Some((t, r)) => (t, r),
                                None => {
                                    let error_msg = "Error: Invalid repository path format\n";
                                    session.data(channel, error_msg.as_bytes().to_vec().into())?;
                                    session.exit_status_request(channel, 1)?;
                                    session.eof(channel)?;
                                    return Ok(());
                                }
                            };

                        let repo_path = self
                            .git_service
                            .get_repository_full_path(&tenant, &repo_name);
                        let storage_root = self.git_service.get_storage_root();
                        let full_path = storage_root.join(&repo_path);

                        if !full_path.exists() {
                            let error_msg =
                                format!("Error: Repository '{}' not found\n", repository);
                            session.data(channel, error_msg.as_bytes().to_vec().into())?;
                            session.exit_status_request(channel, 128)?;
                            session.eof(channel)?;
                            return Ok(());
                        }

                        match self.run_git_upload_pack(channel, session, &full_path).await {
                            Ok(()) => {
                                info!(
                                    "git-upload-pack completed for {} from {}",
                                    repository, self.peer_ip
                                );
                            }
                            Err(e) => {
                                error!("git-upload-pack failed for {}: {}", repository, e);
                                let error_msg = format!("Error: {}\n", e);
                                session.data(channel, error_msg.as_bytes().to_vec().into())?;
                                session.exit_status_request(channel, 1)?;
                                session.eof(channel)?;
                                return Ok(());
                            }
                        }
                    }
                    crate::git::command::GitCommand::ReceivePack { repository } => {
                        let (tenant, repo_name) =
                            match CommandValidator::parse_repository_path(&repository) {
                                Some((t, r)) => (t, r),
                                None => {
                                    let error_msg = "Error: Invalid repository path format\n";
                                    session.data(channel, error_msg.as_bytes().to_vec().into())?;
                                    session.exit_status_request(channel, 1)?;
                                    session.eof(channel)?;
                                    return Ok(());
                                }
                            };

                        let repo_path = self
                            .git_service
                            .get_repository_full_path(&tenant, &repo_name);
                        let storage_root = self.git_service.get_storage_root();
                        let full_path = storage_root.join(&repo_path);

                        if !full_path.exists() {
                            let error_msg =
                                format!("Error: Repository '{}' not found\n", repository);
                            session.data(channel, error_msg.as_bytes().to_vec().into())?;
                            session.exit_status_request(channel, 128)?;
                            session.eof(channel)?;
                            return Ok(());
                        }

                        match self
                            .run_git_receive_pack(channel, session, &full_path)
                            .await
                        {
                            Ok(()) => {
                                info!(
                                    "git-receive-pack completed for {} from {}",
                                    repository, self.peer_ip
                                );
                            }
                            Err(e) => {
                                error!("git-receive-pack failed for {}: {}", repository, e);
                                let error_msg = format!("Error: {}\n", e);
                                session.data(channel, error_msg.as_bytes().to_vec().into())?;
                                session.exit_status_request(channel, 1)?;
                                session.eof(channel)?;
                                return Ok(());
                            }
                        }
                    }
                    crate::git::command::GitCommand::LfsAuthenticate {
                        repository,
                        operation,
                    } => {
                        // Parse tenant/repo from repository path (format: tenant/repo.git)
                        let parts: Vec<&str> = repository.split('/').collect();
                        if parts.len() < 2 {
                            let error_msg = "Error: Invalid repository path format (expected: tenant/repo.git)\n";
                            session.data(channel, error_msg.as_bytes().to_vec().into())?;
                            session.exit_status_request(channel, 1)?;
                            session.eof(channel)?;
                            return Ok(());
                        }

                        let tenant = parts[0];
                        let repo = parts[1..].join("/");

                        let generator =
                            crate::lfs::LfsTokenGenerator::new(self.lfs_config.jwt_secret.clone());
                        let lfs_op = if operation == "download" {
                            crate::lfs::LfsOperation::Download
                        } else {
                            crate::lfs::LfsOperation::Upload
                        };

                        let token = match generator.generate_token(
                            tenant,
                            &repo,
                            lfs_op,
                            self.lfs_config.token_expiration_secs,
                        ) {
                            Ok(t) => t,
                            Err(e) => {
                                let error_msg = format!("Error: Token generation failed: {}\n", e);
                                session.data(channel, error_msg.as_bytes().to_vec().into())?;
                                session.exit_status_request(channel, 1)?;
                                session.eof(channel)?;
                                return Ok(());
                            }
                        };

                        let response = crate::lfs::LfsAuthResponse::new(
                            &self.lfs_config.base_url,
                            token,
                            self.lfs_config.token_expiration_secs,
                        );

                        let json = match serde_json::to_string(&response) {
                            Ok(j) => j,
                            Err(e) => {
                                let error_msg =
                                    format!("Error: JSON serialization failed: {}\n", e);
                                session.data(channel, error_msg.as_bytes().to_vec().into())?;
                                session.exit_status_request(channel, 1)?;
                                session.eof(channel)?;
                                return Ok(());
                            }
                        };

                        info!(
                            "LFS authenticate successful for {}/{} ({})",
                            tenant, repo, operation
                        );
                        session.data(channel, json.as_bytes().to_vec().into())?;
                        session.exit_status_request(channel, 0)?;
                        session.eof(channel)?;
                    }
                }
            }
            Err(validation_error) => {
                warn!(
                    "Command validation failed: {} for command: {} from {}",
                    validation_error, command_str, self.peer_ip
                );

                let error_msg = format!("ERROR: {}\n", validation_error);
                session.data(channel, error_msg.into_bytes().into())?;
                session.exit_status_request(channel, 1)?;
                session.eof(channel)?;
            }
        }

        session.close(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> std::result::Result<(), Self::Error> {
        debug!(
            "Received data on channel {}: {} bytes from {}",
            channel,
            data.len(),
            self.peer_ip
        );

        let senders = self.channel_senders.lock().await;
        if let Some(tx) = senders.get(&channel) {
            let _ = tx.send(data.to_vec());
        }

        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> std::result::Result<(), Self::Error> {
        debug!(
            "Channel {} closed for client from {}",
            channel, self.peer_ip
        );
        let mut clients = self.clients.lock().await;
        clients.remove(&self.client_id);

        let mut senders = self.channel_senders.lock().await;
        senders.remove(&channel);

        Ok(())
    }
}

impl Drop for SshHandler {
    fn drop(&mut self) {
        // Decrement connection count when handler is dropped
        let prev_count = self.connection_count.fetch_sub(1, Ordering::SeqCst);
        info!(
            "SSH connection from {} closed (remaining: {})",
            self.peer_ip,
            prev_count.saturating_sub(1)
        );
    }
}

impl SshHandler {
    async fn run_git_upload_pack(
        &self,
        channel: ChannelId,
        session: &mut Session,
        repo_path: &Path,
    ) -> std::result::Result<(), russh::Error> {
        debug!("Starting native git-upload-pack for {:?}", repo_path);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        {
            let mut senders = self.channel_senders.lock().await;
            senders.insert(channel, tx);
        }

        let reader = crate::git::ssh_channel::SshChannelReader::new(rx);
        let writer = crate::git::ssh_channel::SshChannelWriter::new(channel, session.handle());

        let handler = crate::git::protocol::GitProtocolHandler::new();

        let result = handler.handle_upload_pack(reader, writer, repo_path).await;

        let mut senders = self.channel_senders.lock().await;
        senders.remove(&channel);

        match result {
            Ok(()) => {
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                Ok(())
            }
            Err(e) => {
                error!("git-upload-pack error: {}", e);
                session.exit_status_request(channel, 1)?;
                session.eof(channel)?;
                Err(russh::Error::IO(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e,
                )))
            }
        }
    }

    async fn run_git_receive_pack(
        &self,
        channel: ChannelId,
        session: &mut Session,
        repo_path: &Path,
    ) -> std::result::Result<(), russh::Error> {
        debug!("Starting native git-receive-pack for {:?}", repo_path);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        {
            let mut senders = self.channel_senders.lock().await;
            senders.insert(channel, tx);
        }

        let reader = crate::git::ssh_channel::SshChannelReader::new(rx);
        let writer = crate::git::ssh_channel::SshChannelWriter::new(channel, session.handle());
        
        let handler = crate::git::protocol::GitProtocolHandler::new();
        
        let result = handler.handle_receive_pack(reader, writer, repo_path).await;

        let mut senders = self.channel_senders.lock().await;
        senders.remove(&channel);

        match result {
            Ok(()) => {
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                Ok(())
            }
            Err(e) => {
                error!("git-receive-pack error: {}", e);
                session.exit_status_request(channel, 1)?;
                session.eof(channel)?;
                Err(russh::Error::IO(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e,
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local::LocalStorage;
    use tempfile::TempDir;
    use tokio::fs;

    async fn create_test_ssh_server() -> (SshServer, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(LocalStorage::new(temp_dir.path().to_path_buf()));
        let git_service = Arc::new(GitService::new(storage, None));

        let config = GitConfig {
            ssh_port: 0, // Use random port for testing
            host_key_path: temp_dir.path().join("test_host_key"),
            authorized_keys_path: temp_dir.path().join("authorized_keys"),
            max_connections: 10,
            connection_timeout_secs: 30,
            rate_limit: crate::git::rate_limit::RateLimitConfig::default(),
        };

        let server = SshServer::new(git_service, Some(config), None);
        (server, temp_dir)
    }

    #[tokio::test]
    async fn test_host_key_generation_and_persistence() {
        let (server, _temp_dir) = create_test_ssh_server().await;
        let key_path = &server.config.host_key_path;

        // Ensure key file doesn't exist initially
        assert!(!key_path.exists());

        // Load/generate host key for the first time
        let key1 = server.load_or_generate_host_key().await.unwrap();

        // Key file should now exist
        assert!(key_path.exists());

        // Verify file has proper permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(key_path).await.unwrap();
            let permissions = metadata.permissions();
            assert_eq!(permissions.mode() & 0o777, 0o600);
        }

        // Load the same key again - should be identical
        let key2 = server.load_or_generate_host_key().await.unwrap();

        // Keys should be identical (same private key material)
        let key1_openssh = key1.to_openssh(LineEnding::LF).unwrap();
        let key2_openssh = key2.to_openssh(LineEnding::LF).unwrap();
        assert_eq!(key1_openssh.as_str(), key2_openssh.as_str());
    }

    #[tokio::test]
    async fn test_host_key_regeneration_on_corruption() {
        let (server, _temp_dir) = create_test_ssh_server().await;
        let key_path = &server.config.host_key_path;

        // Create a corrupted key file
        fs::write(key_path, b"invalid key data").await.unwrap();
        assert!(key_path.exists());

        // Should regenerate when key is corrupted
        let result = server.load_or_generate_host_key().await;
        assert!(result.is_ok());

        // File should still exist with valid key
        assert!(key_path.exists());

        // Should be able to load it again successfully
        let key = server.load_or_generate_host_key().await.unwrap();
        let _openssh_data = key.to_openssh(LineEnding::LF).unwrap();
    }

    #[tokio::test]
    async fn test_host_key_directory_creation() {
        let (_server, temp_dir) = create_test_ssh_server().await;

        // Create a config with a nested directory path
        let nested_path = temp_dir.path().join("nested").join("dirs").join("host_key");
        let config = GitConfig {
            ssh_port: 0,
            host_key_path: nested_path.clone(),
            authorized_keys_path: temp_dir.path().join("authorized_keys"),
            max_connections: 10,
            connection_timeout_secs: 30,
            rate_limit: crate::git::rate_limit::RateLimitConfig::default(),
        };

        let storage = Arc::new(LocalStorage::new(temp_dir.path().to_path_buf()));
        let git_service = Arc::new(GitService::new(storage, None));
        let server = SshServer::new(git_service, Some(config), None);

        // Nested directories don't exist initially
        assert!(!nested_path.parent().unwrap().exists());
        assert!(!nested_path.exists());

        // Generate host key - should create directories
        let result = server.load_or_generate_host_key().await;
        assert!(result.is_ok());

        // Directories and key file should now exist
        assert!(nested_path.parent().unwrap().exists());
        assert!(nested_path.exists());
    }

    #[tokio::test]
    async fn test_ssh_server_connection_limits() {
        let (server, _temp_dir) = create_test_ssh_server().await;

        assert_eq!(server.connection_count.load(Ordering::SeqCst), 0);

        let mut server_impl = SshServerImpl {
            git_service: server.git_service.clone(),
            auth_manager: server.auth_manager.clone(),
            rate_limiter: server.rate_limiter.clone(),
            connection_count: server.connection_count.clone(),
            ip_connections: server.ip_connections.clone(),
            max_connections: 2,
            clients: Arc::new(Mutex::new(HashMap::new())),
            channel_senders: Arc::new(Mutex::new(HashMap::new())),
            id: 0,
            lfs_config: server.lfs_config.clone(),
        };

        let addr1 = "192.168.1.100:12345".parse::<SocketAddr>().unwrap();
        let _handler1 = server_impl.new_client(Some(addr1));
        assert_eq!(server.connection_count.load(Ordering::SeqCst), 1);

        let addr2 = "192.168.1.101:12346".parse::<SocketAddr>().unwrap();
        let _handler2 = server_impl.new_client(Some(addr2));
        assert_eq!(server.connection_count.load(Ordering::SeqCst), 2);

        let addr3 = "192.168.1.102:12347".parse::<SocketAddr>().unwrap();
        let handler3 = server_impl.new_client(Some(addr3));
        assert_eq!(server.connection_count.load(Ordering::SeqCst), 2);
        assert!(handler3.rejected);
    }

    #[tokio::test]
    async fn test_ssh_handler_rate_limiting() {
        use russh::server::Handler;

        let (server, _temp_dir) = create_test_ssh_server().await;

        let mut rate_limit_config = crate::git::rate_limit::RateLimitConfig::default();
        rate_limit_config.enabled = true;
        rate_limit_config.tiers.insert(
            "free".to_string(),
            crate::git::rate_limit::RateLimitTier {
                requests_per_minute: 2,
                requests_per_hour: 10,
                burst_size: 1,
                ..Default::default()
            },
        );

        let rate_limiter = RateLimiter::new(Some(rate_limit_config));

        let mut handler = SshHandler {
            git_service: server.git_service.clone(),
            auth_manager: server.auth_manager.clone(),
            rate_limiter,
            client_id: 1,
            clients: Arc::new(Mutex::new(HashMap::new())),
            channel_senders: Arc::new(Mutex::new(HashMap::new())),
            peer_ip: "192.168.1.100".to_string(),
            connection_count: server.connection_count.clone(),
            ip_connections: server.ip_connections.clone(),
            rejected: false,
            lfs_config: server.lfs_config.clone(),
        };

        use russh_keys::{Algorithm, PrivateKey};
        let private_key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        let public_key = private_key.public_key();

        let _result1 = handler.auth_publickey("testuser", &public_key).await;

        for _ in 0..5 {
            let _result = handler.auth_publickey("testuser", &public_key).await;
        }

        assert!(true);
    }
}

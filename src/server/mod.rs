use crate::adp::bootstrap_system_repo;
use crate::api::ApiServer;
use crate::config::AppConfig;
use crate::features::FeatureFlags;
use crate::git::{ssh::SshServer, GitConfig, GitService};
use crate::storage::local::LocalStorage;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::spawn;
use tracing::{error, info};

pub struct Server {
    config: Arc<AppConfig>,
    git_service: Arc<GitService>,
    feature_flags: FeatureFlags,
}

impl Server {
    pub fn new(config: AppConfig) -> Self {
        let storage = Arc::new(LocalStorage::new(PathBuf::from("./repositories")));

        let git_config = GitConfig::default();
        let git_service = Arc::new(GitService::new(storage, Some(git_config.clone())));

        let feature_flags = match &config.features {
            Some(features) => FeatureFlags::load_from_config(features),
            None => FeatureFlags::load_from_env(),
        };

        Self {
            config: Arc::new(config),
            git_service,
            feature_flags,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting repos-mcp server...");

        // Bootstrap the system repo before accepting any connections.
        // This is idempotent and safe to call on every start.
        let storage_root = match self.git_service.get_storage_root() {
            Some(p) => p,
            None => {
                error!("Cannot bootstrap system repo: no local storage root (S3 storage?)");
                return Ok(());
            }
        };
        match bootstrap_system_repo(&storage_root) {
            Ok(()) => info!("System repo bootstrap OK"),
            Err(e) => error!("System repo bootstrap failed (non-fatal): {}", e),
        }

        let config = (*self.config).clone();
        let git_service = self.git_service.clone();
        let feature_flags = self.feature_flags.clone();

        // Start SSH server in background
        info!("Starting SSH server on port 2222...");
        let ssh_git_service = git_service.clone();
        spawn(async move {
            let ssh_server = SshServer::new(ssh_git_service, None, None);
            if let Err(e) = ssh_server.run().await {
                error!("SSH server error: {}", e);
            }
        });

        // Start HTTP API server (blocks until server stops)
        info!("Starting API server on port {}...", config.server.port);
        let lfs_config = config.lfs.clone();
        let api_server = ApiServer::new(git_service.clone(), feature_flags, lfs_config);

        // Run API server - this will block
        api_server.run(config).await
    }
}

pub async fn run_server(config: AppConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = Server::new(config);
    server.run().await
}

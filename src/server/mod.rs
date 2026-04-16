use crate::adp::bootstrap_system_repo;
use crate::adp::bootstrap::{bootstrap_engram_schemas, bootstrap_operational_schemas};
use crate::api::ApiServer;
use crate::config::AppConfig;
use crate::features::FeatureFlags;
use crate::git::{ssh::SshServer, GitConfig, GitService};
use crate::storage::local::LocalStorage;
use actix_web::web::ServiceConfig;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::spawn;
use tracing::{error, info, warn};

/// Callback type for extending the actix App with additional routes/services.
pub type AppConfigurator = Box<dyn Fn(&mut ServiceConfig) + Send + Sync>;

pub struct Server {
    config: Arc<AppConfig>,
    git_service: Arc<GitService>,
    feature_flags: FeatureFlags,
    git_config: GitConfig,
    /// Optional callback to add extra routes/services to the actix App
    app_configurator: Option<Arc<AppConfigurator>>,
}

impl Server {
    pub fn new(config: AppConfig) -> Self {
        // Use AGENTIC_REPOS_STORAGE_ROOT env var, or fall back to ./repositories
        let storage_root = std::env::var("AGENTIC_REPOS_STORAGE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./repositories"));
        let storage = Arc::new(LocalStorage::new(storage_root.clone()));

        // Read SSH config from the ADP git-refs (the real config store).
        // Fall back to defaults if the system repo isn't bootstrapped yet.
        use crate::adp::bootstrap::SYSTEM_REPO_PATH;
        use crate::adp::storage::AdpStore;
        use crate::adp::SERVER_CONFIG_REF;
        let adp_cfg = {
            let system_repo = storage_root.join(SYSTEM_REPO_PATH);
            AdpStore::read_ref_blob(&system_repo, SERVER_CONFIG_REF)
                .ok()
                .and_then(|v| serde_json::from_value::<crate::adp::config::ServerConfig>(v).ok())
                .unwrap_or_default()
        };

        let git_config = GitConfig {
            ssh_port: adp_cfg.ssh_port,
            host_key_path: adp_cfg.ssh_host_key_path,
            authorized_keys_path: adp_cfg.ssh_authorized_keys_path,
            max_connections: adp_cfg.ssh_max_connections,
            connection_timeout_secs: adp_cfg.ssh_connection_timeout_secs,
            ..GitConfig::default()
        };
        let git_service = Arc::new(GitService::new(storage, Some(git_config.clone())));

        let feature_flags = match &config.features {
            Some(features) => FeatureFlags::load_from_config(features),
            None => FeatureFlags::load_from_env(),
        };

        Self {
            config: Arc::new(config),
            git_service,
            feature_flags,
            git_config,
            app_configurator: None,
        }
    }

    /// Set a callback to extend the actix App with additional routes/services.
    /// Called during HttpServer::new, after all agentic-repos routes are configured.
    pub fn with_app_configurator(mut self, configurator: AppConfigurator) -> Self {
        self.app_configurator = Some(Arc::new(configurator));
        self
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting agentic-repos server...");

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

        // Bootstrap engram entity schemas into system repo
        if let Err(e) = bootstrap_engram_schemas(&storage_root) {
            warn!("Engram schema bootstrap failed (non-fatal): {}", e);
        }

        // Bootstrap operational schemas (webhook-config, deploy-target, etc.)
        if let Err(e) = bootstrap_operational_schemas(&storage_root) {
            warn!("Operational schema bootstrap failed (non-fatal): {}", e);
        }

        let config = (*self.config).clone();
        let git_service = self.git_service.clone();
        let feature_flags = self.feature_flags.clone();

        // Start SSH server in background
        info!("Starting SSH server on port {}...", self.git_config.ssh_port);
        let ssh_git_service = git_service.clone();
        let ssh_git_config = self.git_config.clone();
        spawn(async move {
            let ssh_server = SshServer::new(ssh_git_service, Some(ssh_git_config), None);
            if let Err(e) = ssh_server.run().await {
                error!("SSH server error: {}", e);
            }
        });

        // Start HTTP API server (blocks until server stops)
        info!("Starting API server on port {}...", config.server.port);
        let lfs_config = config.lfs.clone();
        let api_server = ApiServer::new(git_service.clone(), feature_flags, lfs_config);

        // Run API server - this will block
        api_server.run_with_configurator(config, self.app_configurator.clone()).await
    }
}

pub async fn run_server(config: AppConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = Server::new(config);
    server.run().await
}

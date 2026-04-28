use crate::api::ApiServer;
use crate::auth::session::{InMemorySessionStore, SessionManager};
use crate::config::AppConfig;
use crate::features::FeatureFlags;
use crate::git::{ssh::SshServer, GitConfig, GitService};
use crate::hooks::Hooks;
use crate::storage::local::LocalStorage;
use actix_web::web;
use actix_web::web::ServiceConfig;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::spawn;
use tracing::{error, info, warn};

/// Callback type for extending the actix App with additional routes/services.
pub type AppConfigurator = Box<dyn Fn(&mut ServiceConfig) + Send + Sync>;

/// Configurator that runs INSIDE the /api/v1 scope.
/// Used by adp to register enterprise routes that share
/// the same /api/v1 prefix as agentic-repos core routes.
/// Without this, actix-web's scope shadowing would cause
/// enterprise routes to never match.
pub type ApiConfigurator = Box<dyn Fn(&mut web::ServiceConfig) + Send + Sync>;

/// Bootstraps the server on startup.
///
/// Open-core default: no bootstrapping needed.
/// ADP implementation: creates _adp/system repo and loads engram/operational schemas.
pub trait ServerBootstrap: Send + Sync {
    fn bootstrap(&self, storage_root: &Path) -> Result<(), String>;
}

pub struct Server {
    config: Arc<AppConfig>,
    git_service: Arc<GitService>,
    feature_flags: FeatureFlags,
    git_config: GitConfig,
    bootstrap: Arc<dyn ServerBootstrap>,
    /// Optional callback to add extra routes/services to the actix App
    app_configurator: Option<Arc<AppConfigurator>>,
    /// Optional callback to add routes INSIDE the /api/v1 scope
    api_configurator: Option<Arc<ApiConfigurator>>,
    session_manager: Option<Arc<SessionManager<InMemorySessionStore>>>,
}

impl Server {
    /// Create a server with default open-core bootstrapping (no ADP dependency).
    ///
    /// For ADP behavior, use `Server::with_adp(config, adp_hooks)`.
    pub fn new(config: AppConfig) -> Self {
        Self::with_bootstrap(config, Hooks::default(), Arc::new(NoOpServerBootstrap))
    }

    /// Create a server with custom hooks and bootstrap.
    pub fn with_bootstrap(
        config: AppConfig,
        hooks: Hooks,
        bootstrap: Arc<dyn ServerBootstrap>,
    ) -> Self {
        // Use AGENTIC_REPOS_STORAGE_ROOT env var, or fall back to ./repositories
        let storage_root = std::env::var("AGENTIC_REPOS_STORAGE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./repositories"));
        let storage = Arc::new(LocalStorage::new(storage_root.clone()));

        // Set storage_root in hooks so AuthBackend can resolve identities
        let mut hooks = hooks;
        hooks.storage_root = Some(storage_root.clone());

        let git_config = GitConfig::default();
        let git_service = Arc::new(GitService::new(storage, Some(git_config.clone()), hooks.clone(), Some(storage_root.clone())));

        let feature_flags = match &config.features {
            Some(features) => FeatureFlags::load_from_config(features),
            None => FeatureFlags::load_from_env(),
        };

        Self {
            config: Arc::new(config),
            git_service,
            feature_flags,
            git_config,
            bootstrap,
            app_configurator: None,
            api_configurator: None,
            session_manager: None,
        }
    }

    /// Set a callback to extend the actix App with additional routes/services.
    /// Called during HttpServer::new, after all agentic-repos routes are configured.
    pub fn with_app_configurator(mut self, configurator: AppConfigurator) -> Self {
        self.app_configurator = Some(Arc::new(configurator));
        self
    }

    /// Set a callback to add routes INSIDE the /api/v1 scope.
    /// This avoids actix-web scope shadowing — enterprise routes registered
    /// here share the same /api/v1 prefix as core agentic-repos routes.
    pub fn with_api_configurator(mut self, configurator: ApiConfigurator) -> Self {
        self.api_configurator = Some(Arc::new(configurator));
        self
    }

    /// Set a session manager for OAuth/session-based auth on /api/v1 routes.
    /// When set, the OAuthMiddleware wraps the entire /api/v1 scope.
    pub fn with_session_manager(mut self, sm: Arc<SessionManager<InMemorySessionStore>>) -> Self {
        self.session_manager = Some(sm);
        self
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting agentic-repos server...");

        // Run bootstrap hook (no-op in open-core; ADP implementation handles
        // system repo creation, schema registration, etc.)
        if let Some(storage_root) = self.git_service.get_storage_root() {
            if let Err(e) = self.bootstrap.bootstrap(&storage_root) {
                warn!("Server bootstrap failed (non-fatal): {}", e);
            }
        }

        let config = (*self.config).clone();
        let git_service = self.git_service.clone();
        let feature_flags = self.feature_flags.clone();

        // Start SSH server in background
        info!(
            "Starting SSH server on port {}...",
            self.git_config.ssh_port
        );
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
        let mut api_server = ApiServer::new(git_service.clone(), feature_flags, lfs_config);
        if let Some(ref sm) = self.session_manager {
            api_server = api_server.with_session_manager(sm.clone());
        }

        // Run API server - this will block
        api_server
            .run_with_configurator_and_api(config, self.app_configurator.clone(), self.api_configurator.clone())
            .await
    }
}

/// No-op bootstrap: does nothing. Used by the open-core binary.
pub struct NoOpServerBootstrap;

impl ServerBootstrap for NoOpServerBootstrap {
    fn bootstrap(&self, _storage_root: &Path) -> Result<(), String> {
        Ok(())
    }
}

pub async fn run_server(config: AppConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = Server::new(config);
    server.run().await
}
use crate::api::ApiServer;
use crate::config::AppConfig;
use crate::features::FeatureFlags;
use crate::git::{GitConfig, GitService};
use crate::storage::local::LocalStorage;
use std::path::PathBuf;
use std::sync::Arc;

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
        println!("Starting repos-mcp server...");

        let config = (*self.config).clone();
        let git_service = self.git_service.clone();
        let feature_flags = self.feature_flags.clone();

        println!("Starting API server on port {}...", config.server.port);
        let api_server = ApiServer::new(git_service.clone(), feature_flags);

        api_server.run(config).await
    }
}

pub async fn run_server(config: AppConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = Server::new(config);
    server.run().await
}

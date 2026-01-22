pub mod handlers;
pub mod lfs;
pub mod models;
pub mod routes;

use crate::config::AppConfig;
use crate::features::FeatureFlags;
use crate::git::GitService;
use crate::rbac::storage::InMemoryRbacStorage;
use crate::rbac::RbacService;
use actix_web::{middleware::Logger, web, App, HttpServer};
use std::sync::Arc;

pub struct ApiServer {
    git_service: Arc<GitService>,
    feature_flags: FeatureFlags,
    lfs_config: crate::config::LfsConfig,
}

impl ApiServer {
    pub fn new(
        git_service: Arc<GitService>,
        feature_flags: FeatureFlags,
        lfs_config: crate::config::LfsConfig,
    ) -> Self {
        Self {
            git_service,
            feature_flags,
            lfs_config,
        }
    }

    pub async fn run(
        &self,
        config: AppConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!(
            "API Server starting on {}:{}",
            config.server.host, config.server.port
        );

        let git_service = Arc::clone(&self.git_service);
        let feature_flags = self.feature_flags.clone();
        let lfs_config = self.lfs_config.clone();
        let _enable_web_ui = feature_flags.web_ui;

        let _rbac_service = if feature_flags.rbac {
            let storage = Arc::new(InMemoryRbacStorage::new());
            let service = Arc::new(RbacService::new(storage));

            if let Err(e) = service.initialize().await {
                println!("Warning: Failed to initialize RBAC service: {}", e);
            } else {
                println!("RBAC system initialized with default roles");
            }

            Some(service)
        } else {
            println!("RBAC feature disabled");
            None
        };

        let config_clone = config.clone();
        HttpServer::new(move || {
            let mut app = App::new()
                .app_data(web::Data::new(git_service.clone()))
                .app_data(web::Data::new(lfs_config.clone()))
                .wrap(Logger::default())
                .service(web::scope("/api/v1").configure(routes::configure_routes));

            if feature_flags.git_lfs && lfs_config.enabled {
                use crate::lfs::storage::LfsStorageAdapter;
                use crate::storage::local::LocalStorage;
                use std::path::PathBuf;
                use std::sync::Arc;

                let local_storage = LocalStorage::new(PathBuf::from("./test_repositories"));
                let lfs_storage = LfsStorageAdapter::new(
                    local_storage,
                    lfs_config.storage_path_pattern.clone(),
                    lfs_config.base_url.clone(),
                );
                let app_config = config_clone.clone();
                let handler = lfs::LfsHandler::new(Arc::new(lfs_storage), app_config);

                app = app.app_data(web::Data::new(handler)).service(
                    web::scope("/lfs")
                        .configure(lfs::configure_lfs_routes::<LfsStorageAdapter<LocalStorage>>),
                );
            }

            app
        })
        .bind(format!("{}:{}", config.server.host, config.server.port))?
        .run()
        .await?;

        Ok(())
    }
}

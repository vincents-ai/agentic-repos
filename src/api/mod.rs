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
}

impl ApiServer {
    pub fn new(git_service: Arc<GitService>, feature_flags: FeatureFlags) -> Self {
        Self {
            git_service,
            feature_flags,
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

        HttpServer::new(move || {
            let app = App::new()
                .app_data(web::Data::new(git_service.clone()))
                .wrap(Logger::default())
                .service(web::scope("/api/v1").configure(routes::configure_routes));

            app
        })
        .bind(format!("{}:{}", config.server.host, config.server.port))?
        .run()
        .await?;

        Ok(())
    }
}

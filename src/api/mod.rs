pub mod handlers;
pub mod models;
pub mod routes;

use crate::config::AppConfig;
use crate::features::FeatureFlags;
use crate::git::GitService;
use crate::rbac::storage::InMemoryRbacStorage;
use crate::rbac::{configure_rbac_routes, RbacMiddleware, RbacService};
use crate::web::WebUI;
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

        let git_service = self.git_service.clone();
        let feature_flags = self.feature_flags.clone();

        let rbac_service = if feature_flags.rbac {
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
                .wrap(Logger::default());

            let app = if let Some(rbac) = &rbac_service {
                println!("RBAC middleware enabled");
                app.app_data(web::Data::new(rbac.clone()))
                    .wrap(RbacMiddleware::new(rbac.clone()))
                    .service(web::scope("/api/v1").configure(configure_rbac_routes))
                    .service(web::scope("/api/v1").configure(routes::configure_routes))
            } else {
                app.service(web::scope("/api/v1").configure(routes::configure_routes))
            };

            if feature_flags.web_ui {
                if let Ok(web_ui) = WebUI::new(git_service.clone(), feature_flags.clone()) {
                    let web_ui_arc = Arc::new(web_ui);
                    app = app.configure(|cfg| WebUI::configure(cfg, web_ui_arc));
                    println!("Web UI enabled at /ui/");
                } else {
                    println!("Web UI feature enabled but failed to initialize");
                }
            } else {
                println!("Web UI feature disabled");
            }

            app
        })
        .bind(format!("{}:{}", config.server.host, config.server.port))?
        .run()
        .await?;

        Ok(())
    }
}

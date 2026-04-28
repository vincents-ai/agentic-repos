pub mod handlers;
pub mod lfs;
pub mod lfs_locks;
pub mod models;
pub mod routes;

use crate::auth::middleware::OAuthMiddleware;
use crate::auth::session::{InMemorySessionStore, SessionManager};
use crate::features::FeatureFlags;
use crate::git::GitService;
use crate::rbac::storage::InMemoryRbacStorage;
use crate::rbac::RbacService;
use crate::server::{ApiConfigurator, AppConfigurator};
use actix_web::{middleware::Logger, web, App, HttpRequest, HttpResponse, HttpServer};
use chrono::Duration as ChronoDuration;
use std::sync::Arc;

async fn metrics_endpoint(req: HttpRequest) -> HttpResponse {
    // Require auth for metrics — prevent information disclosure
    if crate::auth::middleware::get_current_session(&req).is_none() {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Authentication required"
        }));
    }
    match crate::lfs::encode_metrics() {
        Ok(metrics) => HttpResponse::Ok()
            .content_type("text/plain; version=0.0.4")
            .body(metrics),
        Err(e) => {
            tracing::error!("Failed to encode metrics: {}", e);
            HttpResponse::InternalServerError().body("Internal error")
        }
    }
}

pub struct ApiServer {
    git_service: Arc<GitService>,
    feature_flags: FeatureFlags,
    lfs_config: crate::config::LfsConfig,
    session_manager: Option<Arc<SessionManager<InMemorySessionStore>>>,
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
            session_manager: None,
        }
    }

    pub fn with_session_manager(mut self, sm: Arc<SessionManager<InMemorySessionStore>>) -> Self {
        self.session_manager = Some(sm);
        self
    }

    pub async fn run(
        &self,
        config: crate::config::AppConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.run_with_configurator(config, None).await
    }

    pub async fn run_with_configurator(
        &self,
        config: crate::config::AppConfig,
        app_configurator: Option<std::sync::Arc<AppConfigurator>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.run_with_configurator_and_api(config, app_configurator, None).await
    }

    pub async fn run_with_configurator_and_api(
        &self,
        config: crate::config::AppConfig,
        app_configurator: Option<std::sync::Arc<AppConfigurator>>,
        api_configurator: Option<std::sync::Arc<ApiConfigurator>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            host = %config.server.host,
            port = %config.server.port,
            "API server starting"
        );

        let git_service = Arc::clone(&self.git_service);
        let feature_flags = self.feature_flags.clone();
        let lfs_config = self.lfs_config.clone();

        let rbac_service_data: Option<web::Data<Arc<RbacService>>> = if feature_flags.rbac {
            let storage = Arc::new(InMemoryRbacStorage::new());
            let service = Arc::new(RbacService::new(storage));

            if let Err(e) = service.initialize().await {
                tracing::warn!(error = %e, "Failed to initialize RBAC service");
            } else {
                tracing::info!("RBAC system initialized with default roles");
            }

            Some(web::Data::new(service))
        } else {
            tracing::info!("RBAC feature disabled");
            None
        };

        let config_clone = config.clone();
        let session_manager = self.session_manager.clone().unwrap_or_else(|| {
            Arc::new(SessionManager::new(
                InMemorySessionStore::new(ChronoDuration::hours(1)),
                ChronoDuration::hours(1),
            ))
        });

        HttpServer::new(move || {
            let mut app = App::new()
                // Global request body size limit — 10MB max to prevent DoS
                .app_data(
                    web::JsonConfig::default().limit(10 * 1024 * 1024)
                )
                .app_data(web::Data::new(git_service.clone()))
                .app_data(web::Data::new(lfs_config.clone()))
                // RBAC service — required by require_permission middleware
                .app_data(rbac_service_data.clone().unwrap_or_else(|| {
                    web::Data::new(Arc::new(RbacService::new(Arc::new(
                        InMemoryRbacStorage::new(),
                    ))))
                }))
                .wrap(Logger::default())
                // Security headers for all responses
                .wrap(
                    actix_web::middleware::DefaultHeaders::new()
                        .add(("X-Content-Type-Options", "nosniff"))
                        .add(("X-Frame-Options", "DENY"))
                        .add(("X-XSS-Protection", "1; mode=block"))
                        .add(("Referrer-Policy", "strict-origin-when-cross-origin"))
                        .add(("Content-Security-Policy", "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'"))
                        .add(("Strict-Transport-Security", "max-age=31536000"))
                        .add(("Permissions-Policy", "camera=(), microphone=(), geolocation=()")),
                )
                .route("/metrics", web::get().to(metrics_endpoint))
                .service(
                    web::scope("/api/v1")
                        .wrap(
                            OAuthMiddleware::new(session_manager.clone())
                                .with_public_paths(vec![
                                    "/api/v1/health".to_string(),
                                    "/api/v1/info".to_string(),
                                ])
                        )
                        .configure(|cfg| {
                            routes::configure_routes(cfg);
                            if let Some(ref api_cfg) = api_configurator {
                                api_cfg(cfg);
                            }
                        })
                );

            if feature_flags.git_lfs && lfs_config.enabled {
                use crate::lfs::storage::LfsStorageAdapter;
                use crate::lfs::{
                    AuditLogger, BandwidthThrottler, InMemoryAuditStorage, QuotaManager,
                    ThrottleConfig,
                };
                use crate::storage::local::LocalStorage;

                // Use the same storage root as GitService (from AGENTIC_REPOS_STORAGE_ROOT)
                let storage_root = git_service
                    .get_storage_root()
                    .unwrap_or_else(|| std::path::PathBuf::from("./repositories"));
                let local_storage = LocalStorage::new(storage_root);
                let lfs_storage = LfsStorageAdapter::new(
                    local_storage,
                    lfs_config.storage_path_pattern.clone(),
                    lfs_config.base_url.clone(),
                );
                let app_config = config_clone.clone();

                let audit_storage = Arc::new(InMemoryAuditStorage::new(10000));
                let audit_logger = Arc::new(AuditLogger::new(
                    audit_storage,
                    lfs_config.audit.clone().into(),
                ));

                let quota_manager = Arc::new(QuotaManager::new(lfs_config.quota.clone().into()));

                let throttler = if lfs_config.throttle.enabled {
                    let throttle_config = ThrottleConfig {
                        default_bytes_per_second: lfs_config.throttle.default_bytes_per_second,
                        per_tenant_limits: lfs_config.throttle.per_tenant_limits.clone(),
                        per_repo_limits: lfs_config.throttle.per_repo_limits.clone(),
                        burst_size: lfs_config.throttle.burst_size,
                    };
                    Some(Arc::new(BandwidthThrottler::new(throttle_config)))
                } else {
                    None
                };

                let mut handler = lfs::LfsHandler::new(Arc::new(lfs_storage), app_config)
                    .with_audit_logger(audit_logger)
                    .with_quota_manager(quota_manager);

                if let Some(throttler) = throttler {
                    handler = handler.with_throttler(throttler);
                }

                app = app
                    .app_data(web::Data::new(handler))
                    .service(web::scope("/lfs").configure(
                        lfs::configure_lfs_routes::<
                            LfsStorageAdapter<LocalStorage>,
                            InMemoryAuditStorage,
                        >,
                    ));
            }

            // Apply external app configurator (e.g., adp SPA fallback + enterprise routes)
            if let Some(ref configurator) = app_configurator {
                app = app.configure(|cfg| configurator(cfg));
            }

            app
        })
        .bind(format!("{}:{}", config.server.host, config.server.port))?
        .run()
        .await?;

        Ok(())
    }
}

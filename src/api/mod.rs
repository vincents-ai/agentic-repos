pub mod handlers;
pub mod lfs;
pub mod lfs_locks;
pub mod models;
pub mod routes;

use crate::compliance::dora::{DoraService, InMemoryDoraStore};
use crate::compliance::dora_nis2_handlers::DoraNis2State;
use crate::compliance::gdpr_hipaa_handlers::GdprHipaaState;
use crate::compliance::handlers::{configure_compliance_routes, ComplianceState};
use crate::compliance::nis2::{InMemoryNis2Store, Nis2Service};
use crate::features::FeatureFlags;
use crate::git::GitService;
use crate::rbac::middleware::require_permission;
use crate::rbac::storage::InMemoryRbacStorage;
use crate::rbac::RbacService;
use crate::scaling::{ClusterManager, HealthChecker, ScalingConfig};
use crate::sdlc::{InMemorySdlcStore, SdlcService};
use crate::server::AppConfigurator;
use crate::webhooks::service::WebhookService;
use crate::webhooks::storage::InMemoryWebhookStorage;
use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer};
use routes::cluster::ClusterState;
use std::sync::Arc;
use tokio::sync::RwLock;

async fn metrics_endpoint(sdlc: Option<web::Data<Arc<SdlcService>>>) -> HttpResponse {
    // Export SDLC metrics to prometheus gauges before gathering
    if let Some(svc) = sdlc {
        if let Err(e) = crate::sdlc::export_sdlc_metrics(&svc) {
            tracing::warn!("Failed to export SDLC metrics: {}", e);
        }
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
        config: crate::config::AppConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.run_with_configurator(config, None).await
    }

    pub async fn run_with_configurator(
        &self,
        config: crate::config::AppConfig,
        app_configurator: Option<std::sync::Arc<AppConfigurator>>,
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

        let scaling_config = ScalingConfig::from_env();
        let cluster_manager = Arc::new(RwLock::new(ClusterManager::new(scaling_config.clone())));
        let health_checker = Arc::new(
            HealthChecker::new(scaling_config.instance_id.clone())
                .with_cluster_manager(Arc::clone(&cluster_manager)),
        );

        let cluster_state = web::Data::new(ClusterState {
            cluster_manager,
            health_checker,
            leader_election: None,
            config: scaling_config,
            session_affinity: None,
            distributed_lock: None,
        });

        let config_clone = config.clone();
        let sdlc_service = web::Data::new(Arc::new(SdlcService::new(Arc::new(
            InMemorySdlcStore::new(),
        ))));
        let webhook_service = web::Data::new(Arc::new(WebhookService::new(Arc::new(
            InMemoryWebhookStorage::new(),
        ))));
        let compliance_state = web::Data::new(ComplianceState::new(
            crate::compliance::ComplianceConfig::default(),
        ));
        let dora_nis2_state = web::Data::new(Arc::new(DoraNis2State {
            dora: Arc::new(DoraService::new(Arc::new(InMemoryDoraStore::new()))),
            nis2: Arc::new(Nis2Service::new(Arc::new(InMemoryNis2Store::new()))),
        }));
        let gdpr_hipaa_state: web::Data<Arc<GdprHipaaState>> = web::Data::new(Arc::new(GdprHipaaState {
            gdpr: Arc::new(crate::compliance::eu_data_protection::EuComplianceService::new(
                crate::compliance::eu_data_protection::InMemoryEuComplianceStore::shared(),
            )),
            hipaa: Arc::new(crate::compliance::hipaa::HipaaService::new(
                crate::compliance::hipaa::InMemoryBaaStore::shared(),
            )),
        }));

        // Secrets service — AGE key management for SOPS-encrypted secrets
        let secrets_storage_root = git_service
            .get_storage_root()
            .unwrap_or_else(|| std::path::PathBuf::from("./repositories"));
        let secrets_root_key = secrets_storage_root.join("wrapping-key.age");
        let secrets_service = web::Data::new(Arc::new(
            crate::api::routes::secrets_service::SecretsService::new(
                secrets_root_key,
                secrets_storage_root,
            ),
        ));
        HttpServer::new(move || {
            let mut app = App::new()
                .app_data(web::Data::new(git_service.clone()))
                .app_data(web::Data::new(lfs_config.clone()))
                .app_data(cluster_state.clone())
                .app_data(sdlc_service.clone())
                .app_data(webhook_service.clone())
                .app_data(compliance_state.clone())
                .app_data(dora_nis2_state.clone())
                .app_data(secrets_service.clone())
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
                        .add(("Content-Security-Policy", "default-src 'self'"))
                        .add(("Permissions-Policy", "camera=(), microphone=(), geolocation=()")),
                )
                .route("/metrics", web::get().to(metrics_endpoint))
                .service(web::scope("/api/v1/compliance")
                    .wrap(require_permission(crate::rbac::models::Permission::ComplianceManage))
                    .configure(|cfg| {
                    configure_compliance_routes(cfg, compliance_state.clone());
                    crate::compliance::dora_nis2_handlers::configure_dora_nis2_routes(
                        cfg,
                        dora_nis2_state.clone(),
                    );
                    crate::compliance::gdpr_hipaa_handlers::configure_gdpr_hipaa_routes(
                        cfg,
                        gdpr_hipaa_state.clone(),
                    );
                }))
                .service(web::scope("/api/v1").configure(routes::configure_routes));

            if feature_flags.git_lfs && lfs_config.enabled {
                use crate::lfs::storage::LfsStorageAdapter;
                use crate::lfs::{
                    AuditLogger, BandwidthThrottler, InMemoryAuditStorage, QuotaManager,
                    ThrottleConfig,
                };
                use crate::storage::local::LocalStorage;
                use std::sync::Arc;

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

            // Wire Web UI if feature flag is enabled
            if feature_flags.web_ui {
                if let Ok(web_ui) =
                    crate::web::WebUI::new(git_service.clone(), feature_flags.clone())
                {
                    app = app.configure(|cfg| {
                        crate::web::WebUI::configure(cfg, Arc::new(web_ui));
                    });
                }
            }

            // Apply external app configurator (e.g., adp-web-ui SPA fallback)
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

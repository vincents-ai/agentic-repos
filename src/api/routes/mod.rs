use crate::api::handlers;
use actix_web::web;

pub mod adp;
pub mod auth;
pub mod cluster;
pub mod repositories;
pub mod secrets;
pub mod secrets_service;
pub mod theme_service;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    let admin_perm = crate::rbac::middleware::require_permission(
        crate::rbac::models::Permission::SystemAdmin,
    );
    let settings_perm = crate::rbac::middleware::require_permission(
        crate::rbac::models::Permission::SettingsManage,
    );
    cfg.route("/health", web::get().to(handlers::health_check))
        .route("/info", web::get().to(handlers::api_info))
        .service(web::scope("/repositories").configure(repositories::configure_repository_routes))
        .service(web::scope("/auth").configure(auth::configure_auth_routes))
        .service(web::scope("/backups").wrap(settings_perm.clone()).configure(crate::backup::handlers::configure_backup_routes))
        .service(
            web::scope("/webhooks").configure(crate::webhooks::handlers::configure_webhook_routes),
        )
        .service(web::scope("/cluster").wrap(admin_perm.clone()).configure(cluster::configure_cluster_routes))
        .service(web::scope("/adp").wrap(admin_perm.clone()).configure(adp::configure_adp_routes))
        .service(web::scope("/secrets").configure(secrets::configure_secrets_routes))
        .service(web::scope("/rbac").configure(crate::rbac::handlers::configure_rbac_routes))
        .service(
            web::scope("/security").wrap(settings_perm.clone()).configure(crate::security::handlers::configure_security_routes),
        )
        .service(web::scope("/sdlc").wrap(settings_perm.clone()).configure(crate::sdlc::configure_sdlc_routes))
        .service(
            web::scope("/analytics").wrap(settings_perm.clone())
                .configure(crate::analytics::handlers::configure_analytics_routes),
        )
        .service(
            web::scope("/replication").configure(crate::replication::handlers::configure_replication_routes),
        )
        .service(
            web::scope("/cicd").configure(crate::cicd::webhooks::handlers::configure_cicd_routes),
        )
        .service(web::scope("/lfs-locks").configure(crate::api::lfs_locks::configure_locks_routes));
}

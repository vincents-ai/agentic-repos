use crate::api::handlers;
use actix_web::web;

pub mod adp;
pub mod auth;
pub mod cluster;
pub mod repositories;
pub mod secrets;
pub mod secrets_service;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(handlers::health_check))
        .route("/info", web::get().to(handlers::api_info))
        .service(web::scope("/repositories").configure(repositories::configure_repository_routes))
        .service(web::scope("/auth").configure(auth::configure_auth_routes))
        .service(web::scope("/backups").configure(crate::backup::handlers::configure_backup_routes))
        .service(
            web::scope("/webhooks").configure(crate::webhooks::handlers::configure_webhook_routes),
        )
        .service(web::scope("/cluster").configure(cluster::configure_cluster_routes))
        .service(web::scope("/adp").configure(adp::configure_adp_routes))
        .service(web::scope("/secrets").configure(secrets::configure_secrets_routes))
        .service(web::scope("/rbac").configure(crate::rbac::handlers::configure_rbac_routes))
        .service(web::scope("/security").configure(crate::security::handlers::configure_security_routes))
        .service(web::scope("/sdlc").configure(crate::sdlc::configure_sdlc_routes))
        .service(web::scope("/analytics").configure(crate::analytics::handlers::configure_analytics_routes))
        .service(web::scope("/replication").configure(crate::replication::handlers::configure_replication_routes))
        .service(web::scope("/cicd").configure(crate::cicd::webhooks::handlers::configure_cicd_routes))
        .service(web::scope("/lfs-locks").configure(crate::api::lfs_locks::configure_locks_routes))
        .default_service(web::route().to(handlers::not_found));
}

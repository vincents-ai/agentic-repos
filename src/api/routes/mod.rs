use crate::api::handlers;
use actix_web::web;

pub mod adp;
pub mod auth;
pub mod cluster;
pub mod repositories;
pub mod secrets;
pub mod users;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(handlers::health_check))
        .route("/info", web::get().to(handlers::api_info))
        .service(web::scope("/repositories").configure(repositories::configure_repository_routes))
        .service(web::scope("/users").configure(users::configure_user_routes))
        .service(web::scope("/auth").configure(auth::configure_auth_routes))
        .service(web::scope("/backups").configure(crate::backup::handlers::configure_backup_routes))
        .service(
            web::scope("/webhooks").configure(crate::webhooks::handlers::configure_webhook_routes),
        )
        .service(web::scope("/cluster").configure(cluster::configure_cluster_routes))
        .service(web::scope("/adp").configure(adp::configure_adp_routes))
        .service(web::scope("/secrets").configure(secrets::configure_secrets_routes))
        .default_service(web::route().to(handlers::not_found));
}

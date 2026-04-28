use crate::api::handlers;
use actix_web::web;

pub mod auth;
pub mod repositories;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(handlers::health_check))
        .route("/info", web::get().to(handlers::api_info))
        .service(web::scope("/repositories").configure(repositories::configure_repository_routes))
        .service(web::scope("/auth").configure(auth::configure_auth_routes))
        .service(web::scope("/lfs-locks").configure(crate::api::lfs_locks::configure_locks_routes));
}

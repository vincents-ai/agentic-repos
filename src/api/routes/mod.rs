use crate::api::handlers;
use actix_web::web;

pub mod adp;
pub mod auth;
pub mod cluster;
pub mod repositories;
pub mod secrets;
pub mod secrets_service;
pub mod theme_handlers;
pub mod theme_service;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(handlers::health_check))
        .route("/info", web::get().to(handlers::api_info))
        .service(web::scope("/repositories").configure(repositories::configure_repository_routes))
        .service(web::scope("/auth").configure(auth::configure_auth_routes))
        .service(web::scope("/lfs-locks").configure(crate::api::lfs_locks::configure_locks_routes))
        .service(
            web::scope("/theme")
                .service(theme_handlers::get_global_theme)
        )
        .service(
            web::scope("/repositories/{owner}/{repo}/theme")
                .service(theme_handlers::get_repo_theme)
                .service(theme_handlers::get_merged_theme)
        );
}

use crate::api::handlers;
use actix_web::web;

pub mod repositories;
pub mod users;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(handlers::health_check))
        .route("/info", web::get().to(handlers::api_info))
        .service(web::scope("/repositories").configure(repositories::configure_repository_routes))
        .service(web::scope("/users").configure(users::configure_user_routes))
        .default_service(web::route().to(handlers::not_found));
}

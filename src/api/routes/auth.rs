use crate::api::models::ApiError;
use crate::auth::session::InMemorySessionStore;
use crate::auth::SessionManager;
use crate::features::FeatureFlags;
use actix_web::{web, HttpResponse, Result as ActixResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub provider: String,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub authorization_url: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CallbackRequest {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

pub fn configure_auth_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/login", web::post().to(login))
        .route("/callback", web::post().to(callback))
        .route("/refresh", web::post().to(refresh))
        .route("/logout", web::post().to(logout))
        .route("/me", web::get().to(get_current_user));
}

pub async fn login(
    _payload: web::Json<LoginRequest>,
    _feature_flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    let error = ApiError::bad_request("OAuth authentication is not enabled");
    Ok(HttpResponse::BadRequest().json(error))
}

pub async fn callback(
    _payload: web::Json<CallbackRequest>,
    _feature_flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    let error = ApiError::bad_request("OAuth authentication is not enabled");
    Ok(HttpResponse::BadRequest().json(error))
}

pub async fn refresh(
    _payload: web::Json<RefreshRequest>,
    _session_manager: web::Data<Arc<SessionManager<InMemorySessionStore>>>,
) -> ActixResult<HttpResponse> {
    let error = ApiError::bad_request("OAuth authentication is not enabled");
    Ok(HttpResponse::BadRequest().json(error))
}

pub async fn logout(
    _session_manager: web::Data<Arc<SessionManager<InMemorySessionStore>>>,
    _feature_flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    Ok(HttpResponse::Ok().finish())
}

pub async fn get_current_user(
    _feature_flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    let error = ApiError::bad_request("OAuth authentication is not enabled");
    Ok(HttpResponse::BadRequest().json(error))
}

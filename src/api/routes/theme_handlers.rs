//! Theme HTTP handlers — read-only access to themes via API.
//!
//! Themes are written via `git push` to `refs/adp/theme` and read via
//! these endpoints. See theme_service.rs for validation and storage.

use crate::api::routes::theme_service::ThemeService;
use actix_web::{get, web, HttpRequest, HttpResponse};
use std::sync::Arc;

/// GET /api/v1/theme — global theme
#[get("/api/v1/theme")]
pub async fn get_global_theme(
    req: HttpRequest,
    service: web::Data<Arc<ThemeService>>,
) -> HttpResponse {
    if crate::auth::middleware::get_current_session(&req).is_none() {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Authentication required"
        }));
    }

    match service.get_global_theme() {
        Ok(theme) => HttpResponse::Ok().json(theme),
        Err(e) => {
            tracing::warn!("Failed to get global theme: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }))
        }
    }
}

/// GET /api/v1/repositories/{owner}/{repo}/theme — per-repo theme
#[get("/api/v1/repositories/{owner}/{repo}/theme")]
pub async fn get_repo_theme(
    req: HttpRequest,
    service: web::Data<Arc<ThemeService>>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    if crate::auth::middleware::get_current_session(&req).is_none() {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Authentication required"
        }));
    }

    let (owner, repo) = path.into_inner();
    let repo_path = std::path::PathBuf::from(format!(
        "{}/{}.git",
        service.storage_root().display(),
        owner
    ))
    .join(format!("{}.git", repo));

    match service.get_repo_theme(&repo_path) {
        Ok(theme) => HttpResponse::Ok().json(theme),
        Err(e) => {
            tracing::warn!("Failed to get repo theme: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }))
        }
    }
}

/// GET /api/v1/repositories/{owner}/{repo}/theme/merged — merged theme
#[get("/api/v1/repositories/{owner}/{repo}/theme/merged")]
pub async fn get_merged_theme(
    req: HttpRequest,
    service: web::Data<Arc<ThemeService>>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    if crate::auth::middleware::get_current_session(&req).is_none() {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Authentication required"
        }));
    }

    let (owner, repo) = path.into_inner();
    let repo_path = service.storage_root().join(owner).join(format!("{}.git", repo));

    match service.get_merged_theme(&repo_path) {
        Ok(merged) => HttpResponse::Ok().json(merged),
        Err(e) => {
            tracing::warn!("Failed to get merged theme: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Internal error"
            }))
        }
    }
}

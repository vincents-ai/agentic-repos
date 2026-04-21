//! Secrets API routes — wired to git ref storage with AES-256-GCM encryption.
//!
//! REST API endpoints for managing SOPS-encrypted secrets at refs/secrets/*.
//! Implements ADR-024 secrets infrastructure with egress control.

use crate::api::models::ApiError;
use crate::api::routes::secrets_service::SecretsService;
use crate::auth::middleware::get_current_session;
use actix_web::{web, HttpRequest, HttpResponse, Result as ActixResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Require authentication for all secrets endpoints.
/// Secrets are sensitive — even listing them requires auth.
fn require_auth(req: &HttpRequest) -> ActixResult<()> {
    if get_current_session(req).is_some() {
        return Ok(());
    }
    tracing::warn!("Unauthenticated access attempt to secrets: {}", req.path());
    Err(actix_web::error::ErrorUnauthorized(
        serde_json::json!({"error": "Authentication required"}).to_string(),
    ))
}

/// Secrets route configuration.
pub fn configure_secrets_routes(cfg: &mut web::ServiceConfig) {
    let perm = crate::rbac::middleware::require_permission(
        crate::rbac::models::Permission::RepositoryAdmin,
    );
    cfg.service(
        web::scope("/{owner}/{repo}/secrets")
            .wrap(perm)
            .route("/public-key", web::get().to(get_public_key))
            .route("", web::get().to(list_secrets))
            .route("", web::post().to(create_secret))
            .route("/{secret_name}", web::get().to(get_secret))
            .route("/{secret_name}", web::put().to(update_secret))
            .route("/{secret_name}", web::delete().to(delete_secret)),
    );
}

// === Request/Response Models ===

#[derive(Debug, Deserialize)]
pub struct ListSecretsQuery {
    pub prefix: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SecretListResponse {
    pub secrets: Vec<String>,
    pub total: usize,
}

#[derive(Debug, Deserialize)]
pub struct CreateSecretRequest {
    /// Secret name (without refs/secrets/ prefix).
    pub name: String,
    /// Key-value pairs to store.
    pub data: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSecretRequest {
    /// Key-value pairs to merge or replace.
    pub data: HashMap<String, String>,
    /// If true, replaces entire secret; if false, merges with existing.
    #[serde(default = "default_false")]
    pub replace: bool,
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Serialize)]
pub struct SecretResponse {
    pub name: String,
    pub data: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct PublicKeyResponse {
    pub public_key: String,
    pub algorithm: String,
}

/// Resolve owner/repo to a filesystem path.
/// In production this would look up the tenant's storage root.
fn repo_path(owner: &str, repo: &str) -> PathBuf {
    PathBuf::from(format!("/var/lib/repos/{}/{}", owner, repo))
}

// === Handlers ===

/// Get the AGE public key for a repository (for client-side encryption).
///
/// Returns the public key so clients can encrypt secrets before sending.
/// The public key is safe to expose — it can only encrypt, not decrypt.
pub async fn get_public_key(
    service: web::Data<Arc<SecretsService>>,
    req: HttpRequest,
    path: web::Path<(String, String)>,
) -> ActixResult<HttpResponse> {
    // Public key is public by design, but still require auth to prevent enumeration
    require_auth(&req)?;
    let (owner, repo) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);

    match service.get_public_key(&repo_path) {
        Ok(public_key) => Ok(HttpResponse::Ok().json(PublicKeyResponse {
            public_key,
            algorithm: "age-x25519".to_string(),
        })),
        Err(e) => {
            tracing::warn!("Secrets operation failed: {}", e);
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Internal error"})))
        }
    }
}

/// List all secrets for a repository.
pub async fn list_secrets(
    service: web::Data<Arc<SecretsService>>,
    req: HttpRequest,
    path: web::Path<(String, String)>,
    query: web::Query<ListSecretsQuery>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    let (owner, repo) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);

    match service.list_secrets(&repo_path, query.prefix.as_deref()) {
        Ok(secrets) => Ok(HttpResponse::Ok().json(SecretListResponse {
            total: secrets.len(),
            secrets,
        })),
        Err(e) => {
            tracing::warn!("Secrets operation failed: {}", e);
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Internal error"})))
        }
    }
}

/// Get a specific secret (metadata only).
///
/// SECURITY: Only returns metadata (name, version, keys). Plaintext data
/// is never exposed through this endpoint per ADR-024 egress control.
pub async fn get_secret(
    service: web::Data<Arc<SecretsService>>,
    req: HttpRequest,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    let (owner, repo, secret_name) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);

    match service.get_secret(&repo_path, &secret_name) {
        Ok(stored) => {
            // SECURITY: Return metadata-only response by default.
            // Plaintext data only exposed through explicit egress paths (webhooks).
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "name": stored.name,
                "version": stored.meta.version,
                "keys": stored.meta.keys,
                "updated_at": stored.meta.updated_at,
            })))
        }
        Err(e) => match e {
            super::secrets_service::SecretError::NotFound(_) => Ok(HttpResponse::NotFound().json(
                ApiError::not_found(&format!("Secret '{}' not found", secret_name)),
            )),
            err => {
                tracing::warn!("Secrets operation failed: {:?}", err);
                Ok(HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Internal error"})))
            },
        },
    }
}

/// Create a new secret.
///
/// SECURITY: The response contains only metadata, NOT the plaintext data.
/// Per ADR-024, plaintext secrets never leave the server via API.
pub async fn create_secret(
    service: web::Data<Arc<SecretsService>>,
    req: HttpRequest,
    path: web::Path<(String, String)>,
    payload: web::Json<CreateSecretRequest>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    let (owner, repo) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);
    let request = payload.into_inner();

    match service.create_secret(&repo_path, &request.name, request.data) {
        Ok(stored) => Ok(HttpResponse::Created().json(serde_json::json!({
            "name": stored.name,
            "version": stored.meta.version,
            "keys": stored.meta.keys,
            "updated_at": stored.meta.updated_at,
        }))),
        Err(e) => match e {
            super::secrets_service::SecretError::InvalidName(msg) => {
                Ok(HttpResponse::BadRequest().json(ApiError::bad_request(&msg)))
            }
            err => {
                tracing::warn!("Secrets operation failed: {:?}", err);
                Ok(HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Internal error"})))
            },
        },
    }
}

/// Update an existing secret.
///
/// SECURITY: The response contains only metadata, NOT the plaintext data.
pub async fn update_secret(
    service: web::Data<Arc<SecretsService>>,
    req: HttpRequest,
    path: web::Path<(String, String, String)>,
    payload: web::Json<UpdateSecretRequest>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    let (owner, repo, secret_name) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);
    let request = payload.into_inner();

    match service.update_secret(&repo_path, &secret_name, request.data, request.replace) {
        Ok(stored) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "name": stored.name,
            "version": stored.meta.version,
            "keys": stored.meta.keys,
            "updated_at": stored.meta.updated_at,
        }))),
        Err(e) => match e {
            super::secrets_service::SecretError::NotFound(_) => Ok(HttpResponse::NotFound().json(
                ApiError::not_found(&format!("Secret '{}' not found", secret_name)),
            )),
            super::secrets_service::SecretError::InvalidName(msg) => {
                Ok(HttpResponse::BadRequest().json(ApiError::bad_request(&msg)))
            }
            err => {
                tracing::warn!("Secrets operation failed: {:?}", err);
                Ok(HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Internal error"})))
            },
        },
    }
}

/// Delete a secret.
pub async fn delete_secret(
    service: web::Data<Arc<SecretsService>>,
    req: HttpRequest,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    let (owner, repo, secret_name) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);

    match service.delete_secret(&repo_path, &secret_name) {
        Ok(()) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => match e {
            super::secrets_service::SecretError::NotFound(_) => Ok(HttpResponse::NotFound().json(
                ApiError::not_found(&format!("Secret '{}' not found", secret_name)),
            )),
            err => {
                tracing::warn!("Secrets operation failed: {:?}", err);
                Ok(HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Internal error"})))
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_path_format() {
        let path = repo_path("acme", "my-repo");
        assert_eq!(path, PathBuf::from("/var/lib/repos/acme/my-repo"));
    }

    #[test]
    fn test_create_secret_request_deserialize() {
        let json = serde_json::json!({
            "name": "db-creds",
            "data": {"username": "admin", "password": "s3cret"}
        });
        let req: CreateSecretRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "db-creds");
        assert_eq!(req.data["username"], "admin");
    }

    #[test]
    fn test_update_secret_request_default_replace_false() {
        let json = serde_json::json!({"data": {"key": "val"}});
        let req: UpdateSecretRequest = serde_json::from_value(json).unwrap();
        assert!(!req.replace);
    }

    #[test]
    fn test_update_secret_request_replace_true() {
        let json = serde_json::json!({"data": {"key": "val"}, "replace": true});
        let req: UpdateSecretRequest = serde_json::from_value(json).unwrap();
        assert!(req.replace);
    }

    #[test]
    fn test_secret_list_response_serialize() {
        let resp = SecretListResponse {
            secrets: vec!["db-creds".to_string()],
            total: 1,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["total"], 1);
    }

    #[test]
    fn test_public_key_response_serialize() {
        let resp = PublicKeyResponse {
            public_key: "age1abc".to_string(),
            algorithm: "age-x25519".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["algorithm"], "age-x25519");
    }
}

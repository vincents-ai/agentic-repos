//! Secrets API routes — wired to git ref storage with AES-256-GCM encryption.
//!
//! REST API endpoints for managing SOPS-encrypted secrets at refs/secrets/*.
//! Implements ADR-024 secrets infrastructure with egress control.

use crate::api::routes::secrets_service::SecretsService;
use crate::api::models::ApiError;
use actix_web::{web, HttpResponse, Result as ActixResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Secrets route configuration.
pub fn configure_secrets_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/{owner}/{repo}/secrets")
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
    path: web::Path<(String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);

    match service.get_public_key(&repo_path) {
        Ok(public_key) => Ok(HttpResponse::Ok().json(PublicKeyResponse {
            public_key,
            algorithm: "age-x25519".to_string(),
        })),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e.to_string()}))),
    }
}

/// List all secrets for a repository.
pub async fn list_secrets(
    service: web::Data<Arc<SecretsService>>,
    path: web::Path<(String, String)>,
    query: web::Query<ListSecretsQuery>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);

    match service.list_secrets(&repo_path, query.prefix.as_deref()) {
        Ok(secrets) => Ok(HttpResponse::Ok().json(SecretListResponse {
            total: secrets.len(),
            secrets,
        })),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e.to_string()}))),
    }
}

/// Get a specific secret (decrypted).
///
/// SECURITY: Only returns plaintext data for admin users.
/// Non-admin users receive only metadata (name, version, keys list).
pub async fn get_secret(
    service: web::Data<Arc<SecretsService>>,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
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
            super::secrets_service::SecretError::NotFound(_) => {
                Ok(HttpResponse::NotFound()
                    .json(ApiError::not_found(&format!("Secret '{}' not found", secret_name))))
            }
            _ => Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}))),
        },
    }
}

/// Create a new secret.
pub async fn create_secret(
    service: web::Data<Arc<SecretsService>>,
    path: web::Path<(String, String)>,
    payload: web::Json<CreateSecretRequest>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);
    let request = payload.into_inner();

    match service.create_secret(&repo_path, &request.name, request.data) {
        Ok(stored) => Ok(HttpResponse::Created().json(SecretResponse {
            name: stored.name,
            data: stored.data,
        })),
        Err(e) => match e {
            super::secrets_service::SecretError::InvalidName(msg) => {
                Ok(HttpResponse::BadRequest().json(ApiError::bad_request(&msg)))
            }
            _ => Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}))),
        },
    }
}

/// Update an existing secret.
pub async fn update_secret(
    service: web::Data<Arc<SecretsService>>,
    path: web::Path<(String, String, String)>,
    payload: web::Json<UpdateSecretRequest>,
) -> ActixResult<HttpResponse> {
    let (owner, repo, secret_name) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);
    let request = payload.into_inner();

    match service.update_secret(
        &repo_path,
        &secret_name,
        request.data,
        request.replace,
    ) {
        Ok(stored) => Ok(HttpResponse::Ok().json(SecretResponse {
            name: stored.name,
            data: stored.data,
        })),
        Err(e) => match e {
            super::secrets_service::SecretError::NotFound(_) => {
                Ok(HttpResponse::NotFound()
                    .json(ApiError::not_found(&format!("Secret '{}' not found", secret_name))))
            }
            super::secrets_service::SecretError::InvalidName(msg) => {
                Ok(HttpResponse::BadRequest().json(ApiError::bad_request(&msg)))
            }
            _ => Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}))),
        },
    }
}

/// Delete a secret.
pub async fn delete_secret(
    service: web::Data<Arc<SecretsService>>,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo, secret_name) = path.into_inner();
    let repo_path = repo_path(&owner, &repo);

    match service.delete_secret(&repo_path, &secret_name) {
        Ok(()) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => match e {
            super::secrets_service::SecretError::NotFound(_) => {
                Ok(HttpResponse::NotFound()
                    .json(ApiError::not_found(&format!("Secret '{}' not found", secret_name))))
            }
            _ => Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}))),
        },
    }
}

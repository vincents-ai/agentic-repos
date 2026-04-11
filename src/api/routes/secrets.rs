//! Secrets API routes
//!
//! REST API endpoints for managing SOPS-encrypted secrets at refs/secrets/*.

use crate::adp::secrets::{SecretsManager, SECRETS_PREFIX};
use crate::api::models::ApiError;
use actix_web::{web, HttpResponse, Result as ActixResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Secrets route configuration.
pub fn configure_secrets_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/{owner}/{repo}/secrets")
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

// === Handlers ===

/// List all secrets for a repository.
pub async fn list_secrets(
    path: web::Path<(String, String)>,
    query: web::Query<ListSecretsQuery>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let prefix = query.prefix.as_deref().unwrap_or("");

    tracing::info!(owner = %owner, repo = %repo, prefix = %prefix, "listing secrets");

    // TODO: Actually query the git refs to get the list of secrets
    // For now, return an empty list
    let secrets: Vec<String> = vec![];

    Ok(HttpResponse::Ok().json(SecretListResponse {
        secrets,
        total: secrets.len(),
    }))
}

/// Get a specific secret.
pub async fn get_secret(
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo, secret_name) = path.into_inner();

    tracing::info!(owner = %owner, repo = %repo, secret_name = %secret_name, "getting secret");

    // TODO: Read the secret from refs/secrets/{secret_name} and decrypt
    let error = ApiError::not_found(&format!("Secret '{}' not found", secret_name));
    Ok(HttpResponse::NotFound().json(error))
}

/// Create a new secret.
pub async fn create_secret(
    path: web::Path<(String, String)>,
    payload: web::Json<CreateSecretRequest>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let request = payload.into_inner();

    tracing::info!(owner = %owner, repo = %repo, name = %request.name, "creating secret");

    // Validate secret name
    if request.name.is_empty() || request.name.contains('/') {
        let error = ApiError::bad_request("Invalid secret name");
        return Ok(HttpResponse::BadRequest().json(error));
    }

    // TODO: Get the repo's public key and encrypt the secret
    // TODO: Write to refs/secrets/{name}

    Ok(HttpResponse::Created().json(SecretResponse {
        name: request.name,
        data: request.data,
    }))
}

/// Update an existing secret.
pub async fn update_secret(
    path: web::Path<(String, String, String)>,
    payload: web::Json<UpdateSecretRequest>,
) -> ActixResult<HttpResponse> {
    let (owner, repo, secret_name) = path.into_inner();
    let request = payload.into_inner();

    tracing::info!(owner = %owner, repo = %repo, name = %secret_name, replace = request.replace, "updating secret");

    // TODO: Read existing secret, merge or replace, encrypt, write back

    Ok(HttpResponse::Ok().json(SecretResponse {
        name: secret_name,
        data: request.data,
    }))
}

/// Delete a secret.
pub async fn delete_secret(
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo, secret_name) = path.into_inner();

    tracing::info!(owner = %owner, repo = %repo, name = %secret_name, "deleting secret");

    // TODO: Delete refs/secrets/{secret_name}

    Ok(HttpResponse::NoContent().finish())
}
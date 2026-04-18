//! ADP schema + access REST API routes.
//!
//! ## Authentication
//!
//! Endpoints that modify access config or schemas require authentication.
//! The caller identity is extracted from the verified session (NOT from
//! client-supplied headers) and resolved against the repo's access config
//! to enforce owner/admin checks.
//!
//! ## Endpoint summary
//!
//! **Repo-level** (`/repos/{owner}/{repo}/adp`):
//! - `GET/POST /schemas` — list/create schemas
//! - `GET/PUT/DELETE /schemas/{id}` — read/update/delete schema
//! - `GET /schemas/resolve?ref=` — resolve schema by ref name
//! - `GET /schemas/{id}/versions` — list schema version history
//! - `GET /refs[?prefix=]` — list ADP refs
//! - `GET /refs/{ref_path}` — read ref blob
//! - `GET /blobs/{sha}` — read blob by SHA
//! - `GET/PUT /access` — read/update access config (owner/admin only)
//!
//! **System-level** (`/system/adp`):
//! - Same schema endpoints as repo-level
//! - `GET/PUT /access` — read/update system access config (super-admin only)

use crate::adp::glob::resolve_schema;
use crate::adp::identity::CallerIdentity;
use crate::adp::storage::{AdpStore, AdpStoreError};
use crate::adp::{AccessConfig, RefSchema, ADP_REF_PREFIX, SCHEMA_PREFIX};
use crate::api::models::ApiError;
use crate::auth::middleware::get_current_user;
use crate::git::GitService;
use actix_web::{web, HttpRequest, HttpResponse, Result as ActixResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

pub fn configure_adp_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/repos/{owner}/{repo}/adp")
            .service(
                web::scope("/schemas")
                    .route("", web::get().to(list_schemas))
                    .route("", web::post().to(create_schema))
                    .route("/resolve", web::get().to(resolve_schema_handler))
                    .route("/{id}", web::get().to(get_schema))
                    .route("/{id}", web::put().to(update_schema))
                    .route("/{id}", web::delete().to(delete_schema))
                    .route("/{id}/versions", web::get().to(list_schema_versions)),
            )
            .service(
                web::scope("/refs")
                    .route("", web::get().to(list_refs))
                    .route("/{ref_path:.*}", web::get().to(read_ref)),
            )
            .route("/blobs/{sha}", web::get().to(read_blob))
            .route("/access", web::get().to(get_access_config))
            .route("/access", web::put().to(update_access_config)),
    )
    .service(
        web::scope("/system/adp")
            .service(
                web::scope("/schemas")
                    .route("", web::get().to(list_system_schemas))
                    .route("", web::post().to(create_system_schema))
                    .route("/{id}", web::get().to(get_system_schema))
                    .route("/{id}", web::put().to(update_system_schema))
                    .route("/{id}", web::delete().to(delete_system_schema))
                    .route("/{id}/versions", web::get().to(list_system_schema_versions))
                    .route("/resolve", web::get().to(resolve_system_schema_handler)),
            )
            .route("/access", web::get().to(get_system_access_config))
            .route("/access", web::put().to(update_system_access_config)),
    );
}

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RepoPath(PathBuf);

impl RepoPath {
    fn new(owner: &str, repo: &str, root: &std::path::Path) -> Self {
        // System repo uses bare path without .git suffix
        let suffix = if owner == "_adp" && repo == "system" {
            repo.to_string()
        } else {
            format!("{}.git", repo)
        };
        Self(root.join(owner).join(suffix))
    }

    fn as_path(&self) -> &std::path::Path {
        &self.0
    }
}

fn storage_root(git_service: &GitService) -> Result<PathBuf, ApiError> {
    git_service
        .get_storage_root()
        .ok_or_else(|| ApiError::internal_error("No local storage root available (S3 storage?)"))
}

fn adp_error(e: AdpStoreError) -> ApiError {
    match &e {
        AdpStoreError::NotFound(msg) => ApiError::not_found(msg),
        AdpStoreError::Json(msg) => ApiError::bad_request(&msg.to_string()),
        _ => ApiError::internal_error(&e.to_string()),
    }
}

fn system_repo_path(git_service: &GitService) -> Result<PathBuf, ApiError> {
    Ok(storage_root(git_service)?.join("_adp/system"))
}

/// Extract the authenticated username from the verified session.
///
/// IMPORTANT: This reads the identity from the session token (set by
/// OAuthMiddleware), NOT from any client-supplied header. Client headers
/// are untrusted and MUST NOT be used for identity extraction.
fn extract_username(req: &HttpRequest) -> Option<String> {
    get_current_user(req).and_then(|u| u.username)
}

/// Resolve a `CallerIdentity` for the given repo path and request.
///
/// Returns a `CallerIdentity` with `is_owner`/`is_admin` resolved from the
/// repo's access config. If no access config exists or the request is
/// unauthenticated, returns a minimal identity with no privileges.
fn resolve_caller(repo_path: &std::path::Path, req: &HttpRequest) -> CallerIdentity {
    let username = match extract_username(req) {
        Some(u) => u,
        None => {
            return CallerIdentity {
                username: String::new(),
                is_owner: false,
                is_admin: false,
            }
        }
    };

    let access_config = AdpStore::read_access_config(repo_path)
        .ok()
        .flatten()
        .unwrap_or_default();

    CallerIdentity::resolve(&username, &access_config)
}

/// Check that the caller has admin access. Returns an HTTP 403 response if not.
fn require_admin(caller: &CallerIdentity) -> ActixResult<()> {
    if caller.can_see_private_refs() {
        Ok(())
    } else {
        Err(actix_web::error::ErrorForbidden(
            serde_json::to_string(&ApiError::new(
                "forbidden",
                &format!("User '{}' is not an owner or admin", caller.username),
                403,
            ))
            .unwrap_or_default(),
        ))
    }
}

#[derive(Debug, Deserialize)]
pub struct ResolveQuery {
    pub r#ref: String,
}

#[derive(Debug, Deserialize)]
pub struct ListRefsQuery {
    pub prefix: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RefEntry {
    pub name: String,
    pub schema_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BlobResponse {
    pub sha: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SchemaVersion {
    pub version: u32,
    pub ref_name: String,
}

/// Validate a `RefSchema` before writing.
fn validate_schema(schema: &RefSchema) -> Result<(), ApiError> {
    if schema.id.is_empty() {
        return Err(ApiError::bad_request("Schema ID must not be empty"));
    }
    if schema.id.contains('/') || schema.id.contains(' ') {
        return Err(ApiError::bad_request(
            "Schema ID must not contain '/' or spaces",
        ));
    }
    if schema.namespace_pattern.is_empty() {
        return Err(ApiError::bad_request(
            "Schema namespace_pattern must not be empty",
        ));
    }
    if schema.version.is_empty() {
        return Err(ApiError::bad_request("Schema version must not be empty"));
    }

    // Reject platform-internal namespaces
    if schema.namespace_pattern.starts_with("refs/adp/") {
        return Err(ApiError::bad_request(
            "Namespace pattern 'refs/adp/*' is reserved for platform use. \
             Use a user-facing namespace like refs/engram/*, refs/webhooks/*, etc.",
        ));
    }

    // Validate that the schema field is a valid JSON Schema
    if !schema.schema.is_object() {
        return Err(ApiError::bad_request("Schema field must be a JSON object"));
    }
    // Try to compile it — this catches invalid JSON Schema syntax
    use crate::adp::schema::SchemaValidator;
    if let Err(e) = SchemaValidator::compile(&schema.schema) {
        return Err(ApiError::bad_request(&format!(
            "Invalid JSON Schema: {}",
            e
        )));
    }

    Ok(())
}

/// Validate an `AccessConfig` before writing.
fn validate_access_config(config: &AccessConfig) -> Result<(), ApiError> {
    if config.version.is_empty() {
        return Err(ApiError::bad_request(
            "Access config version must not be empty",
        ));
    }
    for owner in &config.owners {
        if owner.is_empty() {
            return Err(ApiError::bad_request("Owner usernames must not be empty"));
        }
    }
    for admin in &config.admins {
        if admin.is_empty() {
            return Err(ApiError::bad_request("Admin usernames must not be empty"));
        }
    }
    for (i, rule) in config.rules.iter().enumerate() {
        if rule.user.is_empty() {
            return Err(ApiError::bad_request(&format!(
                "Rule {} user must not be empty",
                i
            )));
        }
        if rule.paths.is_empty() {
            return Err(ApiError::bad_request(&format!(
                "Rule {} must have at least one path",
                i
            )));
        }
    }
    Ok(())
}

/// Check if a new schema's namespace_pattern overlaps with existing schemas.
/// Logs a warning but does not reject — overlap is allowed, most-specific wins.
fn check_namespace_overlap(
    repo_path: &std::path::Path,
    new_pattern: &str,
    new_id: &str,
) -> Result<(), ApiError> {
    if let Ok(schemas) = AdpStore::list_schemas(repo_path) {
        for existing in &schemas {
            if existing.id == new_id {
                continue; // Skip self
            }
            // Check if patterns could match the same refs
            if patterns_overlap(&existing.namespace_pattern, new_pattern) {
                tracing::warn!(
                    new_schema = %new_id,
                    new_pattern = %new_pattern,
                    existing_schema = %existing.id,
                    existing_pattern = %existing.namespace_pattern,
                    "Schema namespace overlap detected — most-specific match wins"
                );
            }
        }
    }
    Ok(())
}

/// Simple overlap check: two patterns overlap if they share a common prefix
/// up to the first wildcard.
fn patterns_overlap(a: &str, b: &str) -> bool {
    let prefix_a = a.split('*').next().unwrap_or("");
    let prefix_b = b.split('*').next().unwrap_or("");
    // If either prefix is a prefix of the other, they overlap
    prefix_a.starts_with(prefix_b) || prefix_b.starts_with(prefix_a)
}
// ---------------------------------------------------------------------------

pub async fn list_schemas(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service)?);

    match AdpStore::list_schemas(rp.as_path()) {
        Ok(schemas) => Ok(HttpResponse::Ok().json(schemas)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn create_schema(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
    payload: web::Json<RefSchema>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service)?);
    let schema = payload.into_inner();

    validate_schema(&schema)?;

    // Check namespace overlap with existing schemas
    check_namespace_overlap(rp.as_path(), &schema.namespace_pattern, &schema.id)?;

    // Check if schema already exists
    if AdpStore::read_schema(rp.as_path(), &schema.id)
        .ok()
        .flatten()
        .is_some()
    {
        return Ok(HttpResponse::Conflict().json(ApiError::conflict(&format!(
            "Schema '{}' already exists",
            schema.id
        ))));
    }

    match AdpStore::write_schema(rp.as_path(), &schema) {
        Ok(()) => Ok(HttpResponse::Created().json(schema)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn get_schema(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    let (_owner, _repo, id) = path.into_inner();
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service)?);

    match AdpStore::read_schema(rp.as_path(), &id) {
        Ok(Some(schema)) => Ok(HttpResponse::Ok().json(schema)),
        Ok(None) => Ok(HttpResponse::NotFound()
            .json(ApiError::not_found(&format!("Schema '{}' not found", id)))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn update_schema(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String, String)>,
    payload: web::Json<RefSchema>,
) -> ActixResult<HttpResponse> {
    let (_owner, _repo, id) = path.into_inner();
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service)?);
    let mut schema = payload.into_inner();

    if schema.id != id {
        return Ok(HttpResponse::BadRequest().json(ApiError::bad_request(
            "Schema ID in URL does not match body",
        )));
    }

    validate_schema(&schema)?;

    // Check namespace overlap
    check_namespace_overlap(rp.as_path(), &schema.namespace_pattern, &schema.id)?;

    // Require version bump on update
    if let Ok(Some(existing)) = AdpStore::read_schema(rp.as_path(), &id) {
        if schema.version == existing.version {
            return Ok(
                HttpResponse::BadRequest().json(ApiError::bad_request(&format!(
                    "Version must be bumped (current: {})",
                    existing.version
                ))),
            );
        }
    }

    schema.updated_at = chrono::Utc::now();

    match AdpStore::write_schema(rp.as_path(), &schema) {
        Ok(()) => Ok(HttpResponse::Ok().json(schema)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn delete_schema(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    let (_owner, _repo, id) = path.into_inner();
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service)?);
    let refname = format!("{}{}", SCHEMA_PREFIX, id);

    if AdpStore::read_schema(rp.as_path(), &id)
        .ok()
        .flatten()
        .is_none()
    {
        return Ok(HttpResponse::NotFound()
            .json(ApiError::not_found(&format!("Schema '{}' not found", id))));
    }

    let gix_repo = match gix::open(rp.as_path()) {
        Ok(r) => r,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(ApiError::not_found("Repository not found")))
        }
    };

    match gix_repo.edit_reference(gix::refs::transaction::RefEdit {
        change: gix::refs::transaction::Change::Delete {
            expected: gix::refs::transaction::PreviousValue::MustExist,
            log: gix::refs::transaction::RefLog::AndReference,
        },
        name: gix::refs::FullName::try_from(refname.as_str())
            .map_err(|_| ApiError::bad_request("Invalid ref name"))?,
        deref: false,
    }) {
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(adp_error(AdpStoreError::GixBoxed(e.to_string())))),
    }
}

/// List all version sidecars for a given schema.
pub async fn list_schema_versions(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    let (_owner, _repo, id) = path.into_inner();
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service)?);

    // Check schema exists
    if AdpStore::read_schema(rp.as_path(), &id)
        .ok()
        .flatten()
        .is_none()
    {
        return Ok(HttpResponse::NotFound()
            .json(ApiError::not_found(&format!("Schema '{}' not found", id))));
    }

    // List version sidecar refs
    let sidecar_prefix = format!("{}v", SCHEMA_PREFIX);
    let refs = match AdpStore::list_refs(rp.as_path(), &sidecar_prefix) {
        Ok(r) => r,
        Err(e) => return Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    };

    // Parse version numbers from refs matching this schema id
    let mut versions: Vec<SchemaVersion> = Vec::new();
    let expected_suffix = format!("/{}", id);
    for r in refs {
        if r.ends_with(&expected_suffix) {
            // Extract version number from refs/adp/schema/v{N}/{id}
            let after_prefix = r.strip_prefix(SCHEMA_PREFIX).unwrap_or(&r);
            // after_prefix = "v{N}/{id}"
            if let Some(v_end) = after_prefix.find('/') {
                let v_str = &after_prefix[1..v_end]; // skip 'v'
                if let Ok(version) = v_str.parse::<u32>() {
                    versions.push(SchemaVersion {
                        version,
                        ref_name: r,
                    });
                }
            }
        }
    }
    versions.sort_by_key(|v| v.version);

    Ok(HttpResponse::Ok().json(versions))
}

pub async fn resolve_schema_handler(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
    query: web::Query<ResolveQuery>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service)?);

    let schemas = match AdpStore::list_schemas(rp.as_path()) {
        Ok(s) => s,
        Err(e) => return Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    };

    match resolve_schema(&schemas, &query.r#ref) {
        Some(schema) => Ok(HttpResponse::Ok().json(schema)),
        None => Ok(HttpResponse::NotFound().json(ApiError::not_found(&format!(
            "No schema matches ref '{}'",
            query.r#ref
        )))),
    }
}

// ---------------------------------------------------------------------------
// Repo-level ref/blob browsing
// ---------------------------------------------------------------------------

pub async fn list_refs(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
    query: web::Query<ListRefsQuery>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service)?);

    let prefix = query.prefix.as_deref().unwrap_or(ADP_REF_PREFIX);

    let refs = match AdpStore::list_refs(rp.as_path(), prefix) {
        Ok(r) => r,
        Err(e) => return Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    };

    let schemas = AdpStore::list_schemas(rp.as_path()).unwrap_or_default();

    let entries: Vec<RefEntry> = refs
        .into_iter()
        .map(|name| {
            let schema_id = resolve_schema(&schemas, &name).map(|s| s.id.clone());
            RefEntry { name, schema_id }
        })
        .collect();

    Ok(HttpResponse::Ok().json(entries))
}

pub async fn read_ref(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    let (_owner, _repo, ref_path) = path.into_inner();
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service)?);

    match AdpStore::read_ref_blob(rp.as_path(), &ref_path) {
        Ok(value) => Ok(HttpResponse::Ok().json(value)),
        Err(AdpStoreError::NotFound(_)) => Ok(HttpResponse::NotFound().json(ApiError::not_found(
            &format!("Ref '{}' not found", ref_path),
        ))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn read_blob(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String, String)>,
) -> ActixResult<HttpResponse> {
    let (_owner, _repo, sha) = path.into_inner();
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service)?);

    let gix_repo = match gix::open(rp.as_path()) {
        Ok(r) => r,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(ApiError::not_found("Repository not found")))
        }
    };

    let oid = match gix::hash::ObjectId::from_hex(sha.as_bytes()) {
        Ok(o) => o,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(ApiError::bad_request("Invalid SHA hash")))
        }
    };

    let obj = match gix_repo.find_object(oid) {
        Ok(o) => o,
        Err(_) => {
            return Ok(HttpResponse::NotFound()
                .json(ApiError::not_found(&format!("Blob '{}' not found", sha))))
        }
    };

    let content = obj.data.to_vec();
    let value: serde_json::Value = serde_json::from_slice(&content).unwrap_or_else(|_| {
        serde_json::json!({
            "raw": String::from_utf8_lossy(&content).to_string()
        })
    });

    Ok(HttpResponse::Ok().json(BlobResponse {
        sha,
        content: value,
    }))
}

// ---------------------------------------------------------------------------
// Repo-level access config (owner/admin only)
// ---------------------------------------------------------------------------

pub async fn get_access_config(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
    req: HttpRequest,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service)?);

    let caller = resolve_caller(rp.as_path(), &req);
    require_admin(&caller)?;

    match AdpStore::read_access_config(rp.as_path()) {
        Ok(Some(config)) => Ok(HttpResponse::Ok().json(config)),
        Ok(None) => {
            Ok(HttpResponse::NotFound().json(ApiError::not_found("Access config not found")))
        }
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn update_access_config(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
    payload: web::Json<AccessConfig>,
    req: HttpRequest,
) -> ActixResult<HttpResponse> {
    let (_owner, _repo) = path.into_inner();
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service)?);
    let config = payload.into_inner();

    validate_access_config(&config)?;

    // Only owners can modify access config
    let caller = resolve_caller(rp.as_path(), &req);
    if !caller.is_owner {
        return Err(actix_web::error::ErrorForbidden(
            serde_json::to_string(&ApiError::new(
                "forbidden",
                &format!("User '{}' is not an owner", caller.username),
                403,
            ))
            .unwrap_or_default(),
        ));
    }

    match AdpStore::write_access_config(rp.as_path(), &config) {
        Ok(()) => Ok(HttpResponse::Ok().json(config)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

// ---------------------------------------------------------------------------
// System-level schema endpoints
// ---------------------------------------------------------------------------

pub async fn list_system_schemas(
    git_service: web::Data<Arc<GitService>>,
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service)?;

    match AdpStore::list_schemas(&rp) {
        Ok(schemas) => Ok(HttpResponse::Ok().json(schemas)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn create_system_schema(
    git_service: web::Data<Arc<GitService>>,
    payload: web::Json<RefSchema>,
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service)?;
    let schema = payload.into_inner();

    validate_schema(&schema)?;

    // Check if schema already exists
    if AdpStore::read_schema(&rp, &schema.id)
        .ok()
        .flatten()
        .is_some()
    {
        return Ok(HttpResponse::Conflict().json(ApiError::conflict(&format!(
            "Schema '{}' already exists",
            schema.id
        ))));
    }

    match AdpStore::write_schema(&rp, &schema) {
        Ok(()) => Ok(HttpResponse::Created().json(schema)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn get_system_schema(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let id = path.into_inner();
    let rp = system_repo_path(&git_service)?;

    match AdpStore::read_schema(&rp, &id) {
        Ok(Some(schema)) => Ok(HttpResponse::Ok().json(schema)),
        Ok(None) => Ok(HttpResponse::NotFound()
            .json(ApiError::not_found(&format!("Schema '{}' not found", id)))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn update_system_schema(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
    payload: web::Json<RefSchema>,
) -> ActixResult<HttpResponse> {
    let id = path.into_inner();
    let rp = system_repo_path(&git_service)?;
    let mut schema = payload.into_inner();

    if schema.id != id {
        return Ok(HttpResponse::BadRequest().json(ApiError::bad_request(
            "Schema ID in URL does not match body",
        )));
    }

    validate_schema(&schema)?;

    schema.updated_at = chrono::Utc::now();

    match AdpStore::write_schema(&rp, &schema) {
        Ok(()) => Ok(HttpResponse::Ok().json(schema)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn delete_system_schema(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let id = path.into_inner();
    let rp = system_repo_path(&git_service)?;
    let refname = format!("{}{}", SCHEMA_PREFIX, id);

    if AdpStore::read_schema(&rp, &id).ok().flatten().is_none() {
        return Ok(HttpResponse::NotFound()
            .json(ApiError::not_found(&format!("Schema '{}' not found", id))));
    }

    let gix_repo = match gix::open(&rp) {
        Ok(r) => r,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(ApiError::not_found("System repo not found")))
        }
    };

    match gix_repo.edit_reference(gix::refs::transaction::RefEdit {
        change: gix::refs::transaction::Change::Delete {
            expected: gix::refs::transaction::PreviousValue::MustExist,
            log: gix::refs::transaction::RefLog::AndReference,
        },
        name: gix::refs::FullName::try_from(refname.as_str())
            .map_err(|_| ApiError::bad_request("Invalid ref name"))?,
        deref: false,
    }) {
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(adp_error(AdpStoreError::GixBoxed(e.to_string())))),
    }
}

/// List all version sidecars for a given system-level schema.
pub async fn list_system_schema_versions(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let id = path.into_inner();
    let rp = system_repo_path(&git_service)?;

    // Check schema exists
    if AdpStore::read_schema(&rp, &id).ok().flatten().is_none() {
        return Ok(HttpResponse::NotFound()
            .json(ApiError::not_found(&format!("Schema '{}' not found", id))));
    }

    let sidecar_prefix = format!("{}v", SCHEMA_PREFIX);
    let refs = match AdpStore::list_refs(&rp, &sidecar_prefix) {
        Ok(r) => r,
        Err(e) => return Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    };

    let mut versions: Vec<SchemaVersion> = Vec::new();
    let expected_suffix = format!("/{}", id);
    for r in refs {
        if r.ends_with(&expected_suffix) {
            let after_prefix = r.strip_prefix(SCHEMA_PREFIX).unwrap_or(&r);
            if let Some(v_end) = after_prefix.find('/') {
                let v_str = &after_prefix[1..v_end];
                if let Ok(version) = v_str.parse::<u32>() {
                    versions.push(SchemaVersion {
                        version,
                        ref_name: r,
                    });
                }
            }
        }
    }
    versions.sort_by_key(|v| v.version);

    Ok(HttpResponse::Ok().json(versions))
}

pub async fn resolve_system_schema_handler(
    git_service: web::Data<Arc<GitService>>,
    query: web::Query<ResolveQuery>,
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service)?;

    let schemas = match AdpStore::list_schemas(&rp) {
        Ok(s) => s,
        Err(e) => return Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    };

    match resolve_schema(&schemas, &query.r#ref) {
        Some(schema) => Ok(HttpResponse::Ok().json(schema)),
        None => Ok(HttpResponse::NotFound().json(ApiError::not_found(&format!(
            "No schema matches ref '{}'",
            query.r#ref
        )))),
    }
}

// ---------------------------------------------------------------------------
// System-level access config (super-admin only)
// ---------------------------------------------------------------------------

pub async fn get_system_access_config(
    git_service: web::Data<Arc<GitService>>,
    req: HttpRequest,
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service)?;

    let caller = resolve_caller(&rp, &req);
    require_admin(&caller)?;

    match AdpStore::read_access_config(&rp) {
        Ok(Some(config)) => Ok(HttpResponse::Ok().json(config)),
        Ok(None) => Ok(
            HttpResponse::NotFound().json(ApiError::not_found("System access config not found"))
        ),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn update_system_access_config(
    git_service: web::Data<Arc<GitService>>,
    payload: web::Json<AccessConfig>,
    req: HttpRequest,
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service)?;

    let caller = resolve_caller(&rp, &req);
    if !caller.is_owner {
        return Err(actix_web::error::ErrorForbidden(
            serde_json::to_string(&ApiError::new(
                "forbidden",
                &format!("User '{}' is not a system owner", caller.username),
                403,
            ))
            .unwrap_or_default(),
        ));
    }

    let config = payload.into_inner();
    validate_access_config(&config)?;

    match AdpStore::write_access_config(&rp, &config) {
        Ok(()) => Ok(HttpResponse::Ok().json(config)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adp::access::AccessConfig;
    use crate::adp::schema::RefSchema;

    fn make_valid_schema() -> RefSchema {
        RefSchema {
            id: "test-schema".to_string(),
            namespace_pattern: "refs/test/*".to_string(),
            title: "Test".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                }
            }),
            ui: None,
        }
    }

    #[test]
    fn test_validate_schema_valid() {
        let schema = make_valid_schema();
        assert!(validate_schema(&schema).is_ok());
    }

    #[test]
    fn test_validate_schema_empty_id() {
        let mut schema = make_valid_schema();
        schema.id = "".to_string();
        let err = validate_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("ID must not be empty"));
    }

    #[test]
    fn test_validate_schema_id_with_slash() {
        let mut schema = make_valid_schema();
        schema.id = "my/schema".to_string();
        let err = validate_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("must not contain '/'"));
    }

    #[test]
    fn test_validate_schema_id_with_space() {
        let mut schema = make_valid_schema();
        schema.id = "my schema".to_string();
        let err = validate_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("must not contain '/' or spaces"));
    }

    #[test]
    fn test_validate_schema_empty_namespace() {
        let mut schema = make_valid_schema();
        schema.namespace_pattern = "".to_string();
        let err = validate_schema(&schema).unwrap_err();
        assert!(err
            .to_string()
            .contains("namespace_pattern must not be empty"));
    }

    #[test]
    fn test_validate_schema_empty_version() {
        let mut schema = make_valid_schema();
        schema.version = "".to_string();
        let err = validate_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("version must not be empty"));
    }

    #[test]
    fn test_validate_schema_reserved_namespace() {
        let mut schema = make_valid_schema();
        schema.namespace_pattern = "refs/adp/internal/*".to_string();
        let err = validate_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("reserved for platform use"));
    }

    #[test]
    fn test_validate_schema_non_object_schema() {
        let mut schema = make_valid_schema();
        schema.schema = serde_json::json!("not an object");
        let err = validate_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("must be a JSON object"));
    }

    #[test]
    fn test_validate_access_config_valid() {
        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec![],
            rules: vec![],
        };
        assert!(validate_access_config(&config).is_ok());
    }

    #[test]
    fn test_validate_access_config_empty_version() {
        let config = AccessConfig {
            version: "".to_string(),
            owners: vec![],
            admins: vec![],
            rules: vec![],
        };
        let err = validate_access_config(&config).unwrap_err();
        assert!(err.to_string().contains("version must not be empty"));
    }

    #[test]
    fn test_validate_access_config_empty_owner() {
        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string(), "".to_string()],
            admins: vec![],
            rules: vec![],
        };
        let err = validate_access_config(&config).unwrap_err();
        assert!(err
            .to_string()
            .contains("Owner usernames must not be empty"));
    }

    #[test]
    fn test_validate_access_config_empty_admin() {
        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec![],
            admins: vec!["".to_string()],
            rules: vec![],
        };
        let err = validate_access_config(&config).unwrap_err();
        assert!(err
            .to_string()
            .contains("Admin usernames must not be empty"));
    }

    #[test]
    fn test_patterns_overlap_same_prefix() {
        assert!(patterns_overlap("refs/test/*", "refs/test/deep/*"));
        assert!(patterns_overlap("refs/webhooks/*", "refs/webhooks/*"));
    }

    #[test]
    fn test_patterns_no_overlap_different_prefix() {
        assert!(!patterns_overlap("refs/test/*", "refs/webhooks/*"));
        assert!(!patterns_overlap("refs/engram/*", "refs/ci/*"));
    }

    #[test]
    fn test_patterns_overlap_one_is_prefix_of_other() {
        assert!(patterns_overlap("refs/*", "refs/test/*"));
    }
}

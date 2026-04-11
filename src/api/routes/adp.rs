use crate::adp::glob::resolve_schema;
use crate::adp::storage::{AdpStore, AdpStoreError};
use crate::adp::{AccessConfig, RefSchema, ADP_REF_PREFIX, SCHEMA_PREFIX};
use crate::api::models::ApiError;
use crate::git::GitService;
use actix_web::{web, HttpResponse, Result as ActixResult};
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
                    .route("/{id}", web::get().to(get_schema))
                    .route("/{id}", web::put().to(update_schema))
                    .route("/{id}", web::delete().to(delete_schema))
                    .route("/resolve", web::get().to(resolve_schema_handler)),
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
                    .route("/resolve", web::get().to(resolve_system_schema_handler)),
            )
            .route("/access", web::get().to(get_system_access_config))
            .route("/access", web::put().to(update_system_access_config)),
    );
}

#[derive(Debug, Clone)]
struct RepoPath(PathBuf);

impl RepoPath {
    fn new(owner: &str, repo: &str, root: &std::path::Path) -> Self {
        Self(root.join(owner).join(format!("{}.git", repo)))
    }

    fn as_path(&self) -> &std::path::Path {
        &self.0
    }
}

fn storage_root(git_service: &GitService) -> PathBuf {
    git_service.get_storage_root()
}

fn adp_error(e: AdpStoreError) -> ApiError {
    match &e {
        AdpStoreError::NotFound(msg) => ApiError::not_found(msg),
        AdpStoreError::Json(msg) => ApiError::bad_request(&msg.to_string()),
        _ => ApiError::internal_error(&e.to_string()),
    }
}

fn system_repo_path(git_service: &GitService) -> PathBuf {
    git_service.get_storage_root().join("_adp/system")
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

// ---------------------------------------------------------------------------
// Repo-level schema endpoints
// ---------------------------------------------------------------------------

pub async fn list_schemas(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service));

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
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service));
    let schema = payload.into_inner();

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
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service));

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
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service));
    let mut schema = payload.into_inner();

    if schema.id != id {
        return Ok(HttpResponse::BadRequest().json(ApiError::bad_request(
            "Schema ID in URL does not match body",
        )));
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
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service));
    let refname = format!("{}{}", SCHEMA_PREFIX, id);

    if AdpStore::read_schema(rp.as_path(), &id)
        .ok()
        .flatten()
        .is_none()
    {
        return Ok(HttpResponse::NotFound()
            .json(ApiError::not_found(&format!("Schema '{}' not found", id))));
    }

    let repo = match git2::Repository::open(rp.as_path()) {
        Ok(r) => r,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(ApiError::not_found("Repository not found")))
        }
    };

    match repo.find_reference(&refname).and_then(|mut r| r.delete()) {
        Ok(()) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(AdpStoreError::Git(e)))),
    }
}

pub async fn resolve_schema_handler(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
    query: web::Query<ResolveQuery>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service));

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
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service));

    let prefix = query.prefix.as_deref().unwrap_or(ADP_REF_PREFIX);

    let refs = match AdpStore::list_refs(rp.as_path(), prefix) {
        Ok(r) => r,
        Err(e) => return Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    };

    let schemas = match AdpStore::list_schemas(rp.as_path()) {
        Ok(s) => s,
        Err(_) => Vec::new(),
    };

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
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service));

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
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service));

    let repo = match git2::Repository::open(rp.as_path()) {
        Ok(r) => r,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(ApiError::not_found("Repository not found")))
        }
    };

    let oid = match git2::Oid::from_str(&sha) {
        Ok(o) => o,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(ApiError::bad_request("Invalid SHA hash")))
        }
    };

    let blob = match repo.find_blob(oid) {
        Ok(b) => b,
        Err(_) => {
            return Ok(HttpResponse::NotFound()
                .json(ApiError::not_found(&format!("Blob '{}' not found", sha))))
        }
    };

    let content = blob.content().to_vec();
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
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();
    let rp = RepoPath::new(&owner, &repo, &storage_root(&git_service));

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
) -> ActixResult<HttpResponse> {
    let (_owner, _repo) = path.into_inner();
    let rp = RepoPath::new(&_owner, &_repo, &storage_root(&git_service));
    let config = payload.into_inner();

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
    let rp = system_repo_path(&git_service);

    match AdpStore::list_schemas(&rp) {
        Ok(schemas) => Ok(HttpResponse::Ok().json(schemas)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

pub async fn create_system_schema(
    git_service: web::Data<Arc<GitService>>,
    payload: web::Json<RefSchema>,
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service);
    let schema = payload.into_inner();

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
    let rp = system_repo_path(&git_service);

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
    let rp = system_repo_path(&git_service);
    let mut schema = payload.into_inner();

    if schema.id != id {
        return Ok(HttpResponse::BadRequest().json(ApiError::bad_request(
            "Schema ID in URL does not match body",
        )));
    }

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
    let rp = system_repo_path(&git_service);
    let refname = format!("{}{}", SCHEMA_PREFIX, id);

    if AdpStore::read_schema(&rp, &id).ok().flatten().is_none() {
        return Ok(HttpResponse::NotFound()
            .json(ApiError::not_found(&format!("Schema '{}' not found", id))));
    }

    let repo = match git2::Repository::open(&rp) {
        Ok(r) => r,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(ApiError::not_found("System repo not found")))
        }
    };

    match repo.find_reference(&refname).and_then(|mut r| r.delete()) {
        Ok(()) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(AdpStoreError::Git(e)))),
    }
}

pub async fn resolve_system_schema_handler(
    git_service: web::Data<Arc<GitService>>,
    query: web::Query<ResolveQuery>,
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service);

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
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service);

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
) -> ActixResult<HttpResponse> {
    let rp = system_repo_path(&git_service);
    let config = payload.into_inner();

    match AdpStore::write_access_config(&rp, &config) {
        Ok(()) => Ok(HttpResponse::Ok().json(config)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(adp_error(e))),
    }
}

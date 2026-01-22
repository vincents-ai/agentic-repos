use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::LfsConfig;
use crate::features::FeatureFlags;
use crate::lfs::{
    CreateLockRequest, DeleteLockRequest, LfsLockManager, LockManagerError, VerifyLocksRequest,
};
use sqlx::PgPool;

pub struct LfsLocksHandler {
    lock_manager: Arc<LfsLockManager>,
    lfs_config: LfsConfig,
    features: FeatureFlags,
}

impl LfsLocksHandler {
    pub fn new(pool: Arc<PgPool>, lfs_config: LfsConfig, features: FeatureFlags) -> Self {
        Self {
            lock_manager: Arc::new(LfsLockManager::new(pool)),
            lfs_config,
            features,
        }
    }
}

fn extract_username_from_request(
    req: &HttpRequest,
    jwt_secret: &str,
) -> Result<String, HttpResponse> {
    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

                let validation = Validation::new(Algorithm::HS256);

                if let Ok(token_data) = decode::<crate::lfs::LfsClaims>(
                    token,
                    &DecodingKey::from_secret(jwt_secret.as_ref()),
                    &validation,
                ) {
                    return Ok(token_data.claims.tenant.clone());
                }
            }
        }
    }

    Err(HttpResponse::Unauthorized().json(json!({
        "message": "Missing or invalid authentication token"
    })))
}

pub async fn create_lock(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<CreateLockRequest>,
    handler: web::Data<LfsLocksHandler>,
) -> impl Responder {
    let (tenant, repo) = path.into_inner();

    if !handler.lfs_config.enabled || !handler.features.git_lfs {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    if !handler.features.lfs_locks {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS file locking is not enabled on this server"
        }));
    }

    let username = match extract_username_from_request(&req, &handler.lfs_config.jwt_secret) {
        Ok(name) => name,
        Err(response) => return response,
    };

    match handler
        .lock_manager
        .create_lock(&tenant, &repo, body.into_inner(), &username, None)
        .await
    {
        Ok(response) => HttpResponse::Created().json(response),
        Err(LockManagerError::AlreadyLocked(path)) => HttpResponse::Conflict().json(json!({
            "message": format!("File {} is already locked", path)
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "message": e.to_string()
        })),
    }
}

pub async fn list_locks(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    query: web::Query<ListLocksQuery>,
    handler: web::Data<LfsLocksHandler>,
) -> impl Responder {
    let (tenant, repo) = path.into_inner();

    if !handler.lfs_config.enabled || !handler.features.git_lfs {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    if !handler.features.lfs_locks {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS file locking is not enabled on this server"
        }));
    }

    let _username = match extract_username_from_request(&req, &handler.lfs_config.jwt_secret) {
        Ok(name) => name,
        Err(response) => return response,
    };

    let id_filter = query.id.as_ref().and_then(|s| Uuid::parse_str(s).ok());

    match handler
        .lock_manager
        .list_locks(
            &tenant,
            &repo,
            query.path.as_deref(),
            id_filter,
            query.refspec.as_deref(),
            query.limit,
            query.cursor.as_deref(),
        )
        .await
    {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "message": e.to_string()
        })),
    }
}

pub async fn delete_lock(
    req: HttpRequest,
    path: web::Path<(String, String, String)>,
    body: web::Json<DeleteLockRequest>,
    handler: web::Data<LfsLocksHandler>,
) -> impl Responder {
    let (tenant, repo, lock_id_str) = path.into_inner();

    if !handler.lfs_config.enabled || !handler.features.git_lfs {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    if !handler.features.lfs_locks {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS file locking is not enabled on this server"
        }));
    }

    let username = match extract_username_from_request(&req, &handler.lfs_config.jwt_secret) {
        Ok(name) => name,
        Err(response) => return response,
    };

    let lock_id = match Uuid::parse_str(&lock_id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(json!({
                "message": "Invalid lock ID format"
            }))
        }
    };

    match handler
        .lock_manager
        .delete_lock(&tenant, &repo, lock_id, &username, body.force)
        .await
    {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(LockManagerError::NotFound(id)) => HttpResponse::NotFound().json(json!({
            "message": format!("Lock {} not found", id)
        })),
        Err(LockManagerError::Forbidden(reason)) => HttpResponse::Forbidden().json(json!({
            "message": reason
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "message": e.to_string()
        })),
    }
}

pub async fn verify_locks(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<VerifyLocksRequest>,
    handler: web::Data<LfsLocksHandler>,
) -> impl Responder {
    let (tenant, repo) = path.into_inner();

    if !handler.lfs_config.enabled || !handler.features.git_lfs {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    if !handler.features.lfs_locks {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS file locking is not enabled on this server"
        }));
    }

    let username = match extract_username_from_request(&req, &handler.lfs_config.jwt_secret) {
        Ok(name) => name,
        Err(response) => return response,
    };

    let refspec = body.git_ref.as_ref().map(|r| r.name.as_str());

    match handler
        .lock_manager
        .verify_locks(
            &tenant,
            &repo,
            &username,
            refspec,
            body.limit,
            body.cursor.as_deref(),
        )
        .await
    {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "message": e.to_string()
        })),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ListLocksQuery {
    pub path: Option<String>,
    pub id: Option<String>,
    pub refspec: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

pub fn configure_locks_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/{tenant}/{repo}/info/lfs")
            .route("/locks", web::post().to(create_lock))
            .route("/locks", web::get().to(list_locks))
            .route("/locks/{id}/unlock", web::post().to(delete_lock))
            .route("/locks/verify", web::post().to(verify_locks)),
    );
}

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::LfsConfig;
use crate::features::FeatureFlags;
use crate::lfs::{
    CreateLockRequest, DeleteLockRequest, LfsLockManager, LockManagerError,
    LockStorage, VerifyLocksRequest,
};
use crate::rbac::{Permission, RbacService};

pub struct LfsLocksHandler {
    lock_manager: Arc<LfsLockManager>,
    lfs_config: LfsConfig,
    features: FeatureFlags,
    rbac_service: Option<Arc<RbacService>>,
}

impl LfsLocksHandler {
    pub fn new(lfs_config: LfsConfig, features: FeatureFlags) -> Self {
        Self {
            lock_manager: Arc::new(LfsLockManager::new()),
            lfs_config,
            features,
            rbac_service: None,
        }
    }

    pub fn with_rbac_service(mut self, rbac_service: Arc<RbacService>) -> Self {
        self.rbac_service = Some(rbac_service);
        self
    }

    async fn check_permission(
        &self,
        username: &str,
        permission: Permission,
    ) -> Result<(), HttpResponse> {
        if let Some(rbac) = &self.rbac_service {
            match rbac.user_has_permission(username, &permission).await {
                Ok(true) => Ok(()),
                Ok(false) => Err(HttpResponse::Forbidden().json(json!({
                    "message": format!("Permission denied: {:?} required", permission)
                }))),
                Err(e) => Err(HttpResponse::InternalServerError().json(json!({
                    "message": format!("RBAC error: {}", e)
                }))),
            }
        } else {
            Ok(())
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

    if let Err(response) = handler
        .check_permission(&username, Permission::LfsLockCreate)
        .await
    {
        return response;
    }

    let expiration_secs = if handler.lfs_config.locking_enabled {
        Some(handler.lfs_config.lock_expiration_secs)
    } else {
        None
    };

    match handler
        .lock_manager
        .create_lock(
            &tenant,
            &repo,
            body.into_inner(),
            &username,
            None,
            expiration_secs,
        )
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

    let username = match extract_username_from_request(&req, &handler.lfs_config.jwt_secret) {
        Ok(name) => name,
        Err(response) => return response,
    };

    if let Err(response) = handler
        .check_permission(&username, Permission::LfsLockRead)
        .await
    {
        return response;
    }

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

    if body.force {
        if let Err(response) = handler
            .check_permission(&username, Permission::LfsLockForce)
            .await
        {
            return response;
        }
    } else if let Err(response) = handler
        .check_permission(&username, Permission::LfsLockDelete)
        .await
    {
        return response;
    }

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

    if let Err(response) = handler
        .check_permission(&username, Permission::LfsLockRead)
        .await
    {
        return response;
    }

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

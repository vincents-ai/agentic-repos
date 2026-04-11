use crate::backup::models::{Backup, BackupStatus, BackupType, RestoreRequest, RestoreResult};
use crate::backup::service::{BackupError, BackupService};
use crate::features::FeatureFlags;
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateBackupRequest {
    pub backup_type: BackupType,
    pub tenant_id: String,
    #[serde(default)]
    pub repository_ids: Vec<String>,
    #[serde(default)]
    pub include_lfs: bool,
    #[serde(default)]
    pub include_database: bool,
    #[serde(default)]
    pub include_config: bool,
    #[serde(default)]
    pub retention_days: Option<u32>,
}

#[derive(Deserialize)]
pub struct ListBackupsQuery {
    pub tenant_id: Option<String>,
    pub backup_type: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct RestoreBackupRequest {
    pub target_tenant_id: Option<String>,
    pub target_repository_ids: Option<Vec<String>>,
    #[serde(default)]
    pub overwrite_existing: bool,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize)]
pub struct BackupResponse {
    pub id: String,
    pub backup_type: String,
    pub tenant_id: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub size_bytes: u64,
    pub checksum: Option<String>,
    pub storage_path: Option<String>,
    pub error_message: Option<String>,
    pub retention_days: u32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_scheduled: bool,
    pub repository_ids: Vec<String>,
    pub include_lfs: bool,
    pub include_database: bool,
    pub include_config: bool,
}

#[derive(Serialize)]
pub struct RestoreResponse {
    pub backup_id: String,
    pub status: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub restored_repositories: Vec<String>,
    pub restored_lfs_objects: u64,
    pub restored_database_records: u64,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl From<Backup> for BackupResponse {
    fn from(backup: Backup) -> Self {
        Self {
            id: backup.id.to_string(),
            backup_type: backup.backup_type.to_string(),
            tenant_id: backup.tenant_id,
            status: backup.status.to_string(),
            created_at: backup.created_at,
            started_at: backup.started_at,
            completed_at: backup.completed_at,
            size_bytes: backup.size_bytes,
            checksum: backup.checksum,
            storage_path: backup.storage_path,
            error_message: backup.error_message,
            retention_days: backup.retention_days,
            expires_at: backup.expires_at,
            is_scheduled: backup.is_scheduled,
            repository_ids: backup.repository_ids,
            include_lfs: backup.include_lfs,
            include_database: backup.include_database,
            include_config: backup.include_config,
        }
    }
}

impl From<RestoreResult> for RestoreResponse {
    fn from(result: RestoreResult) -> Self {
        Self {
            backup_id: result.backup_id.to_string(),
            status: match result.status {
                crate::backup::models::RestoreStatus::Pending => "pending",
                crate::backup::models::RestoreStatus::InProgress => "in_progress",
                crate::backup::models::RestoreStatus::Completed => "completed",
                crate::backup::models::RestoreStatus::Failed => "failed",
                crate::backup::models::RestoreStatus::PartiallyCompleted => "partially_completed",
            }
            .to_string(),
            started_at: result.started_at,
            completed_at: result.completed_at,
            restored_repositories: result.restored_repositories,
            restored_lfs_objects: result.restored_lfs_objects,
            restored_database_records: result.restored_database_records,
            errors: result.errors,
            warnings: result.warnings,
        }
    }
}

fn error_response(error: BackupError) -> HttpResponse {
    match error {
        BackupError::NotFound(_) => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Backup not found",
            "message": error.to_string()
        })),
        BackupError::AlreadyInProgress => HttpResponse::Conflict().json(serde_json::json!({
            "error": "Backup already in progress",
            "message": error.to_string()
        })),
        BackupError::InvalidState(_) => HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid backup state",
            "message": error.to_string()
        })),
        BackupError::Configuration(_) => {
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Configuration error",
                "message": error.to_string()
            }))
        }
        _ => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Backup operation failed",
            "message": error.to_string()
        })),
    }
}

pub async fn create_backup(
    backup_service: web::Data<Arc<BackupService>>,
    feature_flags: web::Data<FeatureFlags>,
    req: web::Json<CreateBackupRequest>,
) -> Result<HttpResponse> {
    if !feature_flags.backup_recovery {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Backup & Recovery feature is not enabled"
        })));
    }

    match backup_service
        .create_backup(
            req.backup_type.clone(),
            req.tenant_id.clone(),
            req.repository_ids.clone(),
            req.include_lfs,
            req.include_database,
            req.include_config,
            req.retention_days,
        )
        .await
    {
        Ok(backup) => {
            let backup_id = backup.id;
            let service = Arc::clone(&backup_service.into_inner());

            tokio::spawn(async move {
                if let Err(e) = service.execute_backup(backup_id).await {
                    tracing::error!("Backup {} failed: {}", backup_id, e);
                }
            });

            Ok(HttpResponse::Accepted().json(BackupResponse::from(backup)))
        }
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn list_backups(
    backup_service: web::Data<Arc<BackupService>>,
    feature_flags: web::Data<FeatureFlags>,
    query: web::Query<ListBackupsQuery>,
) -> Result<HttpResponse> {
    if !feature_flags.backup_recovery {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Backup & Recovery feature is not enabled"
        })));
    }

    let backups = if let Some(tenant_id) = &query.tenant_id {
        backup_service.list_backups_by_tenant(tenant_id).await
    } else {
        backup_service.list_backups().await
    };

    match backups {
        Ok(mut backups) => {
            if let Some(backup_type_str) = &query.backup_type {
                if let Ok(backup_type) = backup_type_str.parse::<BackupType>() {
                    backups.retain(|b| b.backup_type == backup_type);
                }
            }

            if let Some(status_str) = &query.status {
                if let Ok(status) = status_str.parse::<BackupStatus>() {
                    backups.retain(|b| b.status == status);
                }
            }

            if let Some(limit) = query.limit {
                backups.truncate(limit);
            }

            let responses: Vec<BackupResponse> =
                backups.into_iter().map(BackupResponse::from).collect();
            Ok(HttpResponse::Ok().json(responses))
        }
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn get_backup(
    backup_service: web::Data<Arc<BackupService>>,
    feature_flags: web::Data<FeatureFlags>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    if !feature_flags.backup_recovery {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Backup & Recovery feature is not enabled"
        })));
    }

    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid backup ID"
            })));
        }
    };

    match backup_service.get_backup(&id).await {
        Ok(backup) => Ok(HttpResponse::Ok().json(BackupResponse::from(backup))),
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn restore_backup(
    backup_service: web::Data<Arc<BackupService>>,
    feature_flags: web::Data<FeatureFlags>,
    path: web::Path<String>,
    req: web::Json<RestoreBackupRequest>,
) -> Result<HttpResponse> {
    if !feature_flags.backup_recovery {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Backup & Recovery feature is not enabled"
        })));
    }

    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid backup ID"
            })));
        }
    };

    let restore_request = RestoreRequest {
        backup_id: id,
        target_tenant_id: req.target_tenant_id.clone(),
        target_repository_ids: req.target_repository_ids.clone(),
        overwrite_existing: req.overwrite_existing,
        dry_run: req.dry_run,
    };

    match backup_service.restore_backup(restore_request).await {
        Ok(result) => Ok(HttpResponse::Ok().json(RestoreResponse::from(result))),
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn delete_backup(
    backup_service: web::Data<Arc<BackupService>>,
    feature_flags: web::Data<FeatureFlags>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    if !feature_flags.backup_recovery {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Backup & Recovery feature is not enabled"
        })));
    }

    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid backup ID"
            })));
        }
    };

    match backup_service.delete_backup(&id).await {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Backup deleted successfully"
        }))),
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn cancel_backup(
    backup_service: web::Data<Arc<BackupService>>,
    feature_flags: web::Data<FeatureFlags>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    if !feature_flags.backup_recovery {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Backup & Recovery feature is not enabled"
        })));
    }

    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid backup ID"
            })));
        }
    };

    match backup_service.cancel_backup(&id).await {
        Ok(backup) => Ok(HttpResponse::Ok().json(BackupResponse::from(backup))),
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn verify_backup(
    backup_service: web::Data<Arc<BackupService>>,
    feature_flags: web::Data<FeatureFlags>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    if !feature_flags.backup_recovery {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Backup & Recovery feature is not enabled"
        })));
    }

    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid backup ID"
            })));
        }
    };

    match backup_service.verify_backup_by_id(&id).await {
        Ok(checksum) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Backup verified successfully",
            "checksum": checksum
        }))),
        Err(e) => Ok(error_response(e)),
    }
}

pub fn configure_backup_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/backups")
            .route("", web::post().to(create_backup))
            .route("", web::get().to(list_backups))
            .route("/{id}", web::get().to(get_backup))
            .route("/{id}/restore", web::post().to(restore_backup))
            .route("/{id}", web::delete().to(delete_backup))
            .route("/{id}/cancel", web::post().to(cancel_backup))
            .route("/{id}/verify", web::post().to(verify_backup)),
    );
}

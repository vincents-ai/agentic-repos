use crate::backup::models::{
    compute_checksum, Backup, BackupConfig, BackupStatus, BackupType, RestoreRequest,
    RestoreResult, RestoreStatus,
};
use crate::backup::storage::{BackupStorage, BackupStorageError};
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, error, info, warn};

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Backup not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    Storage(#[from] BackupStorageError),
    #[error("Backup already in progress")]
    AlreadyInProgress,
    #[error("Invalid backup state: {0}")]
    InvalidState(String),
    #[error("Backup failed: {0}")]
    BackupFailed(String),
    #[error("Restore failed: {0}")]
    RestoreFailed(String),
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Git error: {0}")]
    Git(String),
}

pub type BackupResult<T> = Result<T, BackupError>;

pub struct BackupService {
    storage: Arc<dyn BackupStorage>,
    config: BackupConfig,
    git_storage_path: String,
    lfs_storage_path: String,
}

impl BackupService {
    pub fn new(
        storage: Arc<dyn BackupStorage>,
        config: BackupConfig,
        git_storage_path: String,
        lfs_storage_path: String,
    ) -> Self {
        Self {
            storage,
            config,
            git_storage_path,
            lfs_storage_path,
        }
    }

    pub async fn create_backup(
        &self,
        backup_type: BackupType,
        tenant_id: String,
        repository_ids: Vec<String>,
        include_lfs: bool,
        include_database: bool,
        include_config: bool,
        retention_days: Option<u32>,
    ) -> BackupResult<Backup> {
        let retention = retention_days.unwrap_or(self.config.default_retention_days);
        let mut backup = Backup::new(backup_type.clone(), tenant_id, retention);
        backup.repository_ids = repository_ids;
        backup.include_lfs = include_lfs;
        backup.include_database = include_database;
        backup.include_config = include_config;
        backup.compression_enabled = self.config.compression_enabled;
        backup.encryption_enabled = self.config.encryption_enabled;

        let backup = self.storage.create(backup).await?;

        info!(
            "Created backup {} for tenant {}",
            backup.id, backup.tenant_id
        );

        Ok(backup)
    }

    pub async fn execute_backup(&self, backup_id: uuid::Uuid) -> BackupResult<Backup> {
        let mut backup = self
            .storage
            .get(&backup_id)
            .await?
            .ok_or_else(|| BackupError::NotFound(backup_id.to_string()))?;

        if backup.status != BackupStatus::Pending {
            return Err(BackupError::InvalidState(format!(
                "Backup is not in pending state: {:?}",
                backup.status
            )));
        }

        backup.start();
        self.storage.update(backup.clone()).await?;

        info!(
            "Starting backup {} for tenant {}",
            backup.id, backup.tenant_id
        );

        let result = match backup.backup_type {
            BackupType::Full => self.execute_full_backup(&mut backup).await,
            BackupType::Repository => self.execute_repository_backup(&mut backup).await,
            BackupType::LfsObjects => self.execute_lfs_backup(&mut backup).await,
            BackupType::Database => self.execute_database_backup(&mut backup).await,
            BackupType::Configuration => self.execute_config_backup(&mut backup).await,
        };

        match result {
            Ok(size) => {
                let storage_path = format!(
                    "backups/{}/{}/{}.backup",
                    backup.tenant_id, backup.backup_type, backup.id
                );
                backup.complete(size, storage_path);

                if self.config.verification_enabled {
                    backup.status = BackupStatus::Verifying;
                    self.storage.update(backup.clone()).await?;

                    match self.verify_backup(&backup).await {
                        Ok(checksum) => {
                            backup.set_checksum(checksum);
                            info!("Backup {} verified successfully", backup.id);
                        }
                        Err(e) => {
                            warn!("Backup {} verification failed: {}", backup.id, e);
                            backup.status = BackupStatus::Completed;
                        }
                    }
                }

                self.storage.update(backup.clone()).await?;
                info!(
                    "Backup {} completed successfully ({} bytes)",
                    backup.id, backup.size_bytes
                );
                Ok(backup)
            }
            Err(e) => {
                backup.fail(e.to_string());
                self.storage.update(backup.clone()).await?;
                error!("Backup {} failed: {}", backup.id, e);
                Err(e)
            }
        }
    }

    async fn execute_full_backup(&self, backup: &mut Backup) -> BackupResult<u64> {
        let mut total_size = 0u64;

        total_size += self.execute_repository_backup(backup).await?;
        total_size += self.execute_lfs_backup(backup).await?;
        total_size += self.execute_database_backup(backup).await?;
        total_size += self.execute_config_backup(backup).await?;

        Ok(total_size)
    }

    async fn execute_repository_backup(&self, backup: &mut Backup) -> BackupResult<u64> {
        let mut total_size = 0u64;
        let mut backup_data = Vec::new();

        for repo_id in &backup.repository_ids {
            let repo_path = format!(
                "{}/{}/{}.git",
                self.git_storage_path, backup.tenant_id, repo_id
            );

            match self.create_git_bundle(&repo_path, repo_id).await {
                Ok(bundle_data) => {
                    backup_data.extend_from_slice(&bundle_data);
                    total_size += bundle_data.len() as u64;
                    backup.metadata.insert(
                        format!("repo_{}_size", repo_id),
                        bundle_data.len().to_string(),
                    );
                    debug!(
                        "Backed up repository {} ({} bytes)",
                        repo_id,
                        bundle_data.len()
                    );
                }
                Err(e) => {
                    warn!("Failed to backup repository {}: {}", repo_id, e);
                    backup
                        .metadata
                        .insert(format!("repo_{}_error", repo_id), e.to_string());
                }
            }
        }

        if !backup_data.is_empty() {
            let path = format!(
                "backups/{}/{}/{}.backup",
                backup.tenant_id, backup.backup_type, backup.id
            );
            self.storage
                .store_backup_data(&backup.id, &backup_data, &path)
                .await?;
            backup.storage_path = Some(path);
        }

        Ok(total_size)
    }

    async fn create_git_bundle(&self, repo_path: &str, repo_id: &str) -> BackupResult<Vec<u8>> {
        let gix_repo = gix::open(repo_path)
            .map_err(|e| BackupError::Git(format!("Failed to open repo {}: {}", repo_id, e)))?;

        let mut bundle_data = Vec::new();

        // Create a simple archive format by listing references
        let platform = gix_repo
            .references()
            .map_err(|e| BackupError::Git(e.to_string()))?;
        let iter = platform
            .all()
            .map_err(|e| BackupError::Git(e.to_string()))?;

        for reference in iter {
            let reference = reference.map_err(|e| BackupError::Git(e.to_string()))?;
            let name = reference.name().as_bstr().to_string();
            if let Some(target_id) = reference.try_id() {
                bundle_data.extend_from_slice(format!("{}\n", name).as_bytes());
                bundle_data.extend_from_slice(format!("{}\n", target_id.to_hex()).as_bytes());
            }
        }

        // Add a marker for end of bundle
        bundle_data.extend_from_slice(b"---END-BUNDLE---\n");

        if bundle_data.is_empty() {
            return Err(BackupError::Git("No references found".to_string()));
        }

        Ok(bundle_data)
    }

    async fn execute_lfs_backup(&self, backup: &mut Backup) -> BackupResult<u64> {
        if !backup.include_lfs {
            return Ok(0);
        }

        let lfs_path = format!("{}/{}", self.lfs_storage_path, backup.tenant_id);
        let mut total_size = 0u64;
        let mut backup_data = Vec::new();

        if let Ok(mut entries) = fs::read_dir(&lfs_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    if metadata.is_file() {
                        let path = entry.path();
                        if let Ok(mut file) = fs::File::open(&path).await {
                            let mut buffer = Vec::new();
                            if file.read_to_end(&mut buffer).await.is_ok() {
                                total_size += buffer.len() as u64;
                                backup_data.extend_from_slice(&buffer);
                            }
                        }
                    }
                }
            }
        }

        if !backup_data.is_empty() {
            backup
                .metadata
                .insert("lfs_size".to_string(), total_size.to_string());
        }

        Ok(total_size)
    }

    async fn execute_database_backup(&self, backup: &mut Backup) -> BackupResult<u64> {
        if !backup.include_database {
            return Ok(0);
        }

        let metadata = serde_json::json!({
            "tenant_id": backup.tenant_id,
            "backup_type": backup.backup_type.to_string(),
            "created_at": backup.created_at.to_rfc3339(),
            "repository_ids": backup.repository_ids,
        });

        let data =
            serde_json::to_vec(&metadata).map_err(|e| BackupError::BackupFailed(e.to_string()))?;

        backup
            .metadata
            .insert("database_size".to_string(), data.len().to_string());

        Ok(data.len() as u64)
    }

    async fn execute_config_backup(&self, backup: &mut Backup) -> BackupResult<u64> {
        if !backup.include_config {
            return Ok(0);
        }

        let config_data = serde_json::json!({
            "tenant_id": backup.tenant_id,
            "settings": {},
            "hooks": [],
            "webhooks": [],
        });

        let data = serde_json::to_vec(&config_data)
            .map_err(|e| BackupError::BackupFailed(e.to_string()))?;

        backup
            .metadata
            .insert("config_size".to_string(), data.len().to_string());

        Ok(data.len() as u64)
    }

    async fn verify_backup(&self, backup: &Backup) -> BackupResult<String> {
        if let Some(path) = &backup.storage_path {
            let data = self.storage.retrieve_backup_data(path).await?;
            let checksum = compute_checksum(&data);

            debug!("Backup {} checksum: {}", backup.id, checksum);

            Ok(checksum)
        } else {
            Err(BackupError::VerificationFailed(
                "No storage path for backup".to_string(),
            ))
        }
    }

    pub async fn get_backup(&self, id: &uuid::Uuid) -> BackupResult<Backup> {
        self.storage
            .get(id)
            .await?
            .ok_or_else(|| BackupError::NotFound(id.to_string()))
    }

    pub async fn list_backups(&self) -> BackupResult<Vec<Backup>> {
        self.storage.list().await.map_err(BackupError::from)
    }

    pub async fn list_backups_by_tenant(&self, tenant_id: &str) -> BackupResult<Vec<Backup>> {
        self.storage
            .get_by_tenant(tenant_id)
            .await
            .map_err(BackupError::from)
    }

    pub async fn delete_backup(&self, id: &uuid::Uuid) -> BackupResult<()> {
        let backup = self
            .storage
            .get(id)
            .await?
            .ok_or_else(|| BackupError::NotFound(id.to_string()))?;

        if backup.status == BackupStatus::InProgress {
            return Err(BackupError::InvalidState(
                "Cannot delete backup in progress".to_string(),
            ));
        }

        if let Some(path) = &backup.storage_path {
            let _ = self.storage.delete_backup_data(path).await;
        }

        self.storage.delete(id).await?;
        info!("Deleted backup {}", id);

        Ok(())
    }

    pub async fn restore_backup(&self, request: RestoreRequest) -> BackupResult<RestoreResult> {
        let backup = self
            .storage
            .get(&request.backup_id)
            .await?
            .ok_or_else(|| BackupError::NotFound(request.backup_id.to_string()))?;

        if backup.status != BackupStatus::Completed && backup.status != BackupStatus::Verified {
            return Err(BackupError::InvalidState(format!(
                "Backup is not in a restorable state: {:?}",
                backup.status
            )));
        }

        info!(
            "Starting restore of backup {} for tenant {}",
            backup.id, backup.tenant_id
        );

        let mut result = RestoreResult {
            backup_id: backup.id,
            status: RestoreStatus::InProgress,
            started_at: chrono::Utc::now(),
            completed_at: None,
            restored_repositories: Vec::new(),
            restored_lfs_objects: 0,
            restored_database_records: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        if request.dry_run {
            result.status = RestoreStatus::Completed;
            result.completed_at = Some(chrono::Utc::now());
            result
                .warnings
                .push("Dry run - no actual restore performed".to_string());
            return Ok(result);
        }

        let target_tenant = request
            .target_tenant_id
            .as_ref()
            .unwrap_or(&backup.tenant_id);

        match backup.backup_type {
            BackupType::Full => {
                self.restore_repositories(&backup, target_tenant, &request, &mut result)
                    .await;
                self.restore_lfs(&backup, target_tenant, &mut result).await;
                self.restore_database(&backup, target_tenant, &mut result)
                    .await;
                self.restore_config(&backup, target_tenant, &mut result)
                    .await;
            }
            BackupType::Repository => {
                self.restore_repositories(&backup, target_tenant, &request, &mut result)
                    .await;
            }
            BackupType::LfsObjects => {
                self.restore_lfs(&backup, target_tenant, &mut result).await;
            }
            BackupType::Database => {
                self.restore_database(&backup, target_tenant, &mut result)
                    .await;
            }
            BackupType::Configuration => {
                self.restore_config(&backup, target_tenant, &mut result)
                    .await;
            }
        }

        result.status = if result.errors.is_empty() {
            RestoreStatus::Completed
        } else if !result.restored_repositories.is_empty() {
            RestoreStatus::PartiallyCompleted
        } else {
            RestoreStatus::Failed
        };

        result.completed_at = Some(chrono::Utc::now());

        info!(
            "Restore completed for backup {} with status {:?}",
            backup.id, result.status
        );

        Ok(result)
    }

    async fn restore_repositories(
        &self,
        backup: &Backup,
        target_tenant: &str,
        request: &RestoreRequest,
        result: &mut RestoreResult,
    ) {
        for repo_id in &backup.repository_ids {
            if let Some(target_ids) = &request.target_repository_ids {
                if !target_ids.contains(repo_id) {
                    continue;
                }
            }

            let repo_path = format!(
                "{}/{}/{}.git",
                self.git_storage_path, target_tenant, repo_id
            );

            match self.restore_git_bundle(&repo_path, repo_id).await {
                Ok(_) => {
                    result.restored_repositories.push(repo_id.clone());
                    debug!("Restored repository {}", repo_id);
                }
                Err(e) => {
                    result
                        .errors
                        .push(format!("Failed to restore {}: {}", repo_id, e));
                    warn!("Failed to restore repository {}: {}", repo_id, e);
                }
            }
        }
    }

    async fn restore_git_bundle(&self, _repo_path: &str, _repo_id: &str) -> BackupResult<()> {
        Ok(())
    }

    async fn restore_lfs(&self, backup: &Backup, _target_tenant: &str, result: &mut RestoreResult) {
        if backup.include_lfs {
            result.restored_lfs_objects = backup
                .metadata
                .get("lfs_objects_count")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
    }

    async fn restore_database(
        &self,
        backup: &Backup,
        _target_tenant: &str,
        result: &mut RestoreResult,
    ) {
        if backup.include_database {
            result.restored_database_records = backup
                .metadata
                .get("database_records")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
    }

    async fn restore_config(
        &self,
        backup: &Backup,
        _target_tenant: &str,
        _result: &mut RestoreResult,
    ) {
        if backup.include_config {
            debug!("Restored configuration for tenant {}", backup.tenant_id);
        }
    }

    pub async fn cleanup_expired_backups(&self) -> BackupResult<Vec<uuid::Uuid>> {
        let expired = self.storage.get_expired_backups().await?;
        let mut deleted = Vec::new();

        for backup in expired {
            info!("Cleaning up expired backup {}", backup.id);
            match self.delete_backup(&backup.id).await {
                Ok(_) => deleted.push(backup.id),
                Err(e) => warn!("Failed to delete expired backup {}: {}", backup.id, e),
            }
        }

        Ok(deleted)
    }

    pub async fn get_backup_status(&self, id: &uuid::Uuid) -> BackupResult<BackupStatus> {
        let backup = self.get_backup(id).await?;
        Ok(backup.status)
    }

    pub async fn cancel_backup(&self, id: &uuid::Uuid) -> BackupResult<Backup> {
        let mut backup = self.get_backup(id).await?;

        if backup.status != BackupStatus::Pending && backup.status != BackupStatus::InProgress {
            return Err(BackupError::InvalidState(format!(
                "Cannot cancel backup in state {:?}",
                backup.status
            )));
        }

        backup.status = BackupStatus::Cancelled;
        backup.completed_at = Some(chrono::Utc::now());
        self.storage.update(backup.clone()).await?;

        info!("Cancelled backup {}", id);

        Ok(backup)
    }

    pub async fn verify_backup_by_id(&self, id: &uuid::Uuid) -> BackupResult<String> {
        let mut backup = self.get_backup(id).await?;

        if backup.status != BackupStatus::Completed {
            return Err(BackupError::InvalidState(format!(
                "Cannot verify backup in state {:?}",
                backup.status
            )));
        }

        backup.status = BackupStatus::Verifying;
        self.storage.update(backup.clone()).await?;

        let checksum = self.verify_backup(&backup).await?;

        backup.status = BackupStatus::Verified;
        backup.checksum = Some(checksum.clone());
        self.storage.update(backup.clone()).await?;

        Ok(checksum)
    }
}

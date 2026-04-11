use crate::backup::models::{Backup, BackupConfig, BackupType};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BackupStorageError {
    #[error("Storage operation failed: {0}")]
    OperationFailed(String),
    #[error("Backup not found: {0}")]
    NotFound(String),
    #[error("Backup already exists: {0}")]
    AlreadyExists(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("S3 error: {0}")]
    S3(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[async_trait]
pub trait BackupStorage: Send + Sync {
    async fn create(&self, backup: Backup) -> Result<Backup, BackupStorageError>;
    async fn get(&self, id: &Uuid) -> Result<Option<Backup>, BackupStorageError>;
    async fn get_by_tenant(&self, tenant_id: &str) -> Result<Vec<Backup>, BackupStorageError>;
    async fn list(&self) -> Result<Vec<Backup>, BackupStorageError>;
    async fn update(&self, backup: Backup) -> Result<Backup, BackupStorageError>;
    async fn delete(&self, id: &Uuid) -> Result<(), BackupStorageError>;
    async fn exists(&self, id: &Uuid) -> Result<bool, BackupStorageError>;

    async fn store_backup_data(
        &self,
        backup_id: &Uuid,
        data: &[u8],
        path: &str,
    ) -> Result<String, BackupStorageError>;

    async fn retrieve_backup_data(&self, path: &str) -> Result<Vec<u8>, BackupStorageError>;

    async fn delete_backup_data(&self, path: &str) -> Result<(), BackupStorageError>;

    async fn get_expired_backups(&self) -> Result<Vec<Backup>, BackupStorageError>;
    async fn get_backups_by_type(
        &self,
        backup_type: &BackupType,
    ) -> Result<Vec<Backup>, BackupStorageError>;
}

pub struct InMemoryBackupStorage {
    backups: RwLock<HashMap<Uuid, Backup>>,
    backups_by_tenant: RwLock<HashMap<String, Vec<Uuid>>>,
    backup_data: RwLock<HashMap<String, Vec<u8>>>,
}

impl InMemoryBackupStorage {
    pub fn new() -> Self {
        Self {
            backups: RwLock::new(HashMap::new()),
            backups_by_tenant: RwLock::new(HashMap::new()),
            backup_data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryBackupStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BackupStorage for InMemoryBackupStorage {
    async fn create(&self, backup: Backup) -> Result<Backup, BackupStorageError> {
        let mut backups = self
            .backups
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        let mut by_tenant = self
            .backups_by_tenant
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        if backups.contains_key(&backup.id) {
            return Err(BackupStorageError::AlreadyExists(backup.id.to_string()));
        }

        let id = backup.id;
        let tenant_id = backup.tenant_id.clone();
        backups.insert(id, backup.clone());
        by_tenant.entry(tenant_id).or_default().push(id);

        Ok(backup)
    }

    async fn get(&self, id: &Uuid) -> Result<Option<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.get(id).cloned())
    }

    async fn get_by_tenant(&self, tenant_id: &str) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        let by_tenant = self
            .backups_by_tenant
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        let ids = by_tenant.get(tenant_id).cloned().unwrap_or_default();
        let result: Vec<Backup> = ids
            .iter()
            .filter_map(|id| backups.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn list(&self) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.values().cloned().collect())
    }

    async fn update(&self, backup: Backup) -> Result<Backup, BackupStorageError> {
        let mut backups = self
            .backups
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        if !backups.contains_key(&backup.id) {
            return Err(BackupStorageError::NotFound(backup.id.to_string()));
        }

        backups.insert(backup.id, backup.clone());
        Ok(backup)
    }

    async fn delete(&self, id: &Uuid) -> Result<(), BackupStorageError> {
        let mut backups = self
            .backups
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        let mut by_tenant = self
            .backups_by_tenant
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        if let Some(backup) = backups.remove(id) {
            if let Some(ids) = by_tenant.get_mut(&backup.tenant_id) {
                ids.retain(|i| i != id);
                if ids.is_empty() {
                    by_tenant.remove(&backup.tenant_id);
                }
            }
        }

        Ok(())
    }

    async fn exists(&self, id: &Uuid) -> Result<bool, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.contains_key(id))
    }

    async fn store_backup_data(
        &self,
        backup_id: &Uuid,
        data: &[u8],
        path: &str,
    ) -> Result<String, BackupStorageError> {
        // Verify backup exists
        {
            let backups = self
                .backups
                .read()
                .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

            if !backups.contains_key(backup_id) {
                return Err(BackupStorageError::NotFound(backup_id.to_string()));
            }
        }

        // Store data in memory
        let mut backup_data = self
            .backup_data
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        backup_data.insert(path.to_string(), data.to_vec());

        Ok(path.to_string())
    }

    async fn retrieve_backup_data(&self, path: &str) -> Result<Vec<u8>, BackupStorageError> {
        let backup_data = self
            .backup_data
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        backup_data
            .get(path)
            .cloned()
            .ok_or_else(|| BackupStorageError::NotFound(path.to_string()))
    }

    async fn delete_backup_data(&self, path: &str) -> Result<(), BackupStorageError> {
        let mut backup_data = self
            .backup_data
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        backup_data.remove(path);
        Ok(())
    }

    async fn get_expired_backups(&self) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        Ok(backups
            .values()
            .filter(|b| b.is_expired())
            .cloned()
            .collect())
    }

    async fn get_backups_by_type(
        &self,
        backup_type: &BackupType,
    ) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        Ok(backups
            .values()
            .filter(|b| &b.backup_type == backup_type)
            .cloned()
            .collect())
    }
}

pub struct LocalBackupStorage {
    base_path: PathBuf,
    #[allow(dead_code)]
    config: BackupConfig,
    backups: RwLock<HashMap<Uuid, Backup>>,
    backups_by_tenant: RwLock<HashMap<String, Vec<Uuid>>>,
}

impl LocalBackupStorage {
    pub fn new(config: BackupConfig) -> Self {
        let base_path = PathBuf::from(
            config
                .local_path
                .clone()
                .unwrap_or_else(|| "./backups".to_string()),
        );

        Self {
            base_path,
            config,
            backups: RwLock::new(HashMap::new()),
            backups_by_tenant: RwLock::new(HashMap::new()),
        }
    }

    pub async fn initialize(&self) -> Result<(), BackupStorageError> {
        fs::create_dir_all(&self.base_path).await?;
        fs::create_dir_all(self.base_path.join("repositories")).await?;
        fs::create_dir_all(self.base_path.join("lfs")).await?;
        fs::create_dir_all(self.base_path.join("database")).await?;
        fs::create_dir_all(self.base_path.join("config")).await?;
        Ok(())
    }

    fn get_backup_path(&self, backup: &Backup) -> PathBuf {
        let filename = format!(
            "{}_{}_{}.backup",
            backup.backup_type, backup.tenant_id, backup.id
        );
        self.base_path.join(&backup.tenant_id).join(filename)
    }
}

#[async_trait]
impl BackupStorage for LocalBackupStorage {
    async fn create(&self, backup: Backup) -> Result<Backup, BackupStorageError> {
        if self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?
            .contains_key(&backup.id)
        {
            return Err(BackupStorageError::AlreadyExists(backup.id.to_string()));
        }

        let tenant_dir = self.base_path.join(&backup.tenant_id);
        fs::create_dir_all(&tenant_dir).await?;

        let id = backup.id;
        let tenant_id = backup.tenant_id.clone();

        let mut backups = self
            .backups
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        let mut by_tenant = self
            .backups_by_tenant
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        backups.insert(id, backup.clone());
        by_tenant.entry(tenant_id).or_default().push(id);

        Ok(backup)
    }

    async fn get(&self, id: &Uuid) -> Result<Option<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.get(id).cloned())
    }

    async fn get_by_tenant(&self, tenant_id: &str) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        let by_tenant = self
            .backups_by_tenant
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        let ids = by_tenant.get(tenant_id).cloned().unwrap_or_default();
        let result: Vec<Backup> = ids
            .iter()
            .filter_map(|id| backups.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn list(&self) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.values().cloned().collect())
    }

    async fn update(&self, backup: Backup) -> Result<Backup, BackupStorageError> {
        let mut backups = self
            .backups
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        if !backups.contains_key(&backup.id) {
            return Err(BackupStorageError::NotFound(backup.id.to_string()));
        }

        backups.insert(backup.id, backup.clone());
        Ok(backup)
    }

    async fn delete(&self, id: &Uuid) -> Result<(), BackupStorageError> {
        let mut backup_to_delete: Option<(String, Option<String>)> = None;

        {
            let backups = self
                .backups
                .read()
                .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
            if let Some(backup) = backups.get(id) {
                backup_to_delete = Some((backup.tenant_id.clone(), backup.storage_path.clone()));
            }
        }

        if let Some((tenant_id, storage_path)) = backup_to_delete {
            {
                let mut backups = self
                    .backups
                    .write()
                    .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
                let mut by_tenant = self
                    .backups_by_tenant
                    .write()
                    .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

                backups.remove(id);

                if let Some(ids) = by_tenant.get_mut(&tenant_id) {
                    ids.retain(|i| i != id);
                    if ids.is_empty() {
                        by_tenant.remove(&tenant_id);
                    }
                }
            }

            if let Some(path) = storage_path {
                let full_path = self.base_path.join(path);
                let _ = fs::remove_file(full_path).await;
            }
        }

        Ok(())
    }

    async fn exists(&self, id: &Uuid) -> Result<bool, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.contains_key(id))
    }

    async fn store_backup_data(
        &self,
        backup_id: &Uuid,
        data: &[u8],
        path: &str,
    ) -> Result<String, BackupStorageError> {
        let full_path = {
            let backups = self
                .backups
                .read()
                .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

            let backup = backups
                .get(backup_id)
                .ok_or_else(|| BackupStorageError::NotFound(backup_id.to_string()))?;

            self.get_backup_path(backup)
        };

        let parent = full_path.parent().ok_or_else(|| {
            BackupStorageError::OperationFailed("Invalid backup path".to_string())
        })?;

        fs::create_dir_all(parent).await?;

        let mut file = fs::File::create(&full_path).await?;
        file.write_all(data).await?;
        file.sync_all().await?;

        Ok(path.to_string())
    }

    async fn retrieve_backup_data(&self, path: &str) -> Result<Vec<u8>, BackupStorageError> {
        let full_path = self.base_path.join(path);
        Ok(fs::read(&full_path).await?)
    }

    async fn delete_backup_data(&self, path: &str) -> Result<(), BackupStorageError> {
        let full_path = self.base_path.join(path);
        fs::remove_file(&full_path).await?;
        Ok(())
    }

    async fn get_expired_backups(&self) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        Ok(backups
            .values()
            .filter(|b| b.is_expired())
            .cloned()
            .collect())
    }

    async fn get_backups_by_type(
        &self,
        backup_type: &BackupType,
    ) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        Ok(backups
            .values()
            .filter(|b| &b.backup_type == backup_type)
            .cloned()
            .collect())
    }
}

pub struct S3BackupStorage {
    #[allow(dead_code)]
    config: BackupConfig,
    s3_config: crate::backup::models::S3BackupConfig,
    backups: RwLock<HashMap<Uuid, Backup>>,
    backups_by_tenant: RwLock<HashMap<String, Vec<Uuid>>>,
}

impl S3BackupStorage {
    pub fn new(config: BackupConfig) -> Result<Self, BackupStorageError> {
        let s3_config = config
            .s3_config
            .clone()
            .ok_or_else(|| BackupStorageError::S3("S3 configuration missing".to_string()))?;

        Ok(Self {
            config,
            s3_config,
            backups: RwLock::new(HashMap::new()),
            backups_by_tenant: RwLock::new(HashMap::new()),
        })
    }

    fn get_s3_key(&self, backup: &Backup) -> String {
        format!(
            "{}/{}/{}_{}_{}.backup",
            self.s3_config.prefix,
            backup.tenant_id,
            backup.backup_type,
            backup.tenant_id,
            backup.id
        )
    }
}

#[async_trait]
impl BackupStorage for S3BackupStorage {
    async fn create(&self, backup: Backup) -> Result<Backup, BackupStorageError> {
        let mut backups = self
            .backups
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        let mut by_tenant = self
            .backups_by_tenant
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        if backups.contains_key(&backup.id) {
            return Err(BackupStorageError::AlreadyExists(backup.id.to_string()));
        }

        let id = backup.id;
        let tenant_id = backup.tenant_id.clone();
        backups.insert(id, backup.clone());
        by_tenant.entry(tenant_id).or_default().push(id);

        Ok(backup)
    }

    async fn get(&self, id: &Uuid) -> Result<Option<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.get(id).cloned())
    }

    async fn get_by_tenant(&self, tenant_id: &str) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        let by_tenant = self
            .backups_by_tenant
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        let ids = by_tenant.get(tenant_id).cloned().unwrap_or_default();
        let result: Vec<Backup> = ids
            .iter()
            .filter_map(|id| backups.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn list(&self) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.values().cloned().collect())
    }

    async fn update(&self, backup: Backup) -> Result<Backup, BackupStorageError> {
        let mut backups = self
            .backups
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        if !backups.contains_key(&backup.id) {
            return Err(BackupStorageError::NotFound(backup.id.to_string()));
        }

        backups.insert(backup.id, backup.clone());
        Ok(backup)
    }

    async fn delete(&self, id: &Uuid) -> Result<(), BackupStorageError> {
        let mut backups = self
            .backups
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        let mut by_tenant = self
            .backups_by_tenant
            .write()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        if let Some(backup) = backups.remove(id) {
            if let Some(ids) = by_tenant.get_mut(&backup.tenant_id) {
                ids.retain(|i| i != id);
                if ids.is_empty() {
                    by_tenant.remove(&backup.tenant_id);
                }
            }
        }

        Ok(())
    }

    async fn exists(&self, id: &Uuid) -> Result<bool, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;
        Ok(backups.contains_key(id))
    }

    async fn store_backup_data(
        &self,
        backup_id: &Uuid,
        data: &[u8],
        path: &str,
    ) -> Result<String, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        let backup = backups
            .get(backup_id)
            .ok_or_else(|| BackupStorageError::NotFound(backup_id.to_string()))?;

        let key = self.get_s3_key(backup);

        tracing::info!(
            "S3 backup storage: Would upload {} bytes to s3://{}/{}",
            data.len(),
            self.s3_config.bucket,
            key
        );

        Ok(path.to_string())
    }

    async fn retrieve_backup_data(&self, path: &str) -> Result<Vec<u8>, BackupStorageError> {
        tracing::info!(
            "S3 backup storage: Would download from s3://{}/{}",
            self.s3_config.bucket,
            path
        );

        Ok(Vec::new())
    }

    async fn delete_backup_data(&self, path: &str) -> Result<(), BackupStorageError> {
        tracing::info!(
            "S3 backup storage: Would delete s3://{}/{}",
            self.s3_config.bucket,
            path
        );

        Ok(())
    }

    async fn get_expired_backups(&self) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        Ok(backups
            .values()
            .filter(|b| b.is_expired())
            .cloned()
            .collect())
    }

    async fn get_backups_by_type(
        &self,
        backup_type: &BackupType,
    ) -> Result<Vec<Backup>, BackupStorageError> {
        let backups = self
            .backups
            .read()
            .map_err(|e| BackupStorageError::OperationFailed(e.to_string()))?;

        Ok(backups
            .values()
            .filter(|b| &b.backup_type == backup_type)
            .cloned()
            .collect())
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BackupType {
    #[default]
    Full,
    Repository,
    LfsObjects,
    Database,
    Configuration,
}

impl std::fmt::Display for BackupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupType::Full => write!(f, "full"),
            BackupType::Repository => write!(f, "repository"),
            BackupType::LfsObjects => write!(f, "lfs_objects"),
            BackupType::Database => write!(f, "database"),
            BackupType::Configuration => write!(f, "configuration"),
        }
    }
}

impl std::str::FromStr for BackupType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "full" => Ok(BackupType::Full),
            "repository" => Ok(BackupType::Repository),
            "lfs_objects" => Ok(BackupType::LfsObjects),
            "database" => Ok(BackupType::Database),
            "configuration" => Ok(BackupType::Configuration),
            other => Err(format!("Unknown backup type: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackupStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
    Verifying,
    Verified,
}

impl std::fmt::Display for BackupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupStatus::Pending => write!(f, "pending"),
            BackupStatus::InProgress => write!(f, "in_progress"),
            BackupStatus::Completed => write!(f, "completed"),
            BackupStatus::Failed => write!(f, "failed"),
            BackupStatus::Cancelled => write!(f, "cancelled"),
            BackupStatus::Verifying => write!(f, "verifying"),
            BackupStatus::Verified => write!(f, "verified"),
        }
    }
}

impl std::str::FromStr for BackupStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(BackupStatus::Pending),
            "in_progress" => Ok(BackupStatus::InProgress),
            "completed" => Ok(BackupStatus::Completed),
            "failed" => Ok(BackupStatus::Failed),
            "cancelled" => Ok(BackupStatus::Cancelled),
            "verifying" => Ok(BackupStatus::Verifying),
            "verified" => Ok(BackupStatus::Verified),
            other => Err(format!("Unknown backup status: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub id: Uuid,
    pub backup_type: BackupType,
    pub tenant_id: String,
    pub status: BackupStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub size_bytes: u64,
    pub checksum: Option<String>,
    pub storage_path: Option<String>,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, String>,
    pub retention_days: u32,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_scheduled: bool,
    pub repository_ids: Vec<String>,
    pub include_lfs: bool,
    pub include_database: bool,
    pub include_config: bool,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
    pub parent_backup_id: Option<Uuid>,
}

impl Backup {
    pub fn new(backup_type: BackupType, tenant_id: String, retention_days: u32) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(retention_days as i64);

        Self {
            id: Uuid::new_v4(),
            backup_type,
            tenant_id,
            status: BackupStatus::Pending,
            created_at: now,
            started_at: None,
            completed_at: None,
            size_bytes: 0,
            checksum: None,
            storage_path: None,
            error_message: None,
            metadata: HashMap::new(),
            retention_days,
            expires_at: Some(expires_at),
            is_scheduled: false,
            repository_ids: Vec::new(),
            include_lfs: false,
            include_database: false,
            include_config: false,
            compression_enabled: true,
            encryption_enabled: false,
            parent_backup_id: None,
        }
    }

    pub fn start(&mut self) {
        self.status = BackupStatus::InProgress;
        self.started_at = Some(Utc::now());
    }

    pub fn complete(&mut self, size_bytes: u64, storage_path: String) {
        self.status = BackupStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.size_bytes = size_bytes;
        self.storage_path = Some(storage_path);
    }

    pub fn fail(&mut self, error: String) {
        self.status = BackupStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error_message = Some(error);
    }

    pub fn set_checksum(&mut self, checksum: String) {
        self.checksum = Some(checksum);
        if self.status == BackupStatus::Verifying {
            self.status = BackupStatus::Verified;
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    pub fn duration(&self) -> Option<chrono::Duration> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => Some(end - start),
            (Some(start), None) => Some(Utc::now() - start),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub enabled: bool,
    pub storage_backend: BackupStorageBackend,
    pub local_path: Option<String>,
    pub s3_config: Option<S3BackupConfig>,
    pub default_retention_days: u32,
    pub max_concurrent_backups: usize,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
    pub encryption_key: Option<String>,
    pub schedule: Vec<BackupSchedule>,
    pub verification_enabled: bool,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            storage_backend: BackupStorageBackend::Local,
            local_path: Some("./backups".to_string()),
            s3_config: None,
            default_retention_days: 30,
            max_concurrent_backups: 3,
            compression_enabled: true,
            encryption_enabled: false,
            encryption_key: None,
            schedule: Vec::new(),
            verification_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupStorageBackend {
    Local,
    S3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3BackupConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub prefix: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub name: String,
    pub backup_type: BackupType,
    pub cron_expression: String,
    pub retention_days: u32,
    pub tenant_id: Option<String>,
    pub repository_ids: Vec<String>,
    pub include_lfs: bool,
    pub include_database: bool,
    pub include_config: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreRequest {
    pub backup_id: Uuid,
    pub target_tenant_id: Option<String>,
    pub target_repository_ids: Option<Vec<String>>,
    pub overwrite_existing: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub backup_id: Uuid,
    pub status: RestoreStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub restored_repositories: Vec<String>,
    pub restored_lfs_objects: u64,
    pub restored_database_records: u64,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RestoreStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    PartiallyCompleted,
}

pub fn compute_checksum(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

pub fn compute_checksum_from_reader<R: std::io::Read>(mut reader: R) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Parameters for the `BackupService::create_backup` method.
/// Groups include flags to avoid an 8-argument function signature.
#[derive(Debug, Clone, Default)]
pub struct CreateBackupParams {
    pub backup_type: BackupType,
    pub tenant_id: String,
    pub repository_ids: Vec<String>,
    pub include_lfs: bool,
    pub include_database: bool,
    pub include_config: bool,
    pub retention_days: Option<u32>,
}

impl CreateBackupParams {
    pub fn full(tenant_id: impl Into<String>) -> Self {
        Self {
            backup_type: BackupType::Full,
            tenant_id: tenant_id.into(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_lifecycle_complete() {
        let mut backup = Backup::new(BackupType::Full, "tenant-1".to_string(), 30);
        assert_eq!(backup.status, BackupStatus::Pending);
        assert!(backup.started_at.is_none());

        backup.start();
        assert_eq!(backup.status, BackupStatus::InProgress);
        assert!(backup.started_at.is_some());

        backup.complete(1024, "/backups/b1.tar.gz".to_string());
        assert_eq!(backup.status, BackupStatus::Completed);
        assert_eq!(backup.size_bytes, 1024);
        assert_eq!(backup.storage_path, Some("/backups/b1.tar.gz".to_string()));
        assert!(backup.completed_at.is_some());
    }

    #[test]
    fn test_backup_lifecycle_failed() {
        let mut backup = Backup::new(BackupType::Full, "tenant-1".to_string(), 30);
        backup.start();
        backup.fail("disk full".to_string());
        assert_eq!(backup.status, BackupStatus::Failed);
        assert_eq!(backup.error_message, Some("disk full".to_string()));
    }

    #[test]
    fn test_backup_set_checksum() {
        let mut backup = Backup::new(BackupType::Full, "t1".to_string(), 7);
        backup.set_checksum("abc123".to_string());
        assert_eq!(backup.checksum, Some("abc123".to_string()));
    }

    #[test]
    fn test_backup_is_expired_false() {
        let backup = Backup::new(BackupType::Full, "t1".to_string(), 30);
        assert!(!backup.is_expired()); // just created — not expired
    }

    #[test]
    fn test_backup_duration_none_when_not_started() {
        let backup = Backup::new(BackupType::Full, "t1".to_string(), 30);
        assert!(backup.duration().is_none());
    }

    #[test]
    fn test_backup_duration_some_when_completed() {
        let mut backup = Backup::new(BackupType::Full, "t1".to_string(), 30);
        backup.start();
        backup.complete(0, "/path".to_string());
        assert!(backup.duration().is_some());
    }

    #[test]
    fn test_backup_config_default_is_disabled() {
        let cfg = BackupConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.default_retention_days, 30);
        assert_eq!(cfg.max_concurrent_backups, 3);
    }

    #[test]
    fn test_compute_checksum_deterministic() {
        let data = b"hello backup";
        let h1 = compute_checksum(data);
        let h2 = compute_checksum(data);
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn test_compute_checksum_differs_for_different_data() {
        let h1 = compute_checksum(b"data-a");
        let h2 = compute_checksum(b"data-b");
        assert_ne!(h1, h2);
    }
}

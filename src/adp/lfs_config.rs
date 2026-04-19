//! LFS configuration stored at `refs/adp/config/lfs` in the system repo.
//!
//! This is a separate ref from `ServerConfig` because LFS tuning parameters
//! are large and independently mutable.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Git LFS configuration stored at `refs/adp/config/lfs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsConfig {
    /// Enable Git LFS support.
    #[serde(default)]
    pub enabled: bool,

    /// Maximum object size in bytes (default: 5 GB).
    #[serde(default = "default_max_object_size")]
    pub max_object_size: u64,

    /// JWT token expiration in seconds (default: 1 hour).
    #[serde(default = "default_token_expiration")]
    pub token_expiration_secs: u64,

    /// JWT secret for token signing and verification.
    /// **Required in production.** If not set, LFS endpoints will panic at startup.
    #[serde(default)]
    pub jwt_secret: Option<String>,

    /// Base URL for LFS HTTP endpoints (e.g. `"https://lfs.example.com"`).
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Storage path pattern for LFS objects (relative to storage backend root).
    #[serde(default = "default_storage_path_pattern")]
    pub storage_path_pattern: String,

    /// Enable LFS file locking support.
    #[serde(default)]
    pub locking_enabled: bool,

    /// Lock expiration in seconds (default: 24 hours).
    #[serde(default = "default_lock_expiration")]
    pub lock_expiration_secs: u64,

    /// Garbage collection configuration.
    #[serde(default)]
    pub gc: LfsGcConfig,

    /// Storage quota configuration.
    #[serde(default)]
    pub quota: LfsQuotaConfig,

    /// Audit logging configuration.
    #[serde(default)]
    pub audit: LfsAuditConfig,

    /// Bandwidth throttling configuration.
    #[serde(default)]
    pub throttle: LfsThrottleConfig,
}

// ── Sub-configs ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsGcConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Grace period in days before deleting unreferenced objects.
    #[serde(default = "default_gc_grace_days")]
    pub grace_period_days: u64,
    /// GC schedule in cron format.
    #[serde(default = "default_gc_schedule")]
    pub schedule: String,
    /// Maximum objects to delete per GC run (0 = unlimited).
    #[serde(default = "default_gc_max_deletions")]
    pub max_deletions_per_run: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsQuotaConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Default quota per repository in bytes.
    #[serde(default = "default_quota_bytes")]
    pub default_quota_bytes: u64,
    /// Per-tenant quota overrides (tenant_name -> bytes).
    #[serde(default)]
    pub per_tenant_quotas: HashMap<String, u64>,
    /// Per-repository quota overrides (tenant/repo -> bytes).
    #[serde(default)]
    pub per_repo_quotas: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsAuditConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_audit_retention")]
    pub retention_days: u32,
    #[serde(default = "default_true")]
    pub include_ip_addresses: bool,
    #[serde(default = "default_true")]
    pub log_successful_operations: bool,
    #[serde(default = "default_true")]
    pub log_failed_operations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsThrottleConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_throttle_bps")]
    pub default_bytes_per_second: u64,
    #[serde(default)]
    pub per_tenant_limits: HashMap<String, u64>,
    #[serde(default)]
    pub per_repo_limits: HashMap<String, u64>,
    #[serde(default = "default_throttle_burst")]
    pub burst_size: u64,
}

// ── Defaults ──

fn default_max_object_size() -> u64 {
    5 * 1024 * 1024 * 1024 // 5 GB
}
fn default_token_expiration() -> u64 {
    3600
}
fn default_base_url() -> String {
    "http://localhost:8090".to_string()
}
fn default_storage_path_pattern() -> String {
    "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}".to_string()
}
fn default_lock_expiration() -> u64 {
    86400
}
fn default_gc_grace_days() -> u64 {
    7
}
fn default_gc_schedule() -> String {
    "0 2 * * *".to_string()
}
fn default_gc_max_deletions() -> usize {
    1000
}
fn default_quota_bytes() -> u64 {
    10 * 1024 * 1024 * 1024
}
fn default_audit_retention() -> u32 {
    90
}
fn default_true() -> bool {
    true
}
fn default_throttle_bps() -> u64 {
    10 * 1024 * 1024 // 10 MB/s
}
fn default_throttle_burst() -> u64 {
    50 * 1024 * 1024 // 50 MB
}

impl Default for LfsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_object_size: default_max_object_size(),
            token_expiration_secs: default_token_expiration(),
            jwt_secret: None,
            base_url: default_base_url(),
            storage_path_pattern: default_storage_path_pattern(),
            locking_enabled: false,
            lock_expiration_secs: default_lock_expiration(),
            gc: LfsGcConfig::default(),
            quota: LfsQuotaConfig::default(),
            audit: LfsAuditConfig::default(),
            throttle: LfsThrottleConfig::default(),
        }
    }
}

impl Default for LfsGcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            grace_period_days: default_gc_grace_days(),
            schedule: default_gc_schedule(),
            max_deletions_per_run: default_gc_max_deletions(),
        }
    }
}

impl Default for LfsQuotaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_quota_bytes: default_quota_bytes(),
            per_tenant_quotas: HashMap::new(),
            per_repo_quotas: HashMap::new(),
        }
    }
}

impl Default for LfsAuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: default_audit_retention(),
            include_ip_addresses: true,
            log_successful_operations: true,
            log_failed_operations: true,
        }
    }
}

impl Default for LfsThrottleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_bytes_per_second: default_throttle_bps(),
            per_tenant_limits: HashMap::new(),
            per_repo_limits: HashMap::new(),
            burst_size: default_throttle_burst(),
        }
    }
}

/// The ref name for LFS configuration in the system repo.
pub const LFS_CONFIG_REF: &str = "refs/adp/config/lfs";

impl LfsConfig {
    /// Get the JWT secret, panicking at startup if not configured.
    /// This ensures LFS cannot operate with an insecure default secret.
    pub fn require_jwt_secret(&self) -> &str {
        self.jwt_secret.as_deref().unwrap_or_else(|| {
            panic!(
                "FATAL: LFS jwt_secret is not set. \
                 Set it in refs/adp/config/lfs or LFS_JWT_SECRET env var. \
                 LFS endpoints are disabled until this is configured."
            )
        })
    }

    /// Check if LFS is properly configured for use.
    pub fn is_configured(&self) -> bool {
        self.enabled && self.jwt_secret.as_ref().is_some_and(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lfs_config_defaults() {
        let cfg = LfsConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.max_object_size, 5 * 1024 * 1024 * 1024);
        assert_eq!(cfg.token_expiration_secs, 3600);
        assert_eq!(cfg.base_url, "http://localhost:8090");
        assert!(!cfg.locking_enabled);
        assert_eq!(cfg.lock_expiration_secs, 86400);
        // JWT secret is None by default — must be explicitly configured
        assert!(cfg.jwt_secret.is_none());
        assert!(!cfg.is_configured());
    }

    #[test]
    fn test_lfs_gc_defaults() {
        let gc = LfsGcConfig::default();
        assert!(!gc.enabled);
        assert_eq!(gc.grace_period_days, 7);
        assert_eq!(gc.schedule, "0 2 * * *");
    }

    #[test]
    fn test_lfs_quota_defaults() {
        let q = LfsQuotaConfig::default();
        assert!(!q.enabled);
        assert_eq!(q.default_quota_bytes, 10 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_lfs_config_round_trip() {
        let cfg = LfsConfig {
            enabled: true,
            jwt_secret: Some("my-secret".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: LfsConfig = serde_json::from_str(&json).unwrap();
        assert!(back.enabled);
        assert_eq!(back.jwt_secret, Some("my-secret".to_string()));
        assert_eq!(back.max_object_size, 5 * 1024 * 1024 * 1024);
        assert!(back.is_configured());
    }

    #[test]
    fn test_lfs_config_ref_constant() {
        assert_eq!(LFS_CONFIG_REF, "refs/adp/config/lfs");
    }
}

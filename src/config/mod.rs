use crate::features::FeatureFlags;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AppConfig {
    pub server: ServerConfig,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
    #[serde(default)]
    pub lfs: LfsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LfsConfig {
    /// Enable Git LFS support (requires git_lfs feature flag)
    pub enabled: bool,
    /// Maximum object size in bytes (default: 5GB)
    pub max_object_size: u64,
    /// JWT token expiration in seconds (default: 1 hour)
    pub token_expiration_secs: u64,
    /// JWT secret for token signing and verification (MUST be changed in production)
    pub jwt_secret: String,
    /// Base URL for LFS HTTP endpoints (e.g., "https://lfs.example.com")
    pub base_url: String,
    /// Storage path pattern for LFS objects (relative to storage backend root)
    /// Default: {tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}
    pub storage_path_pattern: String,
    /// Enable LFS file locking support (requires lfs_locks feature flag)
    pub locking_enabled: bool,
    /// Lock expiration in seconds (default: 24 hours)
    pub lock_expiration_secs: u64,
    /// Garbage collection configuration
    #[serde(default)]
    pub gc: LfsGcConfig,
    /// Storage quota configuration
    #[serde(default)]
    pub quota: LfsQuotaConfig,
    /// Audit logging configuration
    #[serde(default)]
    pub audit: LfsAuditConfig,
    /// Bandwidth throttling configuration
    #[serde(default)]
    pub throttle: LfsThrottleConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LfsQuotaConfig {
    /// Enable quota enforcement (default: false)
    pub enabled: bool,
    /// Default quota per repository in bytes (default: 10GB)
    pub default_quota_bytes: u64,
    /// Per-tenant quota overrides (tenant_name -> bytes)
    #[serde(default)]
    pub per_tenant_quotas: HashMap<String, u64>,
    /// Per-repository quota overrides (tenant/repo -> bytes)
    #[serde(default)]
    pub per_repo_quotas: HashMap<String, u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LfsGcConfig {
    /// Enable automatic garbage collection (default: false)
    pub enabled: bool,
    /// Grace period in days before deleting unreferenced objects (default: 7)
    pub grace_period_days: u64,
    /// GC schedule in cron format (default: "0 2 * * *" = 2 AM daily)
    pub schedule: String,
    /// Maximum objects to delete per GC run (0 = unlimited, default: 1000)
    pub max_deletions_per_run: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LfsAuditConfig {
    pub enabled: bool,
    pub retention_days: u32,
    pub include_ip_addresses: bool,
    pub log_successful_operations: bool,
    pub log_failed_operations: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LfsThrottleConfig {
    pub enabled: bool,
    pub default_bytes_per_second: u64,
    #[serde(default)]
    pub per_tenant_limits: HashMap<String, u64>,
    #[serde(default)]
    pub per_repo_limits: HashMap<String, u64>,
    pub burst_size: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Check for config path from environment variable
        if let Ok(path) = std::env::var("AGENTIC_REPOS_CONFIG") {
            return Self::from_file(&path);
        }

        // Check for --config argument
        let args: Vec<String> = std::env::args().collect();
        for i in 0..args.len() - 1 {
            if args[i] == "--config" {
                return Self::from_file(&args[i + 1]);
            }
        }

        // Try git-refs config from system repo
        if let Some(storage_root) = Self::detect_storage_root() {
            if let Ok(config) = Self::from_git_refs(&storage_root) {
                return Ok(config);
            }
        }

        Self::from_file("config.yaml")
    }

    /// Detect the storage root from environment or CLI flags.
    fn detect_storage_root() -> Option<std::path::PathBuf> {
        // 1. AGENTIC_REPOS_STORAGE_ROOT env var
        if let Ok(path) = std::env::var("AGENTIC_REPOS_STORAGE_ROOT") {
            return Some(std::path::PathBuf::from(path));
        }

        // 2. --storage CLI flag
        let args: Vec<String> = std::env::args().collect();
        for i in 0..args.len().saturating_sub(1) {
            if args[i] == "--storage" {
                return Some(std::path::PathBuf::from(&args[i + 1]));
            }
        }

        // 3. Default
        None
    }

    /// Read configuration from the ADP system repo git refs.
    ///
    /// Falls back to defaults for any ref that doesn't exist yet.
    pub fn from_git_refs(
        storage_root: &std::path::Path,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use crate::adp::bootstrap::SYSTEM_REPO_PATH;
        use crate::adp::lfs_config::LFS_CONFIG_REF;
        use crate::adp::storage::AdpStore;
        use crate::adp::SERVER_CONFIG_REF;

        let system_repo_path = storage_root.join(SYSTEM_REPO_PATH);

        // Read server config from git ref
        let server_cfg: crate::adp::config::ServerConfig =
            match AdpStore::read_ref_blob(&system_repo_path, SERVER_CONFIG_REF) {
                Ok(value) => serde_json::from_value(value).unwrap_or_default(),
                Err(_) => crate::adp::config::ServerConfig::default(),
            };

        // Read LFS config from git ref
        let lfs_cfg: crate::adp::lfs_config::LfsConfig =
            match AdpStore::read_ref_blob(&system_repo_path, LFS_CONFIG_REF) {
                Ok(value) => serde_json::from_value(value).unwrap_or_default(),
                Err(_) => crate::adp::lfs_config::LfsConfig::default(),
            };

        // Map ADP config → legacy AppConfig
        let config = AppConfig {
            server: ServerConfig {
                host: server_cfg.http_host,
                port: server_cfg.http_port,
            },
            features: if server_cfg.feature_flags.is_empty() {
                None
            } else {
                Some(server_cfg.feature_flags)
            },
            lfs: LfsConfig {
                enabled: lfs_cfg.enabled,
                max_object_size: lfs_cfg.max_object_size,
                token_expiration_secs: lfs_cfg.token_expiration_secs,
                jwt_secret: lfs_cfg.jwt_secret,
                base_url: lfs_cfg.base_url,
                storage_path_pattern: lfs_cfg.storage_path_pattern,
                locking_enabled: lfs_cfg.locking_enabled,
                lock_expiration_secs: lfs_cfg.lock_expiration_secs,
                gc: LfsGcConfig {
                    enabled: lfs_cfg.gc.enabled,
                    grace_period_days: lfs_cfg.gc.grace_period_days,
                    schedule: lfs_cfg.gc.schedule,
                    max_deletions_per_run: lfs_cfg.gc.max_deletions_per_run,
                },
                quota: LfsQuotaConfig {
                    enabled: lfs_cfg.quota.enabled,
                    default_quota_bytes: lfs_cfg.quota.default_quota_bytes,
                    per_tenant_quotas: lfs_cfg.quota.per_tenant_quotas,
                    per_repo_quotas: lfs_cfg.quota.per_repo_quotas,
                },
                audit: LfsAuditConfig {
                    enabled: lfs_cfg.audit.enabled,
                    retention_days: lfs_cfg.audit.retention_days,
                    include_ip_addresses: lfs_cfg.audit.include_ip_addresses,
                    log_successful_operations: lfs_cfg.audit.log_successful_operations,
                    log_failed_operations: lfs_cfg.audit.log_failed_operations,
                },
                throttle: LfsThrottleConfig {
                    enabled: lfs_cfg.throttle.enabled,
                    default_bytes_per_second: lfs_cfg.throttle.default_bytes_per_second,
                    per_tenant_limits: lfs_cfg.throttle.per_tenant_limits,
                    per_repo_limits: lfs_cfg.throttle.per_repo_limits,
                    burst_size: lfs_cfg.throttle.burst_size,
                },
            },
        };

        tracing::info!(
            "Loaded config from git refs: host={}, port={}",
            config.server.host, config.server.port
        );

        Ok(config)
    }

    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_yaml::from_str::<AppConfig>(&content) {
                Ok(app_config) => {
                    tracing::info!("Loading config from {}", path);
                    tracing::info!(
                        "Config loaded: host={}, port={}",
                        app_config.server.host, app_config.server.port
                    );
                    Ok(app_config)
                }
                Err(e) => {
                    tracing::warn!("Config file {} invalid ({}), using defaults", path, e);
                    Ok(Self::default())
                }
            },
            Err(_) => {
                tracing::info!("Config file {} not found, using defaults", path);
                Ok(Self::default())
            }
        }
    }

    pub fn get_feature_flags(&self) -> FeatureFlags {
        match &self.features {
            Some(config_flags) => {
                let env_flags = FeatureFlags::load_from_env();
                let config_feature_flags = FeatureFlags::load_from_config(config_flags);

                FeatureFlags {
                    web_ui: env_flags.web_ui || config_feature_flags.web_ui,
                    ldap_auth: env_flags.ldap_auth || config_feature_flags.ldap_auth,
                    saml_auth: env_flags.saml_auth || config_feature_flags.saml_auth,
                    oauth_auth: env_flags.oauth_auth || config_feature_flags.oauth_auth,
                    audit_logging: env_flags.audit_logging || config_feature_flags.audit_logging,
                    audit_tracking: env_flags.audit_tracking || config_feature_flags.audit_tracking,
                    repo_templates: env_flags.repo_templates || config_feature_flags.repo_templates,
                    webhooks: env_flags.webhooks || config_feature_flags.webhooks,
                    multi_region: env_flags.multi_region || config_feature_flags.multi_region,
                    horizontal_scaling: env_flags.horizontal_scaling
                        || config_feature_flags.horizontal_scaling,
                    redis_cache: env_flags.redis_cache || config_feature_flags.redis_cache,
                    load_balancing: env_flags.load_balancing || config_feature_flags.load_balancing,
                    monitoring: env_flags.monitoring || config_feature_flags.monitoring,
                    security_scanning: env_flags.security_scanning
                        || config_feature_flags.security_scanning,
                    soc2_compliance: env_flags.soc2_compliance
                        || config_feature_flags.soc2_compliance,
                    gdpr_compliance: env_flags.gdpr_compliance
                        || config_feature_flags.gdpr_compliance,
                    hipaa_compliance: env_flags.hipaa_compliance
                        || config_feature_flags.hipaa_compliance,
                    rbac: env_flags.rbac || config_feature_flags.rbac,
                    backup_recovery: env_flags.backup_recovery
                        || config_feature_flags.backup_recovery,
                    github_actions: env_flags.github_actions || config_feature_flags.github_actions,
                    gitlab_ci: env_flags.gitlab_ci || config_feature_flags.gitlab_ci,
                    jenkins: env_flags.jenkins || config_feature_flags.jenkins,
                    graphql_api: env_flags.graphql_api || config_feature_flags.graphql_api,
                    advanced_webhooks: env_flags.advanced_webhooks
                        || config_feature_flags.advanced_webhooks,
                    analytics: env_flags.analytics || config_feature_flags.analytics,
                    git_lfs: env_flags.git_lfs || config_feature_flags.git_lfs,
                    lfs_locks: env_flags.lfs_locks || config_feature_flags.lfs_locks,
                }
            }
            None => FeatureFlags::load_from_env(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8090,
        }
    }
}

impl Default for LfsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_object_size: 5 * 1024 * 1024 * 1024, // 5GB
            token_expiration_secs: 3600,             // 1 hour
            jwt_secret: "CHANGE_THIS_SECRET_IN_PRODUCTION_USE_STRONG_RANDOM_STRING".to_string(),
            base_url: "http://localhost:8090".to_string(),
            storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
                .to_string(),
            locking_enabled: false,
            lock_expiration_secs: 86400, // 24 hours
            gc: LfsGcConfig::default(),
            quota: LfsQuotaConfig::default(),
            audit: LfsAuditConfig::default(),
            throttle: LfsThrottleConfig::default(),
        }
    }
}

impl Default for LfsQuotaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_quota_bytes: 10 * 1024 * 1024 * 1024,
            per_tenant_quotas: HashMap::new(),
            per_repo_quotas: HashMap::new(),
        }
    }
}

impl Default for LfsGcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            grace_period_days: 7,
            schedule: "0 2 * * *".to_string(),
            max_deletions_per_run: 1000,
        }
    }
}

impl Default for LfsAuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: 90,
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
            default_bytes_per_second: 10 * 1024 * 1024,
            per_tenant_limits: HashMap::new(),
            per_repo_limits: HashMap::new(),
            burst_size: 50 * 1024 * 1024,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_server_defaults() {
        let config = AppConfig::default();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8090);
    }

    #[test]
    fn default_lfs_config() {
        let lfs = LfsConfig::default();
        assert!(!lfs.enabled);
        assert_eq!(lfs.max_object_size, 5 * 1024 * 1024 * 1024);
        assert_eq!(lfs.token_expiration_secs, 3600);
        assert_eq!(lfs.base_url, "http://localhost:8090");
        assert!(!lfs.locking_enabled);
        assert_eq!(lfs.lock_expiration_secs, 86400);
    }

    #[test]
    fn default_lfs_gc_config() {
        let gc = LfsGcConfig::default();
        assert!(!gc.enabled);
        assert_eq!(gc.grace_period_days, 7);
        assert_eq!(gc.schedule, "0 2 * * *");
        assert_eq!(gc.max_deletions_per_run, 1000);
    }

    #[test]
    fn default_lfs_quota_config() {
        let quota = LfsQuotaConfig::default();
        assert!(!quota.enabled);
        assert_eq!(quota.default_quota_bytes, 10 * 1024 * 1024 * 1024);
        assert!(quota.per_tenant_quotas.is_empty());
        assert!(quota.per_repo_quotas.is_empty());
    }

    #[test]
    fn default_lfs_audit_config() {
        let audit = LfsAuditConfig::default();
        assert!(!audit.enabled);
        assert_eq!(audit.retention_days, 90);
        assert!(audit.include_ip_addresses);
    }

    #[test]
    fn default_lfs_throttle_config() {
        let throttle = LfsThrottleConfig::default();
        assert!(!throttle.enabled);
        assert_eq!(throttle.default_bytes_per_second, 10 * 1024 * 1024);
        assert_eq!(throttle.burst_size, 50 * 1024 * 1024);
    }

    #[test]
    fn from_missing_file_returns_defaults() {
        let config = AppConfig::from_file("/nonexistent/path/config.yaml").unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8090);
    }

    #[test]
    fn from_valid_yaml_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_config.yaml");
        std::fs::write(&path, "server:\n  host: 0.0.0.0\n  port: 9090\n").unwrap();
        let config = AppConfig::from_file(path.to_str().unwrap()).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9090);
    }

    #[test]
    fn from_yaml_with_features() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("features.yaml");
        std::fs::write(&path, "server:\n  host: 0.0.0.0\n  port: 8080\nfeatures:\n  web_ui: true\n  rbac: true\n").unwrap();
        let config = AppConfig::from_file(path.to_str().unwrap()).unwrap();
        let features = config.get_feature_flags();
        assert!(features.web_ui);
        assert!(features.rbac);
    }

    #[test]
    fn from_yaml_with_lfs_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lfs.yaml");
        std::fs::write(&path, "server:\n  host: 0.0.0.0\n  port: 8080\nlfs:\n  enabled: true\n  max_object_size: 1073741824\n  token_expiration_secs: 7200\n  jwt_secret: test-secret\n  base_url: http://localhost:8080\n  storage_path_pattern: '{oid}'\n  locking_enabled: false\n  lock_expiration_secs: 86400\n").unwrap();
        let config = AppConfig::from_file(path.to_str().unwrap()).unwrap();
        assert!(config.lfs.enabled);
        assert_eq!(config.lfs.max_object_size, 1073741824);
        assert_eq!(config.lfs.token_expiration_secs, 7200);
    }

    #[test]
    fn config_serialization_roundtrip() {
        let original = AppConfig::default();
        let json = serde_json::to_string(&original).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.server.host, original.server.host);
        assert_eq!(back.server.port, original.server.port);
    }
}

use crate::storage::StorageBackend;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum QuotaError {
    #[error("Quota exceeded for {tenant}/{repo}: {current} bytes used, {limit} bytes limit")]
    Exceeded {
        tenant: String,
        repo: String,
        current: u64,
        limit: u64,
    },
    #[error("Storage error: {0}")]
    Storage(String),
}

pub struct QuotaConfig {
    pub default_quota_bytes: u64,
    pub per_tenant_quotas: HashMap<String, u64>,
    pub per_repo_quotas: HashMap<String, u64>,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            default_quota_bytes: 10 * 1024 * 1024 * 1024,
            per_tenant_quotas: HashMap::new(),
            per_repo_quotas: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct QuotaUsage {
    bytes_used: u64,
    object_count: u64,
}

pub struct QuotaManager {
    config: QuotaConfig,
    usage_cache: Arc<RwLock<HashMap<String, QuotaUsage>>>,
}

impl QuotaManager {
    pub fn new(config: QuotaConfig) -> Self {
        Self {
            config,
            usage_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn quota_key(tenant: &str, repo: &str) -> String {
        format!("{}/{}", tenant, repo)
    }

    pub fn get_quota_limit(&self, tenant: &str, repo: &str) -> u64 {
        let repo_key = Self::quota_key(tenant, repo);

        if let Some(&limit) = self.config.per_repo_quotas.get(&repo_key) {
            return limit;
        }

        if let Some(&limit) = self.config.per_tenant_quotas.get(tenant) {
            return limit;
        }

        self.config.default_quota_bytes
    }

    pub async fn get_current_usage(&self, tenant: &str, repo: &str) -> (u64, u64) {
        let key = Self::quota_key(tenant, repo);
        let cache = self.usage_cache.read().await;

        cache
            .get(&key)
            .map(|u| (u.bytes_used, u.object_count))
            .unwrap_or((0, 0))
    }

    pub async fn check_quota(
        &self,
        tenant: &str,
        repo: &str,
        additional_bytes: u64,
    ) -> Result<(), QuotaError> {
        let limit = self.get_quota_limit(tenant, repo);
        let (current, _) = self.get_current_usage(tenant, repo).await;

        let new_total = current + additional_bytes;

        if new_total > limit {
            crate::lfs::metrics::record_quota_exceeded(tenant, repo);
            return Err(QuotaError::Exceeded {
                tenant: tenant.to_string(),
                repo: repo.to_string(),
                current,
                limit,
            });
        }

        Ok(())
    }

    pub async fn update_usage(&self, tenant: &str, repo: &str, bytes_delta: i64) {
        let key = Self::quota_key(tenant, repo);
        let mut cache = self.usage_cache.write().await;

        let usage = cache.entry(key.clone()).or_insert(QuotaUsage {
            bytes_used: 0,
            object_count: 0,
        });

        if bytes_delta > 0 {
            usage.bytes_used += bytes_delta as u64;
            usage.object_count += 1;
        } else {
            usage.bytes_used = usage.bytes_used.saturating_sub((-bytes_delta) as u64);
            if usage.object_count > 0 {
                usage.object_count -= 1;
            }
        }

        crate::lfs::metrics::record_quota_usage(tenant, repo, usage.bytes_used, usage.object_count);
    }

    pub async fn refresh_usage(
        &self,
        tenant: &str,
        repo: &str,
        backend: &dyn StorageBackend,
    ) -> Result<(), QuotaError> {
        let lfs_base = PathBuf::from(format!("{}/{}.git/lfs/objects", tenant, repo));

        let total_bytes = 0u64;
        let object_count = 0u64;

        for prefix1 in 0..256 {
            let prefix1_hex = format!("{:02x}", prefix1);
            let prefix1_path = lfs_base.join(&prefix1_hex);

            if !backend.exists(&prefix1_path).await.unwrap_or(false) {
                continue;
            }

            for prefix2 in 0..256 {
                let prefix2_hex = format!("{:02x}", prefix2);
                let prefix2_path = prefix1_path.join(&prefix2_hex);

                if !backend.exists(&prefix2_path).await.unwrap_or(false) {
                    continue;
                }
            }
        }

        let key = Self::quota_key(tenant, repo);
        let mut cache = self.usage_cache.write().await;

        cache.insert(
            key,
            QuotaUsage {
                bytes_used: total_bytes,
                object_count,
            },
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_config_defaults() {
        let config = QuotaConfig::default();
        assert_eq!(config.default_quota_bytes, 10 * 1024 * 1024 * 1024);
        assert!(config.per_tenant_quotas.is_empty());
        assert!(config.per_repo_quotas.is_empty());
    }

    #[test]
    fn test_quota_key() {
        assert_eq!(QuotaManager::quota_key("tenant1", "repo1"), "tenant1/repo1");
    }

    #[tokio::test]
    async fn test_get_quota_limit_default() {
        let config = QuotaConfig::default();
        let manager = QuotaManager::new(config);

        let limit = manager.get_quota_limit("tenant1", "repo1");
        assert_eq!(limit, 10 * 1024 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_get_quota_limit_per_tenant() {
        let mut config = QuotaConfig::default();
        config
            .per_tenant_quotas
            .insert("tenant1".to_string(), 5 * 1024 * 1024 * 1024);

        let manager = QuotaManager::new(config);

        let limit = manager.get_quota_limit("tenant1", "repo1");
        assert_eq!(limit, 5 * 1024 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_get_quota_limit_per_repo() {
        let mut config = QuotaConfig::default();
        config
            .per_repo_quotas
            .insert("tenant1/repo1".to_string(), 2 * 1024 * 1024 * 1024);

        let manager = QuotaManager::new(config);

        let limit = manager.get_quota_limit("tenant1", "repo1");
        assert_eq!(limit, 2 * 1024 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_check_quota_within_limit() {
        let config = QuotaConfig {
            default_quota_bytes: 1000,
            ..Default::default()
        };
        let manager = QuotaManager::new(config);

        let result = manager.check_quota("tenant1", "repo1", 500).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_check_quota_exceeds_limit() {
        let config = QuotaConfig {
            default_quota_bytes: 1000,
            ..Default::default()
        };
        let manager = QuotaManager::new(config);

        manager.update_usage("tenant1", "repo1", 600).await;

        let result = manager.check_quota("tenant1", "repo1", 500).await;
        assert!(result.is_err());

        if let Err(QuotaError::Exceeded { current, limit, .. }) = result {
            assert_eq!(current, 600);
            assert_eq!(limit, 1000);
        }
    }

    #[tokio::test]
    async fn test_update_usage_increment() {
        let config = QuotaConfig::default();
        let manager = QuotaManager::new(config);

        manager.update_usage("tenant1", "repo1", 100).await;
        manager.update_usage("tenant1", "repo1", 200).await;

        let (bytes, count) = manager.get_current_usage("tenant1", "repo1").await;
        assert_eq!(bytes, 300);
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_update_usage_decrement() {
        let config = QuotaConfig::default();
        let manager = QuotaManager::new(config);

        manager.update_usage("tenant1", "repo1", 500).await;
        manager.update_usage("tenant1", "repo1", -200).await;

        let (bytes, count) = manager.get_current_usage("tenant1", "repo1").await;
        assert_eq!(bytes, 300);
        assert_eq!(count, 0);
    }
}

use crate::cache::{CacheKey, CachePool, CacheResult};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, instrument};

#[derive(Debug, Clone, Serialize)]
pub struct RepositoryMetadata {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub is_private: bool,
    pub size_bytes: u64,
    pub object_count: u64,
    pub last_activity_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_default: bool,
    pub is_protected: bool,
    pub last_commit_sha: Option<String>,
    pub last_commit_message: Option<String>,
    pub last_commit_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepositoryListEntry {
    pub name: String,
    pub tenant_id: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub default_branch: String,
    pub last_activity_at: Option<String>,
}

#[derive(Clone)]
pub struct RepositoryCache {
    pool: Arc<CachePool>,
}

impl RepositoryCache {
    pub fn new(pool: Arc<CachePool>) -> Self {
        Self { pool }
    }

    fn key_prefix(&self) -> &str {
        &self.pool.config().key_prefix
    }

    #[instrument(skip(self, metadata), fields(tenant_id = %tenant_id, repo_name = %name))]
    pub async fn get_metadata(&self, tenant_id: &str, name: &str) -> CacheResult<Option<RepositoryMetadata>> {
        let key = CacheKey::repository(self.key_prefix(), tenant_id, name).with("metadata");
        self.get_json(&key.to_redis_key()).await
    }

    #[instrument(skip(self, metadata), fields(tenant_id = %tenant_id, repo_name = %name))]
    pub async fn set_metadata(&self, tenant_id: &str, name: &str, metadata: &RepositoryMetadata) -> CacheResult<()> {
        let key = CacheKey::repository(self.key_prefix(), tenant_id, name).with("metadata");
        let ttl = self.pool.config().repository_ttl();
        self.set_json(&key.to_redis_key(), metadata, ttl).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, repo_name = %name))]
    pub async fn invalidate_metadata(&self, tenant_id: &str, name: &str) -> CacheResult<bool> {
        let key = CacheKey::repository(self.key_prefix(), tenant_id, name).with("metadata");
        let mut conn = self.pool.get_connection().await?;
        conn.delete(&key.to_redis_key()).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id))]
    pub async fn get_repo_list(&self, tenant_id: &str, page: u32) -> CacheResult<Option<Vec<RepositoryListEntry>>> {
        let key = CacheKey::repo_list(self.key_prefix(), tenant_id, page);
        self.get_json(&key.to_redis_key()).await
    }

    #[instrument(skip(self, repos), fields(tenant_id = %tenant_id, page = %page))]
    pub async fn set_repo_list(&self, tenant_id: &str, page: u32, repos: &[RepositoryListEntry]) -> CacheResult<()> {
        let key = CacheKey::repo_list(self.key_prefix(), tenant_id, page);
        let ttl = self.pool.config().api_response_ttl();
        self.set_json(&key.to_redis_key(), repos, ttl).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id))]
    pub async fn invalidate_repo_lists(&self, tenant_id: &str) -> CacheResult<u64> {
        let pattern = format!("{}:repos:{}:*", self.key_prefix(), tenant_id);
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, repo_name = %name))]
    pub async fn get_branches(&self, tenant_id: &str, name: &str) -> CacheResult<Option<Vec<BranchInfo>>> {
        let key = CacheKey::branch_list(self.key_prefix(), tenant_id, name);
        self.get_json(&key.to_redis_key()).await
    }

    #[instrument(skip(self, branches), fields(tenant_id = %tenant_id, repo_name = %name))]
    pub async fn set_branches(&self, tenant_id: &str, name: &str, branches: &[BranchInfo]) -> CacheResult<()> {
        let key = CacheKey::branch_list(self.key_prefix(), tenant_id, name);
        let ttl = self.pool.config().api_response_ttl();
        self.set_json(&key.to_redis_key(), branches, ttl).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, repo_name = %name))]
    pub async fn invalidate_branches(&self, tenant_id: &str, name: &str) -> CacheResult<bool> {
        let key = CacheKey::branch_list(self.key_prefix(), tenant_id, name);
        let mut conn = self.pool.get_connection().await?;
        conn.delete(&key.to_redis_key()).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, repo_name = %name))]
    pub async fn invalidate_all(&self, tenant_id: &str, name: &str) -> CacheResult<u64> {
        let pattern = CacheKey::repo_pattern(self.key_prefix(), tenant_id, name);
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    async fn get_json<T: DeserializeOwned>(&self, key: &str) -> CacheResult<Option<T>> {
        let mut conn = self.pool.get_connection().await?;
        match conn.get(key).await? {
            Some(json) => {
                let value: T = serde_json::from_str(&json)?;
                debug!(key = key, "Cache hit");
                Ok(Some(value))
            }
            None => {
                debug!(key = key, "Cache miss");
                Ok(None)
            }
        }
    }

    async fn set_json<T: Serialize>(&self, key: &str, value: &T, ttl: Duration) -> CacheResult<()> {
        let json = serde_json::to_string(value)?;
        let mut conn = self.pool.get_connection().await?;
        conn.set(key, &json, ttl).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata() -> RepositoryMetadata {
        RepositoryMetadata {
            id: "repo-123".to_string(),
            tenant_id: "tenant-1".to_string(),
            name: "test-repo".to_string(),
            description: Some("A test repository".to_string()),
            default_branch: "main".to_string(),
            is_private: false,
            size_bytes: 1024,
            object_count: 10,
            last_activity_at: Some("2024-01-01T00:00:00Z".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = test_metadata();
        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: RepositoryMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(metadata.id, deserialized.id);
        assert_eq!(metadata.name, deserialized.name);
    }

    #[test]
    fn test_branch_info_serialization() {
        let branch = BranchInfo {
            name: "main".to_string(),
            is_default: true,
            is_protected: false,
            last_commit_sha: Some("abc123".to_string()),
            last_commit_message: Some("Initial commit".to_string()),
            last_commit_at: Some("2024-01-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&branch).unwrap();
        let deserialized: BranchInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(branch.name, deserialized.name);
        assert!(deserialized.is_default);
    }
}

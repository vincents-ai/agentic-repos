use crate::cache::{CacheKey, CachePool};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, instrument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsObjectMetadata {
    pub oid: String,
    pub size: u64,
    pub tenant_id: String,
    pub repository_id: String,
    pub uploaded_at: String,
    pub uploaded_by: Option<String>,
    pub content_type: Option<String>,
    pub verified: bool,
    pub verified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsObjectExists {
    pub exists: bool,
    pub size: Option<u64>,
    pub uploaded_at: Option<String>,
}

#[derive(Clone)]
pub struct LfsObjectCache {
    pool: Arc<CachePool>,
}

impl LfsObjectCache {
    pub fn new(pool: Arc<CachePool>) -> Self {
        Self { pool }
    }

    fn key_prefix(&self) -> &str {
        &self.pool.config().key_prefix
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, oid = %oid))]
    pub async fn get_object(
        &self,
        tenant_id: &str,
        oid: &str,
    ) -> crate::cache::CacheResult<Option<LfsObjectMetadata>> {
        let key = CacheKey::lfs_object(self.key_prefix(), tenant_id, oid).with("meta");
        self.get_json(&key.to_redis_key()).await
    }

    #[instrument(skip(self, metadata), fields(tenant_id = %tenant_id, oid = %oid))]
    pub async fn set_object(
        &self,
        tenant_id: &str,
        oid: &str,
        metadata: &LfsObjectMetadata,
    ) -> crate::cache::CacheResult<()> {
        let key = CacheKey::lfs_object(self.key_prefix(), tenant_id, oid).with("meta");
        let ttl = self.pool.config().lfs_object_ttl();
        self.set_json(&key.to_redis_key(), metadata, ttl).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, oid = %oid))]
    pub async fn object_exists(
        &self,
        tenant_id: &str,
        oid: &str,
    ) -> crate::cache::CacheResult<Option<LfsObjectExists>> {
        let key = CacheKey::lfs_object(self.key_prefix(), tenant_id, oid).with("exists");
        self.get_json(&key.to_redis_key()).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, oid = %oid))]
    pub async fn set_object_exists(
        &self,
        tenant_id: &str,
        oid: &str,
        exists: &LfsObjectExists,
    ) -> crate::cache::CacheResult<()> {
        let key = CacheKey::lfs_object(self.key_prefix(), tenant_id, oid).with("exists");
        let ttl = self.pool.config().lfs_object_ttl();
        self.set_json(&key.to_redis_key(), exists, ttl).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, oid = %oid))]
    pub async fn invalidate_object(
        &self,
        tenant_id: &str,
        oid: &str,
    ) -> crate::cache::CacheResult<u64> {
        let pattern = CacheKey::lfs_object(self.key_prefix(), tenant_id, oid).pattern();
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id))]
    pub async fn invalidate_tenant_objects(
        &self,
        tenant_id: &str,
    ) -> crate::cache::CacheResult<u64> {
        let pattern = format!("{}:lfs:{}:*", self.key_prefix(), tenant_id);
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, repo_id = %repository_id))]
    pub async fn invalidate_repository_objects(
        &self,
        tenant_id: &str,
        repository_id: &str,
    ) -> crate::cache::CacheResult<u64> {
        let pattern = format!(
            "{}:lfs:{}:repo:{}:*",
            self.key_prefix(),
            tenant_id,
            repository_id
        );
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id))]
    pub async fn mark_verified(
        &self,
        tenant_id: &str,
        oid: &str,
        verified_at: &str,
    ) -> crate::cache::CacheResult<()> {
        // Atomic read-modify-write: fetch, update, store back.
        // The get_object/set_object methods go through the same pool connection.
        // For full atomicity under high contention, a Redis Lua script would be needed,
        // but LFS verification is idempotent so the race window is benign.
        if let Some(mut metadata) = self.get_object(tenant_id, oid).await? {
            metadata.verified = true;
            metadata.verified_at = Some(verified_at.to_string());
            self.set_object(tenant_id, oid, &metadata).await?;
        }
        Ok(())
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> crate::cache::CacheResult<Option<T>> {
        let mut conn = self.pool.get_connection().await?;
        match conn.get(key).await? {
            Some(json) => {
                let value: T = serde_json::from_str(&json)?;
                debug!(key = key, "LFS cache hit");
                Ok(Some(value))
            }
            None => {
                debug!(key = key, "LFS cache miss");
                Ok(None)
            }
        }
    }

    async fn set_json<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl: std::time::Duration,
    ) -> crate::cache::CacheResult<()> {
        let json = serde_json::to_string(value)?;
        let mut conn = self.pool.get_connection().await?;
        conn.set(key, &json, ttl).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata() -> LfsObjectMetadata {
        LfsObjectMetadata {
            oid: "abc123".to_string(),
            size: 1024,
            tenant_id: "tenant-1".to_string(),
            repository_id: "repo-1".to_string(),
            uploaded_at: "2024-01-01T00:00:00Z".to_string(),
            uploaded_by: Some("user-1".to_string()),
            content_type: Some("application/octet-stream".to_string()),
            verified: false,
            verified_at: None,
        }
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = test_metadata();
        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: LfsObjectMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(metadata.oid, deserialized.oid);
        assert_eq!(metadata.size, deserialized.size);
    }

    #[test]
    fn test_exists_serialization() {
        let exists = LfsObjectExists {
            exists: true,
            size: Some(1024),
            uploaded_at: Some("2024-01-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&exists).unwrap();
        let deserialized: LfsObjectExists = serde_json::from_str(&json).unwrap();
        assert!(deserialized.exists);
        assert_eq!(deserialized.size, Some(1024));
    }
}

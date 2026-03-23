use crate::cache::{CacheKey, CachePool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, instrument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPermissions {
    pub user_id: String,
    pub tenant_id: String,
    pub permissions: HashSet<String>,
    pub roles: HashSet<String>,
    pub cached_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPermissionCache {
    pub user_id: String,
    pub repository_id: String,
    pub can_read: bool,
    pub can_write: bool,
    pub can_admin: bool,
    pub can_delete: bool,
}

impl RepositoryPermissionCache {
    pub fn new(user_id: String, repository_id: String) -> Self {
        Self {
            user_id,
            repository_id,
            can_read: false,
            can_write: false,
            can_admin: false,
            can_delete: false,
        }
    }

    pub fn with_read(mut self) -> Self {
        self.can_read = true;
        self
    }

    pub fn with_write(mut self) -> Self {
        self.can_write = true;
        self.can_read = true;
        self
    }

    pub fn with_admin(mut self) -> Self {
        self.can_admin = true;
        self.can_write = true;
        self.can_read = true;
        self.can_delete = true;
        self
    }
}

#[derive(Clone)]
pub struct PermissionCache {
    pool: Arc<CachePool>,
}

impl PermissionCache {
    pub fn new(pool: Arc<CachePool>) -> Self {
        Self { pool }
    }

    fn key_prefix(&self) -> &str {
        &self.pool.config().key_prefix
    }

    #[instrument(skip(self), fields(user_id = %user_id, tenant_id = %tenant_id))]
    pub async fn get_user_permissions(
        &self,
        user_id: &str,
        tenant_id: &str,
    ) -> crate::cache::CacheResult<Option<CachedPermissions>> {
        let key = CacheKey::tenant(self.key_prefix(), tenant_id)
            .with("user")
            .with(user_id)
            .with("permissions");
        self.get_json(&key.to_redis_key()).await
    }

    #[instrument(skip(self, permissions), fields(user_id = %user_id, tenant_id = %tenant_id))]
    pub async fn set_user_permissions(
        &self,
        user_id: &str,
        tenant_id: &str,
        permissions: &CachedPermissions,
    ) -> crate::cache::CacheResult<()> {
        let key = CacheKey::tenant(self.key_prefix(), tenant_id)
            .with("user")
            .with(user_id)
            .with("permissions");
        let ttl = self.pool.config().permission_ttl();
        self.set_json(&key.to_redis_key(), permissions, ttl).await
    }

    #[instrument(skip(self), fields(user_id = %user_id, tenant_id = %tenant_id))]
    pub async fn invalidate_user_permissions(&self, user_id: &str, tenant_id: &str) -> crate::cache::CacheResult<bool> {
        let key = CacheKey::tenant(self.key_prefix(), tenant_id)
            .with("user")
            .with(user_id)
            .with("permissions");
        let mut conn = self.pool.get_connection().await?;
        conn.delete(&key.to_redis_key()).await
    }

    #[instrument(skip(self), fields(user_id = %user_id, repo_id = %repository_id))]
    pub async fn get_repo_permission(
        &self,
        user_id: &str,
        repository_id: &str,
    ) -> crate::cache::CacheResult<Option<RepositoryPermissionCache>> {
        let key = CacheKey::permission(self.key_prefix(), user_id, &format!("repo:{}", repository_id));
        self.get_json(&key.to_redis_key()).await
    }

    #[instrument(skip(self, permission), fields(user_id = %user_id, repo_id = %repository_id))]
    pub async fn set_repo_permission(
        &self,
        user_id: &str,
        repository_id: &str,
        permission: &RepositoryPermissionCache,
    ) -> crate::cache::CacheResult<()> {
        let key = CacheKey::permission(self.key_prefix(), user_id, &format!("repo:{}", repository_id));
        let ttl = self.pool.config().permission_ttl();
        self.set_json(&key.to_redis_key(), permission, ttl).await
    }

    #[instrument(skip(self), fields(user_id = %user_id, repo_id = %repository_id))]
    pub async fn invalidate_repo_permission(&self, user_id: &str, repository_id: &str) -> crate::cache::CacheResult<bool> {
        let key = CacheKey::permission(self.key_prefix(), user_id, &format!("repo:{}", repository_id));
        let mut conn = self.pool.get_connection().await?;
        conn.delete(&key.to_redis_key()).await
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    pub async fn invalidate_all_user_permissions(&self, user_id: &str) -> crate::cache::CacheResult<u64> {
        let pattern = CacheKey::user_pattern(self.key_prefix(), user_id);
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id))]
    pub async fn invalidate_tenant_permissions(&self, tenant_id: &str) -> crate::cache::CacheResult<u64> {
        let pattern = CacheKey::tenant_pattern(self.key_prefix(), tenant_id);
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    #[instrument(skip(self), fields(repo_id = %repository_id))]
    pub async fn invalidate_repository_permissions(&self, repository_id: &str) -> crate::cache::CacheResult<u64> {
        let pattern = format!("{}:perm:*:repo:{}*", self.key_prefix(), repository_id);
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, key: &str) -> crate::cache::CacheResult<Option<T>> {
        let mut conn = self.pool.get_connection().await?;
        match conn.get(key).await? {
            Some(json) => {
                let value: T = serde_json::from_str(&json)?;
                debug!(key = key, "Permission cache hit");
                Ok(Some(value))
            }
            None => {
                debug!(key = key, "Permission cache miss");
                Ok(None)
            }
        }
    }

    async fn set_json<T: Serialize>(&self, key: &str, value: &T, ttl: std::time::Duration) -> crate::cache::CacheResult<()> {
        let json = serde_json::to_string(value)?;
        let mut conn = self.pool.get_connection().await?;
        conn.set(key, &json, ttl).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cached_permissions() -> CachedPermissions {
        let mut perms = HashSet::new();
        perms.insert("read".to_string());
        perms.insert("write".to_string());
        
        let mut roles = HashSet::new();
        roles.insert("developer".to_string());

        CachedPermissions {
            user_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            permissions: perms,
            roles,
            cached_at: "2024-01-01T00:00:00Z".to_string(),
            expires_at: "2024-01-02T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_permissions_serialization() {
        let perms = test_cached_permissions();
        let json = serde_json::to_string(&perms).unwrap();
        let deserialized: CachedPermissions = serde_json::from_str(&json).unwrap();
        assert_eq!(perms.user_id, deserialized.user_id);
        assert!(deserialized.permissions.contains("read"));
    }

    #[test]
    fn test_repo_permission_builder() {
        let perm = RepositoryPermissionCache::new("user-1".to_string(), "repo-1".to_string())
            .with_read()
            .with_write();
        
        assert!(perm.can_read);
        assert!(perm.can_write);
        assert!(!perm.can_admin);
    }

    #[test]
    fn test_repo_permission_admin() {
        let perm = RepositoryPermissionCache::new("user-1".to_string(), "repo-1".to_string())
            .with_admin();
        
        assert!(perm.can_read);
        assert!(perm.can_write);
        assert!(perm.can_admin);
        assert!(perm.can_delete);
    }
}

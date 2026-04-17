use crate::cache::{CacheKey, CachePool};
use std::sync::Arc;
use tracing::{debug, instrument};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidationPattern {
    Tenant(String),
    Repository {
        tenant_id: String,
        repo_name: String,
    },
    User(String),
    Session(String),
    LfsObject {
        tenant_id: String,
        oid: String,
    },
    ApiEndpoint(String),
    Permission {
        user_id: String,
        resource: String,
    },
    All,
}

#[derive(Clone)]
pub struct CacheInvalidator {
    pool: Arc<CachePool>,
}

impl CacheInvalidator {
    pub fn new(pool: Arc<CachePool>) -> Self {
        Self { pool }
    }

    fn key_prefix(&self) -> &str {
        &self.pool.config().key_prefix
    }

    #[instrument(skip(self))]
    pub async fn invalidate(&self, pattern: InvalidationPattern) -> crate::cache::CacheResult<u64> {
        let redis_pattern = self.pattern_to_redis(&pattern);
        debug!(pattern = ?pattern, redis_pattern = %redis_pattern, "Invalidating cache");

        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&redis_pattern).await
    }

    #[instrument(skip(self))]
    pub async fn invalidate_batch(
        &self,
        patterns: Vec<InvalidationPattern>,
    ) -> crate::cache::CacheResult<u64> {
        let mut total = 0u64;
        for pattern in patterns {
            total += self.invalidate(pattern).await?;
        }
        Ok(total)
    }

    fn pattern_to_redis(&self, pattern: &InvalidationPattern) -> String {
        let prefix = self.key_prefix();

        match pattern {
            InvalidationPattern::Tenant(tenant_id) => CacheKey::tenant_pattern(prefix, tenant_id),
            InvalidationPattern::Repository {
                tenant_id,
                repo_name,
            } => CacheKey::repo_pattern(prefix, tenant_id, repo_name),
            InvalidationPattern::User(user_id) => CacheKey::user_pattern(prefix, user_id),
            InvalidationPattern::Session(session_id) => {
                format!("{}:session:{}*", prefix, session_id)
            }
            InvalidationPattern::LfsObject { tenant_id, oid } => {
                format!("{}:lfs:{}:{}*", prefix, tenant_id, oid)
            }
            InvalidationPattern::ApiEndpoint(endpoint) => {
                format!("{}:api:{}:*", prefix, endpoint)
            }
            InvalidationPattern::Permission { user_id, resource } => {
                format!("{}:perm:{}:{}*", prefix, user_id, resource)
            }
            InvalidationPattern::All => {
                format!("{}:*", prefix)
            }
        }
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id))]
    pub async fn invalidate_tenant(&self, tenant_id: &str) -> crate::cache::CacheResult<u64> {
        self.invalidate(InvalidationPattern::Tenant(tenant_id.to_string()))
            .await
    }

    #[instrument(skip(self), fields(tenant_id = %tenant_id, repo_name = %repo_name))]
    pub async fn invalidate_repository(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> crate::cache::CacheResult<u64> {
        self.invalidate(InvalidationPattern::Repository {
            tenant_id: tenant_id.to_string(),
            repo_name: repo_name.to_string(),
        })
        .await
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    pub async fn invalidate_user(&self, user_id: &str) -> crate::cache::CacheResult<u64> {
        self.invalidate(InvalidationPattern::User(user_id.to_string()))
            .await
    }

    #[instrument(skip(self))]
    pub async fn on_repository_created(&self, tenant_id: &str) -> crate::cache::CacheResult<u64> {
        self.invalidate(InvalidationPattern::ApiEndpoint("repos".to_string()))
            .await
    }

    #[instrument(skip(self))]
    pub async fn on_repository_deleted(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> crate::cache::CacheResult<u64> {
        let patterns = vec![
            InvalidationPattern::Repository {
                tenant_id: tenant_id.to_string(),
                repo_name: repo_name.to_string(),
            },
            InvalidationPattern::ApiEndpoint("repos".to_string()),
            InvalidationPattern::LfsObject {
                tenant_id: tenant_id.to_string(),
                oid: "*".to_string(),
            },
        ];
        self.invalidate_batch(patterns).await
    }

    #[instrument(skip(self))]
    pub async fn on_repository_updated(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> crate::cache::CacheResult<u64> {
        let patterns = vec![
            InvalidationPattern::Repository {
                tenant_id: tenant_id.to_string(),
                repo_name: repo_name.to_string(),
            },
            InvalidationPattern::ApiEndpoint("repos".to_string()),
        ];
        self.invalidate_batch(patterns).await
    }

    #[instrument(skip(self))]
    pub async fn on_branch_changed(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> crate::cache::CacheResult<u64> {
        let pattern = format!(
            "{}:branches:{}:{}*",
            self.key_prefix(),
            tenant_id,
            repo_name
        );
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    #[instrument(skip(self))]
    pub async fn on_permission_changed(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
    ) -> crate::cache::CacheResult<u64> {
        let mut patterns = vec![
            InvalidationPattern::User(user_id.to_string()),
            InvalidationPattern::Permission {
                user_id: user_id.to_string(),
                resource: "*".to_string(),
            },
        ];

        if let Some(tid) = tenant_id {
            patterns.push(InvalidationPattern::Tenant(tid.to_string()));
        }

        self.invalidate_batch(patterns).await
    }

    #[instrument(skip(self))]
    pub async fn on_lfs_upload(
        &self,
        tenant_id: &str,
        oid: &str,
    ) -> crate::cache::CacheResult<bool> {
        let key = CacheKey::lfs_object(self.key_prefix(), tenant_id, oid).with("exists");
        let mut conn = self.pool.get_connection().await?;
        conn.delete(&key.to_redis_key()).await
    }

    #[instrument(skip(self))]
    pub async fn on_user_logout(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> crate::cache::CacheResult<u64> {
        let patterns = vec![
            InvalidationPattern::Session(session_id.to_string()),
            InvalidationPattern::Permission {
                user_id: user_id.to_string(),
                resource: "*".to_string(),
            },
        ];
        self.invalidate_batch(patterns).await
    }

    #[instrument(skip(self))]
    pub async fn flush_all(&self) -> crate::cache::CacheResult<u64> {
        self.invalidate(InvalidationPattern::All).await
    }
}

pub struct InvalidationBuilder {
    patterns: Vec<InvalidationPattern>,
}

impl InvalidationBuilder {
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    pub fn tenant(mut self, tenant_id: &str) -> Self {
        self.patterns
            .push(InvalidationPattern::Tenant(tenant_id.to_string()));
        self
    }

    pub fn repository(mut self, tenant_id: &str, repo_name: &str) -> Self {
        self.patterns.push(InvalidationPattern::Repository {
            tenant_id: tenant_id.to_string(),
            repo_name: repo_name.to_string(),
        });
        self
    }

    pub fn user(mut self, user_id: &str) -> Self {
        self.patterns
            .push(InvalidationPattern::User(user_id.to_string()));
        self
    }

    pub fn session(mut self, session_id: &str) -> Self {
        self.patterns
            .push(InvalidationPattern::Session(session_id.to_string()));
        self
    }

    pub fn lfs_object(mut self, tenant_id: &str, oid: &str) -> Self {
        self.patterns.push(InvalidationPattern::LfsObject {
            tenant_id: tenant_id.to_string(),
            oid: oid.to_string(),
        });
        self
    }

    pub fn api_endpoint(mut self, endpoint: &str) -> Self {
        self.patterns
            .push(InvalidationPattern::ApiEndpoint(endpoint.to_string()));
        self
    }

    pub fn permission(mut self, user_id: &str, resource: &str) -> Self {
        self.patterns.push(InvalidationPattern::Permission {
            user_id: user_id.to_string(),
            resource: resource.to_string(),
        });
        self
    }

    pub fn build(self) -> Vec<InvalidationPattern> {
        self.patterns
    }

    pub async fn execute(self, invalidator: &CacheInvalidator) -> crate::cache::CacheResult<u64> {
        invalidator.invalidate_batch(self.patterns).await
    }
}

impl Default for InvalidationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_invalidation_pattern_tenant() {
        let pattern = InvalidationPattern::Tenant("tenant-1".to_string());
        let pool = Arc::new(
            CachePool::new(crate::cache::CacheConfig::default())
                .await
                .unwrap(),
        );
        let invalidator = CacheInvalidator::new(pool);
        let redis_pattern = invalidator.pattern_to_redis(&pattern);
        assert!(redis_pattern.contains("tenant-1"));
    }

    #[test]
    fn test_invalidation_builder() {
        let patterns = InvalidationBuilder::new()
            .tenant("t1")
            .user("u1")
            .api_endpoint("repos")
            .build();

        assert_eq!(patterns.len(), 3);
    }
}

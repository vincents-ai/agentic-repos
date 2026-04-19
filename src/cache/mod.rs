pub mod api;
pub mod config;
pub mod crypto;
pub mod invalidation;
pub mod keys;
pub mod lfs;
pub mod permission;
pub mod pool;
pub mod repository;
pub mod session;

use crate::cache::invalidation::CacheInvalidator;
pub use api::ApiCache;
pub use config::CacheConfig;
pub use keys::hash_params;
pub use keys::CacheKey;
pub use lfs::LfsObjectCache;
pub use permission::PermissionCache;
pub use pool::{CacheConnection, CachePool};
pub use repository::RepositoryCache;
pub use session::RedisSessionStore;

use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Redis connection error: {0}")]
    Connection(String),

    #[error("Redis operation error: {0}")]
    Operation(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Cache miss for key: {0}")]
    Miss(String),

    #[error("Pool error: {0}")]
    Pool(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type CacheResult<T> = std::result::Result<T, CacheError>;

#[derive(Debug, Clone)]
pub struct CacheLayer {
    pool: Option<Arc<CachePool>>,
    config: CacheConfig,
}

impl CacheLayer {
    pub fn new(config: CacheConfig) -> Self {
        Self { pool: None, config }
    }

    pub fn with_pool(pool: Arc<CachePool>) -> Self {
        let config = pool.config().clone();
        Self {
            pool: Some(pool),
            config,
        }
    }

    pub async fn connect(&mut self) -> CacheResult<()> {
        if self.pool.is_some() {
            return Ok(());
        }

        let pool = CachePool::new(self.config.clone()).await?;
        self.pool = Some(Arc::new(pool));
        Ok(())
    }

    pub fn pool(&self) -> Option<&Arc<CachePool>> {
        self.pool.as_ref()
    }

    pub fn is_connected(&self) -> bool {
        self.pool.is_some()
    }

    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    pub fn repository_cache(&self) -> Option<RepositoryCache> {
        self.pool.as_ref().map(|p| RepositoryCache::new(p.clone()))
    }

    pub fn session_store(&self) -> Option<RedisSessionStore> {
        self.pool
            .as_ref()
            .map(|p| RedisSessionStore::new(p.clone()))
    }

    pub fn api_cache(&self) -> Option<ApiCache> {
        self.pool.as_ref().map(|p| ApiCache::new(p.clone()))
    }

    pub fn lfs_cache(&self) -> Option<LfsObjectCache> {
        self.pool.as_ref().map(|p| LfsObjectCache::new(p.clone()))
    }

    pub fn permission_cache(&self) -> Option<PermissionCache> {
        self.pool.as_ref().map(|p| PermissionCache::new(p.clone()))
    }

    pub fn invalidator(&self) -> Option<CacheInvalidator> {
        self.pool.as_ref().map(|p| CacheInvalidator::new(p.clone()))
    }

    pub async fn health_check(&self) -> CacheResult<bool> {
        match &self.pool {
            Some(pool) => pool.health_check().await,
            None => Ok(false),
        }
    }
}

impl Default for CacheLayer {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}

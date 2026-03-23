use crate::cache::{hash_params, CacheKey, CachePool};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, instrument};

#[derive(Clone)]
pub struct ApiCache {
    pool: Arc<CachePool>,
}

impl ApiCache {
    pub fn new(pool: Arc<CachePool>) -> Self {
        Self { pool }
    }

    fn key_prefix(&self) -> &str {
        &self.pool.config().key_prefix
    }

    #[instrument(skip(self))]
    pub async fn get_response<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        params: &str,
    ) -> crate::cache::CacheResult<Option<T>> {
        let params_hash = hash_params(params);
        let key = CacheKey::api_response(self.key_prefix(), endpoint, &params_hash);
        self.get_json(&key.to_redis_key()).await
    }

    #[instrument(skip(self, response))]
    pub async fn set_response<T: Serialize>(
        &self,
        endpoint: &str,
        params: &str,
        response: &T,
    ) -> crate::cache::CacheResult<()> {
        let params_hash = hash_params(params);
        let key = CacheKey::api_response(self.key_prefix(), endpoint, &params_hash);
        let ttl = self.pool.config().api_response_ttl();
        self.set_json(&key.to_redis_key(), response, ttl).await
    }

    #[instrument(skip(self, response))]
    pub async fn set_response_with_ttl<T: Serialize>(
        &self,
        endpoint: &str,
        params: &str,
        response: &T,
        ttl: Duration,
    ) -> crate::cache::CacheResult<()> {
        let params_hash = hash_params(params);
        let key = CacheKey::api_response(self.key_prefix(), endpoint, &params_hash);
        self.set_json(&key.to_redis_key(), response, ttl).await
    }

    #[instrument(skip(self))]
    pub async fn invalidate_endpoint(&self, endpoint: &str) -> crate::cache::CacheResult<u64> {
        let pattern = format!("{}:api:{}:*", self.key_prefix(), endpoint);
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    #[instrument(skip(self))]
    pub async fn get_or_compute<T, F, Fut>(
        &self,
        endpoint: &str,
        params: &str,
        compute: F,
    ) -> crate::cache::CacheResult<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = crate::cache::CacheResult<T>>,
    {
        if let Some(cached) = self.get_response(endpoint, params).await? {
            debug!("API cache hit for {} with params {}", endpoint, params);
            return Ok(cached);
        }

        debug!("API cache miss for {} with params {}", endpoint, params);
        let result = compute().await?;
        self.set_response(endpoint, params, &result).await?;
        Ok(result)
    }

    #[instrument(skip(self, compute))]
    pub async fn get_or_compute_with_ttl<T, F, Fut>(
        &self,
        endpoint: &str,
        params: &str,
        ttl: Duration,
        compute: F,
    ) -> crate::cache::CacheResult<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = crate::cache::CacheResult<T>>,
    {
        if let Some(cached) = self.get_response(endpoint, params).await? {
            debug!("API cache hit for {} with params {}", endpoint, params);
            return Ok(cached);
        }

        debug!("API cache miss for {} with params {}", endpoint, params);
        let result = compute().await?;
        self.set_response_with_ttl(endpoint, params, &result, ttl).await?;
        Ok(result)
    }

    pub async fn invalidate_all(&self) -> crate::cache::CacheResult<u64> {
        let pattern = format!("{}:api:*", self.key_prefix());
        let mut conn = self.pool.get_connection().await?;
        conn.delete_pattern(&pattern).await
    }

    async fn get_json<T: DeserializeOwned>(&self, key: &str) -> crate::cache::CacheResult<Option<T>> {
        let mut conn = self.pool.get_connection().await?;
        match conn.get(key).await? {
            Some(json) => {
                let value: T = serde_json::from_str(&json)?;
                debug!(key = key, "API cache hit");
                Ok(Some(value))
            }
            None => {
                debug!(key = key, "API cache miss");
                Ok(None)
            }
        }
    }

    async fn set_json<T: Serialize>(&self, key: &str, value: &T, ttl: Duration) -> crate::cache::CacheResult<()> {
        let json = serde_json::to_string(value)?;
        let mut conn = self.pool.get_connection().await?;
        conn.set(key, &json, ttl).await
    }
}

pub struct CachedResponse<T> {
    pub data: T,
    pub from_cache: bool,
    pub age: Option<Duration>,
}

impl<T> CachedResponse<T> {
    pub fn fresh(data: T) -> Self {
        Self {
            data,
            from_cache: false,
            age: None,
        }
    }

    pub fn cached(data: T, age: Duration) -> Self {
        Self {
            data,
            from_cache: true,
            age: Some(age),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_consistency() {
        let hash1 = hash_params("page=1&limit=10");
        let hash2 = hash_params("page=1&limit=10");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_different_params() {
        let hash1 = hash_params("page=1&limit=10");
        let hash2 = hash_params("page=2&limit=10");
        assert_ne!(hash1, hash2);
    }
}

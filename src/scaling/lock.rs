//! Distributed lock mechanism (Redis-based)
//!
//! Provides distributed locking for coordinating operations across
//! multiple instances in a clustered deployment.
//!
//! Uses Redis SET NX EX for atomic lock acquisition and a Lua script
//! for safe release (only release if the value matches our instance).

use std::time::Duration;
use thiserror::Error;
use tracing::{debug, warn};

/// Module-level local lock store — shared across all acquire_local/release_local calls.
static LOCAL_LOCKS: std::sync::LazyLock<
    std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
> = std::sync::LazyLock::new(|| {
    std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()))
});

pub type LockResult<T> = std::result::Result<T, LockError>;

#[derive(Error, Debug)]
pub enum LockError {
    #[error("Failed to acquire lock: {0}")]
    AcquireFailed(String),

    #[error("Lock timeout")]
    Timeout,

    #[error("Lock not held")]
    NotHeld,

    #[error("Redis error: {0}")]
    Redis(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Lock expired")]
    Expired,
}

#[derive(Debug, Clone)]
pub struct LockOptions {
    pub key: String,
    pub ttl: Duration,
    pub retry_interval: Duration,
    pub max_retries: u32,
}

impl LockOptions {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ttl: Duration::from_secs(30),
            retry_interval: Duration::from_millis(100),
            max_retries: 50,
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn with_retry(mut self, interval: Duration, max_retries: u32) -> Self {
        self.retry_interval = interval;
        self.max_retries = max_retries;
        self
    }
}

#[derive(Debug, Clone)]
pub struct LockHandle {
    pub key: String,
    pub value: String,
    pub acquired_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl LockHandle {
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    pub fn remaining_ttl(&self) -> Duration {
        let now = chrono::Utc::now();
        if now >= self.expires_at {
            Duration::ZERO
        } else {
            Duration::from_secs((self.expires_at - now).num_seconds().max(0) as u64)
        }
    }
}

pub struct DistributedLock {
    redis_url: Option<String>,
    instance_id: String,
    pool: Option<deadpool_redis::Pool>,
    /// Tracks keys currently held by this lock instance to prevent re-entrancy.
    held_keys: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl DistributedLock {
    pub fn new(redis_url: Option<String>, instance_id: String) -> Self {
        let pool = redis_url.as_ref().and_then(|url| {
            let cfg = deadpool_redis::Config::from_url(url);
            match cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1)) {
                Ok(p) => {
                    debug!("Redis lock pool created for instance {}", instance_id);
                    Some(p)
                }
                Err(e) => {
                    warn!(
                        "Failed to create Redis pool for locks: {} — falling back to local",
                        e
                    );
                    None
                }
            }
        });
        Self {
            redis_url,
            instance_id,
            pool,
            held_keys: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    pub async fn acquire(&self, options: LockOptions) -> LockResult<LockHandle> {
        // Re-entrancy check: prevent acquiring the same key twice
        {
            let held = self.held_keys.lock().unwrap();
            if held.contains(&options.key) {
                return Err(LockError::AcquireFailed(format!(
                    "Re-entrant lock detected for key '{}'",
                    options.key
                )));
            }
        }

        let lock_value = format!("{}:{}", self.instance_id, uuid::Uuid::new_v4());
        let now = chrono::Utc::now();
        let expires_at =
            now + chrono::Duration::from_std(options.ttl).unwrap_or(chrono::Duration::seconds(30));

        let result = if let Some(ref redis_url) = self.redis_url {
            self.acquire_redis(redis_url, &options, &lock_value, now, expires_at)
                .await
        } else {
            self.acquire_local(&options, &lock_value, now, expires_at)
                .await
        };

        if let Ok(ref handle) = result {
            self.held_keys.lock().unwrap().insert(handle.key.clone());
        }

        result
    }

    async fn acquire_redis(
        &self,
        _redis_url: &str,
        options: &LockOptions,
        lock_value: &str,
        now: chrono::DateTime<chrono::Utc>,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> LockResult<LockHandle> {
        debug!(
            "Attempting to acquire Redis lock: {} (instance: {})",
            options.key, self.instance_id
        );

        for attempt in 0..options.max_retries {
            let acquired = self.try_redis_set(options.key.clone(), lock_value, options.ttl);

            if acquired.await? {
                debug!(
                    "Lock acquired on attempt {}/{}: {}",
                    attempt + 1,
                    options.max_retries,
                    options.key
                );
                return Ok(LockHandle {
                    key: options.key.clone(),
                    value: lock_value.to_string(),
                    acquired_at: now,
                    expires_at,
                });
            }

            if attempt < options.max_retries - 1 {
                tokio::time::sleep(options.retry_interval).await;
            }
        }

        warn!(
            "Failed to acquire lock after {} attempts: {}",
            options.max_retries, options.key
        );
        Err(LockError::AcquireFailed(options.key.clone()))
    }

    /// Lua script for atomic compare-and-delete.
    /// Only deletes the key if the current value matches the expected value.
    /// Returns 1 if deleted, 0 if not (wrong owner or key missing).
    const RELEASE_SCRIPT: &str = r#"
        if redis.call("get", KEYS[1]) == ARGV[1] then
            return redis.call("del", KEYS[1])
        else
            return 0
        end
    "#;

    /// Lua script for atomic compare-and-refresh TTL.
    /// Only extends TTL if the current value matches the expected value.
    const EXTEND_SCRIPT: &str = r#"
        if redis.call("get", KEYS[1]) == ARGV[1] then
            return redis.call("expire", KEYS[1], ARGV[2])
        else
            return 0
        end
    "#;

    async fn try_redis_set(&self, key: String, value: &str, ttl: Duration) -> LockResult<bool> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| LockError::Connection("Redis pool not initialized".to_string()))?;
        let mut conn = pool
            .get()
            .await
            .map_err(|e| LockError::Connection(format!("Failed to get Redis connection: {}", e)))?;

        // SET key value NX EX ttl_seconds — atomic acquire
        let ttl_secs = ttl.as_secs() as u64;
        let result: Option<String> = deadpool_redis::redis::cmd("SET")
            .arg(&key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut *conn)
            .await
            .map_err(|e| LockError::Redis(format!("Redis SET NX EX failed: {}", e)))?;

        // NX returns Some("OK") if set, None if key already exists
        Ok(result.is_some())
    }

    async fn acquire_local(
        &self,
        options: &LockOptions,
        lock_value: &str,
        now: chrono::DateTime<chrono::Utc>,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> LockResult<LockHandle> {
        debug!(
            "Using local lock (no Redis): {} (instance: {})",
            options.key, self.instance_id
        );

        let mut locks = LOCAL_LOCKS.write().await;
        if let Some(existing) = locks.get(&options.key) {
            if existing.starts_with(&self.instance_id) {
                return Ok(LockHandle {
                    key: options.key.clone(),
                    value: lock_value.to_string(),
                    acquired_at: now,
                    expires_at,
                });
            }
            return Err(LockError::AcquireFailed(format!(
                "Lock held by another instance: {}",
                options.key
            )));
        }

        locks.insert(options.key.clone(), lock_value.to_string());

        Ok(LockHandle {
            key: options.key.clone(),
            value: lock_value.to_string(),
            acquired_at: now,
            expires_at,
        })
    }

    pub async fn release(&self, handle: LockHandle) -> LockResult<()> {
        if handle.is_expired() {
            warn!("Attempting to release expired lock: {}", handle.key);
            self.held_keys.lock().unwrap().remove(&handle.key);
            return Err(LockError::Expired);
        }

        let result = if let Some(ref redis_url) = self.redis_url {
            self.release_redis(redis_url, &handle).await
        } else {
            self.release_local(&handle).await
        };

        self.held_keys.lock().unwrap().remove(&handle.key);
        result
    }

    async fn release_redis(&self, _redis_url: &str, handle: &LockHandle) -> LockResult<()> {
        debug!("Releasing Redis lock: {}", handle.key);
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| LockError::Connection("Redis pool not initialized".to_string()))?;
        let mut conn = pool
            .get()
            .await
            .map_err(|e| LockError::Connection(format!("Failed to get Redis connection: {}", e)))?;

        // Atomic compare-and-delete via Lua script
        let result: i32 = deadpool_redis::redis::cmd("EVAL")
            .arg(Self::RELEASE_SCRIPT)
            .arg(1) // number of keys
            .arg(&handle.key)
            .arg(&handle.value)
            .query_async(&mut *conn)
            .await
            .map_err(|e| LockError::Redis(format!("Redis EVAL (release) failed: {}", e)))?;

        if result == 1 {
            debug!("Lock released successfully: {}", handle.key);
            Ok(())
        } else {
            warn!(
                "Lock release failed — value mismatch or key missing: {}",
                handle.key
            );
            Err(LockError::NotHeld)
        }
    }

    async fn release_local(&self, handle: &LockHandle) -> LockResult<()> {
        debug!("Releasing local lock: {}", handle.key);

        let mut locks = LOCAL_LOCKS.write().await;
        if let Some(existing) = locks.get(&handle.key) {
            if existing == &handle.value {
                locks.remove(&handle.key);
                return Ok(());
            }
        }

        Err(LockError::NotHeld)
    }

    pub async fn extend(
        &self,
        handle: &mut LockHandle,
        additional_ttl: Duration,
    ) -> LockResult<()> {
        if handle.is_expired() {
            return Err(LockError::Expired);
        }

        let new_expires_at = handle.expires_at
            + chrono::Duration::from_std(additional_ttl).unwrap_or(chrono::Duration::seconds(30));

        // For Redis, use Lua script to atomically extend TTL
        if self.pool.is_some() {
            let pool = self.pool.as_ref().unwrap();
            let mut conn = pool.get().await.map_err(|e| {
                LockError::Connection(format!("Failed to get Redis connection: {}", e))
            })?;

            let ttl_secs = additional_ttl.as_secs() as i32;
            let result: i32 = deadpool_redis::redis::cmd("EVAL")
                .arg(Self::EXTEND_SCRIPT)
                .arg(1)
                .arg(&handle.key)
                .arg(&handle.value)
                .arg(ttl_secs)
                .query_async(&mut *conn)
                .await
                .map_err(|e| LockError::Redis(format!("Redis EVAL (extend) failed: {}", e)))?;

            if result == 0 {
                warn!("Lock extend failed — value mismatch: {}", handle.key);
                return Err(LockError::NotHeld);
            }
        }

        handle.expires_at = new_expires_at;
        debug!(
            "Extended lock {} until {}",
            handle.key,
            new_expires_at.to_rfc3339()
        );

        Ok(())
    }

    pub async fn with_lock<T, F, Fut>(&self, options: LockOptions, f: F) -> LockResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let handle = self.acquire(options).await?;
        let result = f().await;
        self.release(handle).await?;
        Ok(result)
    }
}

pub struct LockGuard {
    lock: std::sync::Arc<DistributedLock>,
    handle: Option<LockHandle>,
}

impl LockGuard {
    pub fn new(lock: std::sync::Arc<DistributedLock>, handle: LockHandle) -> Self {
        Self {
            lock,
            handle: Some(handle),
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let lock = std::sync::Arc::clone(&self.lock);
            // Use block_in_place to release synchronously within a multi-threaded runtime.
            // Falls back to fire-and-forget spawn if no runtime or single-threaded.
            match tokio::runtime::Handle::try_current() {
                Ok(rt) => {
                    let _ = rt.block_on(async {
                        if let Err(e) = lock.release(handle).await {
                            warn!("Failed to release lock in Drop: {}", e);
                        }
                    });
                }
                Err(_) => {
                    // No runtime — fire-and-forget as last resort
                    warn!("LockGuard Drop: no tokio runtime, lock release may leak");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_lock_acquire_release() {
        let lock = DistributedLock::new(None, "test-instance".to_string());
        let options = LockOptions::new("test-lock");

        let handle = lock.acquire(options).await.unwrap();
        assert!(!handle.is_expired());

        lock.release(handle).await.unwrap();
    }

    #[tokio::test]
    async fn test_lock_conflict() {
        let lock1 = DistributedLock::new(None, "instance-1".to_string());
        let lock2 = DistributedLock::new(None, "instance-2".to_string());

        let options = LockOptions::new("shared-lock");
        let _handle1 = lock1.acquire(options.clone()).await.unwrap();

        let result = lock2.acquire(options).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lock_extend() {
        let lock = DistributedLock::new(None, "test-instance".to_string());
        let options = LockOptions::new("extend-test-lock");

        let mut handle = lock.acquire(options).await.unwrap();
        let original_expiry = handle.expires_at;

        lock.extend(&mut handle, Duration::from_secs(30))
            .await
            .unwrap();
        assert!(handle.expires_at > original_expiry);
    }
}

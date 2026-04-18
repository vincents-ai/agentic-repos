use crate::cache::{CacheConfig, CacheError, CacheResult};
use deadpool_redis::redis::AsyncCommands;
use deadpool_redis::{Config as DeadpoolConfig, Pool, PoolConfig};
use std::time::Duration;
use tracing::{debug, warn};

pub type RedisConnection = deadpool_redis::Connection;

#[derive(Debug, Clone)]
pub struct CachePool {
    pool: Pool,
    config: CacheConfig,
}

impl CachePool {
    pub async fn new(config: CacheConfig) -> CacheResult<Self> {
        let mut cfg = DeadpoolConfig::from_url(&config.url);

        cfg.pool = Some(PoolConfig::new(config.max_connections as usize));

        let pool = cfg
            .builder()
            .map_err(|e| CacheError::Pool(format!("Failed to create pool builder: {}", e)))?
            .build()
            .map_err(|e| CacheError::Pool(format!("Failed to create pool: {}", e)))?;

        let pool_instance = Self { pool, config };

        if let Err(e) = pool_instance.health_check().await {
            warn!("Initial Redis health check failed: {}", e);
        }

        Ok(pool_instance)
    }

    pub async fn get_connection(&self) -> CacheResult<CacheConnection> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| CacheError::Connection(format!("Failed to get connection: {}", e)))?;
        Ok(CacheConnection { conn })
    }

    pub async fn health_check(&self) -> CacheResult<bool> {
        let mut conn = self.get_connection().await?;
        let result: String = conn
            .redis_cmd("PING")
            .await
            .map_err(|e| CacheError::Operation(format!("Health check failed: {}", e)))?;
        Ok(result == "PONG")
    }

    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub fn status(&self) -> PoolStatus {
        PoolStatus {
            max_size: self.pool.status().max_size as u32,
            size: self.pool.status().size as u32,
            available: self.pool.status().available as u32,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub max_size: u32,
    pub size: u32,
    pub available: u32,
}

pub struct CacheConnection {
    conn: RedisConnection,
}

impl CacheConnection {
    pub async fn get<K: AsRef<str>>(&mut self, key: K) -> CacheResult<Option<String>> {
        let result: Option<String> = self
            .conn
            .get(key.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("GET failed: {}", e)))?;
        Ok(result)
    }

    pub async fn get_bytes<K: AsRef<str>>(&mut self, key: K) -> CacheResult<Option<Vec<u8>>> {
        let result: Option<Vec<u8>> = self
            .conn
            .get(key.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("GET failed: {}", e)))?;
        Ok(result)
    }

    pub async fn set<K: AsRef<str>, V: AsRef<str>>(
        &mut self,
        key: K,
        value: V,
        ttl: Duration,
    ) -> CacheResult<()> {
        let key = key.as_ref();
        let value = value.as_ref();
        let ttl_secs = ttl.as_secs();

        // Use SETEX for setting with TTL
        let _: () = self
            .conn
            .set_ex(key, value, ttl_secs)
            .await
            .map_err(|e| CacheError::Operation(format!("SET failed: {}", e)))?;

        debug!(key = key, ttl_secs = ttl_secs, "Cache SET");
        Ok(())
    }

    pub async fn set_bytes<K: AsRef<str>>(
        &mut self,
        key: K,
        value: &[u8],
        ttl: Duration,
    ) -> CacheResult<()> {
        let key = key.as_ref();
        let ttl_secs = ttl.as_secs();

        // Use SETEX for setting with TTL
        let _: () = self
            .conn
            .set_ex(key, value, ttl_secs)
            .await
            .map_err(|e| CacheError::Operation(format!("SET failed: {}", e)))?;

        debug!(key = key, ttl_secs = ttl_secs, "Cache SET bytes");
        Ok(())
    }

    pub async fn delete<K: AsRef<str>>(&mut self, key: K) -> CacheResult<bool> {
        let result: i32 = self
            .conn
            .del(key.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("DEL failed: {}", e)))?;
        Ok(result > 0)
    }

    pub async fn delete_pattern(&mut self, pattern: &str) -> CacheResult<u64> {
        // Use SCAN instead of KEYS to avoid blocking Redis.
        // SCAN iterates in small batches, yielding between cursors.
        let mut total_deleted: u64 = 0;
        let mut cursor: u64 = 0;
        let batch_size: u64 = 100;

        loop {
            let (next_cursor, keys): (u64, Vec<String>) = deadpool_redis::redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(batch_size)
                .query_async(&mut self.conn)
                .await
                .map_err(|e| CacheError::Operation(format!("SCAN failed: {}", e)))?;

            if !keys.is_empty() {
                let count: i32 = self
                    .conn
                    .del(&keys)
                    .await
                    .map_err(|e| CacheError::Operation(format!("DEL failed: {}", e)))?;
                total_deleted += count as u64;
            }

            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        debug!(
            pattern = pattern,
            count = total_deleted,
            "Cache delete by pattern (SCAN)"
        );
        Ok(total_deleted)
    }

    pub async fn exists<K: AsRef<str>>(&mut self, key: K) -> CacheResult<bool> {
        let result: i32 = self
            .conn
            .exists(key.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("EXISTS failed: {}", e)))?;
        Ok(result == 1)
    }
    pub async fn ttl<K: AsRef<str>>(&mut self, key: K) -> CacheResult<i64> {
        let result: i64 = self
            .conn
            .ttl(key.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("TTL failed: {}", e)))?;
        Ok(result)
    }
    pub async fn expire<K: AsRef<str>>(&mut self, key: K, ttl: Duration) -> CacheResult<bool> {
        let result: i32 = self
            .conn
            .expire(key.as_ref(), ttl.as_secs() as i64)
            .await
            .map_err(|e| CacheError::Operation(format!("EXPIRE failed: {}", e)))?;
        Ok(result == 1)
    }
    pub async fn incr<K: AsRef<str>>(&mut self, key: K) -> CacheResult<i64> {
        let result: i64 = self
            .conn
            .incr(key.as_ref(), 1)
            .await
            .map_err(|e| CacheError::Operation(format!("INCR failed: {}", e)))?;
        Ok(result)
    }
    pub async fn incr_by<K: AsRef<str>>(&mut self, key: K, amount: i64) -> CacheResult<i64> {
        // Use INCR command with amount
        let result: i64 = self
            .conn
            .incr(key.as_ref(), amount)
            .await
            .map_err(|e| CacheError::Operation(format!("INCRBY failed: {}", e)))?;
        Ok(result)
    }
    pub async fn sadd<K: AsRef<str>, V: AsRef<str>>(
        &mut self,
        key: K,
        member: V,
    ) -> CacheResult<bool> {
        let result: i32 = self
            .conn
            .sadd(key.as_ref(), member.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("SADD failed: {}", e)))?;
        Ok(result > 0)
    }
    pub async fn sismember<K: AsRef<str>, V: AsRef<str>>(
        &mut self,
        key: K,
        member: V,
    ) -> CacheResult<bool> {
        let result: bool = self
            .conn
            .sismember(key.as_ref(), member.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("SISMEMBER failed: {}", e)))?;
        Ok(result)
    }
    pub async fn srem<K: AsRef<str>, V: AsRef<str>>(
        &mut self,
        key: K,
        member: V,
    ) -> CacheResult<bool> {
        let result: i32 = self
            .conn
            .srem(key.as_ref(), member.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("SREM failed: {}", e)))?;
        Ok(result > 0)
    }
    pub async fn smembers<K: AsRef<str>>(&mut self, key: K) -> CacheResult<Vec<String>> {
        let result: Vec<String> = self
            .conn
            .smembers(key.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("SMEMBERS failed: {}", e)))?;
        Ok(result)
    }
    pub async fn hset<K: AsRef<str>, F: AsRef<str>, V: AsRef<str>>(
        &mut self,
        key: K,
        field: F,
        value: V,
    ) -> CacheResult<bool> {
        let result: i32 = self
            .conn
            .hset(key.as_ref(), field.as_ref(), value.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("HSET failed: {}", e)))?;
        Ok(result > 0)
    }
    pub async fn hget<K: AsRef<str>, F: AsRef<str>>(
        &mut self,
        key: K,
        field: F,
    ) -> CacheResult<Option<String>> {
        let result: Option<String> = self
            .conn
            .hget(key.as_ref(), field.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("HGET failed: {}", e)))?;
        Ok(result)
    }
    pub async fn keys<K: AsRef<str>>(&mut self, pattern: K) -> CacheResult<Vec<String>> {
        // Use SCAN instead of KEYS to avoid blocking Redis
        let mut all_keys: Vec<String> = Vec::new();
        let mut cursor: u64 = 0;
        let batch_size: u64 = 100;

        loop {
            let (next_cursor, batch): (u64, Vec<String>) = deadpool_redis::redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern.as_ref())
                .arg("COUNT")
                .arg(batch_size)
                .query_async(&mut self.conn)
                .await
                .map_err(|e| CacheError::Operation(format!("SCAN failed: {}", e)))?;

            all_keys.extend(batch);
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        Ok(all_keys)
    }
    pub async fn hdel<K: AsRef<str>, F: AsRef<str>>(
        &mut self,
        key: K,
        field: F,
    ) -> CacheResult<bool> {
        let result: i32 = self
            .conn
            .hdel(key.as_ref(), field.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("HDEL failed: {}", e)))?;
        Ok(result > 0)
    }
    pub async fn hgetall<K: AsRef<str>>(
        &mut self,
        key: K,
    ) -> CacheResult<std::collections::HashMap<String, String>> {
        let result: std::collections::HashMap<String, String> = self
            .conn
            .hgetall(key.as_ref())
            .await
            .map_err(|e| CacheError::Operation(format!("HGETALL failed: {}", e)))?;
        Ok(result)
    }
    async fn redis_cmd<T: deadpool_redis::redis::FromRedisValue>(
        &mut self,
        cmd: &str,
    ) -> CacheResult<T> {
        let result = deadpool_redis::redis::cmd(cmd)
            .query_async(&mut self.conn)
            .await
            .map_err(|e| CacheError::Operation(format!("{} failed: {}", cmd, e)))?;
        Ok(result)
    }
}
pub struct CachePoolBuilder {
    config: CacheConfig,
}
impl CachePoolBuilder {
    pub fn new(config: CacheConfig) -> Self {
        Self { config }
    }
    pub fn url(mut self, url: String) -> Self {
        self.config.url = url;
        self
    }
    pub fn prefix(mut self, prefix: String) -> Self {
        self.config.key_prefix = prefix;
        self
    }
    pub fn max_connections(mut self, max: u32) -> Self {
        self.config.max_connections = max;
        self
    }
    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.config.default_ttl_secs = ttl.as_secs();
        self
    }
    pub async fn build(self) -> CacheResult<CachePool> {
        CachePool::new(self.config).await
    }
}

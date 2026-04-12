use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct ThrottleConfig {
    pub default_bytes_per_second: u64,
    pub per_tenant_limits: HashMap<String, u64>,
    pub per_repo_limits: HashMap<String, u64>,
    pub burst_size: u64,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            default_bytes_per_second: 10 * 1024 * 1024,
            per_tenant_limits: HashMap::new(),
            per_repo_limits: HashMap::new(),
            burst_size: 50 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
struct TokenBucket {
    capacity: u64,
    tokens: f64,
    last_refill: Instant,
    refill_rate: f64,
}

impl TokenBucket {
    fn new(bytes_per_second: u64, burst_size: u64) -> Self {
        Self {
            capacity: burst_size,
            tokens: burst_size as f64,
            last_refill: Instant::now(),
            refill_rate: bytes_per_second as f64,
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        let new_tokens = elapsed * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity as f64);
        self.last_refill = now;
    }

    fn try_consume(&mut self, bytes: u64) -> Option<Duration> {
        self.refill();

        if self.tokens >= bytes as f64 {
            self.tokens -= bytes as f64;
            None
        } else {
            let tokens_needed = bytes as f64 - self.tokens;
            let wait_time = tokens_needed / self.refill_rate;
            Some(Duration::from_secs_f64(wait_time))
        }
    }

    async fn consume(&mut self, bytes: u64) {
        while let Some(wait_duration) = self.try_consume(bytes) {
            sleep(wait_duration).await;
        }
    }
}

pub struct BandwidthThrottler {
    config: ThrottleConfig,
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

impl BandwidthThrottler {
    pub fn new(config: ThrottleConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn throttle_key(tenant: &str, repo: &str) -> String {
        format!("{}/{}", tenant, repo)
    }

    fn get_limit(&self, tenant: &str, repo: &str) -> u64 {
        let repo_key = format!("{}/{}", tenant, repo);

        if let Some(&limit) = self.config.per_repo_limits.get(&repo_key) {
            return limit;
        }

        if let Some(&limit) = self.config.per_tenant_limits.get(tenant) {
            return limit;
        }

        self.config.default_bytes_per_second
    }

    pub async fn acquire(&self, tenant: &str, repo: &str, bytes: u64) {
        let key = Self::throttle_key(tenant, repo);
        let limit = self.get_limit(tenant, repo);

        let mut buckets = self.buckets.write().await;
        let bucket = buckets
            .entry(key.clone())
            .or_insert_with(|| TokenBucket::new(limit, self.config.burst_size));

        bucket.consume(bytes).await;
    }

    pub async fn try_acquire(&self, tenant: &str, repo: &str, bytes: u64) -> Result<(), Duration> {
        let key = Self::throttle_key(tenant, repo);
        let limit = self.get_limit(tenant, repo);

        let mut buckets = self.buckets.write().await;
        let bucket = buckets
            .entry(key.clone())
            .or_insert_with(|| TokenBucket::new(limit, self.config.burst_size));

        match bucket.try_consume(bytes) {
            None => Ok(()),
            Some(wait_time) => Err(wait_time),
        }
    }

    pub async fn get_available_tokens(&self, tenant: &str, repo: &str) -> u64 {
        let key = Self::throttle_key(tenant, repo);
        let buckets = self.buckets.read().await;

        if let Some(bucket) = buckets.get(&key) {
            bucket.tokens as u64
        } else {
            self.config.burst_size
        }
    }

    pub fn get_rate_limit(&self, tenant: &str, repo: &str) -> u64 {
        self.get_limit(tenant, repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant as TokioInstant;

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(1024, 2048);
        assert_eq!(bucket.capacity, 2048);
        assert_eq!(bucket.tokens, 2048.0);
        assert_eq!(bucket.refill_rate, 1024.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(1024, 2048);

        let wait = bucket.try_consume(1024);
        assert!(wait.is_none());
        assert!((bucket.tokens - 1024.0).abs() < 0.1);
    }

    #[test]
    fn test_token_bucket_overconsume() {
        let mut bucket = TokenBucket::new(1024, 2048);

        let wait = bucket.try_consume(3000);
        assert!(wait.is_some());

        if let Some(duration) = wait {
            assert!(duration.as_secs_f64() > 0.9);
        }
    }

    #[tokio::test]
    async fn test_bandwidth_throttler_default_limit() {
        let config = ThrottleConfig {
            default_bytes_per_second: 1024,
            per_tenant_limits: HashMap::new(),
            per_repo_limits: HashMap::new(),
            burst_size: 2048,
        };

        let throttler = BandwidthThrottler::new(config);
        assert_eq!(throttler.get_rate_limit("tenant1", "repo1"), 1024);
    }

    #[tokio::test]
    async fn test_bandwidth_throttler_tenant_limit() {
        let mut per_tenant_limits = HashMap::new();
        per_tenant_limits.insert("tenant1".to_string(), 2048);

        let config = ThrottleConfig {
            default_bytes_per_second: 1024,
            per_tenant_limits,
            per_repo_limits: HashMap::new(),
            burst_size: 4096,
        };

        let throttler = BandwidthThrottler::new(config);
        assert_eq!(throttler.get_rate_limit("tenant1", "repo1"), 2048);
        assert_eq!(throttler.get_rate_limit("tenant2", "repo1"), 1024);
    }

    #[tokio::test]
    async fn test_bandwidth_throttler_repo_limit() {
        let mut per_tenant_limits = HashMap::new();
        per_tenant_limits.insert("tenant1".to_string(), 2048);

        let mut per_repo_limits = HashMap::new();
        per_repo_limits.insert("tenant1/repo1".to_string(), 4096);

        let config = ThrottleConfig {
            default_bytes_per_second: 1024,
            per_tenant_limits,
            per_repo_limits,
            burst_size: 8192,
        };

        let throttler = BandwidthThrottler::new(config);
        assert_eq!(throttler.get_rate_limit("tenant1", "repo1"), 4096);
        assert_eq!(throttler.get_rate_limit("tenant1", "repo2"), 2048);
        assert_eq!(throttler.get_rate_limit("tenant2", "repo1"), 1024);
    }

    #[tokio::test]
    async fn test_bandwidth_throttler_acquire() {
        let config = ThrottleConfig {
            default_bytes_per_second: 1024 * 1024,
            per_tenant_limits: HashMap::new(),
            per_repo_limits: HashMap::new(),
            burst_size: 2048 * 1024,
        };

        let throttler = BandwidthThrottler::new(config);

        let start = TokioInstant::now();
        throttler.acquire("tenant1", "repo1", 1024).await;
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 10);
    }

    #[tokio::test]
    async fn test_bandwidth_throttler_try_acquire() {
        let config = ThrottleConfig {
            default_bytes_per_second: 1024,
            per_tenant_limits: HashMap::new(),
            per_repo_limits: HashMap::new(),
            burst_size: 2048,
        };

        let throttler = BandwidthThrottler::new(config);

        let result = throttler.try_acquire("tenant1", "repo1", 1024).await;
        assert!(result.is_ok());

        let result = throttler.try_acquire("tenant1", "repo1", 2048).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_bandwidth_throttler_get_available_tokens() {
        let config = ThrottleConfig {
            default_bytes_per_second: 1024,
            per_tenant_limits: HashMap::new(),
            per_repo_limits: HashMap::new(),
            burst_size: 2048,
        };

        let throttler = BandwidthThrottler::new(config);

        let available = throttler.get_available_tokens("tenant1", "repo1").await;
        assert_eq!(available, 2048);

        throttler.acquire("tenant1", "repo1", 1024).await;

        let available = throttler.get_available_tokens("tenant1", "repo1").await;
        assert!(available < 2048);
        assert!(available >= 1024);
    }
}

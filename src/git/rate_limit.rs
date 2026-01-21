use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    #[serde(default)]
    pub proxy_headers: Vec<String>,
    #[serde(default)]
    pub tiers: HashMap<String, RateLimitTier>,
    #[serde(default)]
    pub global_limits: GlobalRateLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalRateLimits {
    pub max_requests_per_second: u32,
    pub max_connections_per_ip: u32,
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        let mut tiers = HashMap::new();

        tiers.insert(
            "free".to_string(),
            RateLimitTier {
                requests_per_minute: 60,
                requests_per_hour: 1000,
                burst_size: 10,
                ..Default::default()
            },
        );

        tiers.insert(
            "pro".to_string(),
            RateLimitTier {
                requests_per_minute: 300,
                requests_per_hour: 10000,
                burst_size: 50,
                ..Default::default()
            },
        );

        tiers.insert(
            "enterprise".to_string(),
            RateLimitTier {
                requests_per_minute: 0, // 0 means unlimited
                requests_per_hour: 0,
                burst_size: 100,
                ..Default::default()
            },
        );

        Self {
            enabled: true,
            proxy_headers: vec!["x-forwarded-for".to_string(), "x-real-ip".to_string()],
            tiers,
            global_limits: GlobalRateLimits {
                max_requests_per_second: 1000,
                max_connections_per_ip: 10,
                burst_size: 50,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimitTier {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub burst_size: u32,
    #[serde(default)]
    pub max_concurrent_operations: u32,
    #[serde(default)]
    pub bandwidth_limit_mbps: u32,
}

/// Request context for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitContext {
    pub tenant_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub request_id: String,
    pub path: String,
    pub timestamp: Instant,
}

impl RateLimitContext {
    pub fn from_headers(headers: &http::HeaderMap, tenant_id: Option<String>, path: &str) -> Self {
        // Extract IP from proxy headers or direct connection
        let ip_address = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .or(headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
            .or(headers.get("remote-addr").and_then(|v| v.to_str().ok()))
            .unwrap_or("unknown")
            .to_string();

        // Take first IP if multiple are present
        let ip_address = ip_address
            .split(',')
            .next()
            .unwrap_or(&ip_address)
            .trim()
            .to_string();

        let request_id = headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let user_agent = headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        Self {
            tenant_id: tenant_id.unwrap_or_default(),
            ip_address,
            user_agent,
            request_id,
            path: path.to_string(),
            timestamp: Instant::now(),
        }
    }

    /// Generate a unique key for this request context
    pub fn rate_limit_key(&self, scope: RateLimitScope) -> String {
        match scope {
            RateLimitScope::Tenant => format!("tenant:{}", self.tenant_id),
            RateLimitScope::Ip => format!("ip:{}", self.ip_address),
            RateLimitScope::Combined => format!("{}:{}", self.tenant_id, self.ip_address),
            RateLimitScope::Global => "global".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitScope {
    Tenant,
    Ip,
    Combined,
    Global,
}

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded for tenant {tenant_id}: {limit_type} ({current}/{max})")]
    RateLimitExceeded {
        tenant_id: String,
        limit_type: String,
        current: u32,
        max: u32,
        retry_after: u64,
    },
    #[error("Rate limit configuration error: {0}")]
    ConfigError(String),
    #[error("Rate limiter not available")]
    NotAvailable,
}

pub type RateLimitResult<T> = std::result::Result<T, RateLimitError>;

/// Token bucket rate limiter
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

#[allow(dead_code)]
impl TokenBucket {
    fn new(max_tokens: u32, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens as f64,
            max_tokens: max_tokens as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let elapsed = self.last_refill.elapsed();
        let tokens_to_add = elapsed.as_secs_f64() * self.refill_rate;
        self.tokens = (self.tokens + tokens_to_add).min(self.max_tokens);
        self.last_refill = Instant::now();
    }

    fn retry_after(&self) -> u64 {
        if self.tokens >= 1.0 {
            return 0;
        }
        let tokens_needed = 1.0 - self.tokens;
        let seconds_needed = tokens_needed / self.refill_rate;
        seconds_needed.ceil() as u64
    }
}

/// In-memory rate limiter store
#[derive(Debug)]
#[allow(dead_code)]
struct RateLimitStore {
    buckets: RwLock<HashMap<String, TokenBucket>>,
    request_counts: RwLock<HashMap<String, Vec<Instant>>>,
}

#[allow(dead_code)]
impl RateLimitStore {
    fn new() -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
            request_counts: RwLock::new(HashMap::new()),
        }
    }

    async fn check_rate_limit(
        &self,
        key: &str,
        max_requests: u32,
        window_secs: u64,
    ) -> RateLimitResult<()> {
        let now = Instant::now();

        // Clean old entries
        let window_start = now - Duration::from_secs(window_secs);

        {
            let mut counts = self.request_counts.write().await;
            let entry = counts.entry(key.to_string()).or_default();
            entry.retain(|&time| time > window_start);

            let current_count = entry.len() as u32;
            if current_count >= max_requests && max_requests > 0 {
                let retry_after = window_secs;
                return Err(RateLimitError::RateLimitExceeded {
                    tenant_id: key.to_string(),
                    limit_type: format!("{}/{}s", max_requests, window_secs),
                    current: current_count,
                    max: max_requests,
                    retry_after,
                });
            }

            entry.push(now);
        }

        Ok(())
    }

    async fn get_token_bucket(&self, key: &str, max_burst: u32, refill_rate: f64) -> TokenBucket {
        let mut buckets = self.buckets.write().await;
        buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(max_burst, refill_rate))
            .clone()
    }
}

/// Proxy-aware rate limiter service
#[derive(Debug, Clone)]
pub struct RateLimiter {
    store: Arc<RateLimitStore>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: Option<RateLimitConfig>) -> Self {
        Self {
            store: Arc::new(RateLimitStore::new()),
            config: config.unwrap_or_default(),
        }
    }

    /// Check rate limits for a request
    pub async fn check_rate_limit(
        &self,
        context: &RateLimitContext,
        tier: &str,
    ) -> RateLimitResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let tier_config = self
            .config
            .tiers
            .get(tier)
            .ok_or_else(|| RateLimitError::ConfigError(format!("Unknown tier: {}", tier)))?;

        // Check per-minute limit
        if tier_config.requests_per_minute > 0 {
            let key = format!("{}:min", context.rate_limit_key(RateLimitScope::Tenant));
            self.store
                .check_rate_limit(&key, tier_config.requests_per_minute, 60)
                .await?;
        }

        // Check per-hour limit
        if tier_config.requests_per_hour > 0 {
            let key = format!("{}:hour", context.rate_limit_key(RateLimitScope::Tenant));
            self.store
                .check_rate_limit(&key, tier_config.requests_per_hour, 3600)
                .await?;
        }

        // Check global rate limit
        let global_key = context.rate_limit_key(RateLimitScope::Global);
        self.store
            .check_rate_limit(
                &global_key,
                self.config.global_limits.max_requests_per_second,
                1,
            )
            .await?;

        Ok(())
    }

    /// Check concurrent operation limits
    pub async fn check_concurrent_limit(
        &self,
        context: &RateLimitContext,
        tier: &str,
        current_concurrent: u32,
    ) -> RateLimitResult<()> {
        let tier_config = match self.config.tiers.get(tier) {
            Some(t) => t,
            None => return Ok(()),
        };

        if tier_config.max_concurrent_operations > 0
            && current_concurrent >= tier_config.max_concurrent_operations
        {
            return Err(RateLimitError::RateLimitExceeded {
                tenant_id: context.tenant_id.clone(),
                limit_type: "concurrent_operations".to_string(),
                current: current_concurrent,
                max: tier_config.max_concurrent_operations,
                retry_after: 5,
            });
        }

        Ok(())
    }

    /// Get rate limit headers for response
    pub fn rate_limit_headers(&self, context: &RateLimitContext, tier: &str) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();

        if let Some(tier_config) = self.config.tiers.get(tier) {
            headers.insert(
                "X-RateLimit-Limit-Minute",
                tier_config.requests_per_minute.to_string().parse().unwrap(),
            );
            headers.insert(
                "X-RateLimit-Limit-Hour",
                tier_config.requests_per_hour.to_string().parse().unwrap(),
            );
            headers.insert(
                "X-RateLimit-Burst",
                tier_config.burst_size.to_string().parse().unwrap(),
            );
        }

        headers.insert("X-Request-ID", context.request_id.clone().parse().unwrap());

        headers
    }

    /// Extract client IP considering proxy headers
    pub fn extract_client_ip(headers: &http::HeaderMap) -> String {
        headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .or(headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
            .or(headers.get("remote-addr").and_then(|v| v.to_str().ok()))
            .unwrap_or("unknown")
            .to_string()
            .split(',')
            .next()
            .unwrap_or("unknown")
            .trim()
            .to_string()
    }
}

/// Middleware for adding rate limiting to HTTP requests
pub struct RateLimitMiddleware {
    rate_limiter: RateLimiter,
}

impl RateLimitMiddleware {
    pub fn new(rate_limiter: RateLimiter) -> Self {
        Self { rate_limiter }
    }

    pub async fn check_request(
        &self,
        headers: &http::HeaderMap,
        tenant_id: Option<String>,
        path: &str,
        tier: &str,
    ) -> RateLimitResult<()> {
        let context = RateLimitContext::from_headers(headers, tenant_id, path);
        self.rate_limiter.check_rate_limit(&context, tier).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limit_check() {
        let mut config = RateLimitConfig {
            enabled: true,
            tiers: HashMap::new(),
            ..Default::default()
        };
        config.tiers.insert(
            "test".to_string(),
            RateLimitTier {
                requests_per_minute: 10,
                requests_per_hour: 100,
                burst_size: 5,
                ..Default::default()
            },
        );

        let rate_limiter = RateLimiter::new(Some(config));

        let context = RateLimitContext {
            tenant_id: "test-tenant".to_string(),
            ip_address: "192.168.1.1".to_string(),
            user_agent: "test".to_string(),
            request_id: "test-123".to_string(),
            path: "/test".to_string(),
            timestamp: Instant::now(),
        };

        // First 10 requests should succeed
        for i in 0..10 {
            let result = rate_limiter.check_rate_limit(&context, "test").await;
            assert!(result.is_ok(), "Request {} should succeed", i + 1);
        }

        // 11th request should fail
        let result = rate_limiter.check_rate_limit(&context, "test").await;
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(RateLimitError::RateLimitExceeded { .. })
        ));
    }

    #[test]
    fn test_token_bucket() {
        let mut bucket = TokenBucket::new(10, 1.0); // 10 tokens, refill 1 per second

        // Should be able to consume 10 tokens
        for _ in 0..10 {
            assert!(bucket.try_consume());
        }

        // Should fail on 11th
        assert!(!bucket.try_consume());

        // Wait for refill
        std::thread::sleep(Duration::from_millis(1100));

        // Should be able to consume again
        assert!(bucket.try_consume());
    }
}

//! Rate limiting for Skynet v2 Git Operations Plugin

pub mod config;
pub mod token_bucket;

pub use config::RateLimitConfig;
pub use token_bucket::TokenBucket;

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;

/// Rate limiter
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if request is allowed
    pub fn allow_request(&self, user_id: &str) -> bool {
        let mut buckets = self.buckets.write();
        let bucket = buckets
            .entry(user_id.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.per_user.api_calls_per_minute as f64));

        bucket.try_consume(1.0)
    }
}

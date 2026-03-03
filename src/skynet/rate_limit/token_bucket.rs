//! Token bucket rate limiting algorithm

use std::time::Instant;

/// Token bucket for rate limiting
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    last_refill: Instant,
    refill_rate: f64,
}

impl TokenBucket {
    /// Create a new token bucket
    pub fn new(capacity: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            last_refill: Instant::now(),
            refill_rate: capacity / 60.0, // per second
        }
    }

    /// Try to consume tokens
    pub fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(10.0);
        assert_eq!(bucket.capacity, 10.0);
        assert_eq!(bucket.tokens, 10.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(10.0);
        assert!(bucket.try_consume(5.0));
        assert_eq!(bucket.tokens, 5.0);
        assert!(!bucket.try_consume(10.0));
    }
}

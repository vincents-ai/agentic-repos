//! Configuration for the SSE client.

/// Configuration for the SSE client connection.
#[derive(Debug, Clone)]
pub struct SseClientConfig {
    /// Base URL of the adp server (e.g. "http://localhost:8090").
    pub server_url: String,
    /// Instance identifier.
    pub instance_id: String,
    /// Connection secret.
    pub secret: String,
    /// Repository or project identifier.
    pub repo_identifier: String,
    /// TTL remaining in seconds.
    pub ttl_remaining: i64,
    /// Maximum number of reconnection attempts before giving up.
    pub max_reconnect_attempts: u32,
    /// Initial backoff delay in milliseconds.
    pub initial_backoff_ms: u64,
    /// Maximum backoff delay in milliseconds.
    pub max_backoff_ms: u64,
    /// Backoff multiplier (e.g. 2.0 for doubling).
    pub backoff_multiplier: f64,
    /// Heartbeat timeout in seconds (disconnect if no event within this time).
    pub heartbeat_timeout_secs: u64,
}

impl SseClientConfig {
    /// Create config with defaults.
    pub fn new(server_url: &str, instance_id: &str, secret: &str, repo_identifier: &str, ttl_remaining: i64) -> Self {
        Self {
            server_url: server_url.to_string(),
            instance_id: instance_id.to_string(),
            secret: secret.to_string(),
            repo_identifier: repo_identifier.to_string(),
            ttl_remaining,
            max_reconnect_attempts: 10,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            backoff_multiplier: 2.0,
            heartbeat_timeout_secs: 120,
        }
    }

    /// SSE connect endpoint URL.
    pub fn connect_url(&self) -> String {
        format!("{}/api/agentic-loop/connect", self.server_url.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let config = SseClientConfig::new(
            "http://localhost:8090",
            "instance-1",
            "secret",
            "default",
            3600,
        );
        assert_eq!(config.connect_url(), "http://localhost:8090/api/agentic-loop/connect");
        assert_eq!(config.max_reconnect_attempts, 10);
        assert_eq!(config.initial_backoff_ms, 1000);
        assert_eq!(config.max_backoff_ms, 60000);
    }
}

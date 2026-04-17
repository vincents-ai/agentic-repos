//! Scaling configuration

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScalingConfig {
    /// Enable horizontal scaling mode
    pub enabled: bool,
    /// Unique instance identifier
    pub instance_id: String,
    /// Redis URL for distributed locks and coordination
    pub redis_url: Option<String>,
    /// Cluster name for grouping instances
    pub cluster_name: String,
    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,
    /// Instance timeout in seconds (considered dead after this)
    pub instance_timeout_secs: u64,
    /// Leader election timeout in seconds
    pub leader_election_timeout_secs: u64,
    /// Session affinity configuration
    pub session: SessionAffinityConfig,
    /// Graceful shutdown timeout in seconds
    pub shutdown_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionAffinityConfig {
    /// Enable sticky sessions
    pub enabled: bool,
    /// Session cookie name
    pub cookie_name: String,
    /// Session header name (for load balancer)
    pub header_name: String,
    /// Session TTL in seconds
    pub ttl_secs: u64,
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            instance_id: uuid::Uuid::new_v4().to_string(),
            redis_url: None,
            cluster_name: "agentic-repos-default".to_string(),
            heartbeat_interval_secs: 5,
            instance_timeout_secs: 30,
            leader_election_timeout_secs: 10,
            session: SessionAffinityConfig::default(),
            shutdown_timeout_secs: 30,
        }
    }
}

impl Default for SessionAffinityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cookie_name: "AGENTIC_REPOS_INSTANCE".to_string(),
            header_name: "X-Repos-MCP-Instance".to_string(),
            ttl_secs: 3600,
        }
    }
}

impl ScalingConfig {
    pub fn from_env() -> Self {
        let enabled = std::env::var("SCALING_ENABLED")
            .map(|v| v == "true")
            .unwrap_or(false);

        Self {
            enabled,
            instance_id: std::env::var("SCALING_INSTANCE_ID")
                .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()),
            redis_url: std::env::var("SCALING_REDIS_URL").ok(),
            cluster_name: std::env::var("SCALING_CLUSTER_NAME")
                .unwrap_or_else(|_| "agentic-repos-default".to_string()),
            heartbeat_interval_secs: std::env::var("SCALING_HEARTBEAT_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            instance_timeout_secs: std::env::var("SCALING_INSTANCE_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            leader_election_timeout_secs: std::env::var("SCALING_LEADER_ELECTION_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            session: SessionAffinityConfig::from_env(),
            shutdown_timeout_secs: std::env::var("SCALING_SHUTDOWN_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        }
    }
}

impl SessionAffinityConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("SESSION_AFFINITY_ENABLED")
                .map(|v| v == "true")
                .unwrap_or(false),
            cookie_name: std::env::var("SESSION_AFFINITY_COOKIE_NAME")
                .unwrap_or_else(|_| "AGENTIC_REPOS_INSTANCE".to_string()),
            header_name: std::env::var("SESSION_AFFINITY_HEADER_NAME")
                .unwrap_or_else(|_| "X-Repos-MCP-Instance".to_string()),
            ttl_secs: std::env::var("SESSION_AFFINITY_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
        }
    }
}

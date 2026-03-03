//! Rate limiting configuration

use serde::{Deserialize, Serialize};

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub per_user: PerUserLimits,
    pub per_operation: PerOperationLimits,
    pub global: GlobalLimits,
}

/// Per-user rate limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerUserLimits {
    pub api_calls_per_minute: u32,
    pub concurrent_connections: u32,
    pub data_transfer_mb_per_hour: u32,
}

/// Per-operation rate limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerOperationLimits {
    pub clone_timeout_secs: u32,
    pub push_timeout_secs: u32,
}

/// Global rate limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalLimits {
    pub max_concurrent_users: u32,
    pub max_data_transfer_mb_per_hour: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            per_user: PerUserLimits {
                api_calls_per_minute: 1000,
                concurrent_connections: 10,
                data_transfer_mb_per_hour: 1000,
            },
            per_operation: PerOperationLimits {
                clone_timeout_secs: 300,
                push_timeout_secs: 300,
            },
            global: GlobalLimits {
                max_concurrent_users: 1000,
                max_data_transfer_mb_per_hour: 100000,
            },
        }
    }
}

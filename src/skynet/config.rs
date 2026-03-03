//! Configuration for Skynet v2 Git Operations Plugin

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Enable/disable plugin
    pub enabled: bool,

    /// Debug mode
    #[serde(default)]
    pub debug: bool,

    /// API timeout in seconds
    pub api_timeout_secs: u64,

    /// Health check interval in seconds
    pub health_check_interval_secs: u64,

    /// Custom configuration values
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debug: false,
            api_timeout_secs: 30,
            health_check_interval_secs: 30,
            custom: HashMap::new(),
        }
    }
}

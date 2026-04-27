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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = PluginConfig::default();
        assert!(cfg.enabled);
        assert!(!cfg.debug);
        assert_eq!(cfg.api_timeout_secs, 30);
        assert_eq!(cfg.health_check_interval_secs, 30);
        assert!(cfg.custom.is_empty());
    }

    #[test]
    fn serialization_roundtrip() {
        let cfg = PluginConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: PluginConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enabled, cfg.enabled);
        assert_eq!(parsed.api_timeout_secs, cfg.api_timeout_secs);
    }

    #[test]
    fn custom_values_preserved() {
        let mut cfg = PluginConfig::default();
        cfg.custom.insert("key".to_string(), serde_json::json!("value"));
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: PluginConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.custom["key"], "value");
    }
}

//! ADP per-repo and server configuration types.
//!
//! - `RepoConfig` is stored at `refs/adp/config/repo` in each hosted
//!   repository.
//! - `ServerConfig` is stored at `refs/adp/config/server` in the system
//!   repo (`_adp/system`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-repository general settings stored at `refs/adp/config/repo`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    /// The default branch name (e.g. `main`).
    #[serde(default = "default_branch")]
    pub default_branch: String,

    /// Optional human-readable description of the repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Per-repo feature flag overrides.  Values are `true` to enable or
    /// `false` to disable a flag, overriding the server default.
    #[serde(default)]
    pub feature_flags: HashMap<String, bool>,
}

fn default_branch() -> String {
    "main".to_string()
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            default_branch: default_branch(),
            description: None,
            feature_flags: HashMap::new(),
        }
    }
}

/// Global server settings stored at `refs/adp/config/server` in the system
/// repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// HTTP API port (default: `8090`).
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    /// SSH server port (default: `2222`).
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,

    /// Storage backend identifier (e.g. `"local"`, `"s3"`).
    #[serde(default = "default_storage_backend")]
    pub storage_backend: String,

    /// Server-wide feature flag settings.
    #[serde(default)]
    pub feature_flags: HashMap<String, bool>,
}

fn default_http_port() -> u16 {
    8090
}

fn default_ssh_port() -> u16 {
    2222
}

fn default_storage_backend() -> String {
    "local".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            ssh_port: default_ssh_port(),
            storage_backend: default_storage_backend(),
            feature_flags: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_defaults() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.http_port, 8090);
        assert_eq!(cfg.ssh_port, 2222);
        assert_eq!(cfg.storage_backend, "local");
    }

    #[test]
    fn test_repo_config_defaults() {
        let cfg = RepoConfig::default();
        assert_eq!(cfg.default_branch, "main");
        assert!(cfg.description.is_none());
        assert!(cfg.feature_flags.is_empty());
    }

    #[test]
    fn test_server_config_round_trip() {
        let cfg = ServerConfig {
            http_port: 9000,
            ssh_port: 2222,
            storage_backend: "s3".to_string(),
            feature_flags: {
                let mut m = HashMap::new();
                m.insert("webhooks".to_string(), true);
                m
            },
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let decoded: ServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.http_port, 9000);
        assert_eq!(decoded.storage_backend, "s3");
        assert_eq!(decoded.feature_flags.get("webhooks"), Some(&true));
    }
}

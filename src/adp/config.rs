//! ADP per-repo and server configuration types.
//!
//! - `ServerConfig` is stored at `refs/adp/config/server` in the system
//!   repo (`_adp/system`). It is the sole source of truth for all server
//!   settings — no external config files.
//! - `RepoConfig` is stored at `refs/adp/config/repo` in each hosted
//!   repository.
//!
//! ## Bootstrap
//!
//! The server needs exactly ONE piece of information before the system repo
//! is available: the storage root path (where `_adp/system/` lives).
//! This comes from `AGENTIC_REPOS_STORAGE_ROOT` env var or `--storage` CLI flag.
//! Everything else reads from git refs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ── Per-repo config ──

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

// ── Server config ──

/// Global server settings stored at `refs/adp/config/server` in the system
/// repo. This is the single source of truth — replaces legacy config.yaml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    // ── HTTP ──
    /// HTTP bind address (default: `"127.0.0.1"`).
    #[serde(default = "default_http_host")]
    pub http_host: String,

    /// HTTP API port (default: `8090`).
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    // ── SSH ──
    /// SSH server port (default: `2222`).
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,

    /// Path to SSH host key file (default: `"ssh_host_key"`).
    #[serde(default = "default_host_key_path")]
    pub ssh_host_key_path: PathBuf,

    /// Path to authorized_keys file (default: `"authorized_keys"`).
    #[serde(default = "default_authorized_keys_path")]
    pub ssh_authorized_keys_path: PathBuf,

    /// Maximum concurrent SSH connections (default: `100`).
    #[serde(default = "default_max_connections")]
    pub ssh_max_connections: usize,

    /// SSH connection timeout in seconds (default: `30`).
    #[serde(default = "default_connection_timeout")]
    pub ssh_connection_timeout_secs: u64,

    // ── Storage ──
    /// Storage backend identifier (e.g. `"local"`, `"s3"`).
    #[serde(default = "default_storage_backend")]
    pub storage_backend: String,

    /// Local storage root path. When using `"local"` backend, this is where
    /// repositories are stored. The system repo lives at `{storage_path}/_adp/system`.
    /// Default: `"./repositories"`.
    #[serde(default = "default_storage_path")]
    pub storage_path: PathBuf,

    /// S3 bucket name (required when storage_backend is `"s3"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_bucket: Option<String>,

    /// S3 region (default: `"us-east-1"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_region: Option<String>,

    /// S3 custom endpoint (for MinIO, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_endpoint: Option<String>,

    // ── Rate limiting ──
    /// Enable rate limiting (default: `false`).
    #[serde(default)]
    pub rate_limit_enabled: bool,

    /// Requests per minute per IP (default: `60`).
    #[serde(default = "default_rate_limit_rpm")]
    pub rate_limit_requests_per_minute: u32,

    /// Burst size (default: `20`).
    #[serde(default = "default_rate_limit_burst")]
    pub rate_limit_burst: u32,

    // ── Feature flags ──
    /// Server-wide feature flag settings.
    #[serde(default)]
    pub feature_flags: HashMap<String, bool>,
}

fn default_http_host() -> String {
    "127.0.0.1".to_string()
}

fn default_http_port() -> u16 {
    8090
}

fn default_ssh_port() -> u16 {
    2222
}

fn default_host_key_path() -> PathBuf {
    PathBuf::from("ssh_host_key")
}

fn default_authorized_keys_path() -> PathBuf {
    PathBuf::from("authorized_keys")
}

fn default_max_connections() -> usize {
    100
}

fn default_connection_timeout() -> u64 {
    30
}

fn default_storage_backend() -> String {
    "local".to_string()
}

fn default_storage_path() -> PathBuf {
    PathBuf::from("./repositories")
}

fn default_rate_limit_rpm() -> u32 {
    60
}

fn default_rate_limit_burst() -> u32 {
    20
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            http_host: default_http_host(),
            http_port: default_http_port(),
            ssh_port: default_ssh_port(),
            ssh_host_key_path: default_host_key_path(),
            ssh_authorized_keys_path: default_authorized_keys_path(),
            ssh_max_connections: default_max_connections(),
            ssh_connection_timeout_secs: default_connection_timeout(),
            storage_backend: default_storage_backend(),
            storage_path: default_storage_path(),
            s3_bucket: None,
            s3_region: None,
            s3_endpoint: None,
            rate_limit_enabled: false,
            rate_limit_requests_per_minute: default_rate_limit_rpm(),
            rate_limit_burst: default_rate_limit_burst(),
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
        assert_eq!(cfg.http_host, "127.0.0.1");
        assert_eq!(cfg.http_port, 8090);
        assert_eq!(cfg.ssh_port, 2222);
        assert_eq!(cfg.storage_backend, "local");
        assert_eq!(cfg.storage_path, PathBuf::from("./repositories"));
        assert_eq!(cfg.ssh_max_connections, 100);
        assert_eq!(cfg.ssh_connection_timeout_secs, 30);
        assert!(!cfg.rate_limit_enabled);
        assert_eq!(cfg.rate_limit_requests_per_minute, 60);
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
            http_host: "0.0.0.0".to_string(),
            http_port: 9000,
            ssh_port: 2222,
            ssh_host_key_path: PathBuf::from("/etc/ssh/host_key"),
            ssh_authorized_keys_path: PathBuf::from("/etc/ssh/authorized_keys"),
            ssh_max_connections: 200,
            ssh_connection_timeout_secs: 60,
            storage_backend: "s3".to_string(),
            storage_path: PathBuf::from("/data"),
            s3_bucket: Some("my-bucket".to_string()),
            s3_region: Some("eu-west-1".to_string()),
            s3_endpoint: Some("https://s3.example.com".to_string()),
            rate_limit_enabled: true,
            rate_limit_requests_per_minute: 120,
            rate_limit_burst: 50,
            feature_flags: {
                let mut m = HashMap::new();
                m.insert("webhooks".to_string(), true);
                m
            },
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let decoded: ServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.http_host, "0.0.0.0");
        assert_eq!(decoded.http_port, 9000);
        assert_eq!(decoded.storage_backend, "s3");
        assert_eq!(decoded.s3_bucket, Some("my-bucket".to_string()));
        assert_eq!(decoded.feature_flags.get("webhooks"), Some(&true));
        assert!(decoded.rate_limit_enabled);
    }

    #[test]
    fn test_server_config_deserialize_partial() {
        // Only override some fields — rest should get defaults
        let json = r#"{"http_port": 9090, "ssh_port": 2223}"#;
        let cfg: ServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.http_port, 9090);
        assert_eq!(cfg.ssh_port, 2223);
        // Defaults:
        assert_eq!(cfg.http_host, "127.0.0.1");
        assert_eq!(cfg.storage_backend, "local");
        assert_eq!(cfg.ssh_max_connections, 100);
    }
}

pub mod auth;
pub mod branches;
pub mod command;
pub mod compliance;
pub mod error;
pub mod protocol;
pub mod rate_limit;
pub mod retention;
pub mod ssh; // Re-enabled SSH module
pub mod storage;

use crate::storage::StorageBackend;
use std::sync::Arc;

pub struct GitService {
    storage: Arc<dyn StorageBackend>,
    config: GitConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitConfig {
    pub ssh_port: u16,
    pub host_key_path: std::path::PathBuf,
    pub authorized_keys_path: std::path::PathBuf,
    pub max_connections: usize,
    pub connection_timeout_secs: u64,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            ssh_port: 2222,
            host_key_path: std::path::PathBuf::from("host_key"),
            authorized_keys_path: std::path::PathBuf::from("authorized_keys"),
            max_connections: 100,
            connection_timeout_secs: 30,
        }
    }
}

impl GitService {
    pub fn new(storage: Arc<dyn StorageBackend>, config: Option<GitConfig>) -> Self {
        Self {
            storage,
            config: config.unwrap_or_default(),
        }
    }
}

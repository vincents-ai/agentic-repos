//! System repo bootstrap.
//!
//! Creates the `_adp/system` bare git repository under `storage_root` if it
//! does not already exist, and populates it with default ADP refs:
//!
//! - `refs/adp/config/server` — default `ServerConfig`
//! - `refs/adp/access/config` — default `AccessConfig` (no owners yet)
//!
//! This function is **idempotent**: calling it on an already-bootstrapped
//! system repo is a no-op.

use crate::adp::access::AccessConfig;
use crate::adp::config::ServerConfig;
use crate::adp::storage::{AdpStore, AdpStoreError};
use crate::adp::SERVER_CONFIG_REF;
use git2::Repository;
use std::path::Path;
use tracing::{debug, info, warn};

/// Errors produced during bootstrap.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("git2 error: {0}")]
    Git(#[from] git2::Error),

    #[error("ADP store error: {0}")]
    Store(#[from] AdpStoreError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BootstrapError>;

/// The path of the system repo relative to the storage root.
pub const SYSTEM_REPO_PATH: &str = "_adp/system";

/// Bootstrap the `_adp/system` bare git repository.
///
/// Safe to call on every server start — it is idempotent.
pub fn bootstrap_system_repo(storage_root: &Path) -> Result<()> {
    let system_repo_path = storage_root.join(SYSTEM_REPO_PATH);

    // Create parent directories if needed
    if let Some(parent) = system_repo_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Init the bare repo (idempotent — no-op if it already exists)
    if system_repo_path.exists() {
        // Verify it's a valid git repo; if so, skip initialisation
        match Repository::open(&system_repo_path) {
            Ok(_) => {
                debug!(
                    "System repo already exists at {:?}, skipping init",
                    system_repo_path
                );
            }
            Err(e) => {
                warn!(
                    "System repo at {:?} could not be opened ({}), re-initialising",
                    system_repo_path, e
                );
                Repository::init_bare(&system_repo_path)?;
            }
        }
    } else {
        info!("Bootstrapping system repo at {:?}", system_repo_path);
        Repository::init_bare(&system_repo_path)?;
    }

    // Write default server config if not already present
    if AdpStore::read_ref_blob(&system_repo_path, SERVER_CONFIG_REF).is_err() {
        info!("Writing default refs/adp/config/server");
        let server_cfg = ServerConfig::default();
        let value = serde_json::to_value(&server_cfg).map_err(AdpStoreError::Json)?;
        AdpStore::write_ref_blob(&system_repo_path, SERVER_CONFIG_REF, &value)?;
    } else {
        debug!("refs/adp/config/server already exists, skipping");
    }

    // Write default access config if not already present
    if AdpStore::read_access_config(&system_repo_path)?.is_none() {
        info!("Writing default refs/adp/access/config");
        let access_cfg = AccessConfig::default();
        AdpStore::write_access_config(&system_repo_path, &access_cfg)?;
    } else {
        debug!("refs/adp/access/config already exists, skipping");
    }

    info!("System repo bootstrap complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bootstrap_creates_system_repo() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);
        assert!(system_path.exists());
        assert!(Repository::open(&system_path).is_ok());
    }

    #[test]
    fn test_bootstrap_writes_default_refs() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);

        let server_cfg = AdpStore::read_ref_blob(&system_path, SERVER_CONFIG_REF).unwrap();
        assert!(server_cfg.is_object());

        let access_cfg = AdpStore::read_access_config(&system_path).unwrap();
        assert!(access_cfg.is_some());
    }

    #[test]
    fn test_bootstrap_is_idempotent() {
        let dir = TempDir::new().unwrap();

        // First call creates everything
        bootstrap_system_repo(dir.path()).unwrap();

        // Second call must not panic or error
        bootstrap_system_repo(dir.path()).unwrap();

        // Values should still be readable
        let system_path = dir.path().join(SYSTEM_REPO_PATH);
        assert!(AdpStore::read_ref_blob(&system_path, SERVER_CONFIG_REF).is_ok());
    }
}

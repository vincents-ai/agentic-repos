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
use crate::adp::lfs_config::{LfsConfig, LFS_CONFIG_REF};
use crate::adp::storage::{AdpStore, AdpStoreError};
use crate::adp::SERVER_CONFIG_REF;
use std::path::Path;
use tracing::{debug, info, warn};

/// Errors produced during bootstrap.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("gix error: {0}")]
    Git(String),

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
        match gix::open(&system_repo_path) {
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
                gix::init_bare(&system_repo_path)
                    .map_err(|e| BootstrapError::Git(e.to_string()))?;
            }
        }
    } else {
        info!("Bootstrapping system repo at {:?}", system_repo_path);
        gix::init_bare(&system_repo_path).map_err(|e| BootstrapError::Git(e.to_string()))?;
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

    // Write default LFS config if not already present
    if AdpStore::read_ref_blob(&system_repo_path, LFS_CONFIG_REF).is_err() {
        info!("Writing default refs/adp/config/lfs");
        let lfs_cfg = LfsConfig::default();
        let value = serde_json::to_value(&lfs_cfg).map_err(AdpStoreError::Json)?;
        AdpStore::write_ref_blob(&system_repo_path, LFS_CONFIG_REF, &value)?;
    } else {
        debug!("refs/adp/config/lfs already exists, skipping");
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

/// Register an initial admin SSH key in the system repo.
///
/// This is called by `agentic-repos init --admin-key <key>` after
/// bootstrapping. It:
/// 1. Parses the OpenSSH public key string to compute the fingerprint
/// 2. Writes identity refs (key→username, user→identity, ssh_keys)
/// 3. Adds "admin" to the system repo's access config owners list
pub fn register_admin_key(
    storage_root: &Path,
    admin_username: &str,
    public_key_openssh: &str,
) -> Result<()> {
    let system_repo_path = storage_root.join(SYSTEM_REPO_PATH);

    // Compute fingerprint hex from the openssh key
    let fingerprint_hex = compute_fingerprint_hex_from_openssh(public_key_openssh)
        .map_err(|e| BootstrapError::Git(format!("Failed to parse admin key: {}", e)))?;

    info!(
        "Registering admin key for '{}' with fingerprint {}",
        admin_username, fingerprint_hex
    );

    // Write identity refs
    AdpStore::register_user_key(
        &system_repo_path,
        admin_username,
        &fingerprint_hex,
        public_key_openssh,
    )?;

    // Update access config to add admin user as owner
    let mut access_config = AdpStore::read_access_config(&system_repo_path)?
        .unwrap_or_default();

    if !access_config.owners.contains(&admin_username.to_string()) {
        access_config.owners.push(admin_username.to_string());
        AdpStore::write_access_config(&system_repo_path, &access_config)?;
        info!("Added '{}' as system repo owner", admin_username);
    }

    info!("Admin key registration complete");
    Ok(())
}

/// Default path for the root AGE wrapping key.
pub const DEFAULT_ROOT_KEY_PATH: &str = "/etc/adp/wrapping-key.age";

/// Ref for the root public key in the system repo.
pub const ROOT_PUBLIC_KEY_REF: &str = "refs/adp/config/public-key";

/// Bootstrap the root AGE wrapping key.
///
/// Generates a new AGE identity if the key file doesn't exist, writes it to
/// `root_key_path`, and stores the public key in the system repo at
/// `refs/adp/config/public-key`.
///
/// Returns the path to the root key file.
pub fn bootstrap_root_key(
    storage_root: &Path,
    root_key_path: &Path,
) -> Result<std::path::PathBuf> {
    use age::secrecy::ExposeSecret;
    use age::x25519::Identity;

    let system_repo_path = storage_root.join(SYSTEM_REPO_PATH);

    if root_key_path.exists() {
        info!("Root AGE key already exists at {:?}", root_key_path);
    } else {
        info!("Generating root AGE wrapping key at {:?}", root_key_path);

        // Create parent directory
        if let Some(parent) = root_key_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let identity = Identity::generate();
        let identity_str = identity.to_string();
        std::fs::write(root_key_path, identity_str.expose_secret())?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(root_key_path, perms)?;
        }

        info!("Root AGE key generated successfully");
    }

    // Read the key and derive public key
    let key_str = std::fs::read_to_string(root_key_path)?;
    let identity: Identity = key_str
        .trim()
        .parse()
        .map_err(|e| BootstrapError::Git(format!("Failed to parse root AGE key: {}", e)))?;
    let recipient = identity.to_public();
    let public_key_str = recipient.to_string();

    // Store public key in system repo
    let public_key_value = serde_json::json!({
        "public_key": public_key_str,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    AdpStore::write_ref_blob(&system_repo_path, ROOT_PUBLIC_KEY_REF, &public_key_value)?;

    info!("Root public key stored in system repo: {}", public_key_str);
    Ok(root_key_path.to_path_buf())
}

/// Compute the SHA-256 hex fingerprint from an OpenSSH public key string.
fn compute_fingerprint_hex_from_openssh(openssh_key: &str) -> std::result::Result<String, String> {
    use russh::keys::ssh_key::HashAlg;

    let public_key = russh::keys::ssh_key::PublicKey::from_openssh(openssh_key)
        .map_err(|e| format!("Invalid OpenSSH key: {}", e))?;
    let fingerprint = public_key.fingerprint(HashAlg::Sha256);
    Ok(hex::encode(fingerprint.as_bytes()))
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
        assert!(gix::open(&system_path).is_ok());
    }

    #[test]
    fn test_bootstrap_writes_default_refs() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);

        let server_cfg = AdpStore::read_ref_blob(&system_path, SERVER_CONFIG_REF).unwrap();
        assert!(server_cfg.is_object());

        let lfs_cfg = AdpStore::read_ref_blob(&system_path, LFS_CONFIG_REF).unwrap();
        assert!(lfs_cfg.is_object());();
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

    #[test]
    fn test_register_admin_key() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        // Generate a test Ed25519 key
        use russh::keys::{Algorithm, PrivateKey};
        let private_key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        let openssh_key = private_key.public_key().to_openssh().unwrap();

        register_admin_key(dir.path(), "admin", &openssh_key).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);

        // Verify identity ref was created
        let identity = AdpStore::read_ref_blob(&system_path, "refs/adp/users/admin/identity").unwrap();
        assert_eq!(identity["username"].as_str().unwrap(), "admin");

        // Verify SSH keys array was created
        let keys = AdpStore::read_ref_blob(&system_path, "refs/adp/users/admin/ssh_keys").unwrap();
        assert_eq!(keys.as_array().unwrap().len(), 1);

        // Verify admin was added to access config owners
        let access_cfg = AdpStore::read_access_config(&system_path).unwrap().unwrap();
        assert!(access_cfg.owners.contains(&"admin".to_string()));
    }

    #[test]
    fn test_register_admin_key_with_custom_username() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        use russh::keys::{Algorithm, PrivateKey};
        let private_key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        let openssh_key = private_key.public_key().to_openssh().unwrap();

        register_admin_key(dir.path(), "alice", &openssh_key).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);
        let access_cfg = AdpStore::read_access_config(&system_path).unwrap().unwrap();
        assert!(access_cfg.owners.contains(&"alice".to_string()));
    }

    #[test]
    fn test_bootstrap_root_key_generates_and_stores() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        let key_path = dir.path().join("wrapping-key.age");
        assert!(!key_path.exists());

        bootstrap_root_key(dir.path(), &key_path).unwrap();

        // Key file should exist
        assert!(key_path.exists());

        // Key should be parseable as AGE identity
        let key_str = std::fs::read_to_string(&key_path).unwrap();
        let _: age::x25519::Identity = key_str.trim().parse().unwrap();

        // Public key should be in system repo
        let system_path = dir.path().join(SYSTEM_REPO_PATH);
        let pubkey_ref = AdpStore::read_ref_blob(&system_path, "refs/adp/config/public-key").unwrap();
        assert!(pubkey_ref["public_key"].is_string());
        assert!(pubkey_ref["public_key"].as_str().unwrap().starts_with("age1"));
    }

    #[test]
    fn test_bootstrap_root_key_idempotent() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        let key_path = dir.path().join("wrapping-key.age");
        bootstrap_root_key(dir.path(), &key_path).unwrap();
        let key1 = std::fs::read_to_string(&key_path).unwrap();

        // Second call should not regenerate
        bootstrap_root_key(dir.path(), &key_path).unwrap();
        let key2 = std::fs::read_to_string(&key_path).unwrap();

        assert_eq!(key1, key2, "root key should not change on second bootstrap");
    }
}

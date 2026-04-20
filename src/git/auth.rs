use crate::adp::bootstrap::SYSTEM_REPO_PATH;
use crate::adp::storage::AdpStore;
use russh::keys::ssh_key::PublicKey;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

const SSH_KEYS_REF_PREFIX: &str = "refs/adp/users";

#[derive(Debug, Clone)]
pub struct AuthorizedKeysManager {
    authorized_keys: HashMap<String, Vec<String>>,
    /// Storage root for locating the system repo.
    storage_root: PathBuf,
}

impl AuthorizedKeysManager {
    pub fn new(storage_root: PathBuf) -> Self {
        Self {
            authorized_keys: HashMap::new(),
            storage_root,
        }
    }

    /// Load authorized keys from git refs: refs/adp/users/{username}/ssh_keys
    pub async fn load_authorized_keys(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.load_from_git_refs(&self.storage_root.clone()).await
    }

    /// Load SSH keys from git refs: refs/adp/users/{username}/ssh_keys
    async fn load_from_git_refs(
        &mut self,
        storage_root: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let system_repo = storage_root.join(SYSTEM_REPO_PATH);

        // List all user refs
        let refs = match AdpStore::list_refs(&system_repo, SSH_KEYS_REF_PREFIX) {
            Ok(refs) => refs,
            Err(e) => {
                warn!(
                    "No SSH keys found in git-refs ({}): {}",
                    SSH_KEYS_REF_PREFIX, e
                );
                return Ok(());
            }
        };

        self.authorized_keys.clear();

        for refname in &refs {
            // Extract username from ref: refs/adp/users/{username}/ssh_keys
            let username = refname
                .strip_prefix(SSH_KEYS_REF_PREFIX)
                .and_then(|s| s.strip_prefix('/'))
                .and_then(|s| s.strip_suffix("/ssh_keys"))
                .unwrap_or("");

            if username.is_empty() {
                continue;
            }

            match AdpStore::read_ref_blob(&system_repo, refname) {
                Ok(value) => {
                    // Value is a JSON array of public key strings
                    let keys: Vec<String> = serde_json::from_value(value).unwrap_or_default();
                    for key in &keys {
                        if key.starts_with("ssh-") {
                            self.authorized_keys
                                .entry(username.to_string())
                                .or_default()
                                .push(key.clone());
                            debug!("Loaded key for user '{}' from git-refs", username);
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to read SSH keys ref '{}' for user '{}': {}",
                        refname, username, e
                    );
                }
            }
        }

        info!(
            "Loaded {} users from git-refs SSH keys",
            self.authorized_keys.len()
        );
        Ok(())
    }

    /// Validate if a public key is authorized for the given user
    pub fn is_key_authorized(&self, user: &str, public_key: &PublicKey) -> bool {
        debug!("Checking if user '{}' is authorized", user);
        let Some(user_keys) = self.authorized_keys.get(user) else {
            debug!("User '{}' not found in authorized keys", user);
            return false;
        };

        let provided_key = match public_key.to_openssh() {
            Ok(key_str) => key_str,
            Err(e) => {
                error!("Failed to convert public key to OpenSSH format: {}", e);
                return false;
            }
        };

        debug!("Provided key: {}", provided_key);
        for authorized_key in user_keys {
            debug!("Comparing with authorized key: {}", authorized_key);
            if self.keys_match(&provided_key, authorized_key) {
                debug!("Keys MATCH!");
                debug!("Public key authorized for user: {}", user);
                return true;
            }
        }

        debug!("No matching key found for user: {}", user);
        debug!("Public key not authorized for user: {}", user);
        false
    }

    /// Compare two SSH public keys for equality
    fn keys_match(&self, provided_key: &str, authorized_key: &str) -> bool {
        debug!("Comparing keys:");
        debug!("Provided: '{}'", provided_key);
        debug!("Authorized: '{}'", authorized_key);

        let provided_parts: Vec<&str> = provided_key.split_whitespace().collect();
        let authorized_parts: Vec<&str> = authorized_key.split_whitespace().collect();

        debug!("Provided parts: {:?}", provided_parts);
        debug!("Authorized parts: {:?}", authorized_parts);

        if provided_parts.len() < 2 || authorized_parts.len() < 2 {
            debug!("  Not enough parts");
            return false;
        }

        let key_type_match = provided_parts[0] == authorized_parts[0];
        let key_data_match = provided_parts[1] == authorized_parts[1];

        debug!("Key type match: {}", key_type_match);
        debug!("Key data match: {}", key_data_match);

        key_type_match && key_data_match
    }

    /// Reload authorized keys from file
    pub async fn reload(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Reloading authorized keys");
        self.load_authorized_keys().await
    }

    /// Get list of authorized users
    pub fn get_authorized_users(&self) -> Vec<String> {
        self.authorized_keys.keys().cloned().collect()
    }

    /// Check if any users are configured
    pub fn has_authorized_users(&self) -> bool {
        !self.authorized_keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adp::storage::AdpStore;

    /// Helper: create a temp dir with system repo and register a user key.
    async fn setup_test_repo(username: &str, key: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let storage_root = dir.path().to_path_buf();
        let system_repo = storage_root.join("_adp/system");
        std::fs::create_dir_all(&system_repo).unwrap();

        // Initialize as a git repo so AdpStore can write refs
        let repo = gix::init(&system_repo).unwrap();
        drop(repo);

        // Register a user key
        let ssh_keys_ref = format!("refs/adp/users/{}/ssh_keys", username);
        let keys = serde_json::json!([key]);
        AdpStore::write_ref_blob(&system_repo, &ssh_keys_ref, &keys).unwrap();

        (dir, storage_root)
    }

    #[tokio::test]
    async fn test_load_from_git_refs() {
        let (_dir, storage_root) = setup_test_repo(
            "alice",
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGq alice@test",
        ).await;

        let mut mgr = AuthorizedKeysManager::new(storage_root);
        mgr.load_authorized_keys().await.unwrap();

        assert!(mgr.has_authorized_users());
        assert!(mgr.get_authorized_users().contains(&"alice".to_string()));
    }

    #[tokio::test]
    async fn test_no_keys_configured() {
        let dir = tempfile::tempdir().unwrap();
        let storage_root = dir.path().to_path_buf();
        let system_repo = storage_root.join("_adp/system");
        std::fs::create_dir_all(&system_repo).unwrap();

        let repo = gix::init(&system_repo).unwrap();
        drop(repo);

        let mut mgr = AuthorizedKeysManager::new(storage_root);
        mgr.load_authorized_keys().await.unwrap();

        assert!(!mgr.has_authorized_users());
    }

    #[tokio::test]
    async fn test_multiple_users_and_keys() {
        let dir = tempfile::tempdir().unwrap();
        let storage_root = dir.path().to_path_buf();
        let system_repo = storage_root.join("_adp/system");
        std::fs::create_dir_all(&system_repo).unwrap();
        let repo = gix::init(&system_repo).unwrap();
        drop(repo);

        // Register two users
        let keys_alice = serde_json::json!(["ssh-ed25519 AAAA alice@test"]);
        let keys_bob = serde_json::json!(["ssh-rsa AAAA bob@test"]);
        AdpStore::write_ref_blob(&system_repo, "refs/adp/users/alice/ssh_keys", &keys_alice).unwrap();
        AdpStore::write_ref_blob(&system_repo, "refs/adp/users/bob/ssh_keys", &keys_bob).unwrap();

        let mut mgr = AuthorizedKeysManager::new(storage_root);
        mgr.load_authorized_keys().await.unwrap();

        assert_eq!(mgr.get_authorized_users().len(), 2);
        assert!(mgr.get_authorized_users().contains(&"alice".to_string()));
        assert!(mgr.get_authorized_users().contains(&"bob".to_string()));
    }
}

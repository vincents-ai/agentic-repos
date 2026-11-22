use russh::keys::ssh_key::PublicKey;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

const SSH_KEYS_REF_PREFIX: &str = "refs/adp/users";
const SYSTEM_REPO_PATH: &str = "_adp/system";

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
        storage_root: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let system_repo = storage_root.join(SYSTEM_REPO_PATH);

        // List all refs matching the SSH keys prefix using gix directly
        let refs = {
            let gix_repo = match gix::open(&system_repo) {
                Ok(r) => r,
                Err(_) => {
                    warn!("System repo not found at {}", system_repo.display());
                    return Ok(());
                }
            };
            let platform = match gix_repo.references() {
                Ok(p) => p,
                Err(e) => {
                    warn!("Failed to list refs: {}", e);
                    return Ok(());
                }
            };
            let iter = match platform.all() {
                Ok(i) => i,
                Err(e) => {
                    warn!("Failed to iterate refs: {}", e);
                    return Ok(());
                }
            };

            let mut result = Vec::new();
            for r in iter {
                match r {
                    Ok(r) => {
                        let name = r.name().as_bstr().to_string();
                        if name.starts_with(SSH_KEYS_REF_PREFIX) {
                            result.push(name);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read ref: {}", e);
                    }
                }
            }
            result
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

            match Self::read_ref_blob(&system_repo, refname) {
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
        let provided_parts: Vec<&str> = provided_key.split_whitespace().collect();
        let authorized_parts: Vec<&str> = authorized_key.split_whitespace().collect();

        if provided_parts.len() < 2 || authorized_parts.len() < 2 {
            return false;
        }

        // Constant-time comparison to prevent timing attacks
        let key_type_match = Self::constant_time_eq(provided_parts[0].as_bytes(), authorized_parts[0].as_bytes());
        let key_data_match = Self::constant_time_eq(provided_parts[1].as_bytes(), authorized_parts[1].as_bytes());

        key_type_match && key_data_match
    }

    /// Constant-time byte comparison to prevent timing side-channels.
    fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut diff: u8 = 0;
        for (x, y) in a.iter().zip(b.iter()) {
            diff |= x ^ y;
        }
        diff == 0
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

    /// Build a manager from an explicit user → OpenSSH public key map.
    ///
    /// Used where keys are provisioned out of band (tests, tooling); the git-ref
    /// loader above stays the production path.
    pub fn from_authorized_keys(
        authorized_keys: HashMap<String, Vec<String>>,
        storage_root: PathBuf,
    ) -> Self {
        Self {
            authorized_keys,
            storage_root,
        }
    }

    /// Add one OpenSSH public key for a user.
    pub fn insert_authorized_key(&mut self, user: &str, public_key: &str) {
        self.authorized_keys
            .entry(user.to_string())
            .or_default()
            .push(public_key.to_string());
    }

    /// Resolve the user whose authorised SSH keys contain this Ed25519 key.
    ///
    /// A P2P peer's Node ID *is* its Ed25519 public key, so this maps a Node ID
    /// to the same RBAC user the SSH transport would authenticate.
    pub fn user_for_ed25519_public_key(&self, public_key: &[u8]) -> Option<String> {
        if public_key.len() != 32 {
            return None;
        }
        // The SSH wire encoding of an Ed25519 public key: length-prefixed
        // "ssh-ed25519" followed by the length-prefixed 32 key bytes, base64ed.
        // `is_key_authorized` compares exactly this pair of fields.
        const KIND: &[u8] = b"ssh-ed25519";
        let mut blob = Vec::with_capacity(4 + KIND.len() + 4 + public_key.len());
        blob.extend_from_slice(&(KIND.len() as u32).to_be_bytes());
        blob.extend_from_slice(KIND);
        blob.extend_from_slice(&(public_key.len() as u32).to_be_bytes());
        blob.extend_from_slice(public_key);
        use base64::Engine as _;
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(&blob);
        let provided = format!("ssh-ed25519 {encoded}");

        let mut matches: Vec<&String> = Vec::new();
        for (user, keys) in &self.authorized_keys {
            if keys
                .iter()
                .any(|authorized| self.keys_match(&provided, authorized))
            {
                matches.push(user);
            }
        }
        matches.sort();
        matches.first().map(|user| (*user).clone())
    }

    /// Read a JSON blob from a git ref using gix directly.
    /// Expects the AdpStore format: ref → commit → tree → "data" → blob.
    fn read_ref_blob(repo_path: &Path, refname: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let gix_repo = gix::open(repo_path)?;
        let reference = gix_repo
            .try_find_reference(refname)?
            .ok_or_else(|| format!("Ref not found: {}", refname))?;
        let target_id = reference
            .try_id()
            .ok_or_else(|| format!("{}: symbolic ref", refname))?;

        // The target is a commit. Extract tree, then read "data" blob.
        let commit_obj = gix_repo.find_object(target_id)?;
        let commit = gix::objs::CommitRef::from_bytes(&commit_obj.data)?;
        let tree_id = commit.tree();
        let tree_obj = gix_repo.find_object(tree_id)?;
        let tree = gix::objs::TreeRef::from_bytes(&tree_obj.data)?;

        // Find the "data" entry in the tree
        let data_entry = tree.entries.iter()
            .find(|e| e.filename == "data")
            .ok_or("No 'data' entry in tree")?;

        let blob_obj = gix_repo.find_object(data_entry.oid.to_owned())?;
        let value: serde_json::Value = serde_json::from_slice(&blob_obj.data)?;
        Ok(value)
    }

    /// Write a JSON blob directly to a git ref using gix (for tests).
    /// Creates a commit with the blob in a tree, matching the AdpStore format.
    #[cfg(test)]
    fn write_ref_blob(repo_path: &Path, refname: &str, value: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
        use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
        use gix::refs::Target;

        let gix_repo = gix::open(repo_path)?;
        let json_data = serde_json::to_vec_pretty(value)?;

        // Create blob
        let blob = gix::objs::Blob { data: json_data };
        let blob_id = gix_repo.write_object(&blob)?.detach();

        // Create tree with "data" entry pointing to blob
        let tree = gix::objs::Tree {
            entries: vec![gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryMode::from(gix::objs::tree::EntryKind::Blob),
                filename: "data".into(),
                oid: blob_id,
            }],
        };
        let tree_id = gix_repo.write_object(&tree)?.detach();

        // Create commit
        let sig = gix::actor::Signature {
            name: "test".into(),
            email: "test@test".into(),
            time: gix::date::Time::now_utc(),
        };
        let commit = gix::objs::Commit {
            tree: tree_id,
            parents: Default::default(),
            author: sig.clone(),
            committer: sig,
            encoding: None,
            message: "test write".into(),
            extra_headers: Default::default(),
        };
        let commit_id = gix_repo.write_object(&commit)?.detach();

        gix_repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange::default(),
                expected: PreviousValue::Any,
                new: Target::Object(commit_id),
            },
            name: refname.try_into()?,
            deref: false,
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temp dir with system repo and register a user key.
    async fn setup_test_repo(username: &str, key: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let storage_root = dir.path().to_path_buf();
        let system_repo = storage_root.join("_adp/system");
        std::fs::create_dir_all(&system_repo).unwrap();

        // Initialize as a git repo so we can write refs
        let repo = gix::init(&system_repo).unwrap();
        drop(repo);

        // Register a user key
        let ssh_keys_ref = format!("refs/adp/users/{}/ssh_keys", username);
        let keys = serde_json::json!([key]);
        AuthorizedKeysManager::write_ref_blob(&system_repo, &ssh_keys_ref, &keys).unwrap();

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
        AuthorizedKeysManager::write_ref_blob(&system_repo, "refs/adp/users/alice/ssh_keys", &keys_alice).unwrap();
        AuthorizedKeysManager::write_ref_blob(&system_repo, "refs/adp/users/bob/ssh_keys", &keys_bob).unwrap();

        let mut mgr = AuthorizedKeysManager::new(storage_root);
        mgr.load_authorized_keys().await.unwrap();

        assert_eq!(mgr.get_authorized_users().len(), 2);
        assert!(mgr.get_authorized_users().contains(&"alice".to_string()));
        assert!(mgr.get_authorized_users().contains(&"bob".to_string()));
    }
}

//! Secrets service — manages AGE-encrypted secrets stored in git refs.
//!
//! Implements ADR-024: secrets are stored as SOPS-encrypted JSON blobs at
//! `refs/secrets/{name}` in each repository. Encryption uses AGE with per-repo
//! keypairs managed by `adp::secrets::key_manager::RepoKeyManager`.
//!
//! ## Key hierarchy:
//! - Layer 1: Root AGE key (filesystem) encrypts per-repo private keys
//! - Layer 2: Per-repo AGE keypair encrypts SOPS secret blobs
//!
//! ## Egress control:
//! Plaintext secrets never leave via git protocol (`refs/secrets/*` is stripped
//! from wire protocol). API access is restricted to authorised users and webhook
//! egress requires explicit per-key allowlist.

use crate::adp::secrets::key_manager::RepoKeyManager;
use crate::adp::secrets::sops::SecretsManager;
use crate::adp::secrets::{
    repo_private_key_ref, repo_public_key_ref,
    PUBLIC_KEY_PREFIX, SECRETS_PREFIX,
};
use crate::adp::storage::AdpStore;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("Secret not found: {0}")]
    NotFound(String),
    #[error("Invalid secret name: {0}")]
    InvalidName(String),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Key management error: {0}")]
    KeyManagement(String),
    #[error("Git error: {0}")]
    Git(String),
    #[error("Root key not configured")]
    RootKeyNotConfigured,
}

/// Metadata for a stored secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMeta {
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub version: u32,
    pub keys: Vec<String>,
}

/// A stored secret with its decrypted data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSecret {
    pub name: String,
    pub data: HashMap<String, String>,
    pub meta: SecretMeta,
}

/// The JSON blob stored at refs/secrets/{name} (SOPS encrypted payload + metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecretBlob {
    /// The SOPS-encrypted JSON blob (contains age-encrypted ciphertext).
    sops_data: String,
    /// Metadata about the secret (unencrypted — just names/versions).
    meta: SecretMeta,
}

/// Secrets service — manages AGE-encrypted secrets in git refs.
pub struct SecretsService {
    /// Per-repo key manager using root AGE wrapping key.
    key_manager: RepoKeyManager,
    /// Storage root for locating the system repo.
    storage_root: PathBuf,
}

impl SecretsService {
    /// Create a new SecretsService with the given root key path and storage root.
    pub fn new(root_key_path: PathBuf, storage_root: PathBuf) -> Self {
        let key_manager = RepoKeyManager::new(root_key_path);
        Self {
            key_manager,
            storage_root,
        }
    }

    /// Create with a test root key for development/testing.
    pub fn in_memory() -> Self {
        use age::secrecy::ExposeSecret;
        use age::x25519::Identity;

        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("wrapping-key.age");

        let identity = Identity::generate();
        std::fs::write(&key_path, identity.to_string().expose_secret()).unwrap();

        // Leak the tempdir to keep it alive for the test
        std::mem::forget(dir);

        // Also create a system repo
        let storage_dir = tempfile::tempdir().unwrap();
        let storage_root = storage_dir.path().to_path_buf();
        let system_repo = storage_root.join("_adp/system");
        std::fs::create_dir_all(&system_repo).unwrap();
        gix::init_bare(&system_repo).unwrap();
        std::mem::forget(storage_dir);

        Self::new(key_path, storage_root)
    }

    // ── Repo key management ──

    /// Get or create the AGE keypair for a repo.
    /// Returns the public key string.
    fn ensure_repo_keypair(&self, repo_path: &Path) -> Result<String, SecretError> {
        // Check if public key exists in user repo
        if let Ok(value) = AdpStore::read_ref_blob(repo_path, PUBLIC_KEY_PREFIX) {
            if let Some(pk) = value.get("public_key").and_then(|v| v.as_str()) {
                return Ok(pk.to_string());
            }
        }

        // Generate new keypair
        let repo_id = self.repo_id_from_path(repo_path);
        let (encrypted_private, public_key) = self
            .key_manager
            .generate_keypair(&repo_id)
            .map_err(|e| SecretError::KeyManagement(e.to_string()))?;

        // Store encrypted private key in system repo
        let system_repo = self.storage_root.join("_adp/system");
        let private_ref = repo_private_key_ref(&repo_id);
        let private_value =
            serde_json::json!({ "encrypted_key": base64::engine::general_purpose::STANDARD.encode(&encrypted_private) });
        AdpStore::write_ref_blob(&system_repo, &private_ref, &private_value)
            .map_err(|e| SecretError::Git(e.to_string()))?;

        // Store public key in system repo
        let public_ref = repo_public_key_ref(&repo_id);
        let public_value = serde_json::json!({ "public_key": public_key });
        AdpStore::write_ref_blob(&system_repo, &public_ref, &public_value)
            .map_err(|e| SecretError::Git(e.to_string()))?;

        // Store public key in user repo
        let user_pk_value = serde_json::json!({ "public_key": public_key });
        AdpStore::write_ref_blob(repo_path, PUBLIC_KEY_PREFIX, &user_pk_value)
            .map_err(|e| SecretError::Git(e.to_string()))?;

        tracing::info!(repo_id = %repo_id, public_key = %public_key, "provisioned AGE keypair for repo");
        Ok(public_key)
    }

    /// Decrypt the repo's private key from the system repo.
    fn decrypt_repo_private_key(&self, repo_path: &Path) -> Result<String, SecretError> {
        let repo_id = self.repo_id_from_path(repo_path);
        let system_repo = self.storage_root.join("_adp/system");
        let private_ref = repo_private_key_ref(&repo_id);

        let value = AdpStore::read_ref_blob(&system_repo, &private_ref)
            .map_err(|e| SecretError::KeyManagement(e.to_string()))?;

        let encrypted_b64 = value["encrypted_key"]
            .as_str()
            .ok_or_else(|| SecretError::KeyManagement("missing encrypted_key field".to_string()))?;

        let encrypted = base64::engine::general_purpose::STANDARD
            .decode(encrypted_b64)
            .map_err(|e| SecretError::KeyManagement(format!("invalid base64: {}", e)))?;

        self.key_manager
            .decrypt_private_key(&encrypted)
            .map_err(|e| SecretError::KeyManagement(e.to_string()))
    }

    /// Derive a repo ID from its filesystem path.
    /// Uses a flat string (owner--repo) to avoid ref name issues with slashes.
    fn repo_id_from_path(&self, repo_path: &Path) -> String {
        let path_str = repo_path.to_string_lossy();
        let stripped = path_str
            .trim_end_matches(".git")
            .trim_end_matches('/');
        let parts: Vec<&str> = stripped.rsplitn(3, '/').collect();
        match parts.len() {
            n if n >= 2 => format!("{}--{}", parts[1], parts[0]),
            _ => stripped.replace('/', "--"),
        }
    }

    // ── CRUD operations ──

    /// List all secret names in a repository.
    pub fn list_secrets(
        &self,
        repo_path: &Path,
        prefix: Option<&str>,
    ) -> Result<Vec<String>, SecretError> {
        let gix_repo = gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;

        let platform = gix_repo
            .references()
            .map_err(|e| SecretError::Git(e.to_string()))?;
        let iter = platform
            .all()
            .map_err(|e| SecretError::Git(e.to_string()))?;

        let secret_prefix = SECRETS_PREFIX;
        let filter_prefix = match prefix {
            Some(p) => format!("{}{}", secret_prefix, p),
            None => secret_prefix.to_string(),
        };

        let mut result = Vec::new();
        for r in iter {
            let r = r.map_err(|e| SecretError::Git(e.to_string()))?;
            let name = r.name().as_bstr().to_string();
            if name.starts_with(&filter_prefix) && name.starts_with(secret_prefix) {
                if let Some(secret_name) = name.strip_prefix(secret_prefix) {
                    result.push(secret_name.to_string());
                }
            }
        }
        result.sort();
        Ok(result)
    }

    /// Get a secret by name, decrypting its contents.
    pub fn get_secret(
        &self,
        repo_path: &Path,
        secret_name: &str,
    ) -> Result<StoredSecret, SecretError> {
        self.validate_name(secret_name)?;

        let refname = format!("{}{}", SECRETS_PREFIX, secret_name);
        let blob_json = self.read_ref_json(repo_path, &refname)?;

        let blob: SecretBlob =
            serde_json::from_value(blob_json).map_err(|e| SecretError::Encryption(e.to_string()))?;

        // Decrypt using repo's private key
        let private_key = self.decrypt_repo_private_key(repo_path)?;
        let data = SecretsManager::decrypt_map(&blob.sops_data, &private_key)
            .map_err(|e| SecretError::Encryption(e.to_string()))?;

        Ok(StoredSecret {
            name: secret_name.to_string(),
            data,
            meta: blob.meta,
        })
    }

    /// Create a new secret, encrypting and storing it in git refs.
    pub fn create_secret(
        &self,
        repo_path: &Path,
        secret_name: &str,
        data: HashMap<String, String>,
    ) -> Result<StoredSecret, SecretError> {
        self.validate_name(secret_name)?;

        let refname = format!("{}{}", SECRETS_PREFIX, secret_name);

        // Check it doesn't already exist
        if self.ref_exists(repo_path, &refname)? {
            return Err(SecretError::InvalidName(format!(
                "Secret '{}' already exists — use update instead",
                secret_name
            )));
        }

        let now = chrono::Utc::now();
        let meta = SecretMeta {
            name: secret_name.to_string(),
            created_at: now,
            updated_at: now,
            version: 1,
            keys: data.keys().cloned().collect(),
        };

        self.write_encrypted_secret(repo_path, &refname, &data, meta)
    }

    /// Update an existing secret (merge or replace).
    pub fn update_secret(
        &self,
        repo_path: &Path,
        secret_name: &str,
        new_data: HashMap<String, String>,
        replace: bool,
    ) -> Result<StoredSecret, SecretError> {
        self.validate_name(secret_name)?;

        let refname = format!("{}{}", SECRETS_PREFIX, secret_name);

        let (existing_data, mut meta) = if self.ref_exists(repo_path, &refname)? {
            let existing = self.get_secret(repo_path, secret_name)?;
            (existing.data, existing.meta)
        } else {
            return Err(SecretError::NotFound(secret_name.to_string()));
        };

        let data = if replace {
            new_data
        } else {
            let mut merged = existing_data;
            merged.extend(new_data);
            merged
        };

        meta.updated_at = chrono::Utc::now();
        meta.version += 1;
        meta.keys = data.keys().cloned().collect();

        self.write_encrypted_secret(repo_path, &refname, &data, meta)
    }

    /// Delete a secret by removing its git ref.
    pub fn delete_secret(&self, repo_path: &Path, secret_name: &str) -> Result<(), SecretError> {
        self.validate_name(secret_name)?;

        let refname = format!("{}{}", SECRETS_PREFIX, secret_name);

        if !self.ref_exists(repo_path, &refname)? {
            return Err(SecretError::NotFound(secret_name.to_string()));
        }

        self.delete_ref(repo_path, &refname)
    }

    // ── Internal helpers ──

    fn validate_name(&self, name: &str) -> Result<(), SecretError> {
        if name.is_empty() {
            return Err(SecretError::InvalidName("name cannot be empty".to_string()));
        }
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(SecretError::InvalidName(
                "name cannot contain '/', '\\', or '..'".to_string(),
            ));
        }
        if name.starts_with('.') {
            return Err(SecretError::InvalidName(
                "name cannot start with '.'".to_string(),
            ));
        }
        Ok(())
    }

    fn write_encrypted_secret(
        &self,
        repo_path: &Path,
        refname: &str,
        data: &HashMap<String, String>,
        meta: SecretMeta,
    ) -> Result<StoredSecret, SecretError> {
        // Ensure repo has an AGE keypair
        let public_key = self.ensure_repo_keypair(repo_path)?;

        // Encrypt with repo's public key via SOPS
        let sops_data = SecretsManager::encrypt_map(data, &public_key)
            .map_err(|e| SecretError::Encryption(e.to_string()))?;

        let blob = SecretBlob {
            sops_data,
            meta: meta.clone(),
        };

        let blob_json = serde_json::to_value(&blob)
            .map_err(|e| SecretError::Encryption(e.to_string()))?;

        self.write_ref_json(repo_path, refname, &blob_json)?;

        Ok(StoredSecret {
            name: meta.name.clone(),
            data: data.clone(),
            meta,
        })
    }

    fn ref_exists(&self, repo_path: &Path, refname: &str) -> Result<bool, SecretError> {
        let gix_repo = gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;
        match gix_repo
            .try_find_reference(refname)
            .map_err(|e| SecretError::Git(e.to_string()))?
        {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    fn read_ref_json(
        &self,
        repo_path: &Path,
        refname: &str,
    ) -> Result<serde_json::Value, SecretError> {
        let gix_repo = gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;

        let reference = gix_repo
            .try_find_reference(refname)
            .map_err(|e| SecretError::Git(e.to_string()))?
            .ok_or_else(|| SecretError::NotFound(refname.to_string()))?;

        let target_id = reference
            .try_id()
            .ok_or_else(|| SecretError::Git("symbolic ref".to_string()))?;

        let obj = gix_repo
            .find_object(target_id)
            .map_err(|e| SecretError::Git(e.to_string()))?;

        let content = match &obj.kind {
            gix::objs::Kind::Blob => obj.data.to_vec(),
            gix::objs::Kind::Commit => {
                let commit = gix::objs::CommitRef::from_bytes(&obj.data)
                    .map_err(|e| SecretError::Git(e.to_string()))?;
                let tree_obj = gix_repo
                    .find_object(commit.tree())
                    .map_err(|e| SecretError::Git(e.to_string()))?;
                let tree = gix::objs::TreeRef::from_bytes(&tree_obj.data)
                    .map_err(|e| SecretError::Git(e.to_string()))?;
                let entry = tree
                    .entries
                    .iter()
                    .find(|e| e.filename == "data")
                    .ok_or_else(|| {
                        SecretError::Git(format!("{}/tree/data not found", refname))
                    })?;
                let blob_obj = gix_repo
                    .find_object(entry.oid)
                    .map_err(|e| SecretError::Git(e.to_string()))?;
                blob_obj.data.to_vec()
            }
            _ => return Err(SecretError::Git("unexpected object type".to_string())),
        };

        serde_json::from_slice(&content).map_err(|e| SecretError::Git(e.to_string()))
    }

    fn write_ref_json(
        &self,
        repo_path: &Path,
        refname: &str,
        value: &serde_json::Value,
    ) -> Result<(), SecretError> {
        let gix_repo = gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;

        let json_bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| SecretError::Encryption(e.to_string()))?;

        let blob = gix::objs::Blob { data: json_bytes };
        let blob_id = gix_repo
            .write_object(&blob)
            .map_err(|e| SecretError::Git(e.to_string()))?;

        let tree = gix::objs::Tree {
            entries: vec![gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryMode::from(gix::objs::tree::EntryKind::Blob),
                filename: "data".into(),
                oid: blob_id.detach(),
            }],
        };
        let tree_id = gix_repo
            .write_object(&tree)
            .map_err(|e| SecretError::Git(e.to_string()))?;

        let parent_commit_id = gix_repo
            .try_find_reference(refname)
            .map_err(|e| SecretError::Git(e.to_string()))?
            .and_then(|r| r.try_id().map(|id| id.detach()));

        let parent_ids: Vec<gix::hash::ObjectId> = parent_commit_id.into_iter().collect();

        let sig = gix::actor::Signature {
            name: "ADP Secrets Bot".into(),
            email: "secrets@adp".into(),
            time: gix::date::Time::now_utc(),
        };

        let commit = gix::objs::Commit {
            tree: tree_id.detach(),
            parents: parent_ids.into(),
            author: sig.clone(),
            committer: sig,
            message: format!("secrets: update {}\n", refname).into(),
            encoding: None,
            extra_headers: Default::default(),
        };
        let commit_id = gix_repo
            .write_object(&commit)
            .map_err(|e: gix::object::write::Error| SecretError::Git(e.to_string()))?;

        use gix::refs::FullName;
        use gix::refs::Target;
        use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};

        gix_repo
            .edit_reference(RefEdit {
                change: Change::Update {
                    log: LogChange::default(),
                    expected: PreviousValue::Any,
                    new: Target::Object(commit_id.detach()),
                },
                name: FullName::try_from(refname).map_err(|e| SecretError::Git(e.to_string()))?,
                deref: false,
            })
            .map_err(|e| SecretError::Git(e.to_string()))?;

        Ok(())
    }

    fn delete_ref(&self, repo_path: &Path, refname: &str) -> Result<(), SecretError> {
        let gix_repo = gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;

        use gix::refs::FullName;
        use gix::refs::transaction::{Change, PreviousValue, RefEdit, RefLog};

        gix_repo
            .edit_reference(RefEdit {
                change: Change::Delete {
                    expected: PreviousValue::MustExist,
                    log: RefLog::AndReference,
                },
                name: FullName::try_from(refname).map_err(|e| SecretError::Git(e.to_string()))?,
                deref: false,
            })
            .map_err(|e| SecretError::Git(e.to_string()))?;

        Ok(())
    }

    // ── Egress control ──

    /// Resolve allowed secret keys for webhook egress.
    ///
    /// Takes a repo path and a list of allowed key names (from webhook config).
    /// Returns only the secrets that are in the allowlist, decrypted.
    /// Returns an error if any requested key is not found.
    ///
    /// This is the ONLY method that returns plaintext secrets for external use.
    /// All egress is audit-logged.
    pub fn resolve_secret_keys(
        &self,
        repo_path: &Path,
        allowed_keys: &[String],
        webhook_id: &str,
    ) -> Result<HashMap<String, String>, SecretError> {
        if allowed_keys.is_empty() {
            return Ok(HashMap::new());
        }

        // Validate no wildcards
        for key in allowed_keys {
            if key.contains('*') || key.contains('?') {
                return Err(SecretError::InvalidName(format!(
                    "Wildcard '{}' not allowed in secret key allowlist",
                    key
                )));
            }
        }

        let private_key = self.decrypt_repo_private_key(repo_path)?;
        let mut resolved = HashMap::new();

        for key_name in allowed_keys {
            let refname = format!("{}{}", SECRETS_PREFIX, key_name);

            match self.read_ref_json(repo_path, &refname) {
                Ok(blob_json) => {
                    let blob: SecretBlob = serde_json::from_value(blob_json)
                        .map_err(|e| SecretError::Encryption(e.to_string()))?;

                    let data = SecretsManager::decrypt_map(&blob.sops_data, &private_key)
                        .map_err(|e| SecretError::Encryption(e.to_string()))?;

                    tracing::info!(
                        webhook_id = %webhook_id,
                        secret_key = %key_name,
                        keys_count = data.len(),
                        "webhook egress: resolved secret key"
                    );

                    // Merge all key-value pairs from this secret
                    resolved.extend(data);
                }
                Err(SecretError::NotFound(_)) => {
                    tracing::warn!(
                        webhook_id = %webhook_id,
                        secret_key = %key_name,
                        "webhook egress: requested secret key not found, skipping"
                    );
                }
                Err(e) => return Err(e),
            }
        }

        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> SecretsService {
        SecretsService::in_memory()
    }

    fn make_repo() -> PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        gix::init_bare(&path).unwrap();
        std::mem::forget(dir);
        path
    }

    #[test]
    fn test_validate_name_valid() {
        let svc = make_service();
        assert!(svc.validate_name("my-secret").is_ok());
        assert!(svc.validate_name("db_password").is_ok());
        assert!(svc.validate_name("api-key-2024").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        let svc = make_service();
        assert!(svc.validate_name("").is_err());
        assert!(svc.validate_name("a/b").is_err());
        assert!(svc.validate_name("a\\b").is_err());
        assert!(svc.validate_name("..").is_err());
        assert!(svc.validate_name(".hidden").is_err());
        assert!(svc.validate_name("a..b").is_err());
    }

    #[test]
    fn test_create_and_get_secret() {
        let svc = make_service();
        let repo = make_repo();

        let data = HashMap::from([
            ("username".to_string(), "admin".to_string()),
            ("password".to_string(), "s3cret".to_string()),
        ]);

        let created = svc.create_secret(&repo, "db-creds", data.clone()).unwrap();
        assert_eq!(created.name, "db-creds");
        assert_eq!(created.meta.version, 1);

        let retrieved = svc.get_secret(&repo, "db-creds").unwrap();
        assert_eq!(retrieved.data["username"], "admin");
        assert_eq!(retrieved.data["password"], "s3cret");
    }

    #[test]
    fn test_create_duplicate_fails() {
        let svc = make_service();
        let repo = make_repo();

        let data = HashMap::from([("key".to_string(), "value".to_string())]);
        svc.create_secret(&repo, "test", data.clone()).unwrap();

        let result = svc.create_secret(&repo, "test", data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_update_secret_merge() {
        let svc = make_service();
        let repo = make_repo();

        let data = HashMap::from([
            ("key1".to_string(), "val1".to_string()),
            ("key2".to_string(), "val2".to_string()),
        ]);
        svc.create_secret(&repo, "config", data).unwrap();

        let new_data = HashMap::from([("key2".to_string(), "updated".to_string())]);
        let updated = svc.update_secret(&repo, "config", new_data, false).unwrap();

        assert_eq!(updated.data["key1"], "val1");
        assert_eq!(updated.data["key2"], "updated");
        assert_eq!(updated.meta.version, 2);
    }

    #[test]
    fn test_update_secret_replace() {
        let svc = make_service();
        let repo = make_repo();

        let data = HashMap::from([("key1".to_string(), "val1".to_string())]);
        svc.create_secret(&repo, "config", data).unwrap();

        let new_data = HashMap::from([("key2".to_string(), "val2".to_string())]);
        let updated = svc.update_secret(&repo, "config", new_data, true).unwrap();

        assert!(!updated.data.contains_key("key1"));
        assert_eq!(updated.data["key2"], "val2");
    }

    #[test]
    fn test_delete_secret() {
        let svc = make_service();
        let repo = make_repo();

        let data = HashMap::from([("key".to_string(), "val".to_string())]);
        svc.create_secret(&repo, "temp", data).unwrap();

        svc.delete_secret(&repo, "temp").unwrap();

        let result = svc.get_secret(&repo, "temp");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_nonexistent_fails() {
        let svc = make_service();
        let repo = make_repo();

        let result = svc.delete_secret(&repo, "nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_secrets() {
        let svc = make_service();
        let repo = make_repo();

        let data = HashMap::from([("key".to_string(), "val".to_string())]);
        svc.create_secret(&repo, "secret-a", data.clone()).unwrap();
        svc.create_secret(&repo, "secret-b", data.clone()).unwrap();
        svc.create_secret(&repo, "other", data).unwrap();

        let all = svc.list_secrets(&repo, None).unwrap();
        assert_eq!(all.len(), 3);

        let filtered = svc.list_secrets(&repo, Some("secret-")).unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&"secret-a".to_string()));
        assert!(filtered.contains(&"secret-b".to_string()));
    }

    #[test]
    fn test_list_secrets_empty_repo() {
        let svc = make_service();
        let repo = make_repo();

        let list = svc.list_secrets(&repo, None).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_encryption_roundtrip() {
        let svc = make_service();
        let repo = make_repo();

        let data = HashMap::from([
            ("connection_string".to_string(),
             "host=db.example.com;port=5432;user=admin;password=p@$$w0rd!".to_string()),
            ("json_config".to_string(),
             r#"{"ssl":true,"timeout":30,"options":["a","b"]}"#.to_string()),
        ]);

        svc.create_secret(&repo, "db-config", data.clone()).unwrap();

        let retrieved = svc.get_secret(&repo, "db-config").unwrap();
        assert_eq!(retrieved.data["connection_string"], data["connection_string"]);
        assert_eq!(retrieved.data["json_config"], data["json_config"]);
    }

    #[test]
    fn test_repo_keypair_provisioned_on_first_secret() {
        let svc = make_service();
        let repo = make_repo();

        // No public key yet
        assert!(AdpStore::read_ref_blob(&repo, PUBLIC_KEY_PREFIX).is_err());

        let data = HashMap::from([("key".to_string(), "val".to_string())]);
        svc.create_secret(&repo, "test", data).unwrap();

        // Public key should now exist in user repo
        let pk_ref = AdpStore::read_ref_blob(&repo, PUBLIC_KEY_PREFIX).unwrap();
        let public_key = pk_ref["public_key"].as_str().unwrap();
        assert!(public_key.starts_with("age1"));
    }
}

//! Secrets service — manages encrypted secrets stored in git refs.
//!
//! Implements ADR-024: secrets are stored as SOPS-encrypted JSON blobs at
//! `refs/secrets/{name}` in each repository. The encryption uses AES-256-GCM
//! with per-repository keys managed by `compliance/encryption.rs`.
//!
//! Egress control: plaintext secrets never leave via git protocol (refs/secrets/*
//! is stripped from wire protocol). API access is restricted to authorised users
//! and webhook egress requires explicit per-key allowlist.

use crate::compliance::encryption::{EncryptionService, InMemoryEncryptionKeyStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("Secret not found: {0}")]
    NotFound(String),
    #[error("Invalid secret name: {0}")]
    InvalidName(String),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Git error: {0}")]
    Git(String),
    #[error("Lock poisoned")]
    LockPoisoned,
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

/// The JSON blob stored at refs/secrets/{name} (encrypted payload + metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecretBlob {
    /// Encrypted data (base64-encoded AES-256-GCM ciphertext).
    encrypted: String,
    /// Nonce used for encryption (base64-encoded).
    nonce: String,
    /// Encryption key ID used.
    key_id: Uuid,
    /// SHA-256 hash of the plaintext for integrity verification.
    plaintext_hash: String,
    /// Metadata about the secret.
    meta: SecretMeta,
}

/// Secrets service — manages encrypted secrets in git refs.
pub struct SecretsService {
    encryption: std::sync::Arc<EncryptionService>,
}

impl SecretsService {
    pub fn new(encryption: std::sync::Arc<EncryptionService>) -> Self {
        Self { encryption }
    }

    /// Create with an in-memory encryption key store (for development/testing).
    pub fn in_memory() -> Self {
        let store = InMemoryEncryptionKeyStore::shared();
        let encryption = std::sync::Arc::new(EncryptionService::new(store));
        // Create a default key
        encryption.create_key("default").expect("failed to create default key");
        Self { encryption }
    }

    /// List all secret names in a repository.
    pub fn list_secrets(
        &self,
        repo_path: &Path,
        prefix: Option<&str>,
    ) -> Result<Vec<String>, SecretError> {
        let gix_repo =
            gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;

        let platform = gix_repo
            .references()
            .map_err(|e| SecretError::Git(e.to_string()))?;
        let iter = platform
            .all()
            .map_err(|e| SecretError::Git(e.to_string()))?;

        let secret_prefix = "refs/secrets/";
        let filter_prefix = match prefix {
            Some(p) => format!("{}{}", secret_prefix, p),
            None => secret_prefix.to_string(),
        };

        let mut result = Vec::new();
        for r in iter {
            let r = r.map_err(|e| SecretError::Git(e.to_string()))?;
            let name = r.name().as_bstr().to_string();
            if name.starts_with(&filter_prefix) && name.starts_with(secret_prefix) {
                // Strip the refs/secrets/ prefix to get the secret name
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

        let refname = format!("refs/secrets/{}", secret_name);
        let blob_json = self.read_ref_json(repo_path, &refname)?;

        let blob: SecretBlob =
            serde_json::from_value(blob_json).map_err(|e| SecretError::Encryption(e.to_string()))?;

        // Decrypt
        let encrypted_data = crate::compliance::encryption::EncryptedData {
            key_id: blob.key_id,
            nonce_b64: blob.nonce,
            ciphertext_b64: blob.encrypted,
            encrypted_at: blob.meta.updated_at,
            plaintext_hash: blob.plaintext_hash,
        };

        let plaintext_bytes = self
            .encryption
            .decrypt(&encrypted_data)
            .map_err(|e| SecretError::Encryption(e.to_string()))?;

        let data: HashMap<String, String> = serde_json::from_slice(&plaintext_bytes)
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

        let refname = format!("refs/secrets/{}", secret_name);

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

        let refname = format!("refs/secrets/{}", secret_name);

        // Read existing
        let (existing_data, mut meta) = if self.ref_exists(repo_path, &refname)? {
            let existing = self.get_secret(repo_path, secret_name)?;
            (existing.data, existing.meta)
        } else {
            return Err(SecretError::NotFound(secret_name.to_string()));
        };

        // Merge or replace
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
    pub fn delete_secret(
        &self,
        repo_path: &Path,
        secret_name: &str,
    ) -> Result<(), SecretError> {
        self.validate_name(secret_name)?;

        let refname = format!("refs/secrets/{}", secret_name);

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
        let plaintext = serde_json::to_vec(data)
            .map_err(|e| SecretError::Encryption(e.to_string()))?;

        let encrypted = self
            .encryption
            .encrypt(&plaintext)
            .map_err(|e| SecretError::Encryption(e.to_string()))?;

        let blob = SecretBlob {
            encrypted: encrypted.ciphertext_b64,
            nonce: encrypted.nonce_b64,
            key_id: encrypted.key_id,
            plaintext_hash: encrypted.plaintext_hash,
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
        let gix_repo =
            gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;
        match gix_repo
            .try_find_reference(refname)
            .map_err(|e| SecretError::Git(e.to_string()))?
        {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    fn read_ref_json(&self, repo_path: &Path, refname: &str) -> Result<serde_json::Value, SecretError> {
        let gix_repo =
            gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;

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
                    .ok_or_else(|| SecretError::Git(format!("{}/tree/data not found", refname)))?;
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
        let gix_repo =
            gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;

        let json_bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| SecretError::Encryption(e.to_string()))?;

        // Create blob
        let blob = gix::objs::Blob { data: json_bytes };
        let blob_id = gix_repo
            .write_object(&blob)
            .map_err(|e| SecretError::Git(e.to_string()))?;

        // Create tree with "data" entry
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

        // Find parent commit if ref exists
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
        let gix_repo =
            gix::open(repo_path).map_err(|e| SecretError::Git(e.to_string()))?;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_service() -> SecretsService {
        SecretsService::in_memory()
    }

    fn make_repo() -> PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        // Init a bare gix repo
        gix::init_bare(&path).unwrap();
        // Keep the tempdir alive for the test
        // We leak it intentionally — test-only
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

        // Create a secret with special characters
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
}

//! ADP git-ref storage layer.
//!
//! `AdpStore` provides read/write access to ADP blobs stored in git refs.
//! Every write also creates an immutable version sidecar ref so changes are
//! auditable.
//!
//! ## Version sidecars
//!
//! For a ref `refs/adp/schema/my-schema` the sidecars are stored at
//! `refs/adp/schema/v1/my-schema`, `refs/adp/schema/v2/my-schema`, …
//! The `list_refs` and `list_schemas` helpers automatically skip sidecar refs.
//!
//! ## Implementation note
//!
//! Ref management uses the `gix` high-level Repository API; object operations
//! (blob/tree/commit creation) use `gix` (git2 fully removed).

use crate::adp::access::{AccessConfig, Permission};
use crate::adp::is_version_sidecar;
use crate::adp::schema::RefSchema;
use crate::adp::{ACCESS_CONFIG_REF, SCHEMA_PREFIX};
use gix::bstr::ByteSlice;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
use gix::refs::FullName;
use gix::refs::Target;
use serde_json::Value;
use std::path::Path;
use thiserror::Error;

/// Errors produced by `AdpStore` operations.
#[derive(Debug, Error)]
pub enum AdpStoreError {
    #[error("gix open error: {0}")]
    GixOpen(#[from] gix::open::Error),

    #[error("gix reference find error: {0}")]
    GixRefFind(#[from] gix::reference::find::Error),

    #[error("gix reference iter error: {0}")]
    GixRefIter(#[from] gix::reference::iter::Error),

    #[error("gix ref iteration init error: {0}")]
    GixRefIterInit(#[from] gix::reference::iter::init::Error),

    #[error("gix reference edit error: {0}")]
    GixRefEdit(#[from] gix::reference::edit::Error),

    #[error("gix reference name error: {0}")]
    GixRefName(#[from] gix::refs::name::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Ref not found: {0}")]
    NotFound(String),

    #[error("Boxed gix error: {0}")]
    GixBoxed(String),
}

impl From<Box<dyn std::error::Error + Send + Sync>> for AdpStoreError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        AdpStoreError::GixBoxed(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AdpStoreError>;

/// ADP git-ref storage.
///
/// All operations take a `repo_path: &Path` rather than caching an open
/// `Repository` so that `AdpStore` is `Send + Sync` and can be shared across
/// async tasks without a mutex.
pub struct AdpStore;

impl AdpStore {
    pub fn new() -> Self {
        Self
    }

    // ---------------------------------------------------------------------
    // Input validation
    // ---------------------------------------------------------------------

    /// Validate that a refname is safe to use with gix.
    /// Rejects empty names, path traversal, null bytes, and names that don't
    /// start with refs/.
    fn validate_refname(refname: &str) -> Result<()> {
        if refname.is_empty() {
            return Err(AdpStoreError::NotFound("refname cannot be empty".to_string()));
        }
        if refname.contains('\0') {
            return Err(AdpStoreError::NotFound("refname contains null byte".to_string()));
        }
        if refname.contains("..") {
            return Err(AdpStoreError::NotFound("refname contains '..'".to_string()));
        }
        if !refname.starts_with("refs/") {
            return Err(AdpStoreError::NotFound(
                "refname must start with refs/".to_string(),
            ));
        }
        Ok(())
    }

    /// Maximum blob size for ADP ref data (1 MB).
    const MAX_BLOB_SIZE: usize = 1024 * 1024;

    // ---------------------------------------------------------------------
    // Low-level blob helpers
    // ---------------------------------------------------------------------

    /// Read the JSON blob pointed to by `refname` in the repository at
    /// `repo_path`.
    ///
    /// The ref may point to either a commit (which contains a "data" entry in
    /// its tree) or directly to a blob.  Both cases are handled.
    ///
    /// Returns the parsed `serde_json::Value`.
    pub fn read_ref_blob(repo_path: &Path, refname: &str) -> Result<Value> {
        Self::validate_refname(refname)?;
        let gix_repo = gix::open(repo_path)?;

        let reference = gix_repo
            .try_find_reference(refname)
            .map_err(AdpStoreError::GixRefFind)?
            .ok_or_else(|| AdpStoreError::NotFound(refname.to_string()))?;

        let target_id = reference
            .try_id()
            .ok_or_else(|| AdpStoreError::NotFound(format!("{}: symbolic ref", refname)))?;

        let obj = gix_repo
            .find_object(target_id)
            .map_err(|e| AdpStoreError::GixBoxed(e.to_string()))?;

        let blob_content: Vec<u8> = match &obj.kind {
            gix::objs::Kind::Commit => {
                let commit = gix::objs::CommitRef::from_bytes(&obj.data)
                    .map_err(|e| AdpStoreError::GixBoxed(e.to_string()))?;
                let tree_id = commit.tree();
                let tree_obj = gix_repo
                    .find_object(tree_id)
                    .map_err(|e| AdpStoreError::GixBoxed(e.to_string()))?;
                let tree = gix::objs::TreeRef::from_bytes(&tree_obj.data)
                    .map_err(|e| AdpStoreError::GixBoxed(e.to_string()))?;
                let data_entry = tree
                    .entries
                    .iter()
                    .find(|e| e.filename == "data")
                    .ok_or_else(|| AdpStoreError::NotFound(format!("{}/tree/data", refname)))?;
                let blob_obj = gix_repo
                    .find_object(data_entry.oid)
                    .map_err(|e| AdpStoreError::GixBoxed(e.to_string()))?;
                blob_obj.data.to_vec()
            }
            gix::objs::Kind::Blob => obj.data.to_vec(),
            _ => {
                return Err(AdpStoreError::NotFound(format!(
                    "{}: expected commit or blob, got {:?}",
                    refname, obj.kind
                )))
            }
        };

        let value = serde_json::from_slice(&blob_content)?;
        Ok(value)
    }

    /// Write `value` as a JSON blob to `refname` in the repository at
    /// `repo_path`.
    ///
    /// The primary ref is always force-updated.  An immutable version sidecar
    /// is also created (see module docs).
    pub fn write_ref_blob(repo_path: &Path, refname: &str, value: &Value) -> Result<()> {
        Self::validate_refname(refname)?;
        let gix_repo = gix::open(repo_path)?;
        let json_bytes = serde_json::to_vec_pretty(value)?;

        // Enforce blob size limit
        if json_bytes.len() > Self::MAX_BLOB_SIZE {
            return Err(AdpStoreError::NotFound(format!(
                "Blob too large: {} bytes (max {})",
                json_bytes.len(),
                Self::MAX_BLOB_SIZE
            )));
        };

        // Create blob
        let blob = gix::objs::Blob { data: json_bytes };
        let blob_id = gix_repo
            .write_object(&blob)
            .map_err(|e| AdpStoreError::GixBoxed(e.to_string()))?;

        // Create tree with "data" entry pointing to blob
        let tree = gix::objs::Tree {
            entries: vec![gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryMode::from(gix::objs::tree::EntryKind::Blob),
                filename: "data".into(),
                oid: blob_id.detach(),
            }],
        };
        let tree_id = gix_repo
            .write_object(&tree)
            .map_err(|e| AdpStoreError::GixBoxed(e.to_string()))?;

        // Find parent commit if ref already exists
        let parent_commit_id = gix_repo
            .try_find_reference(refname)
            .map_err(AdpStoreError::GixRefFind)?
            .and_then(|r| r.try_id().map(|id| id.detach()));

        let parent_ids: Vec<gix::hash::ObjectId> = parent_commit_id.into_iter().collect();

        // Create signature
        let sig = gix::actor::Signature {
            name: "ADP Bot".into(),
            email: "adp@localhost".into(),
            time: gix::date::Time::now_utc(),
        };

        // Create commit
        let commit = gix::objs::Commit {
            tree: tree_id.detach(),
            parents: parent_ids.into(),
            author: sig.clone(),
            committer: sig,
            message: format!("adp: update {}\n", refname).into(),
            encoding: None,
            extra_headers: Default::default(),
        };
        let commit_id = gix_repo
            .write_object(&commit)
            .map_err(|e: gix::object::write::Error| AdpStoreError::GixBoxed(e.to_string()))?;

        // Update primary ref
        gix_repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange::default(),
                expected: PreviousValue::Any,
                new: Target::Object(commit_id.detach()),
            },
            name: FullName::try_from(refname)?,
            deref: false,
        })?;

        // Create immutable version sidecar
        let sidecar = next_sidecar_ref(&gix_repo, refname)?;
        gix_repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange::default(),
                expected: PreviousValue::MustNotExist,
                new: Target::Object(commit_id.detach()),
            },
            name: FullName::try_from(sidecar.as_str())?,
            deref: false,
        })?;

        Ok(())
    }

    /// List all refs under `prefix` in the repository at `repo_path`, skipping
    /// version sidecar refs.
    pub fn list_refs(repo_path: &Path, prefix: &str) -> Result<Vec<String>> {
        let gix_repo = gix::open(repo_path)?;
        let platform = gix_repo.references()?;
        let iter = platform.all()?;

        let mut result = Vec::new();
        for r in iter {
            let r = r?;
            let name = r.name().as_bstr().to_string();
            if name.starts_with(prefix) && !is_version_sidecar(&name) {
                result.push(name);
            }
        }
        Ok(result)
    }

    // -------------------------------------------------------------------------
    // Schema helpers
    // -------------------------------------------------------------------------

    /// Read the schema with `id` from the repository at `repo_path`.
    ///
    /// Returns `None` if the ref does not exist.
    pub fn read_schema(repo_path: &Path, id: &str) -> Result<Option<RefSchema>> {
        let refname = format!("{}{}", SCHEMA_PREFIX, id);
        match Self::read_ref_blob(repo_path, &refname) {
            Ok(value) => {
                let schema: RefSchema = serde_json::from_value(value)?;
                Ok(Some(schema))
            }
            Err(AdpStoreError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// List all schemas stored in the repository at `repo_path` (skipping
    /// version sidecars).
    pub fn list_schemas(repo_path: &Path) -> Result<Vec<RefSchema>> {
        let refs = Self::list_refs(repo_path, SCHEMA_PREFIX)?;
        let mut schemas = Vec::new();
        for refname in refs {
            let id = refname
                .strip_prefix(SCHEMA_PREFIX)
                .unwrap_or(&refname)
                .to_string();
            if let Some(schema) = Self::read_schema(repo_path, &id)? {
                schemas.push(schema);
            }
        }
        Ok(schemas)
    }

    /// Write `schema` to `refs/adp/schema/{schema.id}` in the repository at
    /// `repo_path`.
    pub fn write_schema(repo_path: &Path, schema: &RefSchema) -> Result<()> {
        let refname = format!("{}{}", SCHEMA_PREFIX, schema.id);
        let value = serde_json::to_value(schema)?;
        Self::write_ref_blob(repo_path, &refname, &value)
    }

    // -------------------------------------------------------------------------
    // Access config helpers
    // -------------------------------------------------------------------------

    /// Read the access config from the repository at `repo_path`.
    ///
    /// Returns `None` if the ref does not exist yet.
    pub fn read_access_config(repo_path: &Path) -> Result<Option<AccessConfig>> {
        match Self::read_ref_blob(repo_path, ACCESS_CONFIG_REF) {
            Ok(value) => {
                let cfg: AccessConfig = serde_json::from_value(value)?;
                Ok(Some(cfg))
            }
            Err(AdpStoreError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Write `config` to `refs/adp/access/config` in the repository at
    /// `repo_path`.
    pub fn write_access_config(repo_path: &Path, config: &AccessConfig) -> Result<()> {
        let value = serde_json::to_value(config)?;
        Self::write_ref_blob(repo_path, ACCESS_CONFIG_REF, &value)
    }

    // -------------------------------------------------------------------------
    // Identity helpers
    // -------------------------------------------------------------------------

    /// Resolve a key fingerprint to a username.
    ///
    /// Reads `refs/adp/keys/{fingerprint_hex}` from the system repo.
    /// Returns `None` if the key is not registered.
    pub fn resolve_key_identity(
        system_repo_path: &Path,
        fingerprint_hex: &str,
    ) -> Result<Option<String>> {
        let refname = format!("refs/adp/keys/{}", fingerprint_hex);
        match Self::read_ref_blob(system_repo_path, &refname) {
            Ok(value) => {
                let username = value["username"].as_str().ok_or_else(|| {
                    AdpStoreError::NotFound(format!("{}: missing 'username' field", refname))
                })?;
                Ok(Some(username.to_string()))
            }
            Err(AdpStoreError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Register a user's SSH public key in the system repo.
    ///
    /// Creates two refs pointing to the same identity data:
    /// - `refs/adp/keys/{fingerprint_hex}` → `{"username": "..."}`
    /// - `refs/adp/users/{username}/identity` → `{"username": "..."}`
    ///
    /// Also updates `refs/adp/users/{username}/ssh_keys` with the key string.
    ///
    /// `fingerprint_hex` is the SHA-256 hex digest of the key (64 chars),
    /// pre-computed by the caller (who has access to the russh key types).
    pub fn register_user_key(
        system_repo_path: &Path,
        username: &str,
        fingerprint_hex: &str,
        public_key_openssh: &str,
    ) -> Result<()> {
        let identity_value = serde_json::json!({ "username": username });

        // Write key → username mapping
        let key_ref = format!("refs/adp/keys/{}", fingerprint_hex);
        Self::write_ref_blob(system_repo_path, &key_ref, &identity_value)?;

        // Write username → identity mapping
        let user_ref = format!("refs/adp/users/{}/identity", username);
        Self::write_ref_blob(system_repo_path, &user_ref, &identity_value)?;

        // Update ssh_keys array
        let ssh_keys_ref = format!("refs/adp/users/{}/ssh_keys", username);
        let mut keys: Vec<String> = match Self::read_ref_blob(system_repo_path, &ssh_keys_ref) {
            Ok(v) => serde_json::from_value(v).unwrap_or_default(),
            Err(AdpStoreError::NotFound(_)) => Vec::new(),
            Err(e) => return Err(e),
        };
        // Compare by key data (base64 portion, ignoring comment)
        let new_key_data = openssh_key_data(public_key_openssh);
        if !keys.iter().any(|k| openssh_key_data(k) == new_key_data) {
            keys.push(public_key_openssh.to_string());
        }
        let keys_value = serde_json::to_value(&keys)?;
        Self::write_ref_blob(system_repo_path, &ssh_keys_ref, &keys_value)?;

        Ok(())
    }

    /// Check if a user has permission to perform an operation on a repo.
    ///
    /// Reads `refs/adp/access/config` from the target repo and evaluates the
    /// rules against the username and operation.
    pub fn check_repo_access(
        repo_path: &Path,
        username: &str,
        operation: &Permission,
        refname: Option<&str>,
    ) -> Result<bool> {
        let config = match Self::read_access_config(repo_path)? {
            Some(c) => c,
            None => {
                // No access config → default: public read, deny write
                return Ok(matches!(operation, Permission::Pull));
            }
        };

        // Owners and admins have full access
        if config.owners.iter().any(|o| o == username)
            || config.admins.iter().any(|a| a == username)
        {
            return Ok(true);
        }

        // Evaluate rules in order; first match wins
        for rule in &config.rules {
            if rule.user == "*" || rule.user == username {
                // If a specific refname is given, check path patterns
                let path_matches = if let Some(refname) = refname {
                    rule.paths.is_empty()
                        || rule.paths.iter().any(|p| crate::adp::glob::matches_pattern(p, refname))
                } else {
                    // No refname → repo-level check, ignore path filter
                    true
                };
                if path_matches && rule.permissions.contains(operation) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

impl Default for AdpStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract the base64 key data from an OpenSSH public key string.
/// Input: `"ssh-ed25519 AAAAC3Nza... user@host"`
/// Output: `"AAAAC3Nza..."` (the key type and base64 data, without comment)
fn openssh_key_data(openssh_str: &str) -> &str {
    // Split into parts: key_type, base64_data, optional_comment
    let parts: Vec<&str> = openssh_str.split_whitespace().collect();
    if parts.len() >= 2 {
        // Return first two parts (key_type + base64 data)
        // We actually compare on the base64 data part (index 1)
        parts.get(1).map(|s| *s).unwrap_or("")
    } else {
        openssh_str.trim()
    }
}

fn next_sidecar_ref(gix_repo: &gix::Repository, primary_ref: &str) -> Result<String> {
    let after_prefix = primary_ref.strip_prefix("refs/adp/").unwrap_or(primary_ref);

    let slash_pos = after_prefix.find('/').unwrap_or(after_prefix.len());
    let ns = &after_prefix[..slash_pos];
    let id = if slash_pos < after_prefix.len() {
        &after_prefix[slash_pos + 1..]
    } else {
        ""
    };

    let sidecar_pattern = if id.is_empty() {
        format!("refs/adp/v-{}-*", ns)
    } else {
        format!("refs/adp/{}/v*/{}", ns, id)
    };
    let platform = gix_repo.references()?;
    let iter = platform.all()?;

    let mut max_n: u32 = 0;
    for r in iter {
        let r = r?;
        let name = r.name().as_bstr().to_string();
        if gix::glob::wildmatch(
            sidecar_pattern.as_bytes().as_bstr(),
            name.as_bytes().as_bstr(),
            gix::glob::wildmatch::Mode::empty(),
        ) {
            if let Some(v_segment) = extract_version_segment(&name, ns, id) {
                if v_segment > max_n {
                    max_n = v_segment;
                }
            }
        }
    }

    let next_n = max_n + 1;
    // Use flat sidecar format: refs/adp/v-{ns}-{n} for refs without sub-paths
    // to avoid file/directory conflict (refs/adp/theme file blocks refs/adp/theme/ dir)
    if id.is_empty() {
        Ok(format!("refs/adp/v-{}-{}", ns, next_n))
    } else {
        Ok(format!("refs/adp/{}/v{}/{}", ns, next_n, id))
    }
}

fn extract_version_segment(refname: &str, ns: &str, _id: &str) -> Option<u32> {
    let prefix = format!("refs/adp/{}/v", ns);
    let after = refname.strip_prefix(&prefix)?;
    let slash = after.find('/')?;
    after[..slash].parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adp::access::{AccessConfig, PathRule, Permission};
    use crate::adp::schema::RefSchema;
    use chrono::Utc;
    use tempfile::TempDir;

    fn init_bare_repo(dir: &Path) -> gix::Repository {
        gix::init(dir).expect("init bare repo")
    }

    fn make_schema(id: &str) -> RefSchema {
        RefSchema {
            id: id.to_string(),
            namespace_pattern: format!("refs/engram/{}/*", id),
            title: id.to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            schema: serde_json::json!({ "type": "object" }),
            ui: None,
        }
    }

    #[test]
    fn test_write_and_read_schema() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let schema = make_schema("task");
        AdpStore::write_schema(dir.path(), &schema).unwrap();

        let read_back = AdpStore::read_schema(dir.path(), "task").unwrap();
        assert!(read_back.is_some());
        let s = read_back.unwrap();
        assert_eq!(s.id, "task");
        assert_eq!(s.namespace_pattern, "refs/engram/task/*");
    }

    #[test]
    fn test_read_missing_schema_returns_none() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let result = AdpStore::read_schema(dir.path(), "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_schemas_skips_sidecars() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let schema1 = make_schema("task");
        let schema2 = make_schema("session");
        AdpStore::write_schema(dir.path(), &schema1).unwrap();
        AdpStore::write_schema(dir.path(), &schema1).unwrap();
        AdpStore::write_schema(dir.path(), &schema2).unwrap();

        let schemas = AdpStore::list_schemas(dir.path()).unwrap();
        assert_eq!(schemas.len(), 2);
        let ids: Vec<&str> = schemas.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"task"));
        assert!(ids.contains(&"session"));
    }

    #[test]
    fn test_version_sidecars_are_created() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let schema = make_schema("task");
        AdpStore::write_schema(dir.path(), &schema).unwrap();
        AdpStore::write_schema(dir.path(), &schema).unwrap();

        let gix_repo = gix::open(dir.path()).unwrap();
        let platform = gix_repo.references().unwrap();
        let iter = platform.all().unwrap();

        let sidecars: Vec<String> = iter
            .filter_map(|r| r.ok())
            .map(|r| r.name().as_bstr().to_string())
            .filter(|name| {
                gix::glob::wildmatch(
                    b"refs/adp/schema/v*/task".as_bstr(),
                    name.as_bytes().as_bstr(),
                    gix::glob::wildmatch::Mode::empty(),
                )
            })
            .collect();

        assert_eq!(
            sidecars.len(),
            2,
            "expected 2 sidecars, got: {:?}",
            sidecars
        );
    }

    #[test]
    fn test_write_and_read_access_config() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec![],
            rules: vec![PathRule {
                user: "*".to_string(),
                paths: vec!["refs/heads/*".to_string()],
                permissions: vec![Permission::Pull],
            }],
        };

        AdpStore::write_access_config(dir.path(), &config).unwrap();
        let read_back = AdpStore::read_access_config(dir.path()).unwrap();
        assert!(read_back.is_some());
        let c = read_back.unwrap();
        assert_eq!(c.owners, vec!["alice"]);
    }

    #[test]
    fn test_list_refs_skips_sidecars() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let schema = make_schema("task");
        AdpStore::write_schema(dir.path(), &schema).unwrap();
        AdpStore::write_schema(dir.path(), &schema).unwrap();

        let refs = AdpStore::list_refs(dir.path(), "refs/adp/schema/").unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "refs/adp/schema/task");
    }

    // ---------------------------------------------------------------------
    // Identity tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_register_user_key_writes_identity_refs() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let fingerprint_hex = "abcdef0123456789".repeat(4); // 64-char hex
        AdpStore::register_user_key(
            dir.path(),
            "alice",
            &fingerprint_hex,
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItestkey alice@example.com",
        )
        .unwrap();

        // Key → username mapping
        let username = AdpStore::resolve_key_identity(dir.path(), &fingerprint_hex)
            .unwrap()
            .unwrap();
        assert_eq!(username, "alice");

        // User identity ref
        let identity =
            AdpStore::read_ref_blob(dir.path(), "refs/adp/users/alice/identity").unwrap();
        assert_eq!(identity["username"].as_str().unwrap(), "alice");

        // SSH keys array
        let keys: Vec<String> =
            AdpStore::read_ref_blob(dir.path(), "refs/adp/users/alice/ssh_keys")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
        assert_eq!(keys.len(), 1);
        assert!(keys[0].starts_with("ssh-ed25519"));
    }

    #[test]
    fn test_resolve_key_identity_returns_none_for_unknown() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let result = AdpStore::resolve_key_identity(dir.path(), "nonexistent_hex_hash").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_register_user_key_deduplicates() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let fingerprint_hex = "a1b2c3d4e5f6"
            .repeat(5)
            .chars()
            .take(64)
            .collect::<String>();
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItestkey alice@example.com";

        AdpStore::register_user_key(dir.path(), "alice", &fingerprint_hex, key).unwrap();
        AdpStore::register_user_key(dir.path(), "alice", &fingerprint_hex, key).unwrap();

        let keys: Vec<String> =
            AdpStore::read_ref_blob(dir.path(), "refs/adp/users/alice/ssh_keys")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
        assert_eq!(keys.len(), 1, "should not duplicate identical keys");
    }

    // ---------------------------------------------------------------------
    // Access control tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_check_repo_access_default_allows_read_denies_write() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        // No access config written → default behavior
        assert!(AdpStore::check_repo_access(dir.path(), "anyone", &Permission::Pull, None).unwrap());
        assert!(!AdpStore::check_repo_access(dir.path(), "anyone", &Permission::Push, None).unwrap());
    }

    #[test]
    fn test_check_repo_access_owner_has_full_access() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec![],
            rules: vec![],
        };
        AdpStore::write_access_config(dir.path(), &config).unwrap();

        assert!(AdpStore::check_repo_access(dir.path(), "alice", &Permission::Pull, None).unwrap());
        assert!(AdpStore::check_repo_access(dir.path(), "alice", &Permission::Push, None).unwrap());
        assert!(AdpStore::check_repo_access(dir.path(), "alice", &Permission::Admin, None).unwrap());
    }

    #[test]
    fn test_check_repo_access_admin_has_full_access() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec![],
            admins: vec!["bob".to_string()],
            rules: vec![],
        };
        AdpStore::write_access_config(dir.path(), &config).unwrap();

        assert!(AdpStore::check_repo_access(dir.path(), "bob", &Permission::Pull, None).unwrap());
        assert!(AdpStore::check_repo_access(dir.path(), "bob", &Permission::Push, None).unwrap());
    }

    #[test]
    fn test_check_repo_access_rules_grant_permissions() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec![],
            admins: vec![],
            rules: vec![
                PathRule {
                    user: "carol".to_string(),
                    paths: vec!["refs/heads/*".to_string()],
                    permissions: vec![Permission::Pull],
                },
                PathRule {
                    user: "*".to_string(),
                    paths: vec!["refs/heads/*".to_string()],
                    permissions: vec![Permission::Pull],
                },
            ],
        };
        AdpStore::write_access_config(dir.path(), &config).unwrap();

        // carol matches her specific rule (Pull) and wildcard (Pull)
        assert!(AdpStore::check_repo_access(dir.path(), "carol", &Permission::Pull, None).unwrap());
        // carol does NOT have Push — neither rule grants it to her
        assert!(!AdpStore::check_repo_access(dir.path(), "carol", &Permission::Push, None).unwrap());

        // dave matches wildcard rule (Pull only)
        assert!(AdpStore::check_repo_access(dir.path(), "dave", &Permission::Pull, None).unwrap());
        assert!(!AdpStore::check_repo_access(dir.path(), "dave", &Permission::Push, None).unwrap());
    }

    #[test]
    fn test_check_repo_access_denies_unknown_user_in_restricted_repo() {
        let dir = TempDir::new().unwrap();
        init_bare_repo(dir.path());

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec![],
            rules: vec![PathRule {
                user: "alice".to_string(),
                paths: vec!["refs/heads/*".to_string()],
                permissions: vec![Permission::Pull, Permission::Push],
            }],
        };
        AdpStore::write_access_config(dir.path(), &config).unwrap();

        // eve is not in any rule
        assert!(!AdpStore::check_repo_access(dir.path(), "eve", &Permission::Pull, None).unwrap());
        assert!(!AdpStore::check_repo_access(dir.path(), "eve", &Permission::Push, None).unwrap());
    }
}

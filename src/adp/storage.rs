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

use crate::adp::access::AccessConfig;
use crate::adp::is_version_sidecar;
use crate::adp::schema::RefSchema;
use crate::adp::{ACCESS_CONFIG_REF, SCHEMA_PREFIX};
use git2::{Repository, Signature};
use serde_json::Value;
use std::path::Path;
use thiserror::Error;

/// Errors produced by `AdpStore` operations.
#[derive(Debug, Error)]
pub enum AdpStoreError {
    #[error("git2 error: {0}")]
    Git(#[from] git2::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Ref not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AdpStoreError>;

/// ADP git-ref storage.
///
/// All operations take a `repo_path: &Path` rather than caching an open
/// `Repository` so that `AdpStore` is `Send + Sync` and can be shared across
/// async tasks without a mutex.
pub struct AdpStore;

impl AdpStore {
    /// Create a new `AdpStore`.  The store itself holds no state; all
    /// operations receive the repo path as a parameter.
    pub fn new() -> Self {
        Self
    }

    // -------------------------------------------------------------------------
    // Low-level blob helpers
    // -------------------------------------------------------------------------

    /// Read the JSON blob pointed to by `refname` in the repository at
    /// `repo_path`.
    ///
    /// The ref may point to either a commit (which contains a "data" entry in
    /// its tree) or directly to a blob.  Both cases are handled.
    ///
    /// Returns the parsed `serde_json::Value`.
    pub fn read_ref_blob(repo_path: &Path, refname: &str) -> Result<Value> {
        let repo = Repository::open(repo_path)?;
        let reference = repo
            .find_reference(refname)
            .map_err(|_| AdpStoreError::NotFound(refname.to_string()))?;

        // Try to peel to commit first (our write format: commit→tree→"data" blob).
        let blob_content: Vec<u8> = if let Ok(commit) = reference.peel_to_commit() {
            let tree = commit.tree()?;
            let entry = tree
                .get_name("data")
                .ok_or_else(|| AdpStoreError::NotFound(format!("{}/tree/data", refname)))?;
            let obj = entry.to_object(&repo)?;
            let blob = obj
                .as_blob()
                .ok_or_else(|| AdpStoreError::NotFound(format!("{}/data not a blob", refname)))?;
            blob.content().to_vec()
        } else {
            // Fall back: the ref points directly to a blob.
            let blob = reference.peel_to_blob().map_err(AdpStoreError::Git)?;
            blob.content().to_vec()
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
        let repo = Repository::open(repo_path)?;
        let json_bytes = serde_json::to_vec_pretty(value)?;

        let blob_oid = repo.blob(&json_bytes)?;

        // Build a minimal commit wrapping the blob so we can point a ref at it.
        // We use a tree with a single entry named "data".
        let sig = adp_signature();
        let mut tb = repo.treebuilder(None)?;
        tb.insert("data", blob_oid, 0o100644)?;
        let tree_oid = tb.write()?;
        let tree = repo.find_tree(tree_oid)?;

        // Find parent commit if the ref already exists
        let parent_commit = repo
            .find_reference(refname)
            .ok()
            .and_then(|r| r.peel_to_commit().ok());

        let parents: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();

        let commit_oid = repo.commit(
            None, // don't update any ref yet
            &sig,
            &sig,
            &format!("adp: update {}", refname),
            &tree,
            &parents,
        )?;

        // Force-update the primary ref
        repo.reference(refname, commit_oid, true, "adp write")?;

        // Create an immutable version sidecar
        let sidecar = next_sidecar_ref(&repo, refname)?;
        // Sidecars are never force-updated (they must be immutable)
        repo.reference(&sidecar, commit_oid, false, "adp version sidecar")?;

        Ok(())
    }

    /// List all refs under `prefix` in the repository at `repo_path`, skipping
    /// version sidecar refs.
    pub fn list_refs(repo_path: &Path, prefix: &str) -> Result<Vec<String>> {
        let repo = Repository::open(repo_path)?;
        let refs = repo.references_glob(&format!("{}*", prefix))?;

        let mut result = Vec::new();
        for r in refs {
            let r = r?;
            if let Some(name) = r.name() {
                if !is_version_sidecar(name) {
                    result.push(name.to_string());
                }
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
}

impl Default for AdpStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build an ADP bot signature for commits.
fn adp_signature() -> Signature<'static> {
    // Unwrap is safe: the strings are valid UTF-8 and the time is valid.
    Signature::now("ADP Bot", "adp@localhost").expect("valid signature")
}

/// Compute the next available version sidecar ref name for `primary_ref`.
///
/// For `refs/adp/schema/my-schema` the sidecars live at
/// `refs/adp/schema/v{N}/my-schema`.  This function returns the first `vN`
/// that does not yet exist in the repo.
fn next_sidecar_ref(repo: &Repository, primary_ref: &str) -> Result<String> {
    // We need to reconstruct the sidecar pattern.
    // primary_ref = refs/adp/{ns}/{id}
    // sidecar     = refs/adp/{ns}/v{N}/{id}
    //
    // Split at "refs/adp/" then decompose "{ns}/{id}".
    let after_prefix = primary_ref.strip_prefix("refs/adp/").unwrap_or(primary_ref);

    let slash_pos = after_prefix.find('/').unwrap_or(after_prefix.len());
    let ns = &after_prefix[..slash_pos];
    let id = if slash_pos < after_prefix.len() {
        &after_prefix[slash_pos + 1..]
    } else {
        ""
    };

    // Count existing sidecars
    let sidecar_glob = format!("refs/adp/{}/v*/{}", ns, id);
    let existing = repo.references_glob(&sidecar_glob)?;
    let mut max_n: u32 = 0;
    for r in existing {
        let r = r?;
        if let Some(name) = r.name() {
            // Extract the version number
            if let Some(v_segment) = extract_version_segment(name, ns, id) {
                if v_segment > max_n {
                    max_n = v_segment;
                }
            }
        }
    }

    let next_n = max_n + 1;
    Ok(format!("refs/adp/{}/v{}/{}", ns, next_n, id))
}

/// Extract the version number from a sidecar ref name like
/// `refs/adp/schema/v3/my-schema`.
fn extract_version_segment(refname: &str, ns: &str, _id: &str) -> Option<u32> {
    // refs/adp/{ns}/v{N}/{id}
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

    fn init_bare_repo(dir: &Path) -> Repository {
        Repository::init_bare(dir).expect("init bare repo")
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
        // Write schema1 again to create a v2 sidecar
        AdpStore::write_schema(dir.path(), &schema1).unwrap();
        AdpStore::write_schema(dir.path(), &schema2).unwrap();

        let schemas = AdpStore::list_schemas(dir.path()).unwrap();
        // Should have exactly 2 schemas (task + session), not 3 (no sidecars)
        assert_eq!(schemas.len(), 2);
        let ids: Vec<&str> = schemas.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"task"));
        assert!(ids.contains(&"session"));
    }

    #[test]
    fn test_version_sidecars_are_created() {
        let dir = TempDir::new().unwrap();
        let repo = init_bare_repo(dir.path());

        let schema = make_schema("task");
        AdpStore::write_schema(dir.path(), &schema).unwrap();
        AdpStore::write_schema(dir.path(), &schema).unwrap();

        // There should be two sidecar refs: v1 and v2
        let sidecars: Vec<_> = repo
            .references_glob("refs/adp/schema/v*/task")
            .unwrap()
            .filter_map(|r| r.ok())
            .filter_map(|r| r.name().map(|n| n.to_string()))
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
        AdpStore::write_schema(dir.path(), &schema).unwrap(); // creates v2 sidecar

        let refs = AdpStore::list_refs(dir.path(), "refs/adp/schema/").unwrap();
        // Only the primary ref should appear
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "refs/adp/schema/task");
    }
}

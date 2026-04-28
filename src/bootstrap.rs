//! Core bootstrap for fresh installations.
//!
//! Creates the `_adp/system` bare git repo, writes default config refs,
//! and registers the initial admin SSH key. This module is available in
//! all build configs (open-core and ADP) — no licensing dependencies.

use std::path::Path;
use tracing::{debug, info, warn};

/// The path of the system repo relative to the storage root.
pub const SYSTEM_REPO_PATH: &str = "_adp/system";

/// Default server config ref.
const SERVER_CONFIG_REF: &str = "refs/adp/config/server";

/// Default access config ref.
const ACCESS_CONFIG_REF: &str = "refs/adp/access/config";

/// Errors produced during bootstrap.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("gix error: {0}")]
    Git(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BootstrapError>;

/// Bootstrap the `_adp/system` bare git repository.
///
/// Idempotent — safe to call on every server start or repeated init.
pub fn bootstrap_system_repo(storage_root: &Path) -> Result<()> {
    let system_repo_path = storage_root.join(SYSTEM_REPO_PATH);

    // Create parent directories
    if let Some(parent) = system_repo_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Init the bare repo (idempotent)
    if system_repo_path.exists() {
        match gix::open(&system_repo_path) {
            Ok(_) => {
                debug!("System repo already exists at {:?}, skipping init", system_repo_path);
            }
            Err(e) => {
                warn!("System repo at {:?} could not be opened ({}), re-initialising", system_repo_path, e);
                gix::init_bare(&system_repo_path)
                    .map_err(|e| BootstrapError::Git(e.to_string()))?;
            }
        }
    } else {
        info!("Bootstrapping system repo at {:?}", system_repo_path);
        gix::init_bare(&system_repo_path)
            .map_err(|e| BootstrapError::Git(e.to_string()))?;
    }

    // Write default server config if not present
    if !ref_exists(&system_repo_path, SERVER_CONFIG_REF) {
        info!("Writing default {}", SERVER_CONFIG_REF);
        let config = serde_json::json!({
            "http_host": "0.0.0.0",
            "http_port": 8090,
            "ssh_port": 2222,
        });
        write_ref_json(&system_repo_path, SERVER_CONFIG_REF, &config)?;
    }

    // Write default access config if not present
    if !ref_exists(&system_repo_path, ACCESS_CONFIG_REF) {
        info!("Writing default {}", ACCESS_CONFIG_REF);
        let access = serde_json::json!({
            "owners": [],
            "permissions": [],
        });
        write_ref_json(&system_repo_path, ACCESS_CONFIG_REF, &access)?;
    }

    Ok(())
}

/// Register an admin SSH public key.
///
/// Parses the OpenSSH key, computes its SHA-256 fingerprint, and writes
/// identity refs to the system repo. Also adds the username to the access
/// config owners list.
pub fn register_admin_key(
    storage_root: &Path,
    admin_username: &str,
    public_key_openssh: &str,
) -> Result<()> {
    let system_repo_path = storage_root.join(SYSTEM_REPO_PATH);

    // Compute SHA-256 fingerprint from the openssh key
    let fingerprint = compute_fingerprint(public_key_openssh)
        .map_err(|e| BootstrapError::Git(format!("Failed to parse admin key: {}", e)))?;

    info!("Registering admin key for '{}' with fingerprint {}", admin_username, fingerprint);

    // Write key → username mapping
    let key_ref = format!("refs/adp/keys/{}", fingerprint);
    let identity = serde_json::json!({ "username": admin_username });
    write_ref_json(&system_repo_path, &key_ref, &identity)?;

    // Write username → identity mapping
    let user_ref = format!("refs/adp/users/{}/identity", admin_username);
    write_ref_json(&system_repo_path, &user_ref, &identity)?;

    // Write ssh_keys list
    let ssh_keys_ref = format!("refs/adp/users/{}/ssh_keys", admin_username);
    let keys = serde_json::json!([public_key_openssh]);
    write_ref_json(&system_repo_path, &ssh_keys_ref, &keys)?;

    // Update access config owners
    let mut access: serde_json::Value = read_ref_json(&system_repo_path, ACCESS_CONFIG_REF)?
        .unwrap_or(serde_json::json!({ "owners": [] }));

    if let Some(owners) = access.get_mut("owners").and_then(|o| o.as_array_mut()) {
        let username_val = serde_json::Value::String(admin_username.to_string());
        if !owners.contains(&username_val) {
            owners.push(username_val);
            write_ref_json(&system_repo_path, ACCESS_CONFIG_REF, &access)?;
            info!("Added '{}' as system repo owner", admin_username);
        }
    }

    info!("Admin key registration complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// Public helpers — gix-based ref read/write
// ---------------------------------------------------------------------------

/// Check if a ref exists in the repo.
pub fn ref_exists(repo_path: &Path, refname: &str) -> bool {
    let repo = match gix::open(repo_path) {
        Ok(r) => r,
        Err(_) => return false,
    };
    repo.try_find_reference(refname)
        .ok()
        .flatten()
        .is_some()
}

/// Write a JSON value to a ref as a commit (force-update).
///
/// Creates a blob → tree → commit chain, matching the AdpStore pattern.
pub fn write_ref_json(repo_path: &Path, refname: &str, value: &serde_json::Value) -> Result<()> {
    use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
    use gix::refs::{FullName, Target};

    let gix_repo = gix::open(repo_path).map_err(|e| BootstrapError::Git(e.to_string()))?;
    let json_bytes = serde_json::to_vec_pretty(value)?;

    // Create blob
    let blob = gix::objs::Blob { data: json_bytes };
    let blob_id = gix_repo
        .write_object(&blob)
        .map_err(|e| BootstrapError::Git(e.to_string()))?;

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
        .map_err(|e| BootstrapError::Git(e.to_string()))?;

    // Find parent commit if ref already exists
    let parent_commit_id = gix_repo
        .try_find_reference(refname)
        .map_err(|e| BootstrapError::Git(e.to_string()))?
        .and_then(|r| r.try_id().map(|id| id.detach()));

    let parent_ids: Vec<gix::hash::ObjectId> = parent_commit_id.into_iter().collect();

    // Create signature
    let sig = gix::actor::Signature {
        name: "agentic-repos".into(),
        email: "init@localhost".into(),
        time: gix::date::Time::now_utc(),
    };

    // Create commit
    let commit = gix::objs::Commit {
        tree: tree_id.detach(),
        parents: parent_ids.into(),
        author: sig.clone(),
        committer: sig,
        message: format!("init: bootstrap {}\n", refname).into(),
        encoding: None,
        extra_headers: Default::default(),
    };
    let commit_id = gix_repo
        .write_object(&commit)
        .map_err(|e| BootstrapError::Git(e.to_string()))?;

    // Update ref
    let full_name = FullName::try_from(refname)
        .map_err(|e| BootstrapError::Git(e.to_string()))?;
    gix_repo.edit_reference(RefEdit {
        change: Change::Update {
            log: LogChange::default(),
            expected: PreviousValue::Any,
            new: Target::Object(commit_id.detach()),
        },
        name: full_name,
        deref: false,
    }).map_err(|e| BootstrapError::Git(e.to_string()))?;

    Ok(())
}

/// Read a JSON value from a ref. Returns None if ref doesn't exist.
pub fn read_ref_json(repo_path: &Path, refname: &str) -> Result<Option<serde_json::Value>> {
    let repo = gix::open(repo_path).map_err(|e| BootstrapError::Git(e.to_string()))?;
    let reference = match repo.try_find_reference(refname).map_err(|e| BootstrapError::Git(e.to_string()))? {
        Some(r) => r,
        None => return Ok(None),
    };

    let commit_id = reference.try_id()
        .ok_or_else(|| BootstrapError::Git(format!("Ref {} has no target", refname)))?
        .detach();

    let commit = repo.find_object(commit_id)
        .map_err(|e| BootstrapError::Git(e.to_string()))?;
    let commit_obj = commit.try_into_commit()
        .map_err(|e| BootstrapError::Git(e.to_string()))?;

    let tree_id = commit_obj.tree_id().map_err(|e| BootstrapError::Git(e.to_string()))?;
    let tree_data = repo.find_object(tree_id)
        .map_err(|e| BootstrapError::Git(e.to_string()))?;
    let tree_ref = gix::objs::TreeRef::from_bytes(&tree_data.data)
        .map_err(|e| BootstrapError::Git(e.to_string()))?;

    // Find the "data" entry
    for entry in &tree_ref.entries {
        if entry.filename == "data" {
            let blob = repo.find_object(entry.oid).map_err(|e| BootstrapError::Git(e.to_string()))?;
            let value: serde_json::Value = serde_json::from_slice(&blob.data)?;
            return Ok(Some(value));
        }
    }

    Ok(None)
}

/// Compute SHA-256 fingerprint from an OpenSSH public key string.
fn compute_fingerprint(public_key_openssh: &str) -> Result<String> {
    let parts: Vec<&str> = public_key_openssh.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(BootstrapError::Git("Invalid OpenSSH public key format".to_string()));
    }

    use base64::Engine;
    let key_data = base64::engine::general_purpose::STANDARD
        .decode(parts[1])
        .map_err(|e| BootstrapError::Git(format!("Base64 decode failed: {}", e)))?;

    use sha2::Digest;
    let hash = sha2::Sha256::digest(&key_data);
    Ok(format!("SHA256:{}", base64::engine::general_purpose::STANDARD.encode(hash)))
}

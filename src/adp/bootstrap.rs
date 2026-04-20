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
use crate::adp::schema::{FieldConfig, RefSchema, UiHints};
use crate::adp::storage::{AdpStore, AdpStoreError};
use crate::adp::SERVER_CONFIG_REF;
use std::collections::HashMap;
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

/// Bootstrap engram entity schemas into the system repo.
///
/// Writes RefSchema definitions for all 11 engram entity types.
/// Idempotent — skips schemas that already exist.
pub fn bootstrap_engram_schemas(storage_root: &Path) -> Result<()> {
    let system_repo_path = storage_root.join(SYSTEM_REPO_PATH);

    let schemas = vec![
        schema_def(
            "engram-task",
            "refs/engram/task/*",
            "Engram Task",
            json_schema_task(),
            ui_hints_task(),
        ),
        schema_def(
            "engram-context",
            "refs/engram/context/*",
            "Engram Context",
            json_schema_generic("context"),
            default_ui(),
        ),
        schema_def(
            "engram-reasoning",
            "refs/engram/reasoning/*",
            "Engram Reasoning",
            json_schema_generic("reasoning"),
            default_ui(),
        ),
        schema_def(
            "engram-knowledge",
            "refs/engram/knowledge/*",
            "Engram Knowledge",
            json_schema_generic("knowledge"),
            default_ui(),
        ),
        schema_def(
            "engram-session",
            "refs/engram/session/*",
            "Engram Session",
            json_schema_session(),
            ui_hints_session(),
        ),
        schema_def(
            "engram-adr",
            "refs/engram/adr/*",
            "Engram ADR",
            json_schema_adr(),
            ui_hints_adr(),
        ),
        schema_def(
            "engram-theory",
            "refs/engram/theory/*",
            "Engram Theory",
            json_schema_generic("theory"),
            default_ui(),
        ),
        schema_def(
            "engram-state-reflection",
            "refs/engram/state_reflection/*",
            "Engram State Reflection",
            json_schema_generic("state_reflection"),
            default_ui(),
        ),
        schema_def(
            "engram-doc-fragment",
            "refs/engram/doc_fragment/*",
            "Engram DocFragment",
            json_schema_generic("doc_fragment"),
            default_ui(),
        ),
        schema_def(
            "engram-compliance",
            "refs/engram/compliance/*",
            "Engram Compliance",
            json_schema_generic("compliance"),
            default_ui(),
        ),
        schema_def(
            "engram-workflow",
            "refs/engram/workflow/*",
            "Engram Workflow",
            json_schema_generic("workflow"),
            default_ui(),
        ),
    ];

    let mut created = 0u32;
    let mut skipped = 0u32;

    for schema in schemas {
        if AdpStore::read_schema(&system_repo_path, &schema.id)?.is_some() {
            skipped += 1;
            continue;
        }
        AdpStore::write_schema(&system_repo_path, &schema)?;
        created += 1;
    }

    info!(
        created = created,
        skipped = skipped,
        "Bootstrapped engram schemas"
    );
    Ok(())
}

/// Bootstrap operational schemas (webhook-config, deploy-target, ci-pipeline,
/// team-member, review-checklist) into the system repo.
pub fn bootstrap_operational_schemas(storage_root: &Path) -> Result<()> {
    let system_repo_path = storage_root.join(SYSTEM_REPO_PATH);

    let schemas = vec![
        schema_def_with_desc(
            "webhook-config",
            "refs/webhooks/*",
            "Webhook Configuration",
            "Outbound webhook configuration stored in git refs",
            json_schema_webhook_config(),
            ui_hints_webhook_config(),
        ),
        schema_def_with_desc(
            "deploy-target",
            "refs/deploy/*",
            "Deploy Target",
            "Deployment target configuration (staging, production, etc.)",
            json_schema_deploy_target(),
            ui_hints_deploy_target(),
        ),
        schema_def_with_desc(
            "ci-pipeline",
            "refs/ci/*",
            "CI Pipeline",
            "Continuous integration pipeline definition",
            json_schema_ci_pipeline(),
            ui_hints_ci_pipeline(),
        ),
        schema_def_with_desc(
            "team-member",
            "refs/team/*",
            "Team Member",
            "Team member profile and role assignment",
            json_schema_team_member(),
            ui_hints_team_member(),
        ),
        schema_def_with_desc(
            "review-checklist",
            "refs/review/*",
            "Review Checklist",
            "Code review checklist template",
            json_schema_review_checklist(),
            ui_hints_review_checklist(),
        ),
    ];

    let mut created = 0u32;
    let mut skipped = 0u32;

    for schema in schemas {
        if AdpStore::read_schema(&system_repo_path, &schema.id)?.is_some() {
            skipped += 1;
            continue;
        }
        AdpStore::write_schema(&system_repo_path, &schema)?;
        created += 1;
    }

    info!(
        created = created,
        skipped = skipped,
        "Bootstrapped operational schemas"
    );
    Ok(())
}

fn schema_def(
    id: &str,
    namespace_pattern: &str,
    title: &str,
    schema: serde_json::Value,
    ui: Option<UiHints>,
) -> RefSchema {
    let description = format!("Engram {} entity", id.strip_prefix("engram-").unwrap_or(id));
    schema_def_with_desc(id, namespace_pattern, title, &description, schema, ui)
}

fn schema_def_with_desc(
    id: &str,
    namespace_pattern: &str,
    title: &str,
    description: &str,
    schema: serde_json::Value,
    ui: Option<UiHints>,
) -> RefSchema {
    use chrono::Utc;
    RefSchema {
        id: id.to_string(),
        namespace_pattern: namespace_pattern.to_string(),
        title: title.to_string(),
        description: Some(description.to_string()),
        version: "1.0.0".to_string(),
        author: Some("agentic-repos/bootstrap".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        schema,
        ui,
    }
}

fn default_ui() -> Option<UiHints> {
    None
}

fn ui_hints_task() -> Option<UiHints> {
    Some(UiHints {
        view: Some("entity-list".to_string()),
        list_fields: vec!["title".into(), "status".into(), "priority".into()],
        detail_fields: vec![
            "title".into(),
            "description".into(),
            "status".into(),
            "priority".into(),
        ],
        fields: {
            let mut m = HashMap::new();
            m.insert(
                "status".into(),
                FieldConfig {
                    widget: Some("badge".into()),
                    color_map: {
                        let mut c = HashMap::new();
                        c.insert("todo".into(), "gray".into());
                        c.insert("in_progress".into(), "blue".into());
                        c.insert("done".into(), "green".into());
                        c.insert("blocked".into(), "red".into());
                        c
                    },
                },
            );
            m.insert(
                "priority".into(),
                FieldConfig {
                    widget: Some("badge".into()),
                    color_map: {
                        let mut c = HashMap::new();
                        c.insert("low".into(), "gray".into());
                        c.insert("medium".into(), "yellow".into());
                        c.insert("high".into(), "orange".into());
                        c.insert("critical".into(), "red".into());
                        c
                    },
                },
            );
            m
        },
    })
}

fn ui_hints_session() -> Option<UiHints> {
    Some(UiHints {
        view: Some("entity-list".to_string()),
        list_fields: vec!["name".into(), "agent".into()],
        detail_fields: vec!["name".into(), "agent".into(), "started_at".into()],
        fields: Default::default(),
    })
}

fn ui_hints_adr() -> Option<UiHints> {
    Some(UiHints {
        view: Some("entity-list".to_string()),
        list_fields: vec!["title".into(), "number".into()],
        detail_fields: vec!["title".into(), "number".into(), "context".into()],
        fields: Default::default(),
    })
}

fn json_schema_task() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["title", "status"],
        "properties": {
            "title": { "type": "string" },
            "description": { "type": "string" },
            "status": { "type": "string", "enum": ["todo", "in_progress", "done", "blocked", "cancelled"] },
            "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
            "agent": { "type": "string" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn json_schema_session() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": { "type": "string" },
            "agent": { "type": "string" },
            "started_at": { "type": "string", "format": "date-time" },
            "ended_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn json_schema_adr() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["title", "number", "context"],
        "properties": {
            "title": { "type": "string" },
            "number": { "type": "integer" },
            "context": { "type": "string" },
            "status": { "type": "string", "enum": ["proposed", "accepted", "deprecated", "superseded"] }
        }
    })
}

fn json_schema_generic(entity_type: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["title", "content"],
        "properties": {
            "title": { "type": "string" },
            "content": { "type": "string" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "agent": { "type": "string" },
            "entity_type": { "type": "string", "const": entity_type },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        }
    })
}

// ── Operational schema definitions ─────────────────────────────────────────

fn json_schema_webhook_config() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["url", "events"],
        "properties": {
            "url": { "type": "string", "format": "uri" },
            "secret": { "type": "string" },
            "events": { "type": "array", "items": { "type": "string" } },
            "secret_keys": { "type": "array", "items": { "type": "string" }, "description": "Secret keys this webhook may access" },
            "active": { "type": "boolean", "default": true },
            "tenant_id": { "type": "string" },
            "entity_schemas": { "type": "array", "items": { "type": "string" } },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn ui_hints_webhook_config() -> Option<UiHints> {
    Some(UiHints {
        view: Some("entity-list".to_string()),
        list_fields: vec!["url".into(), "events".into(), "active".into()],
        detail_fields: vec![
            "url".into(),
            "events".into(),
            "secret_keys".into(),
            "active".into(),
        ],
        fields: HashMap::new(),
    })
}

fn json_schema_deploy_target() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["name", "environment"],
        "properties": {
            "name": { "type": "string" },
            "environment": { "type": "string", "enum": ["staging", "production", "preview", "development"] },
            "url": { "type": "string", "format": "uri" },
            "branch": { "type": "string", "description": "Branch to deploy from" },
            "auto_deploy": { "type": "boolean", "default": false },
            "variables": { "type": "object", "additionalProperties": { "type": "string" } },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn ui_hints_deploy_target() -> Option<UiHints> {
    Some(UiHints {
        view: Some("entity-list".to_string()),
        list_fields: vec!["name".into(), "environment".into(), "branch".into()],
        detail_fields: vec![
            "name".into(),
            "environment".into(),
            "url".into(),
            "auto_deploy".into(),
        ],
        fields: {
            let mut m = HashMap::new();
            m.insert(
                "environment".into(),
                FieldConfig {
                    widget: Some("badge".to_string()),
                    color_map: {
                        let mut cm = HashMap::new();
                        cm.insert("production".into(), "red".into());
                        cm.insert("staging".into(), "amber".into());
                        cm.insert("preview".into(), "blue".into());
                        cm.insert("development".into(), "green".into());
                        cm
                    },
                },
            );
            m
        },
    })
}

fn json_schema_ci_pipeline() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["name", "steps"],
        "properties": {
            "name": { "type": "string" },
            "trigger": { "type": "string", "enum": ["push", "pull_request", "schedule", "manual"] },
            "branch_filter": { "type": "string" },
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["name", "command"],
                    "properties": {
                        "name": { "type": "string" },
                        "command": { "type": "string" },
                        "working_dir": { "type": "string" },
                        "env": { "type": "object", "additionalProperties": { "type": "string" } },
                        "timeout_seconds": { "type": "integer", "minimum": 1 }
                    }
                }
            },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn ui_hints_ci_pipeline() -> Option<UiHints> {
    Some(UiHints {
        view: Some("entity-list".to_string()),
        list_fields: vec!["name".into(), "trigger".into()],
        detail_fields: vec!["name".into(), "trigger".into(), "steps".into()],
        fields: HashMap::new(),
    })
}

fn json_schema_team_member() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["username", "role"],
        "properties": {
            "username": { "type": "string" },
            "email": { "type": "string", "format": "email" },
            "role": { "type": "string", "enum": ["owner", "admin", "developer", "viewer"] },
            "teams": { "type": "array", "items": { "type": "string" } },
            "ssh_public_key": { "type": "string" },
            "joined_at": { "type": "string", "format": "date-time" },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn ui_hints_team_member() -> Option<UiHints> {
    Some(UiHints {
        view: Some("entity-list".to_string()),
        list_fields: vec!["username".into(), "role".into(), "email".into()],
        detail_fields: vec![
            "username".into(),
            "email".into(),
            "role".into(),
            "teams".into(),
        ],
        fields: {
            let mut m = HashMap::new();
            m.insert(
                "role".into(),
                FieldConfig {
                    widget: Some("badge".to_string()),
                    color_map: {
                        let mut cm = HashMap::new();
                        cm.insert("owner".into(), "purple".into());
                        cm.insert("admin".into(), "red".into());
                        cm.insert("developer".into(), "blue".into());
                        cm.insert("viewer".into(), "green".into());
                        cm
                    },
                },
            );
            m
        },
    })
}

fn json_schema_review_checklist() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["name", "items"],
        "properties": {
            "name": { "type": "string" },
            "description": { "type": "string" },
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["check"],
                    "properties": {
                        "check": { "type": "string" },
                        "required": { "type": "boolean", "default": true },
                        "category": { "type": "string" },
                        "auto": { "type": "boolean", "default": false, "description": "Can be auto-verified" }
                    }
                }
            },
            "applies_to": { "type": "array", "items": { "type": "string" }, "description": "Branches or paths this checklist applies to" },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn ui_hints_review_checklist() -> Option<UiHints> {
    Some(UiHints {
        view: Some("entity-list".to_string()),
        list_fields: vec!["name".into(), "description".into()],
        detail_fields: vec!["name".into(), "description".into(), "items".into()],
        fields: HashMap::new(),
    })
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
    let mut access_config = AdpStore::read_access_config(&system_repo_path)?.unwrap_or_default();

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
pub fn bootstrap_root_key(storage_root: &Path, root_key_path: &Path) -> Result<std::path::PathBuf> {
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
        assert!(lfs_cfg.is_object());
        ();
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
        let identity =
            AdpStore::read_ref_blob(&system_path, "refs/adp/users/admin/identity").unwrap();
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
        let pubkey_ref =
            AdpStore::read_ref_blob(&system_path, "refs/adp/config/public-key").unwrap();
        assert!(pubkey_ref["public_key"].is_string());
        assert!(pubkey_ref["public_key"]
            .as_str()
            .unwrap()
            .starts_with("age1"));
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

    #[test]
    fn test_bootstrap_engram_schemas() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();
        bootstrap_engram_schemas(dir.path()).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);

        // All 11 schemas should exist
        let schemas = AdpStore::list_schemas(&system_path).unwrap();
        assert_eq!(
            schemas.len(),
            11,
            "expected 11 engram schemas, got {}",
            schemas.len()
        );

        // Check specific schema
        let task_schema = AdpStore::read_schema(&system_path, "engram-task").unwrap();
        assert!(task_schema.is_some());
        let task = task_schema.unwrap();
        assert_eq!(task.namespace_pattern, "refs/engram/task/*");
        assert_eq!(task.version, "1.0.0");
    }

    #[test]
    fn test_bootstrap_engram_schemas_idempotent() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        bootstrap_engram_schemas(dir.path()).unwrap();
        bootstrap_engram_schemas(dir.path()).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);
        let schemas = AdpStore::list_schemas(&system_path).unwrap();
        assert_eq!(
            schemas.len(),
            11,
            "should still be exactly 11 schemas after double bootstrap"
        );
    }

    #[test]
    fn test_bootstrap_operational_schemas() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        bootstrap_operational_schemas(dir.path()).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);
        let schemas = AdpStore::list_schemas(&system_path).unwrap();
        assert_eq!(schemas.len(), 5, "should have 5 operational schemas");

        let ids: Vec<&str> = schemas.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"webhook-config"));
        assert!(ids.contains(&"deploy-target"));
        assert!(ids.contains(&"ci-pipeline"));
        assert!(ids.contains(&"team-member"));
        assert!(ids.contains(&"review-checklist"));
    }

    #[test]
    fn test_bootstrap_operational_schemas_idempotent() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        bootstrap_operational_schemas(dir.path()).unwrap();
        bootstrap_operational_schemas(dir.path()).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);
        let schemas = AdpStore::list_schemas(&system_path).unwrap();
        assert_eq!(
            schemas.len(),
            5,
            "should still be exactly 5 schemas after double bootstrap"
        );
    }

    #[test]
    fn test_all_bootstrapped_schemas() {
        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        bootstrap_engram_schemas(dir.path()).unwrap();
        bootstrap_operational_schemas(dir.path()).unwrap();

        let system_path = dir.path().join(SYSTEM_REPO_PATH);
        let schemas = AdpStore::list_schemas(&system_path).unwrap();
        assert_eq!(
            schemas.len(),
            16,
            "11 engram + 5 operational = 16 total schemas"
        );
    }
}

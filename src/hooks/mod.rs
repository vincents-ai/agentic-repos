//! Extension hooks for the open-core git server.
//!
//! These traits define the extension points where ADP enterprise features
//! plug in. The open-core server ships with no-op defaults — the commercial
//! `adp` binary registers concrete implementations at startup.
//!
//! ## Architecture
//!
//! ```text
//! agentic-repos (open-core)
//!   └── hooks::Hooks  —  holds Option<Arc<dyn Trait>> for each extension
//!       ├── RefAccessControl  —  private ref filtering
//!       ├── PushValidation    —  pre-receive schema validation
//!       ├── AuthBackend       —  SSH key → identity resolution
//!       ├── ConfigProvider    —  server/LFS config from git refs
//!       └── SchemaBootstrapper —  system repo + schema bootstrap
//!
//! adp (commercial)
//!   └── implements all traits using moved src/adp/ code
//!       └── wires via HooksBuilder at startup
//! ```

use std::path::Path;

// ── CallerIdentity (core type, not a trait) ─────────────────────

/// Identity of the caller, resolved from SSH key or other auth mechanism.
///
/// This is a core type because the git protocol handlers need it regardless
/// of whether enterprise features are loaded. With no AuthBackend registered,
/// all callers are anonymous (username = "anonymous", no admin/owner).
#[derive(Debug, Clone)]
pub struct CallerIdentity {
    pub username: String,
    pub is_owner: bool,
    pub is_admin: bool,
    /// Whether this is a service key (e.g. agentic-loop, CI runner).
    pub is_service: bool,
    /// Service type for service keys (e.g. "agentic-loop", "ci-runner").
    pub service_type: Option<String>,
}

impl CallerIdentity {
    /// Anonymous caller — used when no AuthBackend is registered.
    pub fn anonymous() -> Self {
        Self {
            username: "anonymous".to_string(),
            is_owner: false,
            is_admin: false,
            is_service: false,
            service_type: None,
        }
    }

    /// Can this caller see private refs (adp config, secrets, etc.)?
    pub fn can_see_private_refs(&self) -> bool {
        self.is_owner || self.is_admin
    }
}

impl Default for CallerIdentity {
    fn default() -> Self {
        Self::anonymous()
    }
}

// ── Permission ─────────────────────────────────────────────────

/// Repository-level permission checked during SSH and HTTP operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Read / clone / pull permission.
    Pull,
    /// Write / push permission.
    Push,
    /// Admin permission (implies Pull + Push + access to private refs).
    Admin,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::Pull => "pull",
            Permission::Push => "push",
            Permission::Admin => "admin",
        }
    }
}

// ── RefAccessControl ─────────────────────────────────────────────

/// Controls which refs are private/restricted and who can access them.
///
/// Open-core default: all refs are public (no private refs exist).
/// ADP implementation: marks refs/adp/*, refs/secrets/*, etc. as private.
pub trait RefAccessControl: Send + Sync {
    /// Is this ref in a private namespace?
    fn is_private(&self, refname: &str) -> bool;

    /// Can the given caller access a private ref?
    fn can_access(&self, identity: &CallerIdentity, refname: &str) -> bool {
        // Default: if we don't know, deny access to private refs
        if self.is_private(refname) {
            identity.can_see_private_refs()
        } else {
            true
        }
    }

    /// Is this ref a keys/user management ref? (restricted write access)
    fn is_restricted_write(&self, refname: &str) -> bool {
        let _ = refname;
        false
    }

    /// Is this a special ref like the theme ref?
    fn is_special_ref(&self, refname: &str) -> bool {
        let _ = refname;
        false
    }

    /// Check if a user has a given permission on a repository.
    /// The storage_root is used to locate the repo's access config.
    /// Returns true if access is granted, false otherwise.
    fn check_repo_access_with_path(
        &self,
        repo_path: &Path,
        username: &str,
        permission: Permission,
        storage_root: &Path,
    ) -> bool {
        let _ = (repo_path, username, permission, storage_root);
        false
    }
}

/// No-op: all refs are public, nothing is restricted.
pub struct NoOpRefAccessControl;

impl RefAccessControl for NoOpRefAccessControl {
    fn is_private(&self, _refname: &str) -> bool {
        false
    }
}

// ── PushValidation ──────────────────────────────────────────────

/// Pre-receive hook for validating pushes.
///
/// Open-core default: all pushes pass (no validation).
/// ADP implementation: schema validation, entity lifecycle hooks.
pub trait PushValidation: Send + Sync {
    /// Validate a set of ref updates before accepting the push.
    /// Return Err with a message to reject, Ok(()) to accept.
    fn validate_push(
        &self,
        repo_path: &Path,
        ref_updates: &[(String, Option<String>, Option<String>)],
    ) -> Result<(), String> {
        let _ = (repo_path, ref_updates);
        Ok(())
    }

    /// Post-receive hook — called after refs are updated.
    /// Used for entity lifecycle events (e.g. creating engram entities).
    fn on_refs_updated(
        &self,
        repo_path: &Path,
        ref_updates: &[(String, Option<String>, Option<String>)],
    ) {
        let _ = (repo_path, ref_updates);
    }
}

/// No-op: all pushes pass validation.
pub struct NoOpPushValidation;

impl PushValidation for NoOpPushValidation {}

// ── AuthBackend ─────────────────────────────────────────────────

/// Resolves SSH keys (or other credentials) to caller identities.
///
/// Open-core default: all callers are anonymous.
/// ADP implementation: resolves against RBAC user store.
pub trait AuthBackend: Send + Sync {
    /// Resolve an SSH key fingerprint to a caller identity.
    fn resolve_key(&self, key_fingerprint: &str) -> Option<CallerIdentity> {
        let _ = key_fingerprint;
        None
    }

    /// Resolve an SSH key fingerprint using the given storage root path.
    /// This is the preferred method — it avoids the need for hooks to store state.
    /// Default: calls resolve_key() for backward compatibility.
    fn resolve_key_with_path(
        &self,
        key_fingerprint: &str,
        _storage_root: &std::path::Path,
    ) -> Option<CallerIdentity> {
        // Backward compat: try resolve_key first
        self.resolve_key(key_fingerprint)
    }

    /// Resolve a service key to a caller identity.
    fn resolve_service_key(&self, service_type: &str, key_fingerprint: &str) -> Option<CallerIdentity> {
        let _ = (service_type, key_fingerprint);
        None
    }
}

/// No-op: all callers are anonymous.
pub struct NoOpAuthBackend;

impl AuthBackend for NoOpAuthBackend {}

// ── ConfigProvider ──────────────────────────────────────────────

/// Loads server configuration from git refs (or other storage).
///
/// Open-core default: returns default configs (from env/CLI).
/// ADP implementation: loads from refs/adp/config in the system repo.
pub trait ConfigProvider: Send + Sync {
    /// Load the server configuration. Returns JSON value or None.
    fn load_server_config(&self) -> Option<serde_json::Value> {
        None
    }

    /// Load the LFS configuration. Returns JSON value or None.
    fn load_lfs_config(&self) -> Option<serde_json::Value> {
        None
    }
}

/// No-op: returns None (caller falls back to defaults).
pub struct NoOpConfigProvider;

impl ConfigProvider for NoOpConfigProvider {}

// ── SchemaBootstrapper ──────────────────────────────────────────

/// Bootstraps system schemas and repositories on first run.
///
/// Open-core default: no bootstrapping.
/// ADP implementation: creates _adp/system repo, loads engram/operational schemas.
pub trait SchemaBootstrapper: Send + Sync {
    /// Bootstrap the system. Called once at server startup.
    fn bootstrap(&self, data_path: &Path) -> Result<(), String> {
        let _ = data_path;
        Ok(())
    }
}

/// No-op: no bootstrapping.
pub struct NoOpSchemaBootstrapper;

impl SchemaBootstrapper for NoOpSchemaBootstrapper {}

// ── RepoLifecycle ───────────────────────────────────────────────

/// Hooks called during repository lifecycle events.
///
/// Open-core default: no extra provisioning.
/// ADP implementation: provisions AGE keypair for secrets.
pub trait RepoLifecycle: Send + Sync {
    /// Called after a new repository is created.
    /// Use this to provision per-repo resources (encryption keys, schemas, etc.).
    fn on_repo_created(
        &self,
        _repo_path: &Path,
        _owner: &str,
        _name: &str,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// No-op: no repo lifecycle hooks.
pub struct NoOpRepoLifecycle;

impl RepoLifecycle for NoOpRepoLifecycle {}

// ── Hooks (container) ──────────────────────────────────────────

/// Container for all registered extension hooks.
/// Stored in server state and passed to protocol handlers.
#[derive(Clone)]
pub struct Hooks {
    pub ref_access: Arc<dyn RefAccessControl>,
    pub push_validation: Arc<dyn PushValidation>,
    pub auth: Arc<dyn AuthBackend>,
    pub config: Arc<dyn ConfigProvider>,
    pub schema_bootstrap: Arc<dyn SchemaBootstrapper>,
    pub repo_lifecycle: Arc<dyn RepoLifecycle>,
    /// Storage root path — set by the server at startup.
    /// Hooks implementations (especially AuthBackend) use this to resolve
    /// identities against the system repo.
    pub storage_root: Option<std::path::PathBuf>,
}

use std::sync::Arc;

impl Default for Hooks {
    fn default() -> Self {
        Self {
            ref_access: Arc::new(NoOpRefAccessControl),
            push_validation: Arc::new(NoOpPushValidation),
            auth: Arc::new(NoOpAuthBackend),
            config: Arc::new(NoOpConfigProvider),
            schema_bootstrap: Arc::new(NoOpSchemaBootstrapper),
            repo_lifecycle: Arc::new(NoOpRepoLifecycle),
            storage_root: None,
        }
    }
}

impl Hooks {
    /// Create a builder for registering custom hooks.
    pub fn builder() -> HooksBuilder {
        HooksBuilder::default()
    }
}

/// Builder for registering ADP (or other) hook implementations.
pub struct HooksBuilder {
    inner: Hooks,
}

impl Default for HooksBuilder {
    fn default() -> Self {
        Self {
            inner: Hooks::default(),
        }
    }
}

impl HooksBuilder {
    pub fn ref_access(mut self, impl_: Arc<dyn RefAccessControl>) -> Self {
        self.inner.ref_access = impl_;
        self
    }

    pub fn push_validation(mut self, impl_: Arc<dyn PushValidation>) -> Self {
        self.inner.push_validation = impl_;
        self
    }

    pub fn auth(mut self, impl_: Arc<dyn AuthBackend>) -> Self {
        self.inner.auth = impl_;
        self
    }

    pub fn config(mut self, impl_: Arc<dyn ConfigProvider>) -> Self {
        self.inner.config = impl_;
        self
    }

    pub fn schema_bootstrap(mut self, impl_: Arc<dyn SchemaBootstrapper>) -> Self {
        self.inner.schema_bootstrap = impl_;
        self
    }

    pub fn repo_lifecycle(mut self, impl_: Arc<dyn RepoLifecycle>) -> Self {
        self.inner.repo_lifecycle = impl_;
        self
    }

    pub fn build(self) -> Hooks {
        self.inner
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_identity() {
        let id = CallerIdentity::anonymous();
        assert_eq!(id.username, "anonymous");
        assert!(!id.is_owner);
        assert!(!id.is_admin);
        assert!(!id.can_see_private_refs());
        assert!(!id.is_service);
    }

    #[test]
    fn default_is_anonymous() {
        let id = CallerIdentity::default();
        assert_eq!(id.username, "anonymous");
    }

    #[test]
    fn no_op_ref_access_allows_all() {
        let ctrl = NoOpRefAccessControl;
        assert!(!ctrl.is_private("refs/heads/main"));
        assert!(!ctrl.is_private("refs/adp/config"));
        assert!(!ctrl.is_restricted_write("refs/adp/keys"));
        assert!(!ctrl.is_special_ref("refs/adp/theme"));
    }

    #[test]
    fn no_op_push_validation_accepts_all() {
        let v = NoOpPushValidation;
        assert!(v.validate_push(Path::new("/tmp"), &[]).is_ok());
    }

    #[test]
    fn no_op_auth_returns_none() {
        let auth = NoOpAuthBackend;
        assert!(auth.resolve_key("SHA256:abc").is_none());
        assert!(auth.resolve_service_key("ci-runner", "SHA256:abc").is_none());
    }

    #[test]
    fn no_op_config_returns_none() {
        let cfg = NoOpConfigProvider;
        assert!(cfg.load_server_config().is_none());
        assert!(cfg.load_lfs_config().is_none());
    }

    #[test]
    fn no_op_schema_bootstrap_succeeds() {
        let b = NoOpSchemaBootstrapper;
        assert!(b.bootstrap(Path::new("/tmp")).is_ok());
    }

    #[test]
    fn hooks_default_uses_no_ops() {
        let hooks = Hooks::default();
        assert!(!hooks.ref_access.is_private("anything"));
        assert!(hooks.push_validation.validate_push(Path::new("/tmp"), &[]).is_ok());
        assert!(hooks.auth.resolve_key("key").is_none());
    }

    #[test]
    fn builder_registers_custom_impls() {
        let hooks = Hooks::builder().build();
        assert!(!hooks.ref_access.is_private("x"));
    }

    #[test]
    fn caller_identity_can_see_private_when_admin() {
        let id = CallerIdentity {
            username: "admin".into(),
            is_owner: false,
            is_admin: true,
            is_service: false,
            service_type: None,
        };
        assert!(id.can_see_private_refs());
    }
}

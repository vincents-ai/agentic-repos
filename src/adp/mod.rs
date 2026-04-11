//! ADP — Agentic Development Platform first-party git-ref namespace.
//!
//! All per-repo and server-level configuration lives exclusively in git refs
//! under the `refs/adp/*` namespace. The repo is always the source of truth.
//!
//! ## Namespace layout
//!
//! ```text
//! refs/adp/schema/{id}          — public: typed ref-namespace schema definitions
//! refs/adp/schema/v{N}/{id}     — immutable version sidecars
//! refs/adp/access/config        — private: repo ACL (owners, admins, path rules)
//! refs/adp/access/v{N}/config   — immutable version sidecars
//! refs/adp/config/repo          — private: general repo settings
//! refs/adp/config/v{N}/repo     — immutable version sidecars
//!
//! System repo only:
//! refs/adp/config/server        — global server settings
//! refs/adp/access/config        — global super-admin ACL
//! refs/adp/schema/{id}          — global schema definitions (visible to all repos)
//! ```
//!
//! ## Visibility (enforced at git wire-protocol layer)
//!
//! - `refs/adp/schema/*`  — advertised to all authenticated readers
//! - `refs/adp/access/*`  — stripped from wire protocol for non-owner/admin clients
//! - `refs/adp/config/*`  — stripped from wire protocol for non-owner/admin clients
//!
//! ## Versioning
//!
//! Every mutable ref has immutable version sidecars following the engram pattern:
//! the primary ref is force-updated on every write; on each write a new
//! `v{N}` sidecar is created (force=false, immutable).

pub mod access;
pub mod bootstrap;
pub mod config;
pub mod glob;
pub mod identity;
pub mod schema;
pub mod secrets;
pub mod storage;

pub use access::{AccessConfig, PathRule, Permission, UserEntry};
pub use bootstrap::bootstrap_system_repo;
pub use config::{RepoConfig, ServerConfig};
pub use identity::CallerIdentity;
pub use schema::{RefSchema, UiHints};
pub use secrets::{RepoKeyManager, SecretsManager, SECRETS_KEYS_PREFIX};
pub use storage::AdpStore;

/// The root namespace for all ADP refs.
pub const ADP_REF_PREFIX: &str = "refs/adp/";

/// Sub-namespaces.
pub const SCHEMA_PREFIX: &str = "refs/adp/schema/";
pub const ACCESS_PREFIX: &str = "refs/adp/access/";
pub const CONFIG_PREFIX: &str = "refs/adp/config/";

/// User-defined namespaces (private, stripped from wire protocol).
pub use secrets::PUBLIC_KEY_PREFIX;
pub use secrets::SECRETS_PREFIX;

/// The special ref name for the repo ACL blob.
pub const ACCESS_CONFIG_REF: &str = "refs/adp/access/config";

/// The special ref name for per-repo general settings.
pub const REPO_CONFIG_REF: &str = "refs/adp/config/repo";

/// The special ref name for global server settings (system repo only).
pub const SERVER_CONFIG_REF: &str = "refs/adp/config/server";

/// Returns true if the ref is an ADP private ref (access or config).
/// These are stripped from the git wire protocol for non-privileged clients.
pub fn is_private(refname: &str) -> bool {
    refname.starts_with(ACCESS_PREFIX)
        || refname.starts_with(CONFIG_PREFIX)
        || refname.starts_with(SECRETS_PREFIX)
}

/// Returns true if the ref is an ADP public ref (schema).
/// These are advertised to all authenticated readers.
pub fn is_public_adp(refname: &str) -> bool {
    refname.starts_with(SCHEMA_PREFIX)
}

/// Returns true if the ref is any ADP ref (private or public).
pub fn is_adp(refname: &str) -> bool {
    refname.starts_with(ADP_REF_PREFIX)
}

/// Returns true if the ref is a version sidecar (immutable).
/// Sidecars contain `/v{N}/` in the path after the sub-namespace prefix.
pub fn is_version_sidecar(refname: &str) -> bool {
    // refs/adp/schema/v1/my-schema  → true
    // refs/adp/access/v3/config     → true
    // refs/adp/schema/my-schema     → false
    let after_prefix = refname.strip_prefix(ADP_REF_PREFIX).unwrap_or(refname);
    // after_prefix is e.g. "schema/v1/my-schema" or "schema/my-schema"
    let segments: Vec<&str> = after_prefix.splitn(3, '/').collect();
    // segments[0] = "schema", segments[1] = "v1" or "my-schema"
    if segments.len() >= 2 {
        let second = segments[1];
        second.starts_with('v') && second[1..].parse::<u32>().is_ok()
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_private() {
        assert!(is_private("refs/adp/access/config"));
        assert!(is_private("refs/adp/access/v1/config"));
        assert!(is_private("refs/adp/config/repo"));
        assert!(is_private("refs/adp/config/server"));
        assert!(!is_private("refs/adp/schema/engram-task"));
        assert!(!is_private("refs/heads/main"));
    }

    #[test]
    fn test_is_public_adp() {
        assert!(is_public_adp("refs/adp/schema/engram-task"));
        assert!(is_public_adp("refs/adp/schema/v1/engram-task"));
        assert!(!is_public_adp("refs/adp/access/config"));
        assert!(!is_public_adp("refs/heads/main"));
    }

    #[test]
    fn test_is_version_sidecar() {
        assert!(is_version_sidecar("refs/adp/schema/v1/engram-task"));
        assert!(is_version_sidecar("refs/adp/access/v3/config"));
        assert!(is_version_sidecar("refs/adp/config/v10/repo"));
        assert!(!is_version_sidecar("refs/adp/schema/engram-task"));
        assert!(!is_version_sidecar("refs/adp/access/config"));
        assert!(!is_version_sidecar("refs/heads/main"));
    }
}

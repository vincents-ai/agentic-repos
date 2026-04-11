//! ADP Secrets — AGE key management for SOPS-encrypted secrets.
//!
//! ## Architecture (ADR-024)
//!
//! Layer 1: Root AGE private key on server filesystem (`/etc/adp/wrapping-key.age`), NOT in git
//! Layer 2: Per-repo AGE keypairs — private key encrypted with root key, stored at
//!          `refs/adp/secrets/keys/{repo-id}/private` in system repo (`_adp/system`)
//! Layer 3: SOPS-encrypted JSON blobs at `refs/secrets/*` in each user repo, encrypted with repo's public key
//!
//! Public key is at `refs/meta/public-key` (public namespace, advertised in wire protocol)
//! enables zero-knowledge secret submission.
//!
//! ## Namespace layout
//!
//! ```text
//! System repo only:
//! refs/adp/secrets/keys/{repo-id}/private   — encrypted per-repo private key
//! refs/adp/secrets/keys/{repo-id}/public    — per-repo public key (plaintext)
//!
//! User repos:
//! refs/meta/public-key                     — repo's public key (advertised in wire protocol)
//! refs/secrets/*                           — SOPS-encrypted secrets (private namespace)
//! ```

pub mod key_manager;
pub mod sops;

pub use key_manager::RepoKeyManager;
pub use sops::SecretsManager;

/// The namespace for secrets keys in the system repo.
pub const SECRETS_KEYS_PREFIX: &str = "refs/adp/secrets/keys/";

/// The namespace for user repo public keys.
pub const PUBLIC_KEY_PREFIX: &str = "refs/meta/public-key";

/// The namespace for encrypted secrets in user repos.
pub const SECRETS_PREFIX: &str = "refs/secrets/";

/// Returns the system repo key ref for a given repo ID.
pub fn repo_private_key_ref(repo_id: &str) -> String {
    format!("{}refs/adp/secrets/keys/{}/private", SECRETS_KEYS_PREFIX, repo_id)
}

/// Returns the system repo public key ref for a given repo ID.
pub fn repo_public_key_ref(repo_id: &str) -> String {
    format!("{}refs/adp/secrets/keys/{}/public", SECRETS_KEYS_PREFIX, repo_id)
}

/// Returns the user repo public key ref.
pub fn user_repo_public_key_ref() -> String {
    PUBLIC_KEY_PREFIX.to_string()
}

/// Returns the secrets namespace prefix for user repos.
pub fn secrets_prefix() -> String {
    SECRETS_PREFIX.to_string()
}
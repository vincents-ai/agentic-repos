//! Caller identity resolved from ADP access control.

use crate::adp::access::AccessConfig;

/// The type of key used for authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    /// Regular user SSH key.
    User,
    /// Automated service key (agentic-loop, CI runner, integration).
    Service,
}

#[derive(Debug, Clone)]
pub struct CallerIdentity {
    pub username: String,
    pub is_owner: bool,
    pub is_admin: bool,
    /// The type of key used (user vs service).
    pub key_type: KeyType,
    /// SSH key fingerprint, if available.
    pub key_fingerprint: Option<String>,
    /// Service type for service keys (e.g. "agentic-loop", "ci-runner").
    pub service_type: Option<String>,
}

impl CallerIdentity {
    pub fn resolve(username: &str, access_config: &AccessConfig) -> Self {
        let is_owner = access_config.owners.iter().any(|o| o == username);
        let is_admin = access_config.admins.iter().any(|a| a == username) || is_owner;

        Self {
            username: username.to_string(),
            is_owner,
            is_admin,
            key_type: KeyType::User,
            key_fingerprint: None,
            service_type: None,
        }
    }

    /// Create a service key identity.
    pub fn service(username: &str, service_type: &str, key_fingerprint: &str) -> Self {
        Self {
            username: username.to_string(),
            is_owner: false,
            is_admin: false,
            key_type: KeyType::Service,
            key_fingerprint: Some(key_fingerprint.to_string()),
            service_type: Some(service_type.to_string()),
        }
    }

    pub fn can_see_private_refs(&self) -> bool {
        self.is_owner || self.is_admin
    }

    /// Whether this identity is a service key.
    pub fn is_service(&self) -> bool {
        self.key_type == KeyType::Service
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adp::access::{AccessConfig, PathRule, Permission};

    fn default_config() -> AccessConfig {
        AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec!["bob".to_string()],
            rules: vec![PathRule {
                user: "*".to_string(),
                paths: vec!["refs/heads/*".to_string()],
                permissions: vec![Permission::Pull, Permission::Push],
            }],
        }
    }

    #[test]
    fn test_owner_identity() {
        let cfg = default_config();
        let id = CallerIdentity::resolve("alice", &cfg);
        assert!(id.is_owner);
        assert!(id.is_admin);
        assert!(id.can_see_private_refs());
        assert!(!id.is_service());
    }

    #[test]
    fn test_admin_identity() {
        let cfg = default_config();
        let id = CallerIdentity::resolve("bob", &cfg);
        assert!(!id.is_owner);
        assert!(id.is_admin);
        assert!(id.can_see_private_refs());
    }

    #[test]
    fn test_regular_user_identity() {
        let cfg = default_config();
        let id = CallerIdentity::resolve("carol", &cfg);
        assert!(!id.is_owner);
        assert!(!id.is_admin);
        assert!(!id.can_see_private_refs());
    }

    #[test]
    fn test_empty_config() {
        let cfg = AccessConfig::default();
        let id = CallerIdentity::resolve("anyone", &cfg);
        assert!(!id.is_owner);
        assert!(!id.is_admin);
    }

    #[test]
    fn test_service_identity() {
        let id = CallerIdentity::service("agentic-loop-1", "agentic-loop", "SHA256:abc123");
        assert!(id.is_service());
        assert_eq!(id.service_type.as_deref(), Some("agentic-loop"));
        assert_eq!(id.key_fingerprint.as_deref(), Some("SHA256:abc123"));
        assert!(!id.can_see_private_refs());
    }
}

//! Caller identity resolved from ADP access control.

use crate::adp::access::AccessConfig;

#[derive(Debug, Clone)]
pub struct CallerIdentity {
    pub username: String,
    pub is_owner: bool,
    pub is_admin: bool,
}

impl CallerIdentity {
    pub fn resolve(username: &str, access_config: &AccessConfig) -> Self {
        let is_owner = access_config.owners.iter().any(|o| o == username);
        let is_admin = access_config.admins.iter().any(|a| a == username) || is_owner;

        Self {
            username: username.to_string(),
            is_owner,
            is_admin,
        }
    }

    pub fn can_see_private_refs(&self) -> bool {
        self.is_owner || self.is_admin
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
}

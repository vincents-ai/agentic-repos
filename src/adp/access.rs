//! ADP access control types.
//!
//! An `AccessConfig` is stored as a JSON blob at `refs/adp/access/config`
//! and controls who can read and write a repository (or the system repo).

use serde::{Deserialize, Serialize};

/// The access-control configuration stored at `refs/adp/access/config`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessConfig {
    /// Schema version string (e.g. `"1"`).
    pub version: String,

    /// Usernames that have full control over this repo, including modifying
    /// the access config itself.
    #[serde(default)]
    pub owners: Vec<String>,

    /// Usernames that can read and write all refs including `refs/adp/access/*`
    /// but cannot modify the owners list.
    #[serde(default)]
    pub admins: Vec<String>,

    /// Ordered list of path-based permission rules.  Rules are evaluated in
    /// order; the first match wins.
    #[serde(default)]
    pub rules: Vec<PathRule>,
}

/// A single path-based permission rule in an `AccessConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRule {
    /// Username this rule applies to, or `"*"` for all authenticated users.
    pub user: String,

    /// Glob patterns (using `*`/`**` syntax) for the ref names this rule
    /// covers.  Examples: `"refs/heads/*"`, `"refs/engram/**"`.
    pub paths: Vec<String>,

    /// Permissions granted by this rule.
    pub permissions: Vec<Permission>,
}

/// A user entry used for resolving per-user access in the ACL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEntry {
    pub username: String,
    pub permissions: Vec<Permission>,
}

/// The set of permissions that can appear in a `PathRule`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Read / clone permission.
    Pull,
    /// Write / push permission.
    Push,
    /// Administrative permission (implies `Pull` + `Push` plus access to
    /// private ADP refs).
    Admin,
}

impl Default for AccessConfig {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            owners: Vec::new(),
            admins: Vec::new(),
            rules: vec![PathRule {
                user: "*".to_string(),
                paths: vec!["refs/heads/*".to_string()],
                permissions: vec![Permission::Pull, Permission::Push],
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_config_round_trip() {
        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec!["bob".to_string()],
            rules: vec![
                PathRule {
                    user: "carol".to_string(),
                    paths: vec!["refs/heads/*".to_string()],
                    permissions: vec![Permission::Pull],
                },
                PathRule {
                    user: "*".to_string(),
                    paths: vec!["refs/heads/main".to_string()],
                    permissions: vec![Permission::Pull],
                },
            ],
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: AccessConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.owners, vec!["alice"]);
        assert_eq!(decoded.admins, vec!["bob"]);
        assert_eq!(decoded.rules.len(), 2);
        assert_eq!(decoded.rules[0].permissions, vec![Permission::Pull]);
    }

    #[test]
    fn test_permission_serialization() {
        let p = Permission::Admin;
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, "\"admin\"");
        let decoded: Permission = serde_json::from_str(&s).unwrap();
        assert_eq!(decoded, Permission::Admin);
    }
}

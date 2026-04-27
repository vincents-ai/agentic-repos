//! RBAC types for service key classification and namespace-based access control.
//!
//! These types extend the existing `agentic_repos::rbac` module to support
//! automated service registration (agentic-loop, CI runners, integrations).

use serde::{Deserialize, Serialize};
use std::fmt;

/// The type of service an SSH key authenticates.
///
/// Each service type is restricted to its own secrets namespace to prevent
/// cross-service impersonation. User keys cannot write to service namespaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    /// agentic-loop workflow engine instances.
    AgenticLoop,
    /// CI/CD runner instances.
    CiRunner,
    /// Third-party integration services (GitHub, Slack, etc.).
    Integration,
    /// Regular user keys.
    User,
}

impl fmt::Display for ServiceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceType::AgenticLoop => write!(f, "agentic-loop"),
            ServiceType::CiRunner => write!(f, "ci-runner"),
            ServiceType::Integration => write!(f, "integration"),
            ServiceType::User => write!(f, "user"),
        }
    }
}

impl std::str::FromStr for ServiceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "agentic-loop" => Ok(ServiceType::AgenticLoop),
            "ci-runner" => Ok(ServiceType::CiRunner),
            "integration" => Ok(ServiceType::Integration),
            "user" => Ok(ServiceType::User),
            other => Err(format!("unknown service type: {}", other)),
        }
    }
}

impl ServiceType {
    /// The default secrets namespace for this service type.
    pub fn default_namespace(&self) -> &'static str {
        match self {
            ServiceType::AgenticLoop => "refs/secrets/agentic-loop/",
            ServiceType::CiRunner => "refs/secrets/ci-runners/",
            ServiceType::Integration => "refs/secrets/integrations/",
            ServiceType::User => "refs/secrets/users/",
        }
    }

    /// Default glob patterns for paths this service type can write to.
    pub fn default_write_patterns(&self) -> Vec<String> {
        vec![format!("{}*", self.default_namespace())]
    }

    /// Default glob patterns for paths this service type is denied from writing.
    ///
    /// User keys are denied access to all service namespaces.
    /// Service keys have no cross-service denials by default.
    pub fn default_denied_patterns(&self) -> Vec<String> {
        match self {
            ServiceType::User => vec![
                "refs/secrets/agentic-loop/*".to_string(),
                "refs/secrets/ci-runners/*".to_string(),
                "refs/secrets/integrations/*".to_string(),
            ],
            _ => vec![],
        }
    }
}

/// Permissions associated with an SSH key, stored at `refs/adp/keys/{key_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPermissions {
    /// SSH key fingerprint or identifier.
    pub key_id: String,
    /// The service type this key is classified as.
    pub service_type: ServiceType,
    /// Optional sub-type for integrations (e.g. "github", "slack").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_subtype: Option<String>,
    /// Glob patterns for allowed write paths.
    pub allowed_write_patterns: Vec<String>,
    /// Glob patterns for denied write paths (takes precedence over allowed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_write_patterns: Vec<String>,
    /// Glob patterns for allowed read paths.
    pub allowed_read_patterns: Vec<String>,
    /// Optional CODEOWNERS-style path restrictions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_restrictions: Option<PathRestrictions>,
    /// Who created this key entry.
    pub created_by: String,
    /// When this key was registered.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional expiration for the key entry itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Arbitrary tags for grouping/filtering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// CODEOWNERS-style path restrictions for monorepo access control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRestrictions {
    /// Path to CODEOWNERS file (default: "CODEOWNERS").
    #[serde(default = "default_codeowners_file")]
    pub codeowners_file: String,
    /// Explicitly allowed paths (if set, deny all others).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_paths: Vec<String>,
    /// Explicitly denied paths (overrides allowed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_paths: Vec<String>,
    /// Whether to enforce CODEOWNERS file for this key.
    #[serde(default)]
    pub enforce_codeowners: bool,
    /// Whether changes to owned paths require owner approval.
    #[serde(default)]
    pub require_owner_approval: bool,
}

fn default_codeowners_file() -> String {
    "CODEOWNERS".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_type_round_trip() {
        let variants = [
            ServiceType::AgenticLoop,
            ServiceType::CiRunner,
            ServiceType::Integration,
            ServiceType::User,
        ];
        for v in &variants {
            let s = v.to_string();
            let parsed: ServiceType = s.parse().unwrap();
            assert_eq!(*v, parsed);
        }
    }

    #[test]
    fn service_type_from_str_invalid() {
        assert!("unknown".parse::<ServiceType>().is_err());
    }

    #[test]
    fn key_permissions_serialization() {
        let perms = KeyPermissions {
            key_id: "SHA256:abc123".to_string(),
            service_type: ServiceType::AgenticLoop,
            service_subtype: None,
            allowed_write_patterns: vec!["refs/secrets/agentic-loop/*".to_string()],
            denied_write_patterns: vec![],
            allowed_read_patterns: vec!["**".to_string()],
            path_restrictions: None,
            created_by: "admin@example.com".to_string(),
            created_at: chrono::Utc::now(),
            expires_at: None,
            tags: vec![],
        };
        let json = serde_json::to_string(&perms).unwrap();
        let decoded: KeyPermissions = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.key_id, "SHA256:abc123");
        assert_eq!(decoded.service_type, ServiceType::AgenticLoop);
    }
}

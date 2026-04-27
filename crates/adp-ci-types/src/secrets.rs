//! Secret mounting — Kubernetes-pattern secret injection.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A secret to be mounted into the build workspace.
///
/// Follows the Kubernetes Secrets API pattern:
/// - Data keys are base64-encoded values
/// - Files are mounted in a tmpfs directory
/// - Environment variables are injected from specified keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMount {
    /// Secret name (used as directory name under /run/secrets/).
    pub name: String,
    /// Secret type (for future extensibility).
    #[serde(default = "default_secret_type")]
    pub kind: String,
    /// Base64-encoded data. Keys become filenames in the mount.
    pub data: HashMap<String, String>,
    /// Map of environment variable name → data key.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env_mapping: HashMap<String, String>,
    /// Where to mount the secret files in the workspace.
    #[serde(default = "default_mount_path")]
    pub mount_path: String,
    /// Whether to mount as tmpfs (recommended).
    #[serde(default = "default_true")]
    pub tmpfs: bool,
    /// Whether the secret is optional (build continues if missing).
    #[serde(default)]
    pub optional: bool,
}

fn default_secret_type() -> String {
    "Opaque".to_string()
}

fn default_mount_path() -> String {
    "/run/secrets".to_string()
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_mount_round_trip() {
        let mount = SecretMount {
            name: "deploy-credentials".into(),
            kind: "Opaque".into(),
            data: {
                let mut m = HashMap::new();
                m.insert("db-password".into(), "czNjcmV0".into());
                m.insert("api-key".into(), "YXBpLWtleQ==".into());
                m
            },
            env_mapping: {
                let mut m = HashMap::new();
                m.insert("DB_PASSWORD".into(), "db-password".into());
                m.insert("API_KEY".into(), "api-key".into());
                m
            },
            mount_path: "/run/secrets/deploy-credentials".into(),
            tmpfs: true,
            optional: false,
        };
        let json = serde_json::to_string(&mount).unwrap();
        let decoded: SecretMount = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "deploy-credentials");
        assert_eq!(decoded.data.len(), 2);
        assert_eq!(decoded.env_mapping.get("DB_PASSWORD").unwrap(), "db-password");
    }
}

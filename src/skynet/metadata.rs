//! Plugin metadata for Skynet v2 integration
//!
//! This module defines the metadata that describes the Git Operations Plugin
//! to the Skynet v2 orchestration platform.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin metadata for Skynet v2 registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name
    pub name: String,

    /// Plugin version
    pub version: String,

    /// Plugin author
    pub author: String,

    /// Plugin description
    pub description: String,

    /// List of capabilities this plugin provides
    pub capabilities: Vec<String>,

    /// List of required plugins
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Minimum Skynet version required
    pub min_skynet_version: String,

    /// Configuration schema
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,

    /// Plugin tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Plugin endpoints
    #[serde(default)]
    pub endpoints: HashMap<String, String>,
}

impl PluginMetadata {
    /// Create metadata for the Git Operations Plugin
    pub fn git_operations() -> Self {
        Self {
            name: "git-operations".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            author: "Vincents.ai Team".to_string(),
            description: "Multi-tenant Git server with SSH authentication, Git LFS support, \
                         S3-compatible storage backends, rate limiting, and comprehensive monitoring"
                .to_string(),
            capabilities: vec![
                // Repository operations
                "repo.create".to_string(),
                "repo.delete".to_string(),
                "repo.list".to_string(),
                "repo.info".to_string(),
                "repo.clone".to_string(),
                // Git operations
                "git.push".to_string(),
                "git.pull".to_string(),
                "git.branch".to_string(),
                "git.tag".to_string(),
                "git.log".to_string(),
                "git.diff".to_string(),
                // SSH authentication
                "auth.ssh_key_add".to_string(),
                "auth.ssh_key_remove".to_string(),
                "auth.ssh_key_list".to_string(),
                "auth.ssh_key_verify".to_string(),
                // Git LFS
                "lfs.upload".to_string(),
                "lfs.download".to_string(),
                "lfs.lock".to_string(),
                "lfs.unlock".to_string(),
                "lfs.batch".to_string(),
                // User management
                "user.create".to_string(),
                "user.delete".to_string(),
                "user.list".to_string(),
                "user.update".to_string(),
                // Tenant management
                "tenant.create".to_string(),
                "tenant.delete".to_string(),
                "tenant.list".to_string(),
                "tenant.update".to_string(),
                // Rate limiting
                "ratelimit.get".to_string(),
                "ratelimit.set".to_string(),
                "ratelimit.reset".to_string(),
                // Metrics and monitoring
                "metrics.get".to_string(),
                "metrics.export".to_string(),
                "health.check".to_string(),
                "health.status".to_string(),
                // Storage management
                "storage.info".to_string(),
                "storage.cleanup".to_string(),
            ],
            dependencies: vec![],
            min_skynet_version: "2.0.0".to_string(),
            config_schema: None,
            tags: vec![
                "git".to_string(),
                "multi-tenant".to_string(),
                "lfs".to_string(),
                "ssh".to_string(),
                "s3".to_string(),
                "storage".to_string(),
            ],
            endpoints: {
                let mut map = HashMap::new();
                map.insert("health".to_string(), "/health".to_string());
                map.insert("metrics".to_string(), "/metrics".to_string());
                map.insert("api".to_string(), "/api/v1".to_string());
                map
            },
        }
    }
}

/// Generate metadata for the Git Operations Plugin
pub fn git_operations_metadata() -> PluginMetadata {
    PluginMetadata::git_operations()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_creation() {
        let metadata = git_operations_metadata();
        assert_eq!(metadata.name, "git-operations");
        assert_eq!(metadata.author, "Vincents.ai Team");
        assert!(!metadata.capabilities.is_empty());
        assert!(!metadata.tags.is_empty());
    }

    #[test]
    fn test_metadata_has_required_capabilities() {
        let metadata = git_operations_metadata();
        assert!(metadata.capabilities.contains(&"repo.create".to_string()));
        assert!(metadata.capabilities.contains(&"repo.delete".to_string()));
        assert!(metadata.capabilities.contains(&"git.push".to_string()));
        assert!(metadata.capabilities.contains(&"git.pull".to_string()));
        assert!(metadata.capabilities.contains(&"lfs.upload".to_string()));
        assert!(metadata.capabilities.contains(&"health.check".to_string()));
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = git_operations_metadata();
        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: PluginMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, metadata.name);
        assert_eq!(deserialized.capabilities.len(), metadata.capabilities.len());
    }
}

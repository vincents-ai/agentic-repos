//! Skynet v2 Git Operations Plugin
//!
//! This module provides a complete Git Operations Plugin for Skynet v2 integration,
//! offering multi-tenant Git server capabilities, SSH authentication, Git LFS support,
//! configurable storage backends, rate limiting, and comprehensive monitoring.
//!
//! The plugin implements the Skynet v2 PluginApi trait and exposes Git operations
//! through a JSON-RPC style interface compatible with the Skynet v2 orchestration platform.

pub mod api;
pub mod config;
pub mod database_models;
pub mod error;
pub mod ffi;
pub mod git_ops;
pub mod health;
pub mod interface;
pub mod metadata;
pub mod monitoring;
pub mod multi_tenant;
pub mod rate_limit;
pub mod ssh_keys;
pub mod storage;

pub use error::{SkynetPluginError, SkynetPluginResult};
pub use interface::{GitOperationsPlugin, PluginResponse};
pub use metadata::{git_operations_metadata, PluginMetadata};

use std::sync::Arc;
use tracing::{debug, info};

/// Plugin version
pub const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Plugin name for Skynet v2 registration
pub const PLUGIN_NAME: &str = "git-operations";

/// Plugin author
pub const PLUGIN_AUTHOR: &str = "Vincents.ai Team";

/// Plugin description
pub const PLUGIN_DESCRIPTION: &str =
    "Multi-tenant Git server with SSH authentication, Git LFS support, \
     S3-compatible storage backends, rate limiting, and comprehensive monitoring";

/// Skynet v2 minimum version compatibility
pub const MIN_SKYNET_VERSION: &str = "2.0.0";

/// Initialize the Git Operations Plugin for Skynet v2
///
/// This function performs all necessary initialization including:
/// - Setting up logging and tracing
/// - Initializing storage backends
/// - Configuring multi-tenancy
/// - Starting monitoring services
///
/// # Errors
///
/// Returns an error if:
/// - Configuration cannot be loaded
/// - Storage backends cannot be initialized
/// - Monitoring services fail to start
pub async fn init_plugin(
    config: crate::config::AppConfig,
) -> SkynetPluginResult<Arc<GitOperationsPlugin>> {
    info!(
        "Initializing {} plugin v{} for Skynet v2",
        PLUGIN_NAME, PLUGIN_VERSION
    );

    debug!("Creating Git Operations Plugin instance");
    let plugin = GitOperationsPlugin::new(config).await?;

    info!("Successfully initialized {} plugin", PLUGIN_NAME);

    Ok(Arc::new(plugin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_constants() {
        assert!(!PLUGIN_NAME.is_empty());
        assert!(!PLUGIN_VERSION.is_empty());
        assert!(!PLUGIN_AUTHOR.is_empty());
        assert!(!PLUGIN_DESCRIPTION.is_empty());
        assert!(!MIN_SKYNET_VERSION.is_empty());
    }

    #[test]
    fn test_plugin_metadata_generation() {
        let metadata = git_operations_metadata();
        assert_eq!(metadata.name, PLUGIN_NAME);
        assert_eq!(metadata.version, PLUGIN_VERSION);
        assert_eq!(metadata.author, PLUGIN_AUTHOR);
        assert!(!metadata.capabilities.is_empty());
    }
}

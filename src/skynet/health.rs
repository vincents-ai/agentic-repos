//! Health checking for Skynet v2 plugin

use crate::skynet::error::SkynetPluginResult;
use crate::skynet::interface::HealthStatus;
use tracing::{debug, warn};

/// Health checker — verifies critical subsystems are operational.
pub struct HealthChecker;

impl HealthChecker {
    /// Create a new health checker
    pub fn new() -> Self {
        Self
    }

    /// Check plugin health by verifying subsystems.
    pub async fn check(&self) -> SkynetPluginResult<HealthStatus> {
        debug!("Performing health check");

        let mut errors: Vec<String> = Vec::new();

        // Check storage backends are registered
        let registry = crate::skynet::storage::interface::global_registry();
        let backends = registry.list_backends();
        if backends.is_empty() {
            let msg = "No storage backends registered".to_string();
            warn!("Health check: {}", msg);
            errors.push(msg);
        }

        let overall_status = if errors.is_empty() {
            "healthy"
        } else {
            "degraded"
        };

        Ok(HealthStatus {
            status: overall_status.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: 0,
            active_repositories: backends.len(), // use backend count as proxy
            active_sessions: 0,
            storage_bytes: 0,
            checked_at: chrono::Utc::now().to_rfc3339(),
            errors,
        })
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

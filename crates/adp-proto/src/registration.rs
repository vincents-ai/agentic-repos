//! Registration types exchanged during the SSE handshake between
//! agentic-loop instances and the adp server.

use crate::rbac::ServiceType;
use serde::{Deserialize, Serialize};

/// Capabilities a service instance can declare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceCapabilities {
    /// Can execute workflows.
    Workflows,
    /// Can manage agent sessions.
    Sessions,
    /// Can create/read/update entities.
    Entities,
    /// Can trigger deployments.
    Deployments,
    /// Can manage webhooks.
    Webhooks,
}

/// Information about a registered service instance, exchanged during
/// the SSE handshake and stored at `refs/adp/instances/{instance_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationInfo {
    /// Unique instance identifier.
    pub instance_id: uuid::Uuid,
    /// Connection secret (short-lived, SOPS-encrypted in git refs).
    pub secret: String,
    /// When this registration was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this registration expires.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// The service type of this instance.
    pub service_type: ServiceType,
    /// Instance metadata (hostname, OS, etc.).
    pub metadata: ServiceMetadata,
    /// Capabilities this instance declares.
    pub capabilities: Vec<InstanceCapabilities>,
    /// Projects this instance works with.
    pub project_ids: Vec<String>,
}

impl RegistrationInfo {
    /// Calculate the TTL remaining in seconds.
    pub fn ttl_remaining(&self) -> i64 {
        let now = chrono::Utc::now();
        self.expires_at
            .signed_duration_since(now)
            .num_seconds()
    }

    /// Check if the registration has expired.
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    /// Check if the registration needs renewal within the given threshold.
    pub fn needs_renewal(&self, threshold_secs: i64) -> bool {
        self.ttl_remaining() < threshold_secs
    }
}

/// Metadata about a service instance's host environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetadata {
    /// Human-readable instance name.
    pub instance_name: String,
    /// Machine hostname.
    pub hostname: String,
    /// Operating system.
    pub os: String,
    /// CPU architecture.
    pub arch: String,
}

impl ServiceMetadata {
    /// Detect metadata from the current environment.
    pub fn detect() -> Self {
        Self {
            instance_name: std::env::var("INSTANCE_NAME").unwrap_or_else(|_| {
                hostname::get()
                    .map(|h| h.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "unknown".to_string())
            }),
            hostname: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

/// Credentials sent by agentic-loop in the SSE connection request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceCredentials {
    /// Instance identifier.
    pub instance_id: String,
    /// Connection secret.
    pub secret: String,
    /// Repository or project identifier.
    pub repo_identifier: String,
    /// Seconds until the secret expires.
    pub ttl_remaining: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_ttl_calculation() {
        let info = RegistrationInfo {
            instance_id: uuid::Uuid::new_v4(),
            secret: "test".to_string(),
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            service_type: ServiceType::AgenticLoop,
            metadata: ServiceMetadata::detect(),
            capabilities: vec![InstanceCapabilities::Workflows],
            project_ids: vec!["default".to_string()],
        };

        let ttl = info.ttl_remaining();
        assert!(ttl > 3500 && ttl <= 3600, "TTL should be ~3600, got {}", ttl);
        assert!(!info.is_expired());
        assert!(!info.needs_renewal(3000)); // 50 min threshold
        assert!(info.needs_renewal(3700)); // just over the TTL
    }

    #[test]
    fn expired_registration() {
        let info = RegistrationInfo {
            instance_id: uuid::Uuid::new_v4(),
            secret: "test".to_string(),
            created_at: chrono::Utc::now() - chrono::Duration::hours(2),
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
            service_type: ServiceType::CiRunner,
            metadata: ServiceMetadata::detect(),
            capabilities: vec![],
            project_ids: vec![],
        };
        assert!(info.is_expired());
        assert!(info.ttl_remaining() < 0);
    }
}

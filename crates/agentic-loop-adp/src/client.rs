//! Unified ADP client — manages registration, SSE connection, and entity writing.

use crate::handlers::{EntityUpdateHandler, SchemaChangeHandler, TtlWarningHandler};
use agentic_loop_registration::{RegistrationManager, RegistrationStatus};
use agentic_loop_sse_client::{SseClient, SseClientConfig};
use agentic_loop_entity_writer::EntityWriter;
use adp_proto::rbac::ServiceType;
use adp_proto::registration::InstanceCapabilities;
use adp_schema_registry::SchemaRegistry;
use std::sync::Arc;
use tracing::{info, instrument};

/// Configuration for the ADP client.
#[derive(Debug, Clone)]
pub struct AdpClientConfig {
    /// ADP server URL (e.g. "http://localhost:8090").
    pub server_url: String,
    /// Repository or project identifier.
    pub repo_identifier: String,
    /// Service type for RBAC classification.
    pub service_type: ServiceType,
    /// Capabilities this instance declares.
    pub capabilities: Vec<InstanceCapabilities>,
    /// Default TTL in seconds.
    pub default_ttl_secs: i64,
    /// Whether to auto-renew secrets.
    pub auto_renew: bool,
    /// Maximum reconnection attempts.
    pub max_reconnect_attempts: u32,
}

impl Default for AdpClientConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:8090".to_string(),
            repo_identifier: "default".to_string(),
            service_type: ServiceType::AgenticLoop,
            capabilities: vec![
                InstanceCapabilities::Workflows,
                InstanceCapabilities::Entities,
                InstanceCapabilities::Sessions,
            ],
            default_ttl_secs: 3600,
            auto_renew: true,
            max_reconnect_attempts: 10,
        }
    }
}

/// Unified ADP client managing the full integration lifecycle.
///
/// 1. Registers with the ADP server (generates secret)
/// 2. Connects SSE stream (receives events)
/// 3. Validates and writes entities (schema-validated)
/// 4. Monitors TTL and auto-renews
pub struct AdpClient {
    config: AdpClientConfig,
    registration: Arc<RegistrationManager>,
    schema_registry: Arc<SchemaRegistry>,
}

impl AdpClient {
    /// Create a new ADP client.
    pub fn new(config: AdpClientConfig) -> Self {
        let reg_config = agentic_loop_registration::RegistrationConfig {
            default_ttl_secs: config.default_ttl_secs,
            renewal_threshold_secs: config.default_ttl_secs / 4,
            max_ttl_secs: 86400,
        };
        Self {
            config,
            registration: Arc::new(RegistrationManager::new(reg_config)),
            schema_registry: Arc::new(SchemaRegistry::new()),
        }
    }

    /// Get the registration manager.
    pub fn registration(&self) -> &Arc<RegistrationManager> {
        &self.registration
    }

    /// Get the schema registry.
    pub fn schema_registry(&self) -> &Arc<SchemaRegistry> {
        &self.schema_registry
    }

    /// Create an entity writer backed by this client's schema registry.
    pub fn entity_writer(&self) -> EntityWriter {
        // Clone the Arc (cheap), EntityWriter takes ownership of a new registry
        // For now, create a new registry since EntityWriter owns its own
        EntityWriter::new(SchemaRegistry::new())
    }

    /// Register with the ADP server and return the registration info.
    #[instrument(skip(self))]
    pub async fn register(&self) -> Result<adp_proto::registration::RegistrationInfo, String> {
        let info = self
            .registration
            .register(
                self.config.service_type,
                self.config.capabilities.clone(),
                vec![self.config.repo_identifier.clone()],
            )
            .map_err(|e| e.to_string())?;

        info!(
            instance_id = %info.instance_id,
            service_type = %self.config.service_type,
            "Registered with ADP server"
        );

        Ok(info)
    }

    /// Connect to the SSE stream using the current registration.
    #[instrument(skip(self))]
    pub async fn connect_sse(&self) -> Result<Arc<SseClient>, String> {
        let info = self
            .registration
            .current()
            .ok_or("Not registered".to_string())?;

        let sse_config = SseClientConfig::new(
            &self.config.server_url,
            &info.instance_id.to_string(),
            &info.secret,
            &self.config.repo_identifier,
            info.ttl_remaining(),
        );

        let mut client = SseClient::new(sse_config);

        // Register default handlers
        client.add_handler(Box::new(SchemaChangeHandler));
        client.add_handler(Box::new(TtlWarningHandler {
            auto_renew: self.config.auto_renew,
        }));
        client.add_handler(Box::new(EntityUpdateHandler));

        let client = Arc::new(client);
        info!("SSE client configured with event handlers");
        Ok(client)
    }

    /// Get the current registration status.
    pub fn status(&self) -> RegistrationStatus {
        self.registration.status()
    }

    /// Renew the registration secret.
    pub fn renew(&self) -> Result<adp_proto::registration::RegistrationInfo, String> {
        self.registration.renew().map_err(|e| e.to_string())
    }

    /// Deregister from the ADP server.
    pub fn deregister(&self) -> Result<(), String> {
        self.registration.deregister().map_err(|e| e.to_string())
    }

    /// Check if the client needs renewal.
    pub fn needs_renewal(&self) -> bool {
        self.registration.needs_renewal()
    }

    /// Get current TTL remaining.
    pub fn ttl_remaining(&self) -> Option<i64> {
        self.registration.ttl_remaining()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let config = AdpClientConfig::default();
        assert_eq!(config.server_url, "http://localhost:8090");
        assert_eq!(config.service_type, ServiceType::AgenticLoop);
        assert_eq!(config.default_ttl_secs, 3600);
        assert!(config.auto_renew);
    }

    #[test]
    fn client_creation() {
        let client = AdpClient::new(AdpClientConfig::default());
        assert_eq!(client.status(), RegistrationStatus::Unregistered);
        assert!(client.ttl_remaining().is_none());
    }

    #[test]
    fn entity_writer_from_client() {
        let client = AdpClient::new(AdpClientConfig::default());
        let _writer = client.entity_writer();
    }

    #[tokio::test]
    async fn register_and_check_status() {
        let client = AdpClient::new(AdpClientConfig::default());
        let info = client.register().await.unwrap();
        assert!(!info.instance_id.to_string().is_empty());
        assert_eq!(client.status(), RegistrationStatus::Active);
        assert!(client.ttl_remaining().is_some());
    }

    #[tokio::test]
    async fn register_then_deregister() {
        let client = AdpClient::new(AdpClientConfig::default());
        client.register().await.unwrap();
        client.deregister().unwrap();
        assert_eq!(client.status(), RegistrationStatus::Unregistered);
    }

    #[tokio::test]
    async fn renew_after_register() {
        let client = AdpClient::new(AdpClientConfig::default());
        let original = client.register().await.unwrap();
        let renewed = client.renew().unwrap();
        assert_ne!(renewed.secret, original.secret);
    }
}

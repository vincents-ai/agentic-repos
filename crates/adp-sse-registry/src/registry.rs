//! SSE connection registry — manages active SSE connections from agentic-loop instances.

use crate::error::SseRegistryError;
use crate::stream;
use adp_proto::sse::SseEvent;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, instrument, warn};

/// Connection identifier (instance UUID).
pub type ConnectionId = String;

/// State tracked for each active connection.
#[derive(Debug, Clone)]
pub struct ConnectionState {
    /// Instance identifier.
    pub instance_id: String,
    /// Repository/project identifier.
    pub repo_identifier: String,
    /// When the secret expires.
    pub secret_expires_at: DateTime<Utc>,
    /// When the connection was established.
    pub connected_at: DateTime<Utc>,
    /// TTL remaining at connection time.
    pub ttl_remaining_at_connect: i64,
}

/// SSE connection registry.
///
/// Manages active SSE connections from agentic-loop instances.
/// Thread-safe via `Arc<RwLock<...>>`.
pub struct SseRegistry {
    /// Active connections keyed by instance ID.
    connections: RwLock<HashMap<ConnectionId, InternalConnection>>,
    /// Broadcast channel for global events (all instances).
    #[allow(dead_code)]
    global_tx: broadcast::Sender<SseEvent>,
}

struct InternalConnection {
    /// Connection state.
    state: ConnectionState,
    /// Per-instance broadcast sender.
    tx: broadcast::Sender<SseEvent>,
}

impl SseRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        let (global_tx, _) = broadcast::channel(1024);
        Self {
            connections: RwLock::new(HashMap::new()),
            global_tx,
        }
    }

    /// Register a new SSE connection.
    ///
    /// Returns a stream of SSE events for the connection.
    /// Validates the TTL and rejects connections with insufficient TTL.
    #[instrument(skip(self), fields(instance_id = %instance_id))]
    pub async fn connect(
        &self,
        instance_id: String,
        _secret: String,
        repo_identifier: String,
        ttl_remaining: i64,
    ) -> Result<crate::stream::SseStream, SseRegistryError> {
        let config = crate::SseRegistryConfig::default();

        // Check minimum TTL
        if ttl_remaining < config.min_ttl_for_connection_secs {
            return Err(SseRegistryError::SecretTooCloseToExpiry {
                ttl_remaining,
                min_required: config.min_ttl_for_connection_secs,
            });
        }

        // Calculate server-side expiry
        let now = Utc::now();
        let secret_expires_at = now + chrono::Duration::seconds(ttl_remaining);

        // Create per-instance channel
        let (tx, rx) = broadcast::channel(config.channel_buffer_size);

        let state = ConnectionState {
            instance_id: instance_id.clone(),
            repo_identifier,
            secret_expires_at,
            connected_at: now,
            ttl_remaining_at_connect: ttl_remaining,
        };

        // Register
        {
            let mut connections = self.connections.write().await;
            connections.insert(
                instance_id.clone(),
                InternalConnection {
                    state: state.clone(),
                    tx: tx.clone(),
                },
            );
        }

        info!(
            instance_id = %instance_id,
            ttl_remaining = ttl_remaining,
            "SSE connection established"
        );

        // Send registered event
        let registered = SseEvent::Registered {
            instance_id: instance_id.clone(),
            ttl_remaining,
            ttl_expires_at: secret_expires_at.to_rfc3339(),
        };
        let _ = tx.send(registered);

        // Create stream
        let sse_stream = stream::create_stream(rx);
        Ok(Box::pin(sse_stream))
    }

    /// Unregister a connection (on disconnect).
    #[instrument(skip(self), fields(instance_id = %instance_id))]
    pub async fn disconnect(&self, instance_id: &str) {
        let mut connections = self.connections.write().await;
        if connections.remove(instance_id).is_some() {
            debug!(instance_id = instance_id, "SSE connection removed");
        }
    }

    /// Send an event to a specific instance.
    pub async fn send_to(&self, instance_id: &str, event: SseEvent) -> Result<(), SseRegistryError> {
        let connections = self.connections.read().await;
        if let Some(conn) = connections.get(instance_id) {
            conn.tx
                .send(event)
                .map_err(|_| SseRegistryError::ChannelSendFailed)?;
            Ok(())
        } else {
            Err(SseRegistryError::InstanceNotFound(instance_id.to_string()))
        }
    }

    /// Broadcast an event to all connected instances.
    pub async fn broadcast(&self, event: SseEvent) {
        let connections = self.connections.read().await;
        let count = connections.len();
        for (_, conn) in connections.iter() {
            let _ = conn.tx.send(event.clone());
        }
        debug!(count, "Broadcast event to all instances");
    }

    /// List all active connections.
    pub async fn list_connections(&self) -> Vec<ConnectionState> {
        let connections = self.connections.read().await;
        connections.values().map(|c| c.state.clone()).collect()
    }

    /// Get connection state for a specific instance.
    pub async fn get_connection(&self, instance_id: &str) -> Option<ConnectionState> {
        let connections = self.connections.read().await;
        connections.get(instance_id).map(|c| c.state.clone())
    }

    /// Check if an instance is connected.
    pub async fn is_connected(&self, instance_id: &str) -> bool {
        let connections = self.connections.read().await;
        connections.contains_key(instance_id)
    }

    /// Get number of active connections.
    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    /// Check all connections for TTL expiry and send warnings/disconnects.
    /// Should be called periodically (e.g., every 60 seconds).
    pub async fn check_ttl_expiry(&self) {
        let now = Utc::now();
        let config = crate::SseRegistryConfig::default();
        let mut expired = Vec::new();

        let connections = self.connections.read().await;
        for (id, conn) in connections.iter() {
            let ttl_remaining = conn.state.secret_expires_at.signed_duration_since(now).num_seconds();

            if ttl_remaining <= 0 {
                // Expired — mark for disconnect
                let _ = conn.tx.send(SseEvent::TtlExpired);
                expired.push(id.clone());
                warn!(instance_id = %id, "Secret expired, disconnecting");
            } else if ttl_remaining <= config.ttl_warning_threshold_secs {
                // Warning threshold
                let _ = conn.tx.send(SseEvent::TtlWarning {
                    ttl_remaining,
                    message: format!("Secret expires in {} seconds", ttl_remaining),
                });
                debug!(instance_id = %id, ttl_remaining, "TTL warning sent");
            }
        }
        drop(connections);

        // Remove expired
        if !expired.is_empty() {
            let mut connections = self.connections.write().await;
            for id in expired {
                connections.remove(&id);
            }
        }
    }
}

impl Default for SseRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_and_disconnect() {
        let registry = SseRegistry::new();
        let instance_id = uuid::Uuid::new_v4().to_string();

        // Connect
        let stream = registry
            .connect(
                instance_id.clone(),
                "secret".to_string(),
                "default".to_string(),
                3600,
            )
            .await;
        assert!(stream.is_ok());
        assert_eq!(registry.connection_count().await, 1);

        // Disconnect
        registry.disconnect(&instance_id).await;
        assert_eq!(registry.connection_count().await, 0);
    }

    #[tokio::test]
    async fn connect_rejects_low_ttl() {
        let registry = SseRegistry::new();
        let result = registry
            .connect(
                "test".to_string(),
                "secret".to_string(),
                "default".to_string(),
                100, // Below 300 min
            )
            .await;
        assert!(result.is_err());
        if let Err(SseRegistryError::SecretTooCloseToExpiry { ttl_remaining, .. }) = result {
            assert_eq!(ttl_remaining, 100);
        } else {
            panic!("Expected SecretTooCloseToExpiry error");
        }
    }

    #[tokio::test]
    async fn send_to_instance() {
        let registry = SseRegistry::new();
        let instance_id = uuid::Uuid::new_v4().to_string();

        let _stream = registry
            .connect(instance_id.clone(), "secret".to_string(), "default".to_string(), 3600)
            .await
            .unwrap();

        let result = registry
            .send_to(&instance_id, SseEvent::Heartbeat { timestamp: "t".to_string() })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn send_to_unknown_instance() {
        let registry = SseRegistry::new();
        let result = registry
            .send_to("unknown", SseEvent::Heartbeat { timestamp: "t".to_string() })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn broadcast_sends_to_all() {
        let registry = SseRegistry::new();

        let id1 = uuid::Uuid::new_v4().to_string();
        let id2 = uuid::Uuid::new_v4().to_string();

        let _s1 = registry.connect(id1, "s".into(), "r".into(), 3600).await.unwrap();
        let _s2 = registry.connect(id2, "s".into(), "r".into(), 3600).await.unwrap();

        registry
            .broadcast(SseEvent::SchemaChange {
                event_type: "created".into(),
                schema_id: "test".into(),
                timestamp: "t".into(),
            })
            .await;

        assert_eq!(registry.connection_count().await, 2);
    }

    #[tokio::test]
    async fn list_connections() {
        let registry = SseRegistry::new();
        let id1 = uuid::Uuid::new_v4().to_string();
        let id2 = uuid::Uuid::new_v4().to_string();

        let _s1 = registry.connect(id1.clone(), "s".into(), "repo1".into(), 3600).await.unwrap();
        let _s2 = registry.connect(id2.clone(), "s".into(), "repo2".into(), 3600).await.unwrap();

        let connections = registry.list_connections().await;
        assert_eq!(connections.len(), 2);

        let ids: Vec<&str> = connections.iter().map(|c| c.instance_id.as_str()).collect();
        assert!(ids.contains(&id1.as_str()));
        assert!(ids.contains(&id2.as_str()));
    }
}

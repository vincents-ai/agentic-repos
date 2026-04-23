//! SSE event types exchanged between adp (server) and agentic-loop (client).
//!
//! All events are tagged with `#[serde(tag = "type")]` for consistent JSON framing:
//! ```json
//! {"type": "registered", "instance_id": "...", "ttl_remaining": 3600}
//! ```

use serde::{Deserialize, Serialize};

/// Events pushed from adp to agentic-loop instances over the SSE connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum SseEvent {
    /// Connection accepted. Confirms the instance is registered and provides
    /// the server's view of the TTL.
    #[serde(rename = "registered")]
    Registered {
        instance_id: String,
        ttl_remaining: i64,
        ttl_expires_at: String,
    },

    /// A schema was created, updated, or deleted. The instance should pull the
    /// latest schema definitions from git refs.
    #[serde(rename = "schema_change")]
    SchemaChange {
        event_type: String,
        schema_id: String,
        timestamp: String,
    },

    /// An entity was created, updated, or deleted. The instance may want to
    /// invalidate caches or trigger local processing.
    #[serde(rename = "entity_update")]
    EntityUpdate {
        entity_id: String,
        schema_id: String,
        action: String,
    },

    /// Server heartbeat. Sent periodically to keep the connection alive.
    #[serde(rename = "heartbeat")]
    Heartbeat { timestamp: String },

    /// The instance's secret is approaching expiry. The instance should renew
    /// its secret before it expires.
    #[serde(rename = "ttl_warning")]
    TtlWarning {
        ttl_remaining: i64,
        message: String,
    },

    /// The instance's secret has expired. The connection will be closed.
    /// The instance must re-register to reconnect.
    #[serde(rename = "ttl_expired")]
    TtlExpired,

    /// The connection was closed by the server. Includes a reason.
    #[serde(rename = "disconnected")]
    Disconnected { reason: String },
}

impl SseEvent {
    /// Returns the event type tag string (e.g. "registered", "heartbeat").
    pub fn event_type(&self) -> &'static str {
        match self {
            SseEvent::Registered { .. } => "registered",
            SseEvent::SchemaChange { .. } => "schema_change",
            SseEvent::EntityUpdate { .. } => "entity_update",
            SseEvent::Heartbeat { .. } => "heartbeat",
            SseEvent::TtlWarning { .. } => "ttl_warning",
            SseEvent::TtlExpired => "ttl_expired",
            SseEvent::Disconnected { .. } => "disconnected",
        }
    }

    /// Format as an SSE data line (`data: {...}\n\n`).
    pub fn to_sse_data(&self) -> String {
        format!("data: {}\n\n", serde_json::to_string(self).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_event_types_are_unique() {
        let events = [
            SseEvent::Registered {
                instance_id: "a".into(),
                ttl_remaining: 0,
                ttl_expires_at: "t".into(),
            },
            SseEvent::SchemaChange {
                event_type: "created".into(),
                schema_id: "s".into(),
                timestamp: "t".into(),
            },
            SseEvent::EntityUpdate {
                entity_id: "e".into(),
                schema_id: "s".into(),
                action: "created".into(),
            },
            SseEvent::Heartbeat {
                timestamp: "t".into(),
            },
            SseEvent::TtlWarning {
                ttl_remaining: 0,
                message: "m".into(),
            },
            SseEvent::TtlExpired,
            SseEvent::Disconnected {
                reason: "r".into(),
            },
        ];

        let types: Vec<&str> = events.iter().map(|e| e.event_type()).collect();
        // Check all unique
        let mut sorted = types.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(types.len(), sorted.len(), "duplicate event type tags found");
    }

    #[test]
    fn heartbeat_to_sse_data() {
        let event = SseEvent::Heartbeat {
            timestamp: "2026-04-23T14:30:00Z".into(),
        };
        let data = event.to_sse_data();
        assert!(data.starts_with("data: "));
        assert!(data.ends_with("\n\n"));
        assert!(data.contains("\"type\":\"heartbeat\""));
    }
}

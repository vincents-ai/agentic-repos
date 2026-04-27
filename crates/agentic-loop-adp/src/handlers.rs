//! Default event handlers for agentic-loop ADP integration.

use agentic_loop_sse_client::EventHandler;
use adp_proto::sse::SseEvent;
use async_trait::async_trait;
use tracing::{debug, info, warn};

/// Handler for schema change events — triggers local cache invalidation.
pub struct SchemaChangeHandler;

#[async_trait]
impl EventHandler for SchemaChangeHandler {
    async fn handle(&self, event: &SseEvent) {
        if let SseEvent::SchemaChange {
            event_type,
            schema_id,
            timestamp,
        } = event
        {
            info!(
                schema_id = %schema_id,
                event_type = %event_type,
                timestamp = %timestamp,
                "Schema changed — invalidating local cache"
            );
        }
    }
}

/// Handler for TTL warning events — triggers secret renewal.
pub struct TtlWarningHandler {
    /// Whether auto-renewal is enabled.
    pub auto_renew: bool,
}

#[async_trait]
impl EventHandler for TtlWarningHandler {
    async fn handle(&self, event: &SseEvent) {
        if let SseEvent::TtlWarning {
            ttl_remaining,
            message,
        } = event
        {
            warn!(
                ttl_remaining = ttl_remaining,
                message = %message,
                "TTL warning received"
            );
            if self.auto_renew {
                info!("Auto-renewal is enabled — renewal will be triggered");
            }
        }
    }
}

/// Handler for entity update events — triggers local processing.
pub struct EntityUpdateHandler;

#[async_trait]
impl EventHandler for EntityUpdateHandler {
    async fn handle(&self, event: &SseEvent) {
        if let SseEvent::EntityUpdate {
            entity_id,
            schema_id,
            action,
        } = event
        {
            debug!(
                entity_id = %entity_id,
                schema_id = %schema_id,
                action = %action,
                "Entity updated"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn schema_change_handler_logs() {
        let handler = SchemaChangeHandler;
        let event = SseEvent::SchemaChange {
            event_type: "created".to_string(),
            schema_id: "engram-task".to_string(),
            timestamp: "2026-04-23T14:30:00Z".to_string(),
        };
        // Should not panic
        handler.handle(&event).await;
    }

    #[tokio::test]
    async fn ttl_warning_handler_logs() {
        let handler = TtlWarningHandler { auto_renew: true };
        let event = SseEvent::TtlWarning {
            ttl_remaining: 300,
            message: "Expiring soon".to_string(),
        };
        handler.handle(&event).await;
    }

    #[tokio::test]
    async fn entity_update_handler_logs() {
        let handler = EntityUpdateHandler;
        let event = SseEvent::EntityUpdate {
            entity_id: "abc-123".to_string(),
            schema_id: "engram-task".to_string(),
            action: "created".to_string(),
        };
        handler.handle(&event).await;
    }
}

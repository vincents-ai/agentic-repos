//! Event handler trait and implementations.

use adp_proto::sse::SseEvent;
use async_trait::async_trait;

/// Trait for handling SSE events.
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an SSE event.
    async fn handle(&self, event: &SseEvent);
}

/// A no-op handler that logs events.
pub struct NoopHandler;

#[async_trait]
impl EventHandler for NoopHandler {
    async fn handle(&self, event: &SseEvent) {
        tracing::debug!(event_type = event.event_type(), "SSE event received (noop handler)");
    }
}

/// Composite handler that dispatches to multiple handlers.
pub struct EventHandlers {
    handlers: Vec<Box<dyn EventHandler>>,
}

impl EventHandlers {
    /// Create an empty handler set.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Add a handler.
    pub fn add(&mut self, handler: Box<dyn EventHandler>) {
        self.handlers.push(handler);
    }

    /// Dispatch an event to all handlers.
    pub async fn dispatch(&self, event: &SseEvent) {
        for handler in &self.handlers {
            handler.handle(event).await;
        }
    }
}

impl Default for EventHandlers {
    fn default() -> Self {
        Self::new()
    }
}

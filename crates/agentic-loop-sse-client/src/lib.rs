//! # agentic-loop-sse-client
//!
//! SSE client for connecting agentic-loop instances to adp servers.
//!
//! Provides:
//! - Auto-reconnecting SSE connection with exponential backoff
//! - Event dispatch to typed handlers
//! - TTL-aware connection management
//! - Heartbeat monitoring

mod client;
mod config;
mod error;
mod handler;

pub use client::SseClient;
pub use config::SseClientConfig;
pub use error::SseClientError;
pub use handler::{EventHandler, EventHandlers, NoopHandler};

/// State of the SSE client connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected.
    Disconnected,
    /// Connecting to the server.
    Connecting,
    /// Connected and receiving events.
    Connected,
    /// Reconnecting after a failure.
    Reconnecting,
    /// Permanently disconnected (TTL expired, auth failed).
    Failed,
}

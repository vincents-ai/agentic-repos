//! # agentic-loop-adp
//!
//! Integration layer connecting agentic-loop to an ADP server.
//!
//! Provides:
//! - **AdpClient**: Unified client that manages registration, SSE connection,
//!   and entity writing in a single lifecycle.
//! - **Event handlers**: Default handlers for schema changes, TTL warnings, etc.
//! - **Background tasks**: TTL renewal, heartbeat monitoring.

mod client;
mod handlers;

pub use client::{AdpClient, AdpClientConfig};
pub use handlers::{
    SchemaChangeHandler, TtlWarningHandler, EntityUpdateHandler,
};

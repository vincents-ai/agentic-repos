//! # agentic-loop-registration
//!
//! Service registration for agentic-loop instances.
//!
//! Handles:
//! - Instance identity generation
//! - Secret generation and TTL management
//! - Registration flow (create → store in git refs → connect SSE)
//! - Secret renewal before TTL expiry
//! - Graceful deregistration

mod error;
mod registration;
mod secret;

pub use error::RegistrationError;
pub use registration::{RegistrationManager, RegistrationStatus};
pub use secret::SecretGenerator;

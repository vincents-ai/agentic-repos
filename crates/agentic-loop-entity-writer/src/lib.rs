//! # agentic-loop-entity-writer
//!
//! Schema-validated entity writer for agentic-loop instances.
//!
//! Provides:
//! - Typed entity creation with automatic schema validation
//! - Entity CRUD operations against git refs
//! - Convenience builders for common entity types

mod error;
mod writer;

pub use error::EntityWriterError;
pub use writer::{EntityRef, EntityWriter, WriteOperation};

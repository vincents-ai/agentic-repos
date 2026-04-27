//! # adp-schema-registry
//!
//! Schema registry with validation and caching for ADP ref schemas.
//!
//! Provides:
//! - Schema storage and lookup by ID or namespace pattern
//! - JSON Schema validation against registered schemas
//! - Schema change notifications
//! - Schema versioning
//!
//! The types `RefSchema` and `UiHints` live in `agentic_repos::adp::schema`.
//! This crate provides the registry, validation, and caching layer on top.

pub mod engine;
pub mod error;
pub mod types;
pub mod validation;

pub use engine::SchemaRegistry;
pub use error::SchemaError;
pub use types::{RefSchema, SchemaChange, SchemaChangeType, UiHints};
pub use validation::ValidationResult;

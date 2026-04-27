//! Error types for the schema registry.

/// Errors from schema registry operations.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    /// Schema not found.
    #[error("schema not found: {0}")]
    NotFound(String),

    /// Schema validation failed.
    #[error("validation failed: {message}")]
    ValidationFailed {
        message: String,
        errors: Vec<String>,
    },

    /// Invalid schema definition.
    #[error("invalid schema: {0}")]
    InvalidSchema(String),

    /// Schema already exists.
    #[error("schema already exists: {0}")]
    AlreadyExists(String),

    /// Version conflict.
    #[error("version conflict: schema {id} has version {existing}, got {provided}")]
    VersionConflict {
        id: String,
        existing: String,
        provided: String,
    },

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Glob pattern error.
    #[error("invalid namespace pattern: {0}")]
    InvalidPattern(String),
}

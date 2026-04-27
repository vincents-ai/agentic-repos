//! Error types for the entity writer.

/// Errors from entity writer operations.
#[derive(Debug, thiserror::Error)]
pub enum EntityWriterError {
    /// Schema validation failed.
    #[error("validation failed for schema '{schema_id}': {errors:?}")]
    ValidationFailed {
        schema_id: String,
        errors: Vec<String>,
    },

    /// Schema not found.
    #[error("schema not found: {0}")]
    SchemaNotFound(String),

    /// Entity not found.
    #[error("entity not found: {path}")]
    EntityNotFound { path: String },

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Permission denied.
    #[error("permission denied: {path} ({reason})")]
    PermissionDenied { path: String, reason: String },

    /// Invalid entity ID.
    #[error("invalid entity ID: {0}")]
    InvalidEntityId(String),

    /// Internal error.
    #[error("{0}")]
    Internal(String),
}

//! Error types for the RBAC engine.

/// Errors from RBAC enforcement operations.
#[derive(Debug, thiserror::Error)]
pub enum RbacError {
    /// The key permissions have expired.
    #[error("key permissions expired at {expires_at}")]
    PermissionsExpired { expires_at: String },

    /// Glob pattern compilation failed.
    #[error("invalid glob pattern '{pattern}': {reason}")]
    InvalidPattern { pattern: String, reason: String },

    /// CODEOWNERS parsing failed.
    #[error("CODEOWNERS parse error at line {line}: {reason}")]
    CodeownersParseError { line: usize, reason: String },

    /// Path restriction violation.
    #[error("path restriction denied: {path} (reason: {reason})")]
    PathDenied { path: String, reason: String },

    /// Internal error.
    #[error("{0}")]
    Internal(String),
}

//! Error types for registration.

/// Errors from registration operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    /// Secret generation failed.
    #[error("secret generation failed: {0}")]
    SecretGenerationFailed(String),

    /// Registration expired.
    #[error("registration expired at {expires_at}")]
    Expired { expires_at: String },

    /// Renewal failed.
    #[error("renewal failed: {0}")]
    RenewalFailed(String),

    /// Invalid instance ID.
    #[error("invalid instance ID: {0}")]
    InvalidInstanceId(String),

    /// Registration already exists.
    #[error("registration already exists for instance {0}")]
    AlreadyRegistered(String),

    /// Not registered.
    #[error("not registered")]
    NotRegistered,

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Internal error.
    #[error("{0}")]
    Internal(String),
}

//! Error types for the SSE registry.

/// Errors that can occur during SSE registry operations.
#[derive(Debug, thiserror::Error)]
pub enum SseRegistryError {
    /// The instance's secret is too close to expiry.
    #[error("secret expires in {ttl_remaining}s, minimum {min_required}s required")]
    SecretTooCloseToExpiry {
        ttl_remaining: i64,
        min_required: i64,
    },

    /// Client and server TTL calculations differ beyond allowed skew.
    #[error("TTL mismatch: client={client_ttl}s, server={server_ttl}s, allowed_skew={allowed_skew}s")]
    TtlMismatch {
        client_ttl: i64,
        server_ttl: i64,
        allowed_skew: i64,
    },

    /// Authentication failed (secret mismatch or instance not registered).
    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    /// The instance is not registered.
    #[error("instance not registered: {0}")]
    InstanceNotFound(String),

    /// The connection was already closed.
    #[error("connection closed")]
    ConnectionClosed,

    /// Channel send failed (connection dropped).
    #[error("channel send failed")]
    ChannelSendFailed,

    /// Internal error.
    #[error("{0}")]
    Internal(String),
}

impl actix_web::ResponseError for SseRegistryError {}

//! Error types for the SSE client.

/// Errors from SSE client operations.
#[derive(Debug, thiserror::Error)]
pub enum SseClientError {
    /// Connection failed.
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// Authentication rejected by server.
    #[error("authentication rejected: {0}")]
    AuthenticationRejected(String),

    /// TTL expired — must re-register.
    #[error("TTL expired: {message}")]
    TtlExpired { message: String },

    /// Failed to parse an SSE event.
    #[error("event parse error: {0}")]
    EventParseError(String),

    /// HTTP request error.
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// Connection closed by server.
    #[error("connection closed: {reason}")]
    ConnectionClosed { reason: String },

    /// Max reconnection attempts exceeded.
    #[error("max reconnection attempts ({attempts}) exceeded")]
    MaxReconnectAttempts { attempts: u32 },

    /// Client was stopped.
    #[error("client stopped")]
    Stopped,
}

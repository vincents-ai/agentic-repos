//! Error types for the Skynet v2 Git Operations Plugin

use serde_json::json;
use std::fmt;
use thiserror::Error;

/// Result type for Skynet plugin operations
pub type SkynetPluginResult<T> = Result<T, SkynetPluginError>;

/// Errors that can occur in the Git Operations Plugin
#[derive(Debug, Error)]
pub enum SkynetPluginError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Configuration error (alias)
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// Storage backend error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Storage backend error (alias)
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Multi-tenant isolation or permission error
    #[error("Tenant error: {0}")]
    Tenant(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// Git operation error
    #[error("Git error: {0}")]
    Git(String),

    /// Git LFS error
    #[error("Git LFS error: {0}")]
    Lfs(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// Repository not found
    #[error("Repository not found: {0}")]
    RepositoryNotFound(String),

    /// Repository already exists
    #[error("Repository already exists: {0}")]
    RepositoryExists(String),

    /// Invalid request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Monitoring/health check error
    #[error("Monitoring error: {0}")]
    Monitoring(String),

    /// Internal server error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Resource exhausted (quota exceeded, etc.)
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// Timeout error
    #[error("Timeout: {0}")]
    Timeout(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid SSH key
    #[error("Invalid SSH key: {0}")]
    InvalidSshKey(String),

    /// Git operation error (more specific than Git)
    #[error("Git operation error: {0}")]
    GitOperation(String),
}

impl SkynetPluginError {
    /// Create a configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }

    /// Create a configuration error (alias)
    pub fn configuration_error(msg: impl Into<String>) -> Self {
        Self::ConfigurationError(msg.into())
    }

    /// Create a storage error
    pub fn storage(msg: impl Into<String>) -> Self {
        Self::Storage(msg.into())
    }

    /// Create a storage error (alias)
    pub fn storage_error(msg: impl Into<String>) -> Self {
        Self::StorageError(msg.into())
    }

    /// Create a tenant error
    pub fn tenant(msg: impl Into<String>) -> Self {
        Self::Tenant(msg.into())
    }

    /// Create a permission denied error
    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }

    /// Create a rate limit exceeded error
    pub fn rate_limit_exceeded(msg: impl Into<String>) -> Self {
        Self::RateLimitExceeded(msg.into())
    }

    /// Create a git error
    pub fn git(msg: impl Into<String>) -> Self {
        Self::Git(msg.into())
    }

    /// Create a git LFS error
    pub fn lfs(msg: impl Into<String>) -> Self {
        Self::Lfs(msg.into())
    }

    /// Create an authentication error
    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Authentication(msg.into())
    }

    /// Create a repository not found error
    pub fn repo_not_found(repo_name: impl Into<String>) -> Self {
        Self::RepositoryNotFound(repo_name.into())
    }

    /// Create a repository already exists error
    pub fn repo_exists(repo_name: impl Into<String>) -> Self {
        Self::RepositoryExists(repo_name.into())
    }

    /// Create an invalid request error
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::InvalidRequest(msg.into())
    }

    /// Create a resource exhausted error
    pub fn resource_exhausted(msg: impl Into<String>) -> Self {
        Self::ResourceExhausted(msg.into())
    }

    /// Create a timeout error
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::Timeout(msg.into())
    }

    /// Create an internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// Get the error code for Skynet v2 protocol
    pub fn error_code(&self) -> String {
        match self {
            Self::Configuration(_) | Self::ConfigurationError(_) => "PLUGIN_CONFIG_ERROR",
            Self::Storage(_) | Self::StorageError(_) => "STORAGE_ERROR",
            Self::Tenant(_) => "TENANT_ERROR",
            Self::PermissionDenied(_) => "PERMISSION_DENIED",
            Self::RateLimitExceeded(_) => "RATE_LIMIT_EXCEEDED",
            Self::Git(_) => "GIT_ERROR",
            Self::Lfs(_) => "LFS_ERROR",
            Self::Authentication(_) => "AUTH_ERROR",
            Self::RepositoryNotFound(_) => "REPO_NOT_FOUND",
            Self::RepositoryExists(_) => "REPO_EXISTS",
            Self::InvalidRequest(_) => "INVALID_REQUEST",
            Self::Monitoring(_) => "MONITORING_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::ResourceExhausted(_) => "RESOURCE_EXHAUSTED",
            Self::Timeout(_) => "TIMEOUT",
            Self::Io(_) => "IO_ERROR",
            Self::Json(_) => "JSON_ERROR",
            Self::InvalidSshKey(_) => "INVALID_SSH_KEY",
            Self::GitOperation(_) => "GIT_OPERATION_ERROR",
        }
        .to_string()
    }

    /// Get HTTP status code for this error
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Configuration(_)
            | Self::ConfigurationError(_)
            | Self::InvalidRequest(_)
            | Self::InvalidSshKey(_) => 400,
            Self::Authentication(_) => 401,
            Self::PermissionDenied(_) => 403,
            Self::RepositoryNotFound(_) => 404,
            Self::RepositoryExists(_) => 409,
            Self::RateLimitExceeded(_) => 429,
            Self::ResourceExhausted(_) => 507,
            Self::Timeout(_) => 504,
            Self::Io(_) | Self::Json(_) | Self::Internal(_) | Self::GitOperation(_) => 500,
            Self::Monitoring(_) => 500,
            Self::Storage(_)
            | Self::StorageError(_)
            | Self::Tenant(_)
            | Self::Git(_)
            | Self::Lfs(_) => 500,
        }
    }

    /// Convert to Skynet v2 error response
    pub fn to_response(self, request_id: impl Into<String>) -> PluginErrorResponse {
        PluginErrorResponse {
            id: request_id.into(),
            error: json!({
                "code": self.error_code(),
                "message": self.to_string(),
                "details": {
                    "http_status": self.http_status(),
                }
            }),
        }
    }
}

/// Skynet v2 plugin error response
#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginErrorResponse {
    pub id: String,
    pub error: serde_json::Value,
}

impl fmt::Display for PluginErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Plugin error [{}]: {}", self.id, self.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        let err = SkynetPluginError::config("test");
        assert_eq!(err.error_code(), "PLUGIN_CONFIG_ERROR");

        let err = SkynetPluginError::rate_limit_exceeded("too many requests");
        assert_eq!(err.error_code(), "RATE_LIMIT_EXCEEDED");

        let err = SkynetPluginError::permission_denied("access denied");
        assert_eq!(err.error_code(), "PERMISSION_DENIED");
    }

    #[test]
    fn test_http_status_codes() {
        assert_eq!(SkynetPluginError::config("test").http_status(), 400);
        assert_eq!(SkynetPluginError::auth("bad token").http_status(), 401);
        assert_eq!(
            SkynetPluginError::permission_denied("access denied").http_status(),
            403
        );
        assert_eq!(SkynetPluginError::repo_not_found("test").http_status(), 404);
        assert_eq!(SkynetPluginError::repo_exists("test").http_status(), 409);
        assert_eq!(
            SkynetPluginError::rate_limit_exceeded("too many").http_status(),
            429
        );
        assert_eq!(
            SkynetPluginError::resource_exhausted("quota").http_status(),
            507
        );
        assert_eq!(SkynetPluginError::timeout("operation").http_status(), 504);
        assert_eq!(SkynetPluginError::internal("error").http_status(), 500);
    }

    #[test]
    fn test_error_response_conversion() {
        let err = SkynetPluginError::permission_denied("test access denied");
        let response = err.to_response("req-123");
        assert_eq!(response.id, "req-123");
        assert!(response.error["code"].is_string());
        assert!(response.error["message"].is_string());
    }
}

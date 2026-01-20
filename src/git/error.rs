use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("SSH authentication failed: {0}")]
    AuthError(String),

    #[error("Repository not found: {0}")]
    RepoNotFound(String),

    #[error("Invalid repository path: {0}")]
    InvalidRepoPath(String),

    #[error("Git protocol error: {0}")]
    ProtocolError(String),

    #[error("Storage error: {0}")]
    StorageError(#[from] crate::storage::StorageError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("Branch limit exceeded: {0}")]
    BranchLimitExceeded(String),

    #[error("Compliance violation: {0}")]
    ComplianceError(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, GitError>;

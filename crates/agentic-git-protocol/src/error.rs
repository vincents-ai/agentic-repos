use thiserror::Error;

/// Errors produced by git protocol operations.
#[derive(Debug, Error)]
pub enum GitProtocolError {
    #[error("Repository not found: {0}")]
    RepoNotFound(String),

    #[error("Git protocol error: {0}")]
    ProtocolError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Pack error: {0}")]
    Pack(String),

    #[error("Reference error: {0}")]
    Ref(String),

    #[error("Object error: {0}")]
    Object(String),
}

pub type Result<T> = std::result::Result<T, GitProtocolError>;

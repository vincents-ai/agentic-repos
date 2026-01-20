use actix_web::error::ResponseError;
use actix_web::http::StatusCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("S3 error: {0}")]
    S3(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Repository not found: {0}")]
    NotFound(String),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Authentication failed")]
    Authentication,

    #[error("Permission denied")]
    PermissionDenied,
}

pub type Result<T> = std::result::Result<T, StorageError>;

impl ResponseError for StorageError {
    fn status_code(&self) -> StatusCode {
        match self {
            StorageError::NotFound(_) => StatusCode::NOT_FOUND,
            StorageError::Authentication | StorageError::PermissionDenied => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

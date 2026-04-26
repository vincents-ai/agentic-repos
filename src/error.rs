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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::error::ResponseError;

    #[test]
    fn status_codes() {
        assert_eq!(StorageError::NotFound("x".into()).status_code(), StatusCode::NOT_FOUND);
        assert_eq!(StorageError::Authentication.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(StorageError::PermissionDenied.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(StorageError::S3("err".into()).status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(StorageError::InvalidPath("bad".into()).status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn display_messages() {
        assert!(StorageError::NotFound("repo".into()).to_string().contains("repo"));
        assert!(StorageError::S3("timeout".into()).to_string().contains("timeout"));
        assert!(StorageError::Authentication.to_string().contains("Authentication"));
    }
}

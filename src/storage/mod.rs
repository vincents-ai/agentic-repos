use async_trait::async_trait;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn exists(&self, path: &Path) -> Result<bool, StorageError>;
    async fn read(&self, path: &Path) -> Result<Vec<u8>, StorageError>;
    async fn write(&self, path: &Path, data: &[u8]) -> Result<(), StorageError>;
    async fn delete(&self, path: &Path) -> Result<(), StorageError>;
    async fn list(&self, prefix: &Path) -> Result<Vec<String>, StorageError>;
    async fn create_directory(&self, path: &Path) -> Result<(), StorageError>;
}

pub mod local {
    use super::{StorageBackend, StorageError};
    use async_trait::async_trait;
    use std::path::{Path, PathBuf};
    use tokio::fs;

    #[derive(Debug)]
    pub struct LocalStorage {
        root: PathBuf,
    }

    impl LocalStorage {
        pub fn new(root: PathBuf) -> Self {
            Self { root }
        }

        fn resolve(&self, path: &Path) -> PathBuf {
            self.root.join(path)
        }
    }

    #[async_trait]
    impl StorageBackend for LocalStorage {
        async fn exists(&self, path: &Path) -> Result<bool, StorageError> {
            let resolved = self.resolve(path);
            Ok(resolved.exists())
        }

        async fn read(&self, path: &Path) -> Result<Vec<u8>, StorageError> {
            let resolved = self.resolve(path);
            fs::read(&resolved)
                .await
                .map_err(|e| StorageError::Io(e.to_string()))
        }

        async fn write(&self, path: &Path, data: &[u8]) -> Result<(), StorageError> {
            let resolved = self.resolve(path);
            if let Some(parent) = resolved.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| StorageError::Io(e.to_string()))?;
                }
            }
            fs::write(&resolved, data)
                .await
                .map_err(|e| StorageError::Io(e.to_string()))
        }

        async fn delete(&self, path: &Path) -> Result<(), StorageError> {
            let resolved = self.resolve(path);
            if resolved.is_dir() {
                fs::remove_dir_all(&resolved)
                    .await
                    .map_err(|e| StorageError::Io(e.to_string()))?;
            } else {
                fs::remove_file(&resolved)
                    .await
                    .map_err(|e| StorageError::Io(e.to_string()))?;
            }
            Ok(())
        }

        async fn list(&self, prefix: &Path) -> Result<Vec<String>, StorageError> {
            let resolved = self.resolve(prefix);
            let mut entries = Vec::new();

            if let Ok(mut read_dir) = fs::read_dir(&resolved).await {
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    if let Ok(path) = entry.path().strip_prefix(&self.root) {
                        if let Some(s) = path.to_str() {
                            entries.push(s.to_string());
                        }
                    }
                }
            }

            Ok(entries)
        }

        async fn create_directory(&self, path: &Path) -> Result<(), StorageError> {
            let resolved = self.resolve(path);
            fs::create_dir_all(&resolved)
                .await
                .map_err(|e| StorageError::Io(e.to_string()))
        }
    }
}

pub mod s3 {
    use super::{StorageBackend, StorageError};
    use async_trait::async_trait;
    use aws_sdk_s3::Client;
    use std::path::Path;

    #[derive(Debug)]
    pub struct S3Storage {
        client: Client,
        bucket: String,
    }

    impl S3Storage {
        pub fn new(client: Client, bucket: String) -> Self {
            Self { client, bucket }
        }
    }

    #[async_trait]
    impl StorageBackend for S3Storage {
        async fn exists(&self, path: &Path) -> Result<bool, StorageError> {
            let key = path.to_string_lossy().into_owned();
            match self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
            {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }

        async fn read(&self, path: &Path) -> Result<Vec<u8>, StorageError> {
            let key = path.to_string_lossy().into_owned();
            let output = self
                .client
                .get_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
            let bytes = output
                .body
                .collect()
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
            Ok(bytes.to_vec())
        }

        async fn write(&self, path: &Path, data: &[u8]) -> Result<(), StorageError> {
            let key = path.to_string_lossy().into_owned();
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(aws_sdk_s3::primitives::ByteStream::from(data.to_vec()))
                .send()
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
            Ok(())
        }

        async fn delete(&self, path: &Path) -> Result<(), StorageError> {
            let key = path.to_string_lossy().into_owned();
            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
            Ok(())
        }

        async fn list(&self, prefix: &Path) -> Result<Vec<String>, StorageError> {
            let prefix_str = prefix.to_string_lossy().into_owned();
            let mut entries = Vec::new();
            let mut continuation = None;

            loop {
                let mut request = self
                    .client
                    .list_objects_v2()
                    .bucket(&self.bucket)
                    .prefix(&prefix_str);

                if let Some(token) = continuation {
                    request = request.continuation_token(token);
                }

                let output = request
                    .send()
                    .await
                    .map_err(|e| StorageError::Io(e.to_string()))?;

                for obj in output.contents().iter() {
                    if let Some(key) = obj.key() {
                        entries.push(key.to_string());
                    }
                }

                if output.is_truncated().unwrap_or(false) {
                    continuation = output.next_continuation_token().map(|s| s.to_string());
                } else {
                    break;
                }
            }

            Ok(entries)
        }

        async fn create_directory(&self, path: &Path) -> Result<(), StorageError> {
            let key = path.to_string_lossy().into_owned();
            if !key.ends_with('/') {
                let key = format!("{}/", key);
                self.client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .body(aws_sdk_s3::primitives::ByteStream::from(vec![]))
                    .send()
                    .await
                    .map_err(|e| StorageError::Io(e.to_string()))?;
            }
            Ok(())
        }
    }
}

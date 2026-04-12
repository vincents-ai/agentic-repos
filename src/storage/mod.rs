use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    pub backend_type: String,
    pub local: Option<LocalConfig>,
    pub s3: Option<S3Config>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub bucket: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend_type: "local".to_string(),
            local: Some(LocalConfig {
                path: PathBuf::from("./data"),
            }),
            s3: None,
        }
    }
}

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
    fn root(&self) -> &Path;
}

pub mod users;

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

        fn root(&self) -> &Path {
            &self.root
        }
    }
}

pub mod s3 {
    use super::{StorageBackend, StorageError};
    use async_trait::async_trait;
    use aws_sdk_s3::primitives::ByteStream;
    use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
    use aws_sdk_s3::Client;
    use futures_util::stream::{FuturesUnordered, StreamExt};
    use std::path::Path;

    /// Threshold size for using multipart upload (100 MB)
    /// AWS recommends multipart for files >100MB, requires it for >5GB
    const DEFAULT_MULTIPART_THRESHOLD: usize = 100 * 1024 * 1024;

    /// Size of each part in multipart upload (64 MB)
    /// With 10,000 max parts (AWS limit), supports files up to 625 GiB
    /// Can be increased to 512 MB for 5 TiB max file size
    const DEFAULT_PART_SIZE: usize = 64 * 1024 * 1024;

    /// Number of concurrent part uploads
    const DEFAULT_CONCURRENCY: usize = 4;

    #[derive(Debug)]
    pub struct S3Storage {
        client: Client,
        bucket: String,
        multipart_threshold: usize,
        part_size: usize,
        concurrency: usize,
    }

    impl S3Storage {
        pub fn new(client: Client, bucket: String) -> Self {
            Self {
                client,
                bucket,
                multipart_threshold: DEFAULT_MULTIPART_THRESHOLD,
                part_size: DEFAULT_PART_SIZE,
                concurrency: DEFAULT_CONCURRENCY,
            }
        }

        /// Upload data to S3 using multipart upload for large files
        async fn multipart_upload(&self, key: &str, data: &[u8]) -> Result<(), StorageError> {
            use std::time::Instant;
            let start = Instant::now();

            // Step 1: Initiate multipart upload
            let upload_id = self
                .client
                .create_multipart_upload()
                .bucket(&self.bucket)
                .key(key)
                .send()
                .await
                .map_err(|e| {
                    StorageError::Io(format!("Failed to initiate multipart upload: {}", e))
                })?
                .upload_id()
                .ok_or_else(|| StorageError::Io("No upload ID returned".to_string()))?
                .to_string();

            // Step 2: Calculate parts
            let total_size = data.len();
            let num_parts = total_size.div_ceil(self.part_size);

            // Step 3: Upload parts concurrently
            let mut part_futures = FuturesUnordered::new();
            let mut completed_parts: Vec<Option<CompletedPart>> = vec![None; num_parts];

            for part_number in 0..num_parts {
                let start = part_number * self.part_size;
                let end = std::cmp::min(start + self.part_size, total_size);
                let part_data = data[start..end].to_vec();
                let part_num = (part_number + 1) as i32;

                let client = self.client.clone();
                let bucket = self.bucket.clone();
                let key_owned = key.to_string();
                let upload_id_owned = upload_id.clone();

                let future = async move {
                    let result = client
                        .upload_part()
                        .bucket(bucket)
                        .key(key_owned)
                        .upload_id(upload_id_owned)
                        .part_number(part_num)
                        .body(ByteStream::from(part_data))
                        .send()
                        .await;

                    (part_number, part_num, result)
                };

                part_futures.push(future);

                // Limit concurrency
                if part_futures.len() >= self.concurrency {
                    if let Some((idx, part_num, result)) = part_futures.next().await {
                        let output = result.map_err(|e| {
                            StorageError::Io(format!("Part {} upload failed: {}", part_num, e))
                        })?;

                        let completed_part = CompletedPart::builder()
                            .part_number(part_num)
                            .e_tag(output.e_tag().unwrap_or_default())
                            .build();

                        completed_parts[idx] = Some(completed_part);
                    }
                }
            }

            // Step 4: Wait for remaining uploads
            while let Some((idx, part_num, result)) = part_futures.next().await {
                let output = result.map_err(|e| {
                    StorageError::Io(format!("Part {} upload failed: {}", part_num, e))
                })?;

                let completed_part = CompletedPart::builder()
                    .part_number(part_num)
                    .e_tag(output.e_tag().unwrap_or_default())
                    .build();

                completed_parts[idx] = Some(completed_part);
            }

            // Step 5: Complete multipart upload
            let parts: Vec<CompletedPart> = completed_parts.into_iter().flatten().collect();

            let completed_upload = CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build();

            self.client
                .complete_multipart_upload()
                .bucket(&self.bucket)
                .key(key)
                .upload_id(&upload_id)
                .multipart_upload(completed_upload)
                .send()
                .await
                .map_err(|e| {
                    StorageError::Io(format!("Failed to complete multipart upload: {}", e))
                })?;

            let duration = start.elapsed().as_secs_f64();
            crate::lfs::metrics::record_s3_multipart_upload(
                &self.bucket,
                num_parts as u64,
                total_size as u64,
                duration,
            );

            Ok(())
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

            if data.len() >= self.multipart_threshold {
                self.multipart_upload(&key, data).await
            } else {
                self.client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .body(ByteStream::from(data.to_vec()))
                    .send()
                    .await
                    .map_err(|e| StorageError::Io(e.to_string()))?;

                crate::lfs::metrics::record_s3_simple_upload(&self.bucket);
                Ok(())
            }
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

        fn root(&self) -> &Path {
            unimplemented!("S3 storage does not have a local root path");
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_multipart_threshold_defaults() {
            assert_eq!(DEFAULT_MULTIPART_THRESHOLD, 100 * 1024 * 1024);
            assert_eq!(DEFAULT_PART_SIZE, 64 * 1024 * 1024);
            assert_eq!(DEFAULT_CONCURRENCY, 4);
        }

        #[test]
        fn test_part_size_calculation() {
            let data_size = 200 * 1024 * 1024;
            let num_parts = (data_size + DEFAULT_PART_SIZE - 1) / DEFAULT_PART_SIZE;
            assert_eq!(num_parts, 4);
        }

        #[test]
        fn test_max_supported_file_size() {
            let max_parts = 10000;
            let max_file_size = max_parts * DEFAULT_PART_SIZE;
            let max_gib = max_file_size / (1024 * 1024 * 1024);
            assert_eq!(max_gib, 625);
        }

        #[test]
        fn test_small_file_threshold() {
            let small_file = 50 * 1024 * 1024;
            assert!(small_file < DEFAULT_MULTIPART_THRESHOLD);
        }

        #[test]
        fn test_large_file_threshold() {
            let large_file = 150 * 1024 * 1024;
            assert!(large_file >= DEFAULT_MULTIPART_THRESHOLD);
        }
    }
}

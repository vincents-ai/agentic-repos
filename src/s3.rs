use super::error::{Result, StorageError};
use super::StorageBackend;
use async_trait::async_trait;
use aws_config::defaults;
use aws_sdk_s3::config::Builder as S3ConfigBuilder;
use aws_sdk_s3::{Client, Region};
use bytes::Bytes;
use std::path::Path;

pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Storage {
    pub async fn new(
        bucket: String,
        prefix: String,
        region: String,
        endpoint: Option<String>,
        _access_key_id: Option<String>,
        _secret_access_key: Option<String>,
    ) -> Result<Self> {
        let shared_config = defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(region))
            .load()
            .await;

        let mut config_builder = S3ConfigBuilder::from(&shared_config);

        if let Some(endpoint) = endpoint {
            config_builder = config_builder.endpoint_uri(endpoint);
        }

        let config = config_builder.build();
        let client = Client::from_conf(&config);

        Ok(Self {
            client,
            bucket,
            prefix,
        })
    }

    fn key_for_path(&self, path: &Path) -> String {
        format!("{}/{}", self.prefix.trim_end_matches('/'), path.display())
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    async fn upload(&self, path: &Path, data: Bytes) -> Result<()> {
        let key = self.key_for_path(path);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(data.into())
            .send()
            .await
            .map_err(|e| StorageError::S3(format!("Upload failed: {}", e)))?;

        Ok(())
    }

    async fn download(&self, path: &Path) -> Result<Bytes> {
        let key = self.key_for_path(path);

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| {
                if e.to_string().contains("NoSuchKey") {
                    StorageError::NotFound(path.display().to_string())
                } else {
                    StorageError::S3(format!("Download failed: {}", e))
                }
            })?;

        let bytes: Bytes = {
            response
                .body()
                .into_bytes()
                .await
                .map_err(|e| StorageError::S3(format!("Failed to read body: {}", e)))?
        };

        Ok(bytes)
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let key = self.key_for_path(path);

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::S3(format!("Delete failed: {}", e)))?;

        Ok(())
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        let key = self.key_for_path(path);

        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) if e.to_string().contains("NotFound") => Ok(false),
            Err(e) => Err(StorageError::S3(format!("Exists check failed: {}", e))),
        }
    }

    async fn list_prefix(&self, prefix: &Path) -> Result<Vec<String>> {
        let s3_prefix = self.key_for_path(prefix);

        let mut result = Vec::new();
        let mut continuation_token = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&s3_prefix);

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|e| StorageError::S3(format!("List failed: {}", e)))?;

            if let Some(objects) = response.contents {
                for object in objects {
                    if let Some(key) = object.key {
                        let relative = key.strip_prefix(&s3_prefix).unwrap_or(key);
                        result.push(relative.to_string());
                    }
                }
            }

            if response.is_truncated.unwrap_or(false) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(result)
    }

    async fn create_dir(&self, _path: &Path) -> Result<()> {
        Ok(())
    }
}

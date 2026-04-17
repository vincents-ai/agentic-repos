//! S3-compatible storage backend with Garage integration

use async_trait::async_trait;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_s3::{
    config::{Credentials, SharedCredentialsProvider},
    primitives::ByteStream,
    Client,
};
use std::path::PathBuf;
use tracing::{debug, info};

use super::StorageBackend;
use crate::skynet::error::{SkynetPluginError, SkynetPluginResult};

/// S3-compatible storage backend configuration
#[derive(Clone, Debug)]
pub struct S3StorageConfig {
    pub endpoint: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
}

impl S3StorageConfig {
    /// Create from environment variables (for Garage integration)
    pub fn from_env() -> Option<Self> {
        let endpoint = std::env::var("GARAGE_ENDPOINT").ok()?;
        let region = std::env::var("GARAGE_REGION").ok()?;
        let access_key_id = std::env::var("GARAGE_ACCESS_KEY_ID").ok()?;
        let secret_access_key = std::env::var("GARAGE_SECRET_ACCESS_KEY").ok()?;
        let bucket = std::env::var("GARAGE_BUCKET").ok()?;

        Some(Self {
            endpoint,
            region,
            access_key_id,
            secret_access_key,
            bucket,
        })
    }

    /// Create with explicit values
    pub fn new(
        endpoint: String,
        region: String,
        access_key_id: String,
        secret_access_key: String,
        bucket: String,
    ) -> Self {
        Self {
            endpoint,
            region,
            access_key_id,
            secret_access_key,
            bucket,
        }
    }
}

/// S3-compatible storage backend
pub struct S3StorageBackend {
    config: S3StorageConfig,
    client: Option<Client>,
}

impl S3StorageBackend {
    /// Create a new S3 storage backend
    pub fn new(config: S3StorageConfig) -> Self {
        Self {
            config,
            client: None,
        }
    }

    /// Create from environment variables
    pub fn from_env() -> SkynetPluginResult<Self> {
        let config = S3StorageConfig::from_env().ok_or_else(|| {
            SkynetPluginError::ConfigurationError(
                "Missing S3 configuration in environment variables".to_string(),
            )
        })?;

        info!("Initializing S3 storage with endpoint: {}", config.endpoint);
        Ok(Self::new(config))
    }

    /// Initialize the S3 client
    pub async fn initialize(&mut self) -> SkynetPluginResult<()> {
        let credentials = Credentials::new(
            &self.config.access_key_id,
            &self.config.secret_access_key,
            None,
            None,
            "garage-static",
        );

        let sdk_config = SdkConfig::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new(self.config.region.clone()))
            .endpoint_url(self.config.endpoint.clone())
            .credentials_provider(SharedCredentialsProvider::new(credentials))
            .build();

        self.client = Some(Client::new(&sdk_config));

        // Ensure bucket exists
        self.ensure_bucket_exists().await?;

        info!("S3 storage backend initialized successfully");
        Ok(())
    }

    /// Ensure the bucket exists, create if not
    async fn ensure_bucket_exists(&self) -> SkynetPluginResult<()> {
        if let Some(ref client) = self.client {
            debug!("Checking if bucket '{}' exists", self.config.bucket);

            // Try to head the bucket
            match client
                .head_bucket()
                .bucket(&self.config.bucket)
                .send()
                .await
            {
                Ok(_) => {
                    debug!("Bucket '{}' exists", self.config.bucket);
                }
                Err(e) => {
                    info!("Bucket check failed: {}, attempting to create", e);
                    // Try to create the bucket
                    client
                        .create_bucket()
                        .bucket(&self.config.bucket)
                        .send()
                        .await
                        .map_err(|e| {
                            SkynetPluginError::StorageError(format!(
                                "Failed to create bucket: {}",
                                e
                            ))
                        })?;

                    info!("Created bucket '{}'", self.config.bucket);
                }
            }

            Ok(())
        } else {
            Err(SkynetPluginError::StorageError(
                "S3 client not initialized".to_string(),
            ))
        }
    }

    /// Get the S3 client
    fn client(&self) -> SkynetPluginResult<&Client> {
        self.client
            .as_ref()
            .ok_or_else(|| SkynetPluginError::StorageError("S3 client not initialized".to_string()))
    }

    /// Generate S3 key for a repository
    fn repo_key(&self, tenant_id: &str, repo_name: &str) -> String {
        format!("{}/repos/{}.git", tenant_id, repo_name)
    }

    /// Generate S3 key for an LFS object
    fn lfs_key(&self, tenant_id: &str, oid: &str) -> String {
        format!("{}/lfs/objects/{}", tenant_id, oid)
    }
}

#[async_trait]
impl StorageBackend for S3StorageBackend {
    async fn create_repository(&self, tenant_id: &str, repo_name: &str) -> SkynetPluginResult<()> {
        let key = self.repo_key(tenant_id, repo_name);
        let client = self.client()?;

        // Create a directory marker object
        client
            .put_object()
            .bucket(&self.config.bucket)
            .key(format!("{}/.gitkeep", key))
            .body(ByteStream::from(Vec::new()))
            .send()
            .await
            .map_err(|e| {
                SkynetPluginError::StorageError(format!("Failed to create repository: {}", e))
            })?;

        info!("Created repository marker: {}", key);
        Ok(())
    }

    async fn delete_repository(&self, tenant_id: &str, repo_name: &str) -> SkynetPluginResult<()> {
        let prefix = self.repo_key(tenant_id, repo_name);
        let client = self.client()?;

        // List all objects with the repository prefix
        let mut continuation_token = None;
        loop {
            let mut list_request = client
                .list_objects_v2()
                .bucket(&self.config.bucket)
                .prefix(&prefix);

            if let Some(token) = &continuation_token {
                list_request = list_request.continuation_token(token);
            }

            let result = list_request.send().await.map_err(|e| {
                SkynetPluginError::StorageError(format!("Failed to list objects: {}", e))
            })?;

            // Delete all objects
            let objects = result.contents();
            for object in objects {
                if let Some(key) = object.key() {
                    client
                        .delete_object()
                        .bucket(&self.config.bucket)
                        .key(key)
                        .send()
                        .await
                        .map_err(|e| {
                            SkynetPluginError::StorageError(format!(
                                "Failed to delete object: {}",
                                e
                            ))
                        })?;
                }
            }

            if result.is_truncated().unwrap_or(false) {
                continuation_token = result.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }

        info!("Deleted repository: {}", prefix);
        Ok(())
    }

    async fn get_repository_path(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> SkynetPluginResult<PathBuf> {
        // For S3, return a virtual path
        Ok(PathBuf::from(format!(
            "s3://{}/{}",
            self.config.bucket,
            self.repo_key(tenant_id, repo_name)
        )))
    }

    async fn upload_lfs_object(
        &self,
        tenant_id: &str,
        oid: &str,
        data: Vec<u8>,
    ) -> SkynetPluginResult<()> {
        let key = self.lfs_key(tenant_id, oid);
        let client = self.client()?;

        client
            .put_object()
            .bucket(&self.config.bucket)
            .key(&key)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| {
                SkynetPluginError::StorageError(format!("Failed to upload LFS object: {}", e))
            })?;

        debug!("Uploaded LFS object: {}", key);
        Ok(())
    }

    async fn download_lfs_object(&self, tenant_id: &str, oid: &str) -> SkynetPluginResult<Vec<u8>> {
        let key = self.lfs_key(tenant_id, oid);
        let client = self.client()?;

        let result = client
            .get_object()
            .bucket(&self.config.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| {
                SkynetPluginError::StorageError(format!("Failed to download LFS object: {}", e))
            })?;

        let bytes = result
            .body
            .collect()
            .await
            .map_err(|e| {
                SkynetPluginError::StorageError(format!("Failed to read LFS object body: {}", e))
            })?
            .to_vec();

        debug!("Downloaded LFS object: {}", key);
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_key_generation() {
        let config = S3StorageConfig {
            endpoint: "http://localhost:3900".to_string(),
            region: "garage".to_string(),
            access_key_id: "key".to_string(),
            secret_access_key: "secret".to_string(),
            bucket: "repos".to_string(),
        };

        let backend = S3StorageBackend::new(config);
        let key = backend.repo_key("tenant1", "myrepo");
        assert_eq!(key, "tenant1/repos/myrepo.git");
    }

    #[test]
    fn test_lfs_key_generation() {
        let config = S3StorageConfig {
            endpoint: "http://localhost:3900".to_string(),
            region: "garage".to_string(),
            access_key_id: "key".to_string(),
            secret_access_key: "secret".to_string(),
            bucket: "repos".to_string(),
        };

        let backend = S3StorageBackend::new(config);
        let key = backend.lfs_key("tenant1", "abc123");
        assert_eq!(key, "tenant1/lfs/objects/abc123");
    }
}

//! Storage backend abstraction for Skynet v2 Git Operations Plugin

pub mod interface;
pub mod local;
pub mod s3;

pub use interface::{
    global_registry, s3_storage_free_buckets_result, s3_storage_free_object_result,
    s3_storage_free_objects_result, s3_storage_registry_get_default,
    s3_storage_registry_get_for_tenant, s3_storage_registry_register, s3_storage_registry_unregister,
    BucketConfig, BucketsResult, HealthResult, ObjectResult, ObjectsResult, RegisteredBackend,
    S3BackendInfo, S3Capability, S3CompatibleStorageV1, S3StoragePtr, S3StorageRegistry,
    StorageConfig, StorageResult, StorageResultCode, BoolResult, STORAGE_ALREADY_EXISTS,
    STORAGE_ERROR, STORAGE_INVALID_CONFIG, STORAGE_NOT_CONNECTED, STORAGE_NOT_FOUND,
    STORAGE_PERMISSION_DENIED, STORAGE_SUCCESS,
};
pub use local::LocalStorageBackend;
pub use s3::S3StorageBackend;

use async_trait::async_trait;
use std::path::PathBuf;

/// Storage backend trait
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Create a repository
    async fn create_repository(&self, tenant_id: &str, repo_name: &str) -> crate::skynet::error::SkynetPluginResult<()>;

    /// Delete a repository
    async fn delete_repository(&self, tenant_id: &str, repo_name: &str) -> crate::skynet::error::SkynetPluginResult<()>;

    /// Get repository path
    async fn get_repository_path(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> crate::skynet::error::SkynetPluginResult<PathBuf>;

    /// Upload LFS object
    async fn upload_lfs_object(
        &self,
        tenant_id: &str,
        oid: &str,
        data: Vec<u8>,
    ) -> crate::skynet::error::SkynetPluginResult<()>;

    /// Download LFS object
    async fn download_lfs_object(
        &self,
        tenant_id: &str,
        oid: &str,
    ) -> crate::skynet::error::SkynetPluginResult<Vec<u8>>;
}

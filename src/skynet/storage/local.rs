//! Local filesystem storage backend

use async_trait::async_trait;
use std::path::PathBuf;
use super::StorageBackend;

/// Local filesystem storage backend
pub struct LocalStorageBackend {
    base_path: PathBuf,
}

impl LocalStorageBackend {
    /// Create a new local storage backend
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

#[async_trait]
impl StorageBackend for LocalStorageBackend {
    async fn create_repository(
        &self,
        _tenant_id: &str,
        _repo_name: &str,
    ) -> crate::skynet::error::SkynetPluginResult<()> {
        Ok(())
    }

    async fn delete_repository(
        &self,
        _tenant_id: &str,
        _repo_name: &str,
    ) -> crate::skynet::error::SkynetPluginResult<()> {
        Ok(())
    }

    async fn get_repository_path(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> crate::skynet::error::SkynetPluginResult<PathBuf> {
        Ok(self.base_path.join(tenant_id).join("repos").join(format!("{}.git", repo_name)))
    }

    async fn upload_lfs_object(
        &self,
        _tenant_id: &str,
        _oid: &str,
        _data: Vec<u8>,
    ) -> crate::skynet::error::SkynetPluginResult<()> {
        Ok(())
    }

    async fn download_lfs_object(
        &self,
        _tenant_id: &str,
        _oid: &str,
    ) -> crate::skynet::error::SkynetPluginResult<Vec<u8>> {
        Ok(vec![])
    }
}

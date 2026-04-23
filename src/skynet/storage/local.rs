//! Local filesystem storage backend

use super::StorageBackend;
use async_trait::async_trait;
use std::path::PathBuf;

/// Local filesystem storage backend
pub struct LocalStorageBackend {
    base_path: PathBuf,
}

/// Validate that a path component does not contain traversal sequences.
fn validate_path_component(name: &str, label: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{} must not be empty", label));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!("{} must not contain path separators", label));
    }
    if name == ".." || name.contains("..") {
        return Err(format!(
            "{} must not contain path traversal sequences",
            label
        ));
    }
    if name.starts_with('.') || name.contains('\0') {
        return Err(format!("{} contains invalid characters", label));
    }
    Ok(())
}

impl LocalStorageBackend {
    /// Create a new local storage backend
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Build a tenant-scoped repository path with traversal protection.
    fn safe_repo_path(&self, tenant_id: &str, repo_name: &str) -> Result<PathBuf, String> {
        validate_path_component(tenant_id, "tenant_id")?;
        validate_path_component(repo_name, "repo_name")?;

        let path = self
            .base_path
            .join(tenant_id)
            .join("repos")
            .join(format!("{}.git", repo_name));

        // Defense-in-depth: verify path stays under base
        if !path.starts_with(&self.base_path) {
            return Err(
                "Resolved path escapes base directory — possible path traversal".to_string(),
            );
        }

        Ok(path)
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
        self.safe_repo_path(tenant_id, repo_name)
            .map_err(crate::skynet::error::SkynetPluginError::InvalidRequest)
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

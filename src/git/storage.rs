use crate::storage::{StorageBackend, StorageError};
use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info};

pub struct GitStorageIntegration<S: StorageBackend> {
    storage: Arc<S>,
    temp_dir: PathBuf,
}

impl<S: StorageBackend> GitStorageIntegration<S> {
    pub fn new(storage: Arc<S>, temp_dir: Option<PathBuf>) -> Self {
        Self {
            storage,
            temp_dir: temp_dir.unwrap_or_else(|| std::env::temp_dir().join("repos-mcp-git")),
        }
    }

    /// Get the storage path for a repository
    pub fn repo_path(&self, tenant_id: &str, repo_name: &str) -> PathBuf {
        PathBuf::from("repos")
            .join(tenant_id)
            .join(format!("{}.git", repo_name))
    }

    /// Check if a repository exists
    pub async fn repo_exists(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<bool, StorageError> {
        let path = self.repo_path(tenant_id, repo_name);
        self.storage.exists(&path).await
    }

    /// Initialize a new bare repository
    pub async fn init_repo(&self, tenant_id: &str, repo_name: &str) -> Result<(), StorageError> {
        let path = self.repo_path(tenant_id, repo_name);

        debug!("Initializing new repository: {}", path.display());

        // Create a temporary directory for the new repository
        let temp_repo = tempfile::Builder::new()
            .prefix("repo-")
            .tempdir_in(&self.temp_dir)
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Initialize the repository using git2
        let repo = git2::Repository::init_bare(temp_repo.path())
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Get the repository content as bytes
        let mut objects = Vec::new();
        let mut refs = Vec::new();

        // Walk the temp directory and upload contents
        self.walk_and_upload(temp_repo.path(), &path, &mut objects, &mut refs)
            .await?;

        // Clean up temp directory
        temp_repo
            .close()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        info!("Repository initialized: {}/{}", tenant_id, repo_name);
        Ok(())
    }

    /// Walk a directory and upload contents to storage
    async fn walk_and_upload(
        &self,
        source: &Path,
        dest: &Path,
        objects: &mut Vec<(PathBuf, Bytes)>,
        refs: &mut Vec<(PathBuf, Bytes)>,
    ) -> Result<(), StorageError> {
        use std::fs;

        for entry in fs::read_dir(source).map_err(|e| StorageError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| StorageError::Io(e.to_string()))?;
            let path = entry.path();
            let relative = path.strip_prefix(source).unwrap_or(&path);
            let storage_path = dest.join(relative);

            if path.is_file() {
                let data = fs::read(&path).map_err(|e| StorageError::Io(e.to_string()))?;
                self.storage.upload(&storage_path, data.into()).await?;
            } else if path.is_dir() && path.file_name().map(|n| n != "target") == Some(true) {
                self.storage.create_dir(&storage_path).await?;
                self.walk_and_upload(&path, &storage_path, objects, refs)
                    .await?;
            }
        }

        Ok(())
    }

    /// Open an existing repository (returns metadata, doesn't download)
    pub async fn open_repo(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<RepoMetadata, StorageError> {
        let path = self.repo_path(tenant_id, repo_name);

        if !self.repo_exists(tenant_id, repo_name).await? {
            return Err(StorageError::NotFound(path.to_string_lossy().to_string()));
        }

        // Get repository size
        let size = self.get_repo_size(tenant_id, repo_name).await?;

        // Get reference count
        let refs = self.storage.list_prefix(&path.join("refs")).await?;

        Ok(RepoMetadata {
            tenant_id: tenant_id.to_string(),
            repo_name: repo_name.to_string(),
            path,
            size_bytes: size,
            ref_count: refs.len(),
        })
    }

    /// Get the total size of a repository
    pub async fn get_repo_size(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<u64, StorageError> {
        let path = self.repo_path(tenant_id, repo_name);
        let files = self.storage.list_prefix(&path).await?;

        // Note: This is a simplified size calculation
        // In production, you'd want to track this in metadata
        Ok(files.len() as u64 * 1000) // Placeholder
    }

    /// Download repository to a temporary location for git operations
    pub async fn download_for_git_ops(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<std::fs::TempDir, StorageError> {
        let temp_dir = tempfile::Builder::new()
            .prefix("git-ops-")
            .tempdir_in(&self.temp_dir)
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let path = self.repo_path(tenant_id, repo_name);
        let files = self.storage.list_prefix(&path).await?;

        // Download all files
        for file_path in files {
            let relative = PathBuf::from(
                file_path
                    .strip_prefix(&path.to_string_lossy())
                    .unwrap_or(&file_path),
            );
            let local_path = temp_dir.path().join(&relative);

            // Create parent directories
            if let Some(parent) = local_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| StorageError::Io(e.to_string()))?;
            }

            let data = self.storage.download(&Path::new(&file_path)).await?;
            std::fs::write(&local_path, data).map_err(|e| StorageError::Io(e.to_string()))?;
        }

        Ok(temp_dir)
    }

    /// Upload git packfile to repository
    pub async fn upload_packfile(
        &self,
        tenant_id: &str,
        repo_name: &str,
        pack_data: Bytes,
    ) -> Result<(), StorageError> {
        let path = self
            .repo_path(tenant_id, repo_name)
            .join("objects/pack/pack.pack");
        self.storage.upload(&path, pack_data).await?;
        Ok(())
    }

    /// Get list of packfiles for a repository
    pub async fn list_packfiles(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<Vec<String>, StorageError> {
        let path = self.repo_path(tenant_id, repo_name).join("objects/pack");
        self.storage.list_prefix(&path).await
    }
}

#[derive(Debug, Clone)]
pub struct RepoMetadata {
    pub tenant_id: String,
    pub repo_name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub ref_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local::LocalStorage;
    use std::path::Path;

    #[tokio::test]
    async fn test_repo_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage_path = temp_dir.path().join("storage");
        let storage = Arc::new(LocalStorage::new(storage_path));

        let git_storage = GitStorageIntegration::new(storage, None);

        // Create a test repository
        git_storage
            .init_repo("test-tenant", "test-repo")
            .await
            .unwrap();

        // Check if repository exists
        assert!(git_storage
            .repo_exists("test-tenant", "test-repo")
            .await
            .unwrap());
    }
}

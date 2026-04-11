use crate::storage::{StorageBackend, StorageError};
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tracing::{debug, info};

pub struct GitStorageIntegration<S: StorageBackend> {
    storage: Arc<S>,
    temp_dir: PathBuf,
}

impl<S: StorageBackend> GitStorageIntegration<S> {
    pub fn new(storage: Arc<S>, temp_dir: Option<PathBuf>) -> Self {
        let temp_dir = temp_dir.unwrap_or_else(|| std::env::temp_dir().join("repos-mcp-git"));
        let _ = std::fs::create_dir_all(&temp_dir);
        Self { storage, temp_dir }
    }

    pub fn repo_path(&self, tenant_id: &str, repo_name: &str) -> PathBuf {
        PathBuf::from("repos")
            .join(tenant_id)
            .join(format!("{}.git", repo_name))
    }

    pub async fn repo_exists(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<bool, StorageError> {
        let path = self.repo_path(tenant_id, repo_name);
        self.storage.exists(&path).await
    }

    pub async fn init_repo(&self, tenant_id: &str, repo_name: &str) -> Result<(), StorageError> {
        let path = self.repo_path(tenant_id, repo_name);
        debug!("Initializing new repository: {}", path.display());
        let temp_repo = tempfile::Builder::new()
            .prefix("repo-")
            .tempdir_in(&self.temp_dir)
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let _repo =
            gix::init_bare(temp_repo.path()).map_err(|e| StorageError::Io(e.to_string()))?;

        self.walk_and_upload(temp_repo.path(), &path).await?;

        temp_repo
            .close()
            .map_err(|e| StorageError::Io(e.to_string()))?;
        info!("Repository initialized: {}/{}", tenant_id, repo_name);
        Ok(())
    }

    async fn walk_and_upload(&self, source: &Path, dest: &Path) -> Result<(), StorageError> {
        use std::fs;

        let mut stack = vec![source.to_path_buf()];
        let mut processed_dirs = std::collections::HashSet::new();

        while let Some(current_source) = stack.pop() {
            let current_dest = if current_source == source {
                dest.to_path_buf()
            } else {
                let relative = current_source
                    .strip_prefix(source)
                    .unwrap_or(&current_source);
                dest.join(relative)
            };

            if current_source.is_dir() && !processed_dirs.contains(&current_source) {
                processed_dirs.insert(current_source.clone());
                self.storage.create_directory(&current_dest).await.ok();

                for entry in
                    fs::read_dir(&current_source).map_err(|e| StorageError::Io(e.to_string()))?
                {
                    let entry = entry.map_err(|e| StorageError::Io(e.to_string()))?;
                    stack.push(entry.path());
                }
            } else if current_source.is_file() {
                let data =
                    fs::read(&current_source).map_err(|e| StorageError::Io(e.to_string()))?;
                self.storage.write(&current_dest, &data).await?;
            }
        }

        Ok(())
    }

    pub async fn open_repo(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<RepoMetadata, StorageError> {
        let path = self.repo_path(tenant_id, repo_name);

        if !self.repo_exists(tenant_id, repo_name).await? {
            return Err(StorageError::NotFound(path.to_string_lossy().to_string()));
        }

        let size = self.get_repo_size(tenant_id, repo_name).await?;
        let refs_list = self.storage.list(&path.join("refs")).await?;

        Ok(RepoMetadata {
            tenant_id: tenant_id.to_string(),
            repo_name: repo_name.to_string(),
            path,
            size_bytes: size,
            ref_count: refs_list.len(),
        })
    }

    pub async fn get_repo_size(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<u64, StorageError> {
        let path = self.repo_path(tenant_id, repo_name);
        let files = self.storage.list(&path).await?;
        Ok((files.len() as u64).saturating_mul(1000))
    }

    pub async fn download_for_git_ops(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<TempDir, StorageError> {
        let temp_dir = tempfile::Builder::new()
            .prefix("git-ops-")
            .tempdir_in(&self.temp_dir)
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let path = self.repo_path(tenant_id, repo_name);
        let files = self.storage.list(&path).await?;

        for file_path in files {
            let relative = PathBuf::from(
                file_path
                    .strip_prefix(path.to_string_lossy().as_ref())
                    .unwrap_or(&file_path),
            );
            let local_path = temp_dir.path().join(&relative);

            if let Some(parent) = local_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| StorageError::Io(e.to_string()))?;
            }

            let data = self.storage.read(&Path::new(&file_path)).await?;
            std::fs::write(&local_path, data).map_err(|e| StorageError::Io(e.to_string()))?;
        }

        Ok(temp_dir)
    }

    pub async fn upload_packfile(
        &self,
        tenant_id: &str,
        repo_name: &str,
        pack_data: Bytes,
    ) -> Result<(), StorageError> {
        let path = self
            .repo_path(tenant_id, repo_name)
            .join("objects/pack/pack.pack");
        self.storage.write(&path, &pack_data).await?;
        Ok(())
    }

    pub async fn list_packfiles(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<Vec<String>, StorageError> {
        let path = self.repo_path(tenant_id, repo_name).join("objects/pack");
        self.storage.list(&path).await
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

    #[tokio::test]
    async fn test_repo_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage_path = temp_dir.path().join("storage");
        let storage = Arc::new(LocalStorage::new(storage_path));

        let git_storage = GitStorageIntegration::new(storage, None);
        git_storage
            .init_repo("test-tenant", "test-repo")
            .await
            .unwrap();
        assert!(git_storage
            .repo_exists("test-tenant", "test-repo")
            .await
            .unwrap());
    }
}

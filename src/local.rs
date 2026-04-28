use super::error::{Result, StorageError};
use super::StorageBackend;
use async_trait::async_trait;
use bytes::Bytes;
use std::fs;
use std::path::Path;

pub struct LocalStorage {
    base_path: std::path::PathBuf,
}

impl LocalStorage {
    pub fn new(base_path: String) -> Result<Self> {
        let path = std::path::PathBuf::from(base_path);

        fs::create_dir_all(&path)?;

        Ok(Self { base_path: path })
    }

    fn full_path(&self, path: &Path) -> std::path::PathBuf {
        self.base_path.join(path.strip_prefix("/").unwrap_or(path))
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn upload(&self, path: &Path, data: Bytes) -> Result<()> {
        let full_path = self.full_path(path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&full_path, data)?;
        Ok(())
    }

    async fn download(&self, path: &Path) -> Result<Bytes> {
        let full_path = self.full_path(path);

        if !full_path.exists() {
            return Err(StorageError::NotFound(path.display().to_string()));
        }

        let data = fs::read(&full_path)?;
        Ok(Bytes::from(data))
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let full_path = self.full_path(path);

        if full_path.exists() {
            fs::remove_file(&full_path)?;
        }

        Ok(())
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        let full_path = self.full_path(path);
        Ok(full_path.exists())
    }

    async fn list_prefix(&self, prefix: &Path) -> Result<Vec<String>> {
        let full_path = self.full_path(prefix);

        if !full_path.exists() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        for entry in walkdir::WalkDir::new(&full_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().is_file() {
                if let Ok(relative) = entry.path().strip_prefix(&full_path) {
                    result.push(relative.display().to_string());
                }
            }
        }

        Ok(result)
    }

    async fn create_dir(&self, path: &Path) -> Result<()> {
        let full_path = self.full_path(path);
        fs::create_dir_all(&full_path)?;
        Ok(())
    }
}

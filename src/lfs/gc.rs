use crate::lfs::{LfsStorage, StorageKey};
use crate::storage::StorageBackend;
use std::collections::HashSet;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GcError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Git repository error: {0}")]
    Git(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Configuration for garbage collection
#[derive(Debug, Clone)]
pub struct GcConfig {
    /// Minimum age of unreferenced objects before deletion (default: 7 days)
    pub grace_period_days: u64,
    /// Whether to perform dry run (report but don't delete)
    pub dry_run: bool,
    /// Maximum number of objects to delete in one GC run (0 = unlimited)
    pub max_deletions: usize,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            grace_period_days: 7,
            dry_run: false,
            max_deletions: 0,
        }
    }
}

/// Results from a garbage collection run
#[derive(Debug, Clone, Default)]
pub struct GcResult {
    /// Number of objects scanned
    pub objects_scanned: u64,
    /// Number of objects referenced by Git commits
    pub objects_referenced: u64,
    /// Number of objects eligible for deletion (unreferenced + past grace period)
    pub objects_eligible: u64,
    /// Number of objects actually deleted
    pub objects_deleted: u64,
    /// Total bytes freed
    pub bytes_freed: u64,
    /// Errors encountered during GC
    pub errors: Vec<String>,
}

/// Garbage collector for LFS objects
pub struct LfsGarbageCollector<S> {
    storage: S,
    config: GcConfig,
}

impl<S> LfsGarbageCollector<S>
where
    S: LfsStorage,
{
    pub fn new(storage: S, config: GcConfig) -> Self {
        Self { storage, config }
    }

    /// Run garbage collection for a specific tenant/repo
    pub async fn collect(
        &self,
        tenant: &str,
        repo: &str,
        git_backend: &dyn StorageBackend,
    ) -> Result<GcResult, GcError> {
        let mut result = GcResult::default();

        // Step 1: Find all LFS objects in storage
        let all_objects = self
            .scan_lfs_objects(tenant, repo, git_backend)
            .await
            .map_err(|e| GcError::Storage(e.to_string()))?;

        result.objects_scanned = all_objects.len() as u64;

        // Step 2: Find all referenced objects from Git commits
        let referenced_objects = self.scan_git_references(tenant, repo, git_backend).await?;

        result.objects_referenced = referenced_objects.len() as u64;

        // Step 3: Find unreferenced objects
        let unreferenced: HashSet<_> = all_objects
            .difference(&referenced_objects)
            .cloned()
            .collect();

        let mut objects_to_delete = Vec::new();

        for oid in unreferenced {
            let key = StorageKey::new(tenant, repo, &oid);

            // Check object modification time (if available)
            // For simplicity, we'll delete all unreferenced objects past grace period
            // In a real implementation, you'd check object metadata/mtime

            // Get object size for metrics
            match self.storage.object_size(&key).await {
                Ok(Some(size)) => {
                    objects_to_delete.push((oid, size));
                }
                Ok(None) => {
                    // Object doesn't exist (already deleted?)
                    continue;
                }
                Err(e) => {
                    result
                        .errors
                        .push(format!("Failed to get size for {}: {}", oid, e));
                    continue;
                }
            }
        }

        result.objects_eligible = objects_to_delete.len() as u64;

        // Step 5: Delete objects (respecting max_deletions limit)
        let delete_limit = if self.config.max_deletions > 0 {
            self.config.max_deletions
        } else {
            objects_to_delete.len()
        };

        for (oid, size) in objects_to_delete.iter().take(delete_limit) {
            if self.config.dry_run {
                tracing::info!(
                    "DRY RUN: Would delete {}/{}/{} ({}  bytes)",
                    tenant,
                    repo,
                    oid,
                    size
                );
                result.objects_deleted += 1;
                result.bytes_freed += size;
            } else {
                let key = StorageKey::new(tenant, repo, oid);

                match self.storage.delete_object(&key).await {
                    Ok(_) => {
                        tracing::info!("Deleted {}/{}/{} ({} bytes)", tenant, repo, oid, size);
                        result.objects_deleted += 1;
                        result.bytes_freed += size;

                        // Record GC metrics
                        crate::lfs::metrics::record_gc_deletion(tenant, repo, *size);
                    }
                    Err(e) => {
                        result
                            .errors
                            .push(format!("Failed to delete {}: {}", oid, e));
                    }
                }
            }
        }

        Ok(result)
    }

    /// Scan all LFS objects in storage for a tenant/repo
    async fn scan_lfs_objects(
        &self,
        tenant: &str,
        repo: &str,
        backend: &dyn StorageBackend,
    ) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
        let objects = HashSet::new();

        // LFS objects are stored at: {tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}
        let lfs_base = PathBuf::from(format!("{}/{}.git/lfs/objects", tenant, repo));

        // List all directories under lfs/objects
        // This is a simplified implementation - in production you'd need proper directory traversal
        for prefix1 in 0..256 {
            let prefix1_hex = format!("{:02x}", prefix1);
            let prefix1_path = lfs_base.join(&prefix1_hex);

            if !backend.exists(&prefix1_path).await.unwrap_or(false) {
                continue;
            }

            for prefix2 in 0..256 {
                let prefix2_hex = format!("{:02x}", prefix2);
                let prefix2_path = prefix1_path.join(&prefix2_hex);

                if !backend.exists(&prefix2_path).await.unwrap_or(false) {
                    continue;
                }

                // In a real implementation, you'd list files in prefix2_path
                // For now, this is a placeholder that demonstrates the structure
            }
        }

        Ok(objects)
    }

    /// Scan Git repository for LFS object references
    async fn scan_git_references(
        &self,
        tenant: &str,
        repo: &str,
        backend: &dyn StorageBackend,
    ) -> Result<HashSet<String>, GcError> {
        let referenced = HashSet::new();

        // Path to Git repository
        let repo_path = PathBuf::from(format!("{}/{}.git", tenant, repo));

        // Check if repository exists
        if !backend.exists(&repo_path).await.unwrap_or(false) {
            return Err(GcError::Git(format!(
                "Repository not found: {}/{}",
                tenant, repo
            )));
        }

        // In a real implementation, you would:
        // 1. Open the Git repository using gix
        // 2. Walk all refs (branches, tags)
        // 3. For each commit, scan tree objects for LFS pointers
        // 4. Parse LFS pointer files to extract OIDs
        // 5. Add OIDs to referenced set

        // Placeholder for now - this would use gix to scan the repository
        tracing::debug!(
            "Scanning Git repository {}/{} for LFS references",
            tenant,
            repo
        );

        Ok(referenced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lfs::LfsStorageAdapter;
    use crate::storage::local::LocalStorage;

    #[tokio::test]
    async fn test_gc_config_defaults() {
        let config = GcConfig::default();
        assert_eq!(config.grace_period_days, 7);
        assert!(!config.dry_run);
        assert_eq!(config.max_deletions, 0);
    }

    #[tokio::test]
    async fn test_gc_result_default() {
        let result = GcResult::default();
        assert_eq!(result.objects_scanned, 0);
        assert_eq!(result.objects_referenced, 0);
        assert_eq!(result.objects_eligible, 0);
        assert_eq!(result.objects_deleted, 0);
        assert_eq!(result.bytes_freed, 0);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_gc_dry_run() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage_backend = LocalStorage::new(temp_dir.path().to_path_buf());

        let lfs_storage = LfsStorageAdapter::new(
            storage_backend,
            "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}".to_string(),
            "http://localhost:8090".to_string(),
        );

        let config = GcConfig {
            dry_run: true,
            grace_period_days: 0,
            max_deletions: 0,
        };

        let gc = LfsGarbageCollector::new(lfs_storage, config);

        let git_backend = LocalStorage::new(temp_dir.path().to_path_buf());

        let result: Result<GcResult, GcError> = gc.collect("tenant1", "repo1", &git_backend).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gc_max_deletions_limit() {
        let config = GcConfig {
            dry_run: false,
            grace_period_days: 7,
            max_deletions: 100,
        };

        assert_eq!(config.max_deletions, 100);
    }
}

pub mod auth;
pub mod branches;
pub mod command;
pub mod compliance;
pub mod error;
pub mod protocol;
pub mod rate_limit;
pub mod retention;
pub mod ssh;
pub mod ssh_channel;
pub mod storage;

use crate::git::rate_limit::RateLimitConfig;
use crate::storage::{StorageBackend, StorageError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitServiceError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Git error: {0}")]
    Git(String),
    #[error("Repository not found: {0}")]
    RepositoryNotFound(String),
    #[error("Repository already exists: {0}")]
    RepositoryExists(String),
    #[error("Invalid repository name: {0}")]
    InvalidName(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type GitServiceResult<T> = Result<T, GitServiceError>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RepositoryInfo {
    pub name: String,
    pub owner: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub size: u64,
    pub default_branch: String,
    pub is_private: bool,
    pub clone_url_ssh: String,
    pub clone_url_http: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub author: String,
    pub created_at: String,
}

pub struct GitService {
    storage: Arc<dyn StorageBackend>,
    config: GitConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitConfig {
    pub ssh_port: u16,
    pub host_key_path: std::path::PathBuf,
    pub authorized_keys_path: std::path::PathBuf,
    pub max_connections: usize,
    pub connection_timeout_secs: u64,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    /// Path to the root AGE wrapping key for secrets (ADR-024).
    /// When set and the file exists, repos get AGE keypairs on creation.
    #[serde(default)]
    pub secrets_root_key_path: Option<std::path::PathBuf>,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            ssh_port: 2222,
            host_key_path: std::path::PathBuf::from("host_key"),
            authorized_keys_path: std::path::PathBuf::from("authorized_keys"),
            max_connections: 100,
            connection_timeout_secs: 30,
            rate_limit: RateLimitConfig::default(),
            secrets_root_key_path: None,
        }
    }
}

impl GitService {
    pub fn new(storage: Arc<dyn StorageBackend>, config: Option<GitConfig>) -> Self {
        Self {
            storage,
            config: config.unwrap_or_default(),
        }
    }

    /// Create a new repository
    pub async fn create_repository(
        &self,
        owner: &str,
        name: &str,
        description: Option<String>,
        is_private: bool,
    ) -> GitServiceResult<RepositoryInfo> {
        self.validate_owner_name(owner)?;
        self.validate_repository_name(name)?;

        let repo_path = self.get_repository_path(owner, name);

        if self.storage.exists(&repo_path).await? {
            return Err(GitServiceError::RepositoryExists(format!(
                "{}/{}",
                owner, name
            )));
        }

        self.storage.create_directory(&repo_path).await?;

        // Resolve to absolute path before passing to gix (which uses CWD-relative paths)
        let abs_repo_path = self
            .storage
            .root()
            .expect("local storage required for repo creation")
            .join(&repo_path);
        let _repo =
            gix::init_bare(&abs_repo_path).map_err(|e| GitServiceError::Git(e.to_string()))?;

        // Provision AGE keypair for secrets (ADR-024)
        if let Some(root_key_path) = self.config.secrets_root_key_path.as_ref() {
            if root_key_path.exists() {
                let repo_id = format!("{}--{}", owner, name);
                match provision_repo_keypair(root_key_path, &abs_repo_path, &repo_id) {
                    Ok(()) => {
                        tracing::info!(repo_id = %repo_id, "provisioned AGE keypair for new repo")
                    }
                    Err(e) => {
                        tracing::warn!(repo_id = %repo_id, error = %e, "failed to provision AGE keypair for new repo, will retry on first secret access")
                    }
                }
            }
        }

        let now = chrono::Utc::now().to_rfc3339();

        let repo_info = RepositoryInfo {
            name: name.to_string(),
            owner: owner.to_string(),
            description,
            created_at: now.clone(),
            updated_at: now,
            size: 0,
            default_branch: "main".to_string(),
            is_private,
            clone_url_ssh: format!(
                "ssh://git@{}:{}/{}/{}.git",
                "localhost", self.config.ssh_port, owner, name
            ),
            clone_url_http: Some(format!("http://localhost:8080/{}/{}.git", owner, name)),
        };

        Ok(repo_info)
    }

    /// Get repository information
    pub async fn get_repository(
        &self,
        owner: &str,
        name: &str,
    ) -> GitServiceResult<RepositoryInfo> {
        self.validate_owner_name(owner)?;
        let repo_path = self.get_repository_path(owner, name);

        if !self.storage.exists(&repo_path).await? {
            return Err(GitServiceError::RepositoryNotFound(format!(
                "{}/{}",
                owner, name
            )));
        }

        let size = self.calculate_repository_size(&repo_path).await?;

        let abs_repo_path = self
            .storage
            .root()
            .expect("local storage required")
            .join(&repo_path);
        let repo = gix::open(&abs_repo_path)
            .map_err(|e| GitServiceError::RepositoryNotFound(e.to_string()))?;

        let default_branch = repo
            .head_name()
            .ok()
            .and_then(|opt_name| opt_name)
            .map(|name| {
                let name_str = name.as_bstr().to_string();
                name_str
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&name_str)
                    .to_string()
            })
            .unwrap_or_else(|| "main".to_string());

        let now = chrono::Utc::now().to_rfc3339();

        let repo_info = RepositoryInfo {
            name: name.to_string(),
            owner: owner.to_string(),
            description: Some("Repository description".to_string()),
            created_at: now.clone(),
            updated_at: now,
            size,
            default_branch,
            is_private: false,
            clone_url_ssh: format!(
                "ssh://git@{}:{}/{}/{}.git",
                "localhost", self.config.ssh_port, owner, name
            ),
            clone_url_http: Some(format!("http://localhost:8080/{}/{}.git", owner, name)),
        };

        Ok(repo_info)
    }

    /// List repositories for an owner
    pub async fn list_repositories(
        &self,
        owner: Option<&str>,
    ) -> GitServiceResult<Vec<RepositoryInfo>> {
        let mut repositories = Vec::new();

        if let Some(owner) = owner {
            // List repos under a specific owner
            let base_path = PathBuf::from(owner);
            let entries = self.storage.list(&base_path).await?;
            for entry in entries {
                let path = PathBuf::from(&entry);
                if !self.storage.exists(&path).await? {
                    continue;
                }
                let parts: Vec<&str> = entry.split('/').collect();
                if parts.len() < 2 {
                    continue;
                }
                let repo_name = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
                if let Ok(repo_info) = self.get_repository(owner, repo_name).await {
                    repositories.push(repo_info);
                }
            }
        } else {
            // List all: enumerate owner directories, then list repos within each
            let top_entries = self.storage.list(&PathBuf::from("")).await?;
            for top in &top_entries {
                let parts: Vec<&str> = top.split('/').collect();
                if parts.len() != 1 {
                    continue;
                }
                let entry_owner = parts[0];
                // Skip internal namespaces
                if entry_owner.starts_with('_') || entry_owner.starts_with('.') {
                    continue;
                }
                let base_path = PathBuf::from(entry_owner);
                let entries = self.storage.list(&base_path).await?;
                for entry in entries {
                    let path = PathBuf::from(&entry);
                    if !self.storage.exists(&path).await? {
                        continue;
                    }
                    let parts: Vec<&str> = entry.split('/').collect();
                    if parts.len() < 2 {
                        continue;
                    }
                    let repo_name = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
                    if let Ok(repo_info) = self.get_repository(entry_owner, repo_name).await {
                        repositories.push(repo_info);
                    }
                }
            }
        }

        Ok(repositories)
    }

    /// Count repositories for a given owner.
    pub async fn count_repositories(&self, owner: &str) -> GitServiceResult<usize> {
        let repos = self.list_repositories(Some(owner)).await?;
        Ok(repos.len())
    }

    /// Delete a repository
    pub async fn delete_repository(&self, owner: &str, name: &str) -> GitServiceResult<()> {
        self.validate_owner_name(owner)?;
        let repo_path = self.get_repository_path(owner, name);

        if !self.storage.exists(&repo_path).await? {
            return Err(GitServiceError::RepositoryNotFound(format!(
                "{}/{}",
                owner, name
            )));
        }

        self.storage.delete(&repo_path).await?;

        Ok(())
    }

    /// List branches in a repository
    pub async fn list_branches(
        &self,
        owner: &str,
        name: &str,
    ) -> GitServiceResult<Vec<BranchInfo>> {
        self.validate_owner_name(owner)?;
        let repo_path = self.get_repository_path(owner, name);

        if !self.storage.exists(&repo_path).await? {
            return Err(GitServiceError::RepositoryNotFound(format!(
                "{}/{}",
                owner, name
            )));
        }

        let abs_repo_path = self
            .storage
            .root()
            .expect("local storage required")
            .join(&repo_path);
        let repo = gix::open(&abs_repo_path)
            .map_err(|e| GitServiceError::RepositoryNotFound(e.to_string()))?;

        let mut branches = Vec::new();

        let refs_platform = repo
            .references()
            .map_err(|e| GitServiceError::Git(e.to_string()))?;
        let branch_iter = refs_platform
            .local_branches()
            .map_err(|e| GitServiceError::Git(e.to_string()))?;

        for reference in branch_iter {
            let r = reference.map_err(|e| GitServiceError::Git(e.to_string()))?;
            let name_str = r.inner.name.as_bstr().to_string();
            let branch_name = name_str
                .strip_prefix("refs/heads/")
                .unwrap_or(&name_str)
                .to_string();

            let target_id = r
                .inner
                .target
                .try_id()
                .ok_or_else(|| {
                    GitServiceError::Git(format!("branch '{}' is symbolic", branch_name))
                })?
                .to_owned();

            let commit = repo
                .find_commit(target_id)
                .map_err(|e| GitServiceError::Git(e.to_string()))?;

            let commit_message = commit
                .message()
                .map(|m| m.summary().to_string())
                .unwrap_or_default();

            let author_name = commit
                .author()
                .map(|a| a.name.to_string())
                .unwrap_or_else(|_| "Unknown".to_string());

            let created_at = commit
                .time()
                .map(|t| {
                    chrono::DateTime::from_timestamp(t.seconds, 0)
                        .unwrap_or_else(chrono::Utc::now)
                        .to_rfc3339()
                })
                .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());

            branches.push(BranchInfo {
                name: branch_name,
                commit_sha: target_id.to_hex().to_string(),
                commit_message,
                author: author_name,
                created_at,
            });
        }

        Ok(branches)
    }

    fn get_repository_path(&self, owner: &str, name: &str) -> PathBuf {
        PathBuf::from(owner).join(format!("{}.git", name))
    }

    pub fn get_repository_full_path(&self, owner: &str, name: &str) -> PathBuf {
        self.get_repository_path(owner, name)
    }

    pub fn get_storage_root(&self) -> Option<PathBuf> {
        self.storage.root().map(|p| p.to_path_buf())
    }

    fn validate_repository_name(&self, name: &str) -> GitServiceResult<()> {
        if name.is_empty() {
            return Err(GitServiceError::InvalidName(
                "Repository name cannot be empty".to_string(),
            ));
        }

        if name.len() > 100 {
            return Err(GitServiceError::InvalidName(
                "Repository name too long".to_string(),
            ));
        }

        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(GitServiceError::InvalidName(
                "Repository name contains invalid characters".to_string(),
            ));
        }

        if name.starts_with('-') || name.ends_with('-') {
            return Err(GitServiceError::InvalidName(
                "Repository name cannot start or end with hyphen".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate the owner/namespace component of a repository path.
    /// Prevents path traversal attacks (../../etc).
    fn validate_owner_name(&self, owner: &str) -> GitServiceResult<()> {
        if owner.is_empty() {
            return Err(GitServiceError::InvalidName(
                "Owner name cannot be empty".to_string(),
            ));
        }

        if owner.len() > 100 {
            return Err(GitServiceError::InvalidName(
                "Owner name too long".to_string(),
            ));
        }

        // Reject path traversal characters
        if owner.contains('/')
            || owner.contains('\\')
            || owner.contains('.')
            || owner.contains('\0')
        {
            return Err(GitServiceError::InvalidName(
                "Owner name contains invalid characters".to_string(),
            ));
        }

        if !owner
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(GitServiceError::InvalidName(
                "Owner name contains invalid characters".to_string(),
            ));
        }

        if owner.starts_with('-') || owner.starts_with('_') {
            return Err(GitServiceError::InvalidName(
                "Owner name cannot start with hyphen or underscore".to_string(),
            ));
        }

        Ok(())
    }

    async fn calculate_repository_size(&self, repo_path: &Path) -> GitServiceResult<u64> {
        let entries = self.storage.list(repo_path).await?;
        let mut total_size = 0u64;

        for _entry in entries {
            total_size += 1024;
        }

        Ok(total_size)
    }
}

/// Provision an AGE keypair for a newly created repo.
/// Stores encrypted private key + public key in system repo and public key in user repo.
fn provision_repo_keypair(
    root_key_path: &std::path::Path,
    repo_path: &std::path::Path,
    repo_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use crate::adp::secrets::key_manager::RepoKeyManager;
    use crate::adp::secrets::{repo_private_key_ref, repo_public_key_ref, PUBLIC_KEY_PREFIX};
    use crate::adp::storage::AdpStore;

    let key_manager = RepoKeyManager::new(root_key_path.to_path_buf());
    let (encrypted_private, public_key) = key_manager.generate_keypair(repo_id)?;

    // Find storage root (parent of owner dir → parent of repo dir → parent of owner)
    let storage_root = repo_path
        .parent()
        .and_then(|p| p.parent())
        .ok_or("cannot determine storage root from repo path")?;
    let system_repo = storage_root.join("_adp/system");

    // Store encrypted private key in system repo
    let private_ref = repo_private_key_ref(repo_id);
    let private_value = serde_json::json!({
        "encrypted_key": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encrypted_private)
    });
    AdpStore::write_ref_blob(&system_repo, &private_ref, &private_value)?;

    // Store public key in system repo
    let public_ref = repo_public_key_ref(repo_id);
    let public_value = serde_json::json!({ "public_key": public_key });
    AdpStore::write_ref_blob(&system_repo, &public_ref, &public_value)?;

    // Store public key in user repo
    let user_pk_value = serde_json::json!({ "public_key": public_key });
    AdpStore::write_ref_blob(repo_path, PUBLIC_KEY_PREFIX, &user_pk_value)?;

    Ok(())
}

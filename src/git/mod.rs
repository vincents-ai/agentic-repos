pub mod auth;
pub mod branches;
pub mod command;
pub mod compliance;
pub mod error;
pub mod protocol;
pub mod rate_limit;
pub mod retention;
pub mod ssh;
pub mod storage;

use crate::git::rate_limit::RateLimitConfig;
use crate::storage::{StorageBackend, StorageError};
use git2::{Repository as Git2Repository, RepositoryInitOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitServiceError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
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
        self.validate_repository_name(name)?;

        let repo_path = self.get_repository_path(owner, name);

        if self.storage.exists(&repo_path).await? {
            return Err(GitServiceError::RepositoryExists(format!(
                "{}/{}",
                owner, name
            )));
        }

        self.storage.create_directory(&repo_path).await?;

        let mut opts = RepositoryInitOptions::new();
        opts.bare(true);
        let _repo = Git2Repository::init_opts(&repo_path, &opts)?;

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
        let repo_path = self.get_repository_path(owner, name);

        if !self.storage.exists(&repo_path).await? {
            return Err(GitServiceError::RepositoryNotFound(format!(
                "{}/{}",
                owner, name
            )));
        }

        let size = self.calculate_repository_size(&repo_path).await?;

        let repo = Git2Repository::open_bare(&repo_path)?;

        let default_branch = match repo.head() {
            Ok(head) => {
                if let Some(name) = head.shorthand() {
                    name.to_string()
                } else {
                    "main".to_string()
                }
            }
            Err(_) => "main".to_string(),
        };

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
        let base_path = if let Some(owner) = owner {
            PathBuf::from(owner)
        } else {
            PathBuf::from("")
        };

        let entries = self.storage.list(&base_path).await?;
        let mut repositories = Vec::new();

        for entry in entries {
            let path = PathBuf::from(&entry);

            if !self.storage.exists(&path).await? {
                continue;
            }

            let parts: Vec<&str> = entry.split('/').collect();
            if parts.len() < 2 {
                continue;
            }

            let entry_owner = parts[0];
            let repo_name = parts[1];

            if let Some(filter_owner) = owner {
                if entry_owner != filter_owner {
                    continue;
                }
            }

            if let Ok(repo_info) = self.get_repository(entry_owner, repo_name).await {
                repositories.push(repo_info);
            }
        }

        Ok(repositories)
    }

    /// Delete a repository
    pub async fn delete_repository(&self, owner: &str, name: &str) -> GitServiceResult<()> {
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
        let repo_path = self.get_repository_path(owner, name);

        if !self.storage.exists(&repo_path).await? {
            return Err(GitServiceError::RepositoryNotFound(format!(
                "{}/{}",
                owner, name
            )));
        }

        let repo = Git2Repository::open_bare(&repo_path)?;

        let mut branches = Vec::new();

        let branch_iter = repo.branches(Some(git2::BranchType::Local))?;
        for branch_result in branch_iter {
            let (branch, _branch_type) = branch_result?;

            if let Some(branch_name) = branch.name()? {
                if let Some(target) = branch.get().target() {
                    let commit = repo.find_commit(target)?;

                    let branch_info = BranchInfo {
                        name: branch_name.to_string(),
                        commit_sha: target.to_string(),
                        commit_message: commit.message().unwrap_or("").to_string(),
                        author: commit.author().name().unwrap_or("Unknown").to_string(),
                        created_at: chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
                            .unwrap_or_else(chrono::Utc::now)
                            .to_rfc3339(),
                    };

                    branches.push(branch_info);
                }
            }
        }

        Ok(branches)
    }

    fn get_repository_path(&self, owner: &str, name: &str) -> PathBuf {
        PathBuf::from(owner).join(format!("{}.git", name))
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

    async fn calculate_repository_size(&self, repo_path: &Path) -> GitServiceResult<u64> {
        let entries = self.storage.list(repo_path).await?;
        let mut total_size = 0u64;

        for _entry in entries {
            total_size += 1024;
        }

        Ok(total_size)
    }
}

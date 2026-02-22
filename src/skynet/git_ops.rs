//! Git operations service for Skynet v2 plugin

use crate::config::AppConfig;
use crate::skynet::error::SkynetPluginResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, error};
use git2::{Repository, BranchType};
use std::path::Path;

/// Git operation service
pub struct GitOperationService {
    #[allow(dead_code)]
    config: AppConfig,
}

/// Branch information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub commit_sha: String,
    pub commit_message: String,
    pub author: String,
    pub created_at: String,
}

/// Commit information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub author: String,
    pub email: String,
    pub message: String,
    pub timestamp: i64,
    pub parent_shas: Vec<String>,
}

/// Diff statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    pub insertions: usize,
    pub deletions: usize,
    pub files_changed: usize,
    pub patches: Vec<FilePatch>,
}

/// File patch information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatch {
    pub path: String,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
}

impl GitOperationService {
    /// Create a new Git operation service
    pub async fn new(config: AppConfig) -> SkynetPluginResult<Self> {
        Ok(Self { config })
    }

    /// Create a repository
    pub async fn create_repository(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> SkynetPluginResult<()> {
        debug!(
            "Creating repository: tenant_id={}, repo_name={}",
            tenant_id, repo_name
        );
        Ok(())
    }

    /// Delete a repository
    pub async fn delete_repository(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> SkynetPluginResult<()> {
        debug!(
            "Deleting repository: tenant_id={}, repo_name={}",
            tenant_id, repo_name
        );
        Ok(())
    }

    /// List repositories
    pub async fn list_repositories(&self, tenant_id: &str) -> SkynetPluginResult<Vec<Value>> {
        debug!("Listing repositories: tenant_id={}", tenant_id);
        Ok(vec![])
    }

    /// Get repository info
    pub async fn get_repository_info(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> SkynetPluginResult<Value> {
        debug!(
            "Getting repository info: tenant_id={}, repo_name={}",
            tenant_id, repo_name
        );
        Ok(json!({
            "tenant_id": tenant_id,
            "name": repo_name,
            "created_at": chrono::Utc::now().to_rfc3339(),
        }))
    }

    /// List branches in a repository
    pub async fn list_branches(
        &self,
        _tenant_id: &str,
        repo_path: &Path,
    ) -> SkynetPluginResult<Vec<BranchInfo>> {
        debug!("Listing branches from: {}", repo_path.display());

        let repo = Repository::open(repo_path)
            .map_err(|e| {
                error!("Failed to open repository: {}", e);
                crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                    repo_path.to_string_lossy().to_string(),
                )
            })?;

        let mut branches = Vec::new();
        let branch_iter = repo.branches(Some(BranchType::Local))
            .map_err(|e| {
                error!("Failed to iterate branches: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to list branches: {}", e))
            })?;

        for branch_result in branch_iter {
            let (branch, _branch_type) = branch_result
                .map_err(|e| {
                    error!("Failed to get branch: {}", e);
                    crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to get branch: {}", e))
                })?;

            let branch_name = branch.name()
                .map_err(|e| {
                    error!("Failed to get branch name: {}", e);
                    crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to get branch name: {}", e))
                })?
                .unwrap_or("unknown")
                .to_string();

            let head = if let Ok(head_ref) = repo.head() {
                head_ref.shorthand().unwrap_or("unknown").to_string()
            } else {
                "unknown".to_string()
            };
            let is_head = branch_name == head;

            let commit = branch.get()
                .peel_to_commit()
                .ok();

            if let Some(commit) = commit {
                let timestamp = commit.time().seconds();
                branches.push(BranchInfo {
                    name: branch_name,
                    is_head,
                    commit_sha: commit.id().to_string(),
                    commit_message: commit.message().unwrap_or("").to_string(),
                    author: commit.author().name().unwrap_or("unknown").to_string(),
                    created_at: chrono::DateTime::from_timestamp(timestamp, 0)
                        .unwrap_or(chrono::Utc::now())
                        .to_rfc3339(),
                });
            }
        }

        Ok(branches)
    }

    /// Create a new branch
    pub async fn create_branch(
        &self,
        _tenant_id: &str,
        repo_path: &Path,
        branch_name: &str,
        start_point: &str,
    ) -> SkynetPluginResult<BranchInfo> {
        debug!(
            "Creating branch '{}' from '{}' at: {}",
            branch_name,
            start_point,
            repo_path.display()
        );

        let repo = Repository::open(repo_path)
            .map_err(|e| {
                error!("Failed to open repository: {}", e);
                crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                    repo_path.to_string_lossy().to_string(),
                )
            })?;

        let obj = repo.revparse_single(start_point)
            .map_err(|e| {
                error!("Failed to find start point: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Invalid start point: {}", e))
            })?;

        let commit = repo.find_commit(obj.id())
            .map_err(|e| {
                error!("Failed to find commit: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to create branch: {}", e))
            })?;

        repo.branch(branch_name, &commit, false)
            .map_err(|e| {
                error!("Failed to create branch: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to create branch: {}", e))
            })?;

        let author_name = commit.author().name().unwrap_or("unknown").to_string();
        let commit_msg = commit.message().unwrap_or("").to_string();

        Ok(BranchInfo {
            name: branch_name.to_string(),
            is_head: false,
            commit_sha: commit.id().to_string(),
            commit_message: commit_msg,
            author: author_name,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Delete a branch
    pub async fn delete_branch(
        &self,
        _tenant_id: &str,
        repo_path: &Path,
        branch_name: &str,
    ) -> SkynetPluginResult<()> {
        debug!(
            "Deleting branch '{}' from: {}",
            branch_name,
            repo_path.display()
        );

        let repo = Repository::open(repo_path)
            .map_err(|e| {
                error!("Failed to open repository: {}", e);
                crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                    repo_path.to_string_lossy().to_string(),
                )
            })?;

        let mut branch = repo.find_branch(branch_name, BranchType::Local)
            .map_err(|e| {
                error!("Failed to find branch: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Branch not found: {}", e))
            })?;

        branch.delete()
            .map_err(|e| {
                error!("Failed to delete branch: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to delete branch: {}", e))
            })?;

        Ok(())
    }

    /// Get commit history
    pub async fn get_commit_history(
        &self,
        _tenant_id: &str,
        repo_path: &Path,
        reference: &str,
        limit: usize,
    ) -> SkynetPluginResult<Vec<CommitInfo>> {
        debug!(
            "Getting commit history for '{}' (limit: {}) from: {}",
            reference,
            limit,
            repo_path.display()
        );

        let repo = Repository::open(repo_path)
            .map_err(|e| {
                error!("Failed to open repository: {}", e);
                crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                    repo_path.to_string_lossy().to_string(),
                )
            })?;

        let obj = repo.revparse_single(reference)
            .map_err(|e| {
                error!("Failed to find reference: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Invalid reference: {}", e))
            })?;

        let mut revwalk = repo.revwalk()
            .map_err(|e| {
                error!("Failed to create revwalk: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to walk commits: {}", e))
            })?;

        revwalk.push(obj.id())
            .map_err(|e| {
                error!("Failed to push object: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to walk commits: {}", e))
            })?;

        let mut commits = Vec::new();
        for (i, oid) in revwalk.enumerate() {
            if i >= limit {
                break;
            }

            if let Ok(oid) = oid {
                if let Ok(commit) = repo.find_commit(oid) {
                    let timestamp = commit.time().seconds();
                    commits.push(CommitInfo {
                        sha: commit.id().to_string(),
                        author: commit.author().name().unwrap_or("unknown").to_string(),
                        email: commit.author().email().unwrap_or("unknown").to_string(),
                        message: commit.message().unwrap_or("").to_string(),
                        timestamp,
                        parent_shas: commit.parent_ids()
                            .map(|id| id.to_string())
                            .collect(),
                    });
                }
            }
        }

        Ok(commits)
    }

    /// Get diff between two commits
    pub async fn get_diff(
        &self,
        _tenant_id: &str,
        repo_path: &Path,
        from_ref: &str,
        to_ref: &str,
    ) -> SkynetPluginResult<DiffStats> {
        debug!(
            "Getting diff between '{}' and '{}' from: {}",
            from_ref,
            to_ref,
            repo_path.display()
        );

        let repo = Repository::open(repo_path)
            .map_err(|e| {
                error!("Failed to open repository: {}", e);
                crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                    repo_path.to_string_lossy().to_string(),
                )
            })?;

        let from_obj = repo.revparse_single(from_ref)
            .map_err(|e| {
                error!("Failed to find 'from' reference: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Invalid 'from' reference: {}", e))
            })?;

        let to_obj = repo.revparse_single(to_ref)
            .map_err(|e| {
                error!("Failed to find 'to' reference: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Invalid 'to' reference: {}", e))
            })?;

        let from_tree = repo.find_commit(from_obj.id())
            .map_err(|e| {
                error!("Failed to find 'from' commit: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to get diff: {}", e))
            })?
            .tree()
            .map_err(|e| {
                error!("Failed to get 'from' tree: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to get diff: {}", e))
            })?;

        let to_tree = repo.find_commit(to_obj.id())
            .map_err(|e| {
                error!("Failed to find 'to' commit: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to get diff: {}", e))
            })?
            .tree()
            .map_err(|e| {
                error!("Failed to get 'to' tree: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to get diff: {}", e))
            })?;

        let diff = repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(|e| {
                error!("Failed to compute diff: {}", e);
                crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to get diff: {}", e))
            })?;

        let mut stats = DiffStats {
            insertions: 0,
            deletions: 0,
            files_changed: 0,
            patches: Vec::new(),
        };

        diff.foreach(
            &mut |delta, _progress| {
                if let Some(path) = delta.new_file().path() {
                    stats.files_changed += 1;
                    stats.patches.push(FilePatch {
                        path: path.to_string_lossy().to_string(),
                        status: format!("{:?}", delta.status()),
                        additions: 0,
                        deletions: 0,
                    });
                }
                true
            },
            None,
            None,
            None,
        )
        .map_err(|e| {
            error!("Failed to iterate diff: {}", e);
            crate::skynet::error::SkynetPluginError::GitOperation(format!("Failed to get diff: {}", e))
        })?;

        Ok(stats)
    }
}

//! Git operations service for Skynet v2 plugin
//!
//! Reference management uses `gix`; object operations (commit lookup, revwalk,
//! diff) still use `git2` (object migration is a separate phase).

use crate::config::AppConfig;
use crate::skynet::error::SkynetPluginResult;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
use gix::refs::{FullName, Target};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tracing::{debug, error};

/// Git operation service
pub struct GitOperationService {
    #[allow(dead_code)]
    config: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub commit_sha: String,
    pub commit_message: String,
    pub author: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub author: String,
    pub email: String,
    pub message: String,
    pub timestamp: i64,
    pub parent_shas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    pub insertions: usize,
    pub deletions: usize,
    pub files_changed: usize,
    pub patches: Vec<FilePatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatch {
    pub path: String,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
}

fn git_err(msg: &str, e: impl std::fmt::Display) -> crate::skynet::error::SkynetPluginError {
    error!("{}", msg);
    crate::skynet::error::SkynetPluginError::GitOperation(format!("{}: {}", msg, e))
}

impl GitOperationService {
    pub async fn new(config: AppConfig) -> SkynetPluginResult<Self> {
        Ok(Self { config })
    }

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

    pub async fn list_repositories(&self, tenant_id: &str) -> SkynetPluginResult<Vec<Value>> {
        debug!("Listing repositories: tenant_id={}", tenant_id);
        Ok(vec![])
    }

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

    /// List branches in a repository.
    ///
    /// Uses `gix` for ref iteration and HEAD resolution; `git2` for commit
    /// object data (author, message, timestamp).
    pub async fn list_branches(
        &self,
        _tenant_id: &str,
        repo_path: &Path,
    ) -> SkynetPluginResult<Vec<BranchInfo>> {
        debug!("Listing branches from: {}", repo_path.display());

        let git2_repo = git2::Repository::open(repo_path).map_err(|_| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                repo_path.to_string_lossy().to_string(),
            )
        })?;
        let gix_repo = gix::open(repo_path).map_err(|e| git_err("Failed to open repo with gix", e))?;

        let head_name = gix_repo
            .head_name()
            .unwrap_or(None)
            .map(|n| n.as_bstr().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let platform = gix_repo
            .references()
            .map_err(|e| git_err("Failed to list branches", e))?;
        let branch_iter = platform
            .local_branches()
            .map_err(|e| git_err("Failed to list local branches", e))?;

        let mut branches = Vec::new();
        for branch_result in branch_iter {
            let reference = match branch_result {
                Ok(r) => r,
                Err(e) => {
                    error!("Failed to get branch ref: {}", e);
                    continue;
                }
            };

            let name_bstr = reference.name().as_bstr();
            let branch_name = String::from_utf8_lossy(
                name_bstr
                    .strip_prefix(b"refs/heads/")
                    .unwrap_or(name_bstr),
            )
            .into_owned();

            let is_head = branch_name == head_name;

            if let Some(id) = reference.try_id() {
                let hex = id.to_hex().to_string();
                if let Ok(git2_oid) = git2::Oid::from_str(&hex) {
                    if let Ok(commit) = git2_repo.find_commit(git2_oid) {
                        let timestamp = commit.time().seconds();
                        branches.push(BranchInfo {
                            name: branch_name,
                            is_head,
                            commit_sha: hex,
                            commit_message: commit.message().unwrap_or("").to_string(),
                            author: commit.author().name().unwrap_or("unknown").to_string(),
                            created_at: chrono::DateTime::from_timestamp(timestamp, 0)
                                .unwrap_or(chrono::Utc::now())
                                .to_rfc3339(),
                        });
                    }
                }
            }
        }

        Ok(branches)
    }

    /// Create a new branch.
    ///
    /// Uses `gix` for ref creation; `git2` for commit object access.
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

        let git2_repo = git2::Repository::open(repo_path).map_err(|_| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                repo_path.to_string_lossy().to_string(),
            )
        })?;

        let gix_repo = gix::open(repo_path).map_err(|e| git_err("Failed to open repo with gix", e))?;

        // Resolve start point to a commit OID using git2
        let obj = git2_repo
            .revparse_single(start_point)
            .map_err(|e| git_err("Invalid start point", e))?;
        let commit = git2_repo
            .find_commit(obj.id())
            .map_err(|e| git_err("Start point is not a commit", e))?;

        let commit_hex = commit.id().to_string();
        let gix_commit_oid = gix::hash::ObjectId::try_from(commit.id().as_bytes())
            .expect("valid 20-byte sha1 from git2");

        // Create the branch ref using gix (fail if already exists)
        let ref_name = format!("refs/heads/{}", branch_name);
        gix_repo
            .edit_reference(RefEdit {
                change: Change::Update {
                    log: LogChange::default(),
                    expected: PreviousValue::MustNotExist,
                    new: Target::Object(gix_commit_oid),
                },
                name: FullName::try_from(ref_name.as_str())
                    .map_err(|e| git_err("Invalid branch name", e))?,
                deref: false,
            })
            .map_err(|e| git_err("Failed to create branch", e))?;

        let author_name = commit.author().name().unwrap_or("unknown").to_string();
        let commit_msg = commit.message().unwrap_or("").to_string();

        Ok(BranchInfo {
            name: branch_name.to_string(),
            is_head: false,
            commit_sha: commit_hex,
            commit_message: commit_msg,
            author: author_name,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Delete a branch.
    ///
    /// Uses `gix` for ref deletion.
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

        let gix_repo = gix::open(repo_path).map_err(|_| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                repo_path.to_string_lossy().to_string(),
            )
        })?;

        let ref_name = format!("refs/heads/{}", branch_name);
        gix_repo
            .edit_reference(RefEdit {
                change: Change::Delete {
                    expected: PreviousValue::MustExist,
                    log: gix::refs::transaction::RefLog::AndReference,
                },
                name: FullName::try_from(ref_name.as_str())
                    .map_err(|e| git_err("Invalid branch name", e))?,
                deref: false,
            })
            .map_err(|e| git_err("Failed to delete branch", e))?;

        Ok(())
    }

    /// Get commit history.
    ///
    /// Still uses `git2` for revwalk (object operation, not ref management).
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

        let repo = git2::Repository::open(repo_path).map_err(|_| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                repo_path.to_string_lossy().to_string(),
            )
        })?;

        let obj = repo
            .revparse_single(reference)
            .map_err(|e| git_err("Invalid reference", e))?;

        let mut revwalk = repo.revwalk().map_err(|e| git_err("Failed to walk commits", e))?;
        revwalk
            .push(obj.id())
            .map_err(|e| git_err("Failed to push object", e))?;

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
                        author: commit
                            .author()
                            .name()
                            .unwrap_or("unknown")
                            .to_string(),
                        email: commit
                            .author()
                            .email()
                            .unwrap_or("unknown")
                            .to_string(),
                        message: commit.message().unwrap_or("").to_string(),
                        timestamp,
                        parent_shas: commit.parent_ids().map(|id| id.to_string()).collect(),
                    });
                }
            }
        }

        Ok(commits)
    }

    /// Get diff between two commits.
    ///
    /// Still uses `git2` for diff computation (object operation, not ref management).
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

        let repo = git2::Repository::open(repo_path).map_err(|_| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                repo_path.to_string_lossy().to_string(),
            )
        })?;

        let from_obj = repo
            .revparse_single(from_ref)
            .map_err(|e| git_err("Invalid 'from' reference", e))?;

        let to_obj = repo
            .revparse_single(to_ref)
            .map_err(|e| git_err("Invalid 'to' reference", e))?;

        let from_tree = repo
            .find_commit(from_obj.id())
            .map_err(|e| git_err("Failed to find 'from' commit", e))?
            .tree()
            .map_err(|e| git_err("Failed to get 'from' tree", e))?;

        let to_tree = repo
            .find_commit(to_obj.id())
            .map_err(|e| git_err("Failed to find 'to' commit", e))?
            .tree()
            .map_err(|e| git_err("Failed to get 'to' tree", e))?;

        let diff = repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(|e| git_err("Failed to compute diff", e))?;

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
        .map_err(|e| git_err("Failed to iterate diff", e))?;

        Ok(stats)
    }
}

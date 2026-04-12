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

        let gix_repo = gix::open(repo_path).map_err(|e| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(format!(
                "Failed to open repo: {}",
                e
            ))
        })?;

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
                name_bstr.strip_prefix(b"refs/heads/").unwrap_or(name_bstr),
            )
            .into_owned();

            let is_head = branch_name == head_name;

            if let Some(id) = reference.try_id() {
                let hex = id.to_hex().to_string();
                if let Ok(obj) = gix_repo.find_object(id) {
                    if let Ok(commit) = gix::objs::CommitRef::from_bytes(&obj.data) {
                        let timestamp = commit.committer.time.seconds;
                        branches.push(BranchInfo {
                            name: branch_name,
                            is_head,
                            commit_sha: hex,
                            commit_message: commit.message.to_string(),
                            author: commit.author.name.to_string(),
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
    /// Uses `gix` for both ref creation and commit object access.
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

        let gix_repo = gix::open(repo_path).map_err(|e| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(format!(
                "Failed to open repo: {}",
                e
            ))
        })?;

        // Resolve start point to a commit OID using gix
        let ref_result = gix_repo
            .try_find_reference(start_point)
            .map_err(|e| git_err("Invalid start point", e))?;

        let commit_id = if let Some(r) = ref_result {
            r.try_id()
                .ok_or_else(|| {
                    git_err(
                        "Start point is symbolic ref",
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, "symbolic ref"),
                    )
                })?
                .detach()
        } else {
            // Try as raw OID
            gix::hash::ObjectId::from_hex(start_point.as_bytes())
                .map_err(|e| git_err("Invalid start point OID", e))?
        };

        // Verify it's a commit
        let obj = gix_repo
            .find_object(commit_id)
            .map_err(|e| git_err("Start point not found", e))?;
        let commit = gix::objs::CommitRef::from_bytes(&obj.data)
            .map_err(|e| git_err("Start point is not a commit", e))?;

        let commit_hex = commit_id.to_hex().to_string();

        // Create the branch ref using gix (fail if already exists)
        let ref_name = format!("refs/heads/{}", branch_name);
        gix_repo
            .edit_reference(RefEdit {
                change: Change::Update {
                    log: LogChange::default(),
                    expected: PreviousValue::MustNotExist,
                    new: Target::Object(commit_id),
                },
                name: FullName::try_from(ref_name.as_str())
                    .map_err(|e| git_err("Invalid branch name", e))?,
                deref: false,
            })
            .map_err(|e| git_err("Failed to create branch", e))?;

        let author_name = commit.author.name.to_string();
        let commit_msg = commit.message.to_string();

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
    /// Uses `gix` for commit traversal (replaces git2 revwalk with manual parent walking).
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

        let gix_repo = gix::open(repo_path).map_err(|_| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                repo_path.to_string_lossy().to_string(),
            )
        })?;

        // Resolve reference to an OID
        let start_id = if let Some(r) = gix_repo
            .try_find_reference(reference)
            .map_err(|e| git_err("Invalid reference", e))?
        {
            r.try_id()
                .ok_or_else(|| {
                    git_err(
                        "Symbolic ref not supported",
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, "symbolic ref"),
                    )
                })?
                .detach()
        } else {
            gix::hash::ObjectId::from_hex(reference.as_bytes())
                .map_err(|e| git_err("Invalid reference OID", e))?
        };

        // Walk commits by following parents (BFS-style like revwalk)
        let mut commits = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        queue.push_back(start_id);
        visited.insert(start_id);

        while let Some(current_id) = queue.pop_front() {
            if commits.len() >= limit {
                break;
            }

            let obj = match gix_repo.find_object(current_id) {
                Ok(o) => o,
                Err(_) => continue,
            };

            let commit = match gix::objs::CommitRef::from_bytes(&obj.data) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let parent_shas: Vec<String> =
                commit.parents().map(|p| p.to_hex().to_string()).collect();

            // Queue unvisited parents
            for parent_id in commit.parents() {
                if !visited.contains(&parent_id) {
                    visited.insert(parent_id);
                    queue.push_back(parent_id);
                }
            }

            commits.push(CommitInfo {
                sha: current_id.to_hex().to_string(),
                author: commit.author.name.to_string(),
                email: commit.author.email.to_string(),
                message: commit.message.to_string(),
                timestamp: commit.committer.time.seconds,
                parent_shas,
            });
        }

        Ok(commits)
    }

    /// Get diff between two commits.
    ///
    /// Uses `gix::diff::tree` for tree comparison.
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

        let gix_repo = gix::open(repo_path).map_err(|_| {
            crate::skynet::error::SkynetPluginError::RepositoryNotFound(
                repo_path.to_string_lossy().to_string(),
            )
        })?;

        // Resolve refs to commit OIDs
        let from_id =
            resolve_ref(&gix_repo, from_ref).map_err(|e| git_err("Invalid 'from' reference", e))?;
        let to_id =
            resolve_ref(&gix_repo, to_ref).map_err(|e| git_err("Invalid 'to' reference", e))?;

        // Get tree IDs from commits
        let from_obj = gix_repo
            .find_object(from_id)
            .map_err(|e| git_err("Failed to find 'from' commit", e))?;
        let from_commit = gix::objs::CommitRef::from_bytes(&from_obj.data)
            .map_err(|e| git_err("'from' is not a commit", e))?;
        let from_tree_id = from_commit.tree();

        let to_obj = gix_repo
            .find_object(to_id)
            .map_err(|e| git_err("Failed to find 'to' commit", e))?;
        let to_commit = gix::objs::CommitRef::from_bytes(&to_obj.data)
            .map_err(|e| git_err("'to' is not a commit", e))?;
        let to_tree_id = to_commit.tree();

        // Parse trees
        let from_tree_obj = gix_repo
            .find_object(from_tree_id)
            .map_err(|e| git_err("Failed to get 'from' tree", e))?;
        let from_tree = gix::objs::TreeRef::from_bytes(&from_tree_obj.data)
            .map_err(|e| git_err("Failed to parse 'from' tree", e))?;

        let to_tree_obj = gix_repo
            .find_object(to_tree_id)
            .map_err(|e| git_err("Failed to get 'to' tree", e))?;
        let to_tree = gix::objs::TreeRef::from_bytes(&to_tree_obj.data)
            .map_err(|e| git_err("Failed to parse 'to' tree", e))?;

        // Collect changes using a simple tree entry comparison
        let mut stats = DiffStats {
            insertions: 0,
            deletions: 0,
            files_changed: 0,
            patches: Vec::new(),
        };

        // Build maps from filename to entry for both trees
        let from_entries: std::collections::HashMap<Vec<u8>, &gix::objs::tree::EntryRef> =
            from_tree
                .entries
                .iter()
                .map(|e| (e.filename.to_vec(), e))
                .collect();
        let to_entries: std::collections::HashMap<Vec<u8>, &gix::objs::tree::EntryRef> = to_tree
            .entries
            .iter()
            .map(|e| (e.filename.to_vec(), e))
            .collect();

        // Find added and modified files
        for (name, entry) in &to_entries {
            let path_str = String::from_utf8_lossy(name).to_string();
            match from_entries.get(name) {
                None => {
                    stats.files_changed += 1;
                    stats.patches.push(FilePatch {
                        path: path_str,
                        status: "added".to_string(),
                        additions: 0,
                        deletions: 0,
                    });
                }
                Some(from_entry) if from_entry.oid != entry.oid => {
                    stats.files_changed += 1;
                    stats.patches.push(FilePatch {
                        path: path_str,
                        status: "modified".to_string(),
                        additions: 0,
                        deletions: 0,
                    });
                }
                _ => {}
            }
        }

        // Find deleted files
        for name in from_entries.keys() {
            if !to_entries.contains_key(name) {
                let path_str = String::from_utf8_lossy(name).to_string();
                stats.files_changed += 1;
                stats.patches.push(FilePatch {
                    path: path_str,
                    status: "deleted".to_string(),
                    additions: 0,
                    deletions: 0,
                });
            }
        }

        Ok(stats)
    }
}

/// Resolve a reference name or raw OID to an ObjectId.
fn resolve_ref(repo: &gix::Repository, reference: &str) -> Result<gix::ObjectId, String> {
    if let Some(r) = repo
        .try_find_reference(reference)
        .map_err(|e| e.to_string())?
    {
        r.try_id()
            .map(|id| id.detach())
            .ok_or_else(|| "symbolic ref not supported".to_string())
    } else {
        gix::hash::ObjectId::from_hex(reference.as_bytes())
            .map_err(|e| format!("Invalid OID: {}", e))
    }
}

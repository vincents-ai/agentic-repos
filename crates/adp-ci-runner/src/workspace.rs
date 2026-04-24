//! Workspace management — isolated temp directories for job execution.

use crate::error::RunnerError;
use std::path::{Path, PathBuf};

/// An isolated workspace for a single job execution.
///
/// Each job gets its own temp directory with:
/// - `workspace/` — git checkout directory
/// - `secrets/` — tmpfs-mounted secrets
/// - `artifacts/` — collected build outputs
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Create a new workspace in the system temp directory.
    pub fn create(job_id: &str) -> Result<Self, RunnerError> {
        let root = std::env::temp_dir().join(format!("adp-ci-{}", job_id));
        std::fs::create_dir_all(&root)?;

        let ws = Self { root };
        ws.ensure_subdirs()?;
        Ok(ws)
    }

    /// Create with a specific root path (for testing).
    pub fn create_at(root: PathBuf) -> Result<Self, RunnerError> {
        std::fs::create_dir_all(&root)?;
        let ws = Self { root };
        ws.ensure_subdirs()?;
        Ok(ws)
    }

    fn ensure_subdirs(&self) -> Result<(), RunnerError> {
        std::fs::create_dir_all(self.workspace_dir())?;
        std::fs::create_dir_all(self.secrets_dir())?;
        std::fs::create_dir_all(self.artifacts_dir())?;
        Ok(())
    }

    /// Root directory of the workspace.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Git checkout directory (`{root}/workspace/`).
    pub fn workspace_dir(&self) -> PathBuf {
        self.root.join("workspace")
    }

    /// Secrets mount directory (`{root}/secrets/`).
    pub fn secrets_dir(&self) -> PathBuf {
        self.root.join("secrets")
    }

    /// Artifacts output directory (`{root}/artifacts/`).
    pub fn artifacts_dir(&self) -> PathBuf {
        self.root.join("artifacts")
    }

    /// Check out a git repository at a specific commit.
    ///
    /// Uses `git clone` followed by `git checkout`. In a full implementation,
    /// this would use gix for pure-Rust git operations. For now, this provides
    /// the workspace directory structure and expects the caller to populate it.
    pub async fn checkout(
        &self,
        repo_url: &str,
        commit_sha: &str,
        init_submodules: bool,
    ) -> Result<(), RunnerError> {
        let ws_dir = self.workspace_dir();

        // Clone the repository
        let clone_output = tokio::process::Command::new("git")
            .args(["clone", repo_url, ws_dir.to_str().unwrap_or(".")])
            .output()
            .await
            .map_err(|e| RunnerError::Workspace(format!("Failed to spawn git clone: {}", e)))?;

        if !clone_output.status.success() {
            return Err(RunnerError::Workspace(format!(
                "git clone failed: {}",
                String::from_utf8_lossy(&clone_output.stderr)
            )));
        }

        // Checkout specific commit
        let checkout_output = tokio::process::Command::new("git")
            .args(["checkout", commit_sha])
            .current_dir(&ws_dir)
            .output()
            .await
            .map_err(|e| RunnerError::Workspace(format!("Failed to spawn git checkout: {}", e)))?;

        if !checkout_output.status.success() {
            return Err(RunnerError::Workspace(format!(
                "git checkout {} failed: {}",
                commit_sha,
                String::from_utf8_lossy(&checkout_output.stderr)
            )));
        }

        // Init submodules if requested
        if init_submodules {
            let submod_output = tokio::process::Command::new("git")
                .args(["submodule", "update", "--init", "--recursive"])
                .current_dir(&ws_dir)
                .output()
                .await
                .map_err(|e| RunnerError::Workspace(format!("Failed to init submodules: {}", e)))?;

            if !submod_output.status.success() {
                return Err(RunnerError::Workspace(format!(
                    "git submodule init failed: {}",
                    String::from_utf8_lossy(&submod_output.stderr)
                )));
            }
        }

        Ok(())
    }

    /// Clean up the workspace (remove temp directory).
    pub fn cleanup(&self) -> Result<(), RunnerError> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        // Best-effort cleanup
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_workspace() {
        let ws = Workspace::create("test-job-123").unwrap();
        assert!(ws.root().exists());
        assert!(ws.workspace_dir().exists());
        assert!(ws.secrets_dir().exists());
        assert!(ws.artifacts_dir().exists());
        // Cleanup happens on drop
    }

    #[test]
    fn create_at_custom_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("custom-workspace");
        let ws = Workspace::create_at(path.clone()).unwrap();
        assert!(path.exists());
        assert!(ws.workspace_dir().exists());
    }

    #[test]
    fn workspace_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cleanup-test");
        let ws = Workspace::create_at(path.clone()).unwrap();
        assert!(path.exists());
        ws.cleanup().unwrap();
        assert!(!path.exists());
    }
}

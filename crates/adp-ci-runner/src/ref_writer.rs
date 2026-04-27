//! Git ref write-back — persist job results and logs to git refs.
//!
//! The audit trail that survives SQLite resets:
//! - `refs/ci/jobs/{job_id}/request` — the original JobRequest
//! - `refs/ci/jobs/{job_id}/result` — the JobResult (on completion)
//! - `refs/ci/jobs/{job_id}/log-stdout` — accumulated stdout
//! - `refs/ci/jobs/{job_id}/log-stderr` — accumulated stderr
//!
//! These refs are in a private namespace (not advertised via wire protocol)
//! and serve as the durable record of CI activity.

use adp_ci_types::{JobRequest, JobResult, LogStream};
use std::path::PathBuf;
use tracing;

/// Error type for ref write operations.
#[derive(Debug, thiserror::Error)]
pub enum RefWriteError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Repository not found: {0}")]
    RepoNotFound(String),
}

/// Writes CI job data to git refs on the local filesystem.
///
/// In production, this would use gix to create git objects and update refs.
/// This implementation writes to a filesystem-based ref store for simplicity,
/// with a clear path to upgrading to real git ref writes.
pub struct RefWriter {
    /// Root path for ref storage (e.g. `_adp/system/refs/ci/jobs/`).
    ref_root: PathBuf,
}

impl RefWriter {
    /// Create a new ref writer targeting the given root path.
    pub fn new(ref_root: PathBuf) -> Self {
        Self { ref_root }
    }

    /// Get the ref path for a job.
    fn job_dir(&self, job_id: &str) -> PathBuf {
        self.ref_root.join("ci").join("jobs").join(job_id)
    }

    /// Write the original job request.
    pub fn write_request(&self, job_id: &str, request: &JobRequest) -> Result<(), RefWriteError> {
        let dir = self.job_dir(job_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("request");
        let json = serde_json::to_string_pretty(request)?;
        std::fs::write(&path, json)?;
        tracing::debug!(job_id, path = %path.display(), "Wrote job request ref");
        Ok(())
    }

    /// Write the job result.
    pub fn write_result(&self, job_id: &str, result: &JobResult) -> Result<(), RefWriteError> {
        let dir = self.job_dir(job_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("result");
        let json = serde_json::to_string_pretty(result)?;
        std::fs::write(&path, json)?;
        tracing::info!(job_id, path = %path.display(), status = ?result.status, "Wrote job result ref");
        Ok(())
    }

    /// Append log data to a stream ref.
    pub fn append_log(
        &self,
        job_id: &str,
        stream: LogStream,
        data: &[u8],
    ) -> Result<(), RefWriteError> {
        let dir = self.job_dir(job_id);
        std::fs::create_dir_all(&dir)?;
        let filename = match stream {
            LogStream::Stdout => "log-stdout",
            LogStream::Stderr => "log-stderr",
        };
        let path = dir.join(filename);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        std::io::Write::write_all(&mut file, data)?;
        tracing::trace!(job_id, stream = ?stream, bytes = data.len(), "Appended log data to ref");
        Ok(())
    }

    /// Read the job result from ref.
    pub fn read_result(&self, job_id: &str) -> Result<Option<JobResult>, RefWriteError> {
        let path = self.job_dir(job_id).join("result");
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(&path)?;
        let result: JobResult = serde_json::from_str(&json)?;
        Ok(Some(result))
    }

    /// Read log data from a stream ref.
    pub fn read_log(
        &self,
        job_id: &str,
        stream: LogStream,
    ) -> Result<Option<String>, RefWriteError> {
        let filename = match stream {
            LogStream::Stdout => "log-stdout",
            LogStream::Stderr => "log-stderr",
        };
        let path = self.job_dir(job_id).join(filename);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(Some(data))
    }

    /// Check if a job ref exists.
    pub fn job_exists(&self, job_id: &str) -> bool {
        self.job_dir(job_id).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adp_ci_types::{
        ArtifactStorage, ComplianceRequirements, JobMetrics, JobStatus, JobStep, JobTrigger,
        StepResult,
    };
    use std::collections::HashMap;

    fn make_request(job_id: &str) -> JobRequest {
        JobRequest {
            job_id: uuid::Uuid::parse_str(job_id).unwrap(),
            trigger: JobTrigger {
                event: "push".into(),
                branch: "main".into(),
                commit_sha: "abc".into(),
                committer: "test".into(),
                init_submodules: false,
            },
            steps: vec![JobStep {
                name: "build".into(),
                environment: "nix".into(),
                command: "nix build".into(),
                timeout_secs: 600,
                working_dir: None,
                env: HashMap::new(),
            }],
            secrets: Vec::new(),
            compliance: ComplianceRequirements::default(),
            artifacts: Vec::new(),
            artifact_storage: ArtifactStorage::default(),
        }
    }

    fn make_result(job_id: &str) -> JobResult {
        JobResult {
            job_id: uuid::Uuid::parse_str(job_id).unwrap(),
            status: JobStatus::Completed,
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            artifacts: Vec::new(),
            metrics: JobMetrics {
                duration_secs: 42,
                cpu_usage_percent: 75.0,
                memory_peak_bytes: 1024 * 1024 * 512,
                disk_usage_bytes: 1024 * 1024 * 100,
            },
            step_results: vec![StepResult {
                name: "build".into(),
                success: true,
                exit_code: 0,
                duration_secs: 42,
                error: None,
            }],
        }
    }

    #[test]
    fn write_and_read_request() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = RefWriter::new(tmp.path().to_path_buf());

        let request = make_request("550e8400-e29b-41d4-a716-446655440000");
        writer
            .write_request("550e8400-e29b-41d4-a716-446655440000", &request)
            .unwrap();

        let path = tmp.path().join("ci/jobs/550e8400-e29b-41d4-a716-446655440000/request");
        assert!(path.exists());
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains("nix build"));
    }

    #[test]
    fn write_and_read_result() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = RefWriter::new(tmp.path().to_path_buf());

        let result = make_result("550e8400-e29b-41d4-a716-446655440000");
        writer
            .write_result("550e8400-e29b-41d4-a716-446655440000", &result)
            .unwrap();

        let read = writer
            .read_result("550e8400-e29b-41d4-a716-446655440000")
            .unwrap()
            .unwrap();
        assert_eq!(read.status, JobStatus::Completed);
        assert_eq!(read.metrics.duration_secs, 42);
    }

    #[test]
    fn append_and_read_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = RefWriter::new(tmp.path().to_path_buf());

        writer
            .append_log(
                "550e8400-e29b-41d4-a716-446655440000",
                LogStream::Stdout,
                b"Compiling v0.1.0\n",
            )
            .unwrap();
        writer
            .append_log(
                "550e8400-e29b-41d4-a716-446655440000",
                LogStream::Stdout,
                b"Build complete\n",
            )
            .unwrap();
        writer
            .append_log(
                "550e8400-e29b-41d4-a716-446655440000",
                LogStream::Stderr,
                b"warning: unused import\n",
            )
            .unwrap();

        let stdout = writer
            .read_log("550e8400-e29b-41d4-a716-446655440000", LogStream::Stdout)
            .unwrap()
            .unwrap();
        assert!(stdout.contains("Compiling v0.1.0"));
        assert!(stdout.contains("Build complete"));

        let stderr = writer
            .read_log("550e8400-e29b-41d4-a716-446655440000", LogStream::Stderr)
            .unwrap()
            .unwrap();
        assert!(stderr.contains("warning: unused import"));
    }

    #[test]
    fn read_nonexistent_job() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = RefWriter::new(tmp.path().to_path_buf());

        assert!(writer
            .read_result("nonexistent")
            .unwrap()
            .is_none());
        assert!(writer
            .read_log("nonexistent", LogStream::Stdout)
            .unwrap()
            .is_none());
    }

    #[test]
    fn job_exists_check() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = RefWriter::new(tmp.path().to_path_buf());

        assert!(!writer.job_exists("550e8400-e29b-41d4-a716-446655440000"));
        writer
            .write_request(
                "550e8400-e29b-41d4-a716-446655440000",
                &make_request("550e8400-e29b-41d4-a716-446655440000"),
            )
            .unwrap();
        assert!(writer.job_exists("550e8400-e29b-41d4-a716-446655440000"));
    }
}

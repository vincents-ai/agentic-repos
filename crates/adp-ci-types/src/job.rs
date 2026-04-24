//! Job types — requests, results, status, steps.

use serde::{Deserialize, Serialize};

/// Job type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    Build,
    Test,
    SecurityScan,
    Deploy,
    Lint,
}

impl std::fmt::Display for JobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobType::Build => write!(f, "build"),
            JobType::Test => write!(f, "test"),
            JobType::SecurityScan => write!(f, "security_scan"),
            JobType::Deploy => write!(f, "deploy"),
            JobType::Lint => write!(f, "lint"),
        }
    }
}

/// Job priority (higher = more urgent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl Default for JobPriority {
    fn default() -> Self {
        JobPriority::Medium
    }
}

/// Job lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting for a runner to pick up.
    Pending,
    /// Router has matched a runner, waiting for acknowledgement.
    Assigned,
    /// Runner has started execution.
    Running,
    /// Job completed successfully.
    Completed,
    /// Job failed.
    Failed,
    /// Job was cancelled by user or system.
    Cancelled,
}

/// What triggered the job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobTrigger {
    /// Trigger event type.
    pub event: String,
    /// Branch ref (e.g. "refs/heads/main").
    pub branch: String,
    /// Commit SHA.
    pub commit_sha: String,
    /// Who triggered the job.
    pub committer: String,
    /// Whether to init submodules.
    #[serde(default)]
    pub init_submodules: bool,
}

/// A single step in a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStep {
    /// Step name.
    pub name: String,
    /// Build environment to use (nix, devbox, flux, bazel).
    pub environment: String,
    /// Command to execute.
    pub command: String,
    /// Timeout in seconds.
    #[serde(default = "default_step_timeout")]
    pub timeout_secs: u64,
    /// Working directory relative to workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Environment variables for this step.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub env: std::collections::HashMap<String, String>,
}

fn default_step_timeout() -> u64 {
    600 // 10 minutes
}

/// Compliance requirements for a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRequirements {
    /// Required data residency regions (e.g. ["eu"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_residency: Vec<String>,
    /// Whether SOC2 certification is required.
    #[serde(default)]
    pub requires_soc2: bool,
    /// Whether HIPAA compliance is required.
    #[serde(default)]
    pub requires_hipaa: bool,
}

impl Default for ComplianceRequirements {
    fn default() -> Self {
        Self {
            data_residency: Vec::new(),
            requires_soc2: false,
            requires_hipaa: false,
        }
    }
}

/// A job request — the full specification of work to be done.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    /// Unique job identifier.
    pub job_id: uuid::Uuid,
    /// What triggered this job.
    pub trigger: JobTrigger,
    /// Steps to execute in order.
    pub steps: Vec<JobStep>,
    /// Refs to decrypt and mount as secrets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
    /// Compliance requirements for runner matching.
    #[serde(default)]
    pub compliance: ComplianceRequirements,
    /// Expected artifacts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    /// Artifact storage backend.
    #[serde(default)]
    pub artifact_storage: super::artifact::ArtifactStorage,
}

/// Job result — written on completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    /// The job ID.
    pub job_id: uuid::Uuid,
    /// Final status.
    pub status: JobStatus,
    /// When the job started.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// When the job completed.
    pub completed_at: chrono::DateTime<chrono::Utc>,
    /// Produced artifacts.
    pub artifacts: Vec<super::artifact::ArtifactRef>,
    /// Execution metrics.
    pub metrics: JobMetrics,
    /// Which steps succeeded/failed.
    pub step_results: Vec<StepResult>,
}

/// Result of a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step name.
    pub name: String,
    /// Whether the step succeeded.
    pub success: bool,
    /// Exit code.
    pub exit_code: i32,
    /// Duration in seconds.
    pub duration_secs: u64,
    /// Error message if failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execution metrics for a completed job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobMetrics {
    /// Total wall-clock duration in seconds.
    pub duration_secs: u64,
    /// Average CPU usage percentage.
    pub cpu_usage_percent: f64,
    /// Peak memory usage in bytes.
    pub memory_peak_bytes: u64,
    /// Disk usage in bytes.
    pub disk_usage_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_type_display() {
        assert_eq!(JobType::Build.to_string(), "build");
        assert_eq!(JobType::SecurityScan.to_string(), "security_scan");
    }

    #[test]
    fn job_priority_ordering() {
        assert!(JobPriority::Critical > JobPriority::High);
        assert!(JobPriority::High > JobPriority::Medium);
        assert!(JobPriority::Medium > JobPriority::Low);
    }

    #[test]
    fn job_request_round_trip() {
        let request = JobRequest {
            job_id: uuid::Uuid::new_v4(),
            trigger: JobTrigger {
                event: "push".into(),
                branch: "refs/heads/main".into(),
                commit_sha: "abc123".into(),
                committer: "alice".into(),
                init_submodules: true,
            },
            steps: vec![JobStep {
                name: "build".into(),
                environment: "nix".into(),
                command: "nix build".into(),
                timeout_secs: 3600,
                working_dir: None,
                env: std::collections::HashMap::new(),
            }],
            secrets: vec!["refs/secrets/ci/deploy-key".into()],
            compliance: ComplianceRequirements {
                data_residency: vec!["eu".into()],
                requires_soc2: true,
                requires_hipaa: false,
            },
            artifacts: vec!["result/bin/adp".into()],
            artifact_storage: super::super::artifact::ArtifactStorage::GitLfs,
        };

        let json = serde_json::to_string(&request).unwrap();
        let decoded: JobRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.job_id, request.job_id);
        assert_eq!(decoded.trigger.init_submodules, true);
        assert_eq!(decoded.compliance.requires_soc2, true);
    }

    #[test]
    fn job_status_serialization() {
        let statuses = [
            JobStatus::Pending,
            JobStatus::Assigned,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ];
        for s in &statuses {
            let json = serde_json::to_string(&s).unwrap();
            let back: JobStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, back);
        }
    }
}

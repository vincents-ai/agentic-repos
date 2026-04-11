//! Shared CI/CD models — build statuses, pipeline events, provider-agnostic types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The outcome of a CI/CD pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BuildStatus {
    Pending,
    Running,
    Success,
    Failure,
    Cancelled,
    Skipped,
    Unknown,
}

impl std::fmt::Display for BuildStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BuildStatus::Pending => "pending",
            BuildStatus::Running => "running",
            BuildStatus::Success => "success",
            BuildStatus::Failure => "failure",
            BuildStatus::Cancelled => "cancelled",
            BuildStatus::Skipped => "skipped",
            BuildStatus::Unknown => "unknown",
        };
        write!(f, "{}", s)
    }
}

/// A CI/CD build / pipeline status report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildReport {
    /// Unique report ID (generated server-side).
    pub id: Uuid,
    /// Git SHA the build ran against.
    pub commit_sha: String,
    /// Branch name (without `refs/heads/` prefix).
    pub branch: String,
    /// Repository identifier (tenant/repo).
    pub repository_id: String,
    /// CI/CD provider that submitted this report.
    pub provider: CiProvider,
    /// Provider-specific pipeline / run ID.
    pub pipeline_id: String,
    /// URL to the pipeline run in the provider UI.
    pub pipeline_url: Option<String>,
    /// Current build status.
    pub status: BuildStatus,
    /// Human-readable description of the status.
    pub description: Option<String>,
    /// When the build started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the build finished.
    pub finished_at: Option<DateTime<Utc>>,
    /// When this record was created.
    pub created_at: DateTime<Utc>,
    /// When this record was last updated.
    pub updated_at: DateTime<Utc>,
}

impl BuildReport {
    pub fn new(
        commit_sha: String,
        branch: String,
        repository_id: String,
        provider: CiProvider,
        pipeline_id: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            commit_sha,
            branch,
            repository_id,
            provider,
            pipeline_id,
            pipeline_url: None,
            status: BuildStatus::Pending,
            description: None,
            started_at: None,
            finished_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_status(mut self, status: BuildStatus) -> Self {
        self.status = status;
        self.updated_at = Utc::now();
        self
    }

    pub fn with_url(mut self, url: String) -> Self {
        self.pipeline_url = Some(url);
        self
    }

    pub fn duration_ms(&self) -> Option<i64> {
        match (self.started_at, self.finished_at) {
            (Some(s), Some(f)) => Some((f - s).num_milliseconds()),
            _ => None,
        }
    }
}

/// Supported CI/CD providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CiProvider {
    GithubActions,
    GitlabCi,
    Jenkins,
    Custom(String),
}

impl std::fmt::Display for CiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CiProvider::GithubActions => write!(f, "github_actions"),
            CiProvider::GitlabCi => write!(f, "gitlab_ci"),
            CiProvider::Jenkins => write!(f, "jenkins"),
            CiProvider::Custom(s) => write!(f, "custom:{}", s),
        }
    }
}

/// Incoming webhook payload from a CI/CD provider.
/// This is a provider-agnostic wrapper produced after parsing the raw payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiWebhookEvent {
    pub provider: CiProvider,
    pub event_type: CiEventType,
    pub repository: String,
    pub commit_sha: String,
    pub branch: Option<String>,
    pub pipeline_id: String,
    pub status: BuildStatus,
    pub pipeline_url: Option<String>,
    pub raw_payload: serde_json::Value,
}

/// High-level CI/CD event categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CiEventType {
    /// A pipeline started.
    PipelineStart,
    /// A pipeline completed (success, failure, cancelled).
    PipelineComplete,
    /// A job within a pipeline completed.
    JobComplete,
    /// A deployment was triggered.
    DeploymentCreated,
    /// A deployment completed.
    DeploymentComplete,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_report_new() {
        let r = BuildReport::new(
            "abc123".to_string(),
            "main".to_string(),
            "org/repo".to_string(),
            CiProvider::GithubActions,
            "run-42".to_string(),
        );
        assert_eq!(r.status, BuildStatus::Pending);
        assert!(r.pipeline_url.is_none());
        assert!(r.duration_ms().is_none());
    }

    #[test]
    fn test_build_report_with_status() {
        let r = BuildReport::new(
            "abc".to_string(),
            "main".to_string(),
            "org/repo".to_string(),
            CiProvider::GitlabCi,
            "pipe-1".to_string(),
        )
        .with_status(BuildStatus::Success);
        assert_eq!(r.status, BuildStatus::Success);
    }

    #[test]
    fn test_build_report_with_url() {
        let r = BuildReport::new(
            "sha".to_string(),
            "main".to_string(),
            "org/repo".to_string(),
            CiProvider::Jenkins,
            "build-99".to_string(),
        )
        .with_url("https://ci.example.com/build/99".to_string());
        assert_eq!(
            r.pipeline_url.as_deref(),
            Some("https://ci.example.com/build/99")
        );
    }

    #[test]
    fn test_build_status_display() {
        assert_eq!(BuildStatus::Success.to_string(), "success");
        assert_eq!(BuildStatus::Failure.to_string(), "failure");
        assert_eq!(BuildStatus::Pending.to_string(), "pending");
    }

    #[test]
    fn test_ci_provider_display() {
        assert_eq!(CiProvider::GithubActions.to_string(), "github_actions");
        assert_eq!(CiProvider::GitlabCi.to_string(), "gitlab_ci");
        assert_eq!(CiProvider::Jenkins.to_string(), "jenkins");
        assert_eq!(
            CiProvider::Custom("drone".to_string()).to_string(),
            "custom:drone"
        );
    }
}

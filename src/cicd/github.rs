//! GitHub Actions webhook payload parsing and event translation.
//!
//! Handles `workflow_run`, `check_run`, and `deployment_status` events
//! sent from GitHub to the ADP webhook endpoint.

use crate::cicd::models::{BuildStatus, CiEventType, CiProvider, CiWebhookEvent};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GithubParseError {
    #[error("Unknown GitHub event type: {0}")]
    UnknownEvent(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Subset of a GitHub `workflow_run` payload we care about.
#[derive(Debug, Deserialize, Serialize)]
pub struct GithubWorkflowRunPayload {
    pub action: String,
    pub workflow_run: GithubWorkflowRun,
    pub repository: GithubRepository,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubWorkflowRun {
    pub id: u64,
    pub name: Option<String>,
    pub head_sha: String,
    pub head_branch: Option<String>,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubRepository {
    pub full_name: String,
}

/// Subset of a GitHub `check_run` payload.
#[derive(Debug, Deserialize, Serialize)]
pub struct GithubCheckRunPayload {
    pub action: String,
    pub check_run: GithubCheckRun,
    pub repository: GithubRepository,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubCheckRun {
    pub id: u64,
    pub head_sha: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
    pub check_suite: Option<GithubCheckSuite>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubCheckSuite {
    pub head_branch: Option<String>,
}

/// Parse a raw GitHub webhook payload identified by its `X-GitHub-Event` header.
pub fn parse_github_event(
    event_type: &str,
    payload: serde_json::Value,
) -> Result<CiWebhookEvent, GithubParseError> {
    match event_type {
        "workflow_run" => parse_workflow_run(payload),
        "check_run" => parse_check_run(payload),
        _ => Err(GithubParseError::UnknownEvent(event_type.to_string())),
    }
}

fn parse_workflow_run(payload: serde_json::Value) -> Result<CiWebhookEvent, GithubParseError> {
    let p: GithubWorkflowRunPayload = serde_json::from_value(payload.clone())?;

    let status = github_conclusion_to_status(&p.workflow_run.status, &p.workflow_run.conclusion);

    let event_type = if p.workflow_run.status == "completed" {
        CiEventType::PipelineComplete
    } else {
        CiEventType::PipelineStart
    };

    Ok(CiWebhookEvent {
        provider: CiProvider::GithubActions,
        event_type,
        repository: p.repository.full_name,
        commit_sha: p.workflow_run.head_sha,
        branch: p.workflow_run.head_branch,
        pipeline_id: p.workflow_run.id.to_string(),
        status,
        pipeline_url: p.workflow_run.html_url,
        raw_payload: payload,
    })
}

fn parse_check_run(payload: serde_json::Value) -> Result<CiWebhookEvent, GithubParseError> {
    let p: GithubCheckRunPayload = serde_json::from_value(payload.clone())?;

    let status = github_conclusion_to_status(&p.check_run.status, &p.check_run.conclusion);

    let branch = p
        .check_run
        .check_suite
        .as_ref()
        .and_then(|s| s.head_branch.clone());

    Ok(CiWebhookEvent {
        provider: CiProvider::GithubActions,
        event_type: CiEventType::JobComplete,
        repository: p.repository.full_name,
        commit_sha: p.check_run.head_sha,
        branch,
        pipeline_id: p.check_run.id.to_string(),
        status,
        pipeline_url: p.check_run.html_url,
        raw_payload: payload,
    })
}

/// Map GitHub's `status` + `conclusion` fields to a [`BuildStatus`].
fn github_conclusion_to_status(status: &str, conclusion: &Option<String>) -> BuildStatus {
    match status {
        "queued" | "waiting" => BuildStatus::Pending,
        "in_progress" => BuildStatus::Running,
        "completed" => match conclusion.as_deref() {
            Some("success") => BuildStatus::Success,
            Some("failure") | Some("timed_out") => BuildStatus::Failure,
            Some("cancelled") => BuildStatus::Cancelled,
            Some("skipped") | Some("neutral") => BuildStatus::Skipped,
            _ => BuildStatus::Unknown,
        },
        _ => BuildStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow_run_payload(status: &str, conclusion: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "action": if status == "completed" { "completed" } else { "requested" },
            "workflow_run": {
                "id": 12345678,
                "name": "CI",
                "head_sha": "abc123def456",
                "head_branch": "main",
                "status": status,
                "conclusion": conclusion,
                "html_url": "https://github.com/org/repo/actions/runs/12345678"
            },
            "repository": {
                "full_name": "org/repo"
            }
        })
    }

    #[test]
    fn test_parse_workflow_run_success() {
        let payload = workflow_run_payload("completed", Some("success"));
        let event = parse_github_event("workflow_run", payload).unwrap();
        assert_eq!(event.provider, CiProvider::GithubActions);
        assert_eq!(event.status, BuildStatus::Success);
        assert_eq!(event.commit_sha, "abc123def456");
        assert_eq!(event.branch.as_deref(), Some("main"));
        assert_eq!(event.event_type, CiEventType::PipelineComplete);
    }

    #[test]
    fn test_parse_workflow_run_failure() {
        let payload = workflow_run_payload("completed", Some("failure"));
        let event = parse_github_event("workflow_run", payload).unwrap();
        assert_eq!(event.status, BuildStatus::Failure);
    }

    #[test]
    fn test_parse_workflow_run_in_progress() {
        let payload = workflow_run_payload("in_progress", None);
        let event = parse_github_event("workflow_run", payload).unwrap();
        assert_eq!(event.status, BuildStatus::Running);
        assert_eq!(event.event_type, CiEventType::PipelineStart);
    }

    #[test]
    fn test_parse_check_run() {
        let payload = serde_json::json!({
            "action": "completed",
            "check_run": {
                "id": 99,
                "head_sha": "deadbeef",
                "status": "completed",
                "conclusion": "success",
                "html_url": "https://github.com/checks/99",
                "check_suite": { "head_branch": "feature/foo" }
            },
            "repository": { "full_name": "org/my-repo" }
        });
        let event = parse_github_event("check_run", payload).unwrap();
        assert_eq!(event.status, BuildStatus::Success);
        assert_eq!(event.event_type, CiEventType::JobComplete);
        assert_eq!(event.branch.as_deref(), Some("feature/foo"));
    }

    #[test]
    fn test_unknown_event_type_returns_error() {
        let result = parse_github_event("pull_request", serde_json::json!({}));
        assert!(matches!(result, Err(GithubParseError::UnknownEvent(_))));
    }

    #[test]
    fn test_github_conclusion_to_status() {
        assert_eq!(
            github_conclusion_to_status("completed", &Some("success".to_string())),
            BuildStatus::Success
        );
        assert_eq!(
            github_conclusion_to_status("completed", &Some("cancelled".to_string())),
            BuildStatus::Cancelled
        );
        assert_eq!(
            github_conclusion_to_status("in_progress", &None),
            BuildStatus::Running
        );
        assert_eq!(
            github_conclusion_to_status("queued", &None),
            BuildStatus::Pending
        );
    }
}

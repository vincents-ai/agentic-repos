//! GitLab CI webhook payload parsing.
//!
//! Handles `Pipeline Hook` and `Job Hook` events from GitLab.

use crate::cicd::models::{BuildStatus, CiEventType, CiProvider, CiWebhookEvent};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitlabParseError {
    #[error("Unknown GitLab event type: {0}")]
    UnknownEvent(String),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitlabPipelinePayload {
    pub object_kind: String,
    pub object_attributes: GitlabPipelineAttributes,
    pub project: GitlabProject,
    pub commit: Option<GitlabCommit>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitlabPipelineAttributes {
    pub id: u64,
    pub status: String,
    pub ref_: Option<String>,
    pub sha: String,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitlabJobPayload {
    pub object_kind: String,
    pub build_id: u64,
    pub build_status: String,
    pub sha: String,
    pub ref_: Option<String>,
    pub project_id: u64,
    pub repository: GitlabJobRepository,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitlabJobRepository {
    pub name: String,
    pub homepage: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitlabProject {
    pub path_with_namespace: String,
    pub web_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitlabCommit {
    pub id: String,
}

/// Parse a raw GitLab webhook payload.
/// `object_kind` is read from the payload itself (GitLab always includes it).
pub fn parse_gitlab_event(payload: serde_json::Value) -> Result<CiWebhookEvent, GitlabParseError> {
    let kind = payload
        .get("object_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match kind.as_str() {
        "pipeline" => parse_pipeline(payload),
        "build" => parse_job(payload),
        _ => Err(GitlabParseError::UnknownEvent(kind)),
    }
}

fn parse_pipeline(payload: serde_json::Value) -> Result<CiWebhookEvent, GitlabParseError> {
    // GitLab uses `ref` but we need to rename for Rust keyword
    let mut v = payload.clone();
    if let Some(obj) = v
        .get_mut("object_attributes")
        .and_then(|o| o.as_object_mut())
    {
        if let Some(r) = obj.remove("ref") {
            obj.insert("ref_".to_string(), r);
        }
        // rename url field (present in some versions)
    }
    if let Some(obj) = v.get_mut("project").and_then(|o| o.as_object_mut()) {
        if let Some(u) = obj.remove("web_url") {
            obj.insert("web_url".to_string(), u);
        }
    }

    let p: GitlabPipelinePayload = serde_json::from_value(v)?;
    let status = gitlab_status_to_build(&p.object_attributes.status);

    let event_type = match p.object_attributes.status.as_str() {
        "running" | "waiting_for_resource" | "preparing" | "pending" => CiEventType::PipelineStart,
        _ => CiEventType::PipelineComplete,
    };

    let pipeline_url = p.object_attributes.url.or(p.project.web_url);

    Ok(CiWebhookEvent {
        provider: CiProvider::GitlabCi,
        event_type,
        repository: p.project.path_with_namespace,
        commit_sha: p.object_attributes.sha,
        branch: p.object_attributes.ref_,
        pipeline_id: p.object_attributes.id.to_string(),
        status,
        pipeline_url,
        raw_payload: payload,
    })
}

fn parse_job(payload: serde_json::Value) -> Result<CiWebhookEvent, GitlabParseError> {
    let mut v = payload.clone();
    if let Some(obj) = v.as_object_mut() {
        if let Some(r) = obj.remove("ref") {
            obj.insert("ref_".to_string(), r);
        }
    }

    let p: GitlabJobPayload = serde_json::from_value(v)?;
    let status = gitlab_status_to_build(&p.build_status);

    Ok(CiWebhookEvent {
        provider: CiProvider::GitlabCi,
        event_type: CiEventType::JobComplete,
        repository: p.repository.name,
        commit_sha: p.sha,
        branch: p.ref_,
        pipeline_id: p.build_id.to_string(),
        status,
        pipeline_url: None,
        raw_payload: payload,
    })
}

fn gitlab_status_to_build(status: &str) -> BuildStatus {
    match status {
        "created" | "waiting_for_resource" | "preparing" | "pending" => BuildStatus::Pending,
        "running" => BuildStatus::Running,
        "success" => BuildStatus::Success,
        "failed" => BuildStatus::Failure,
        "canceled" | "cancelled" => BuildStatus::Cancelled,
        "skipped" | "manual" => BuildStatus::Skipped,
        _ => BuildStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline_payload(status: &str) -> serde_json::Value {
        serde_json::json!({
            "object_kind": "pipeline",
            "object_attributes": {
                "id": 456,
                "status": status,
                "ref": "main",
                "sha": "cafe1234",
                "url": "https://gitlab.com/org/repo/-/pipelines/456"
            },
            "project": {
                "path_with_namespace": "org/repo",
                "web_url": "https://gitlab.com/org/repo"
            },
            "commit": { "id": "cafe1234" }
        })
    }

    #[test]
    fn test_parse_pipeline_success() {
        let event = parse_gitlab_event(pipeline_payload("success")).unwrap();
        assert_eq!(event.provider, CiProvider::GitlabCi);
        assert_eq!(event.status, BuildStatus::Success);
        assert_eq!(event.commit_sha, "cafe1234");
        assert_eq!(event.event_type, CiEventType::PipelineComplete);
    }

    #[test]
    fn test_parse_pipeline_running() {
        let event = parse_gitlab_event(pipeline_payload("running")).unwrap();
        assert_eq!(event.status, BuildStatus::Running);
        assert_eq!(event.event_type, CiEventType::PipelineStart);
    }

    #[test]
    fn test_parse_pipeline_failed() {
        let event = parse_gitlab_event(pipeline_payload("failed")).unwrap();
        assert_eq!(event.status, BuildStatus::Failure);
    }

    #[test]
    fn test_parse_pipeline_cancelled() {
        let event = parse_gitlab_event(pipeline_payload("canceled")).unwrap();
        assert_eq!(event.status, BuildStatus::Cancelled);
    }

    #[test]
    fn test_parse_job() {
        let payload = serde_json::json!({
            "object_kind": "build",
            "build_id": 789,
            "build_status": "success",
            "sha": "abcdef",
            "ref": "feature/x",
            "project_id": 1,
            "repository": {
                "name": "my-repo",
                "homepage": "https://gitlab.com/org/my-repo"
            }
        });
        let event = parse_gitlab_event(payload).unwrap();
        assert_eq!(event.event_type, CiEventType::JobComplete);
        assert_eq!(event.status, BuildStatus::Success);
        assert_eq!(event.branch.as_deref(), Some("feature/x"));
    }

    #[test]
    fn test_unknown_object_kind() {
        let payload = serde_json::json!({ "object_kind": "note" });
        let result = parse_gitlab_event(payload);
        assert!(matches!(result, Err(GitlabParseError::UnknownEvent(_))));
    }
}

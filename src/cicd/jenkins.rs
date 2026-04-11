//! Jenkins webhook payload parsing.
//!
//! Handles build notifications from the Jenkins Notification Plugin
//! (https://plugins.jenkins.io/notification/) which sends JSON POST payloads.

use crate::cicd::models::{BuildStatus, CiEventType, CiProvider, CiWebhookEvent};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JenkinsParseError {
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

/// Jenkins Notification Plugin payload structure.
#[derive(Debug, Deserialize, Serialize)]
pub struct JenkinsPayload {
    pub name: String,
    pub url: Option<String>,
    pub build: JenkinsBuild,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct JenkinsBuild {
    pub number: u64,
    pub phase: String,
    pub status: Option<String>,
    pub url: Option<String>,
    pub scm: Option<JenkinsScm>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct JenkinsScm {
    pub url: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

/// Parse a Jenkins Notification Plugin payload.
pub fn parse_jenkins_event(
    payload: serde_json::Value,
) -> Result<CiWebhookEvent, JenkinsParseError> {
    let p: JenkinsPayload = serde_json::from_value(payload.clone())?;

    let commit_sha = p
        .build
        .scm
        .as_ref()
        .and_then(|s| s.commit.clone())
        .unwrap_or_else(|| p.build.number.to_string());

    let branch = p
        .build
        .scm
        .as_ref()
        .and_then(|s| s.branch.clone())
        .map(|b| {
            // Jenkins often sends "origin/main" — strip the remote prefix
            b.strip_prefix("origin/")
                .or_else(|| b.strip_prefix("refs/heads/"))
                .unwrap_or(&b)
                .to_string()
        });

    let status = jenkins_phase_to_status(&p.build.phase, &p.build.status);

    let event_type = match p.build.phase.as_str() {
        "STARTED" => CiEventType::PipelineStart,
        _ => CiEventType::PipelineComplete,
    };

    let pipeline_url = p.build.url.or(p.url);
    // Use job name + build number as pipeline ID
    let pipeline_id = format!("{}/{}", p.name, p.build.number);

    // Derive a repository ID from the SCM URL if available, else use job name
    let repository = p
        .build
        .scm
        .as_ref()
        .and_then(|s| s.url.as_ref())
        .map(|u| scm_url_to_repo(u))
        .unwrap_or_else(|| p.name.clone());

    Ok(CiWebhookEvent {
        provider: CiProvider::Jenkins,
        event_type,
        repository,
        commit_sha,
        branch,
        pipeline_id,
        status,
        pipeline_url,
        raw_payload: payload,
    })
}

/// Map Jenkins `phase` + `status` to a [`BuildStatus`].
fn jenkins_phase_to_status(phase: &str, status: &Option<String>) -> BuildStatus {
    match phase {
        "STARTED" => BuildStatus::Running,
        "QUEUED" => BuildStatus::Pending,
        "COMPLETED" | "FINALIZED" => match status.as_deref() {
            Some("SUCCESS") => BuildStatus::Success,
            Some("FAILURE") | Some("UNSTABLE") => BuildStatus::Failure,
            Some("ABORTED") => BuildStatus::Cancelled,
            Some("NOT_BUILT") => BuildStatus::Skipped,
            _ => BuildStatus::Unknown,
        },
        _ => BuildStatus::Unknown,
    }
}

/// Heuristically extract `org/repo` from a git remote URL.
fn scm_url_to_repo(url: &str) -> String {
    // Handles: https://github.com/org/repo.git  git@github.com:org/repo.git
    let stripped = url.trim_end_matches(".git").trim_end_matches('/');

    // SSH style: git@host:org/repo  (contains `:` but not `://`)
    if !stripped.contains("://") {
        if let Some((_host, path)) = stripped.split_once(':') {
            return path.to_string();
        }
    }

    // HTTP(S) style: take last two path segments after the authority
    let path_part = stripped.split_once("://").map(|(_, p)| p).unwrap_or(stripped);
    // Skip the host — split on `/` and take last two non-empty parts
    let parts: Vec<&str> = path_part.split('/').filter(|s| !s.is_empty()).collect();
    let n = parts.len();
    if n >= 2 {
        format!("{}/{}", parts[n - 2], parts[n - 1])
    } else {
        stripped.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_payload(phase: &str, status: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "name": "my-pipeline",
            "url": "https://jenkins.example.com/job/my-pipeline/",
            "build": {
                "number": 42,
                "phase": phase,
                "status": status,
                "url": "https://jenkins.example.com/job/my-pipeline/42/",
                "scm": {
                    "url": "https://github.com/org/my-repo.git",
                    "branch": "origin/main",
                    "commit": "feedcafe"
                }
            }
        })
    }

    #[test]
    fn test_parse_started() {
        let event = parse_jenkins_event(make_payload("STARTED", None)).unwrap();
        assert_eq!(event.provider, CiProvider::Jenkins);
        assert_eq!(event.status, BuildStatus::Running);
        assert_eq!(event.event_type, CiEventType::PipelineStart);
        assert_eq!(event.commit_sha, "feedcafe");
        assert_eq!(event.branch.as_deref(), Some("main")); // origin/ stripped
    }

    #[test]
    fn test_parse_completed_success() {
        let event = parse_jenkins_event(make_payload("COMPLETED", Some("SUCCESS"))).unwrap();
        assert_eq!(event.status, BuildStatus::Success);
        assert_eq!(event.event_type, CiEventType::PipelineComplete);
    }

    #[test]
    fn test_parse_completed_failure() {
        let event = parse_jenkins_event(make_payload("COMPLETED", Some("FAILURE"))).unwrap();
        assert_eq!(event.status, BuildStatus::Failure);
    }

    #[test]
    fn test_parse_completed_aborted() {
        let event = parse_jenkins_event(make_payload("FINALIZED", Some("ABORTED"))).unwrap();
        assert_eq!(event.status, BuildStatus::Cancelled);
    }

    #[test]
    fn test_pipeline_id_format() {
        let event = parse_jenkins_event(make_payload("STARTED", None)).unwrap();
        assert_eq!(event.pipeline_id, "my-pipeline/42");
    }

    #[test]
    fn test_repository_extracted_from_scm_url() {
        let event = parse_jenkins_event(make_payload("STARTED", None)).unwrap();
        assert_eq!(event.repository, "org/my-repo");
    }

    #[test]
    fn test_scm_url_to_repo_https() {
        assert_eq!(
            scm_url_to_repo("https://github.com/org/repo.git"),
            "org/repo"
        );
        assert_eq!(
            scm_url_to_repo("https://gitlab.com/group/project"),
            "group/project"
        );
    }

    #[test]
    fn test_scm_url_to_repo_ssh() {
        assert_eq!(scm_url_to_repo("git@github.com:org/repo.git"), "org/repo");
    }

    #[test]
    fn test_no_scm_falls_back_to_job_name() {
        let payload = serde_json::json!({
            "name": "fallback-job",
            "build": {
                "number": 1,
                "phase": "STARTED",
                "status": null,
                "url": null,
                "scm": null
            }
        });
        let event = parse_jenkins_event(payload).unwrap();
        assert_eq!(event.repository, "fallback-job");
    }
}

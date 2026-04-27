//! CI events — SSE events between adp and runners.

use serde::{Deserialize, Serialize};

/// Events pushed from adp to CI runners via SSE.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum CiEvent {
    /// A new job is available matching this runner's capabilities.
    #[serde(rename = "job_available")]
    JobAvailable {
        job_id: String,
        repo_id: String,
        branch: String,
        commit_sha: String,
        job_type: super::job::JobType,
        priority: super::job::JobPriority,
    },

    /// A job has been assigned to this specific runner.
    #[serde(rename = "job_assigned")]
    JobAssigned {
        job_id: String,
        deadline: String,
        secret_refs: Vec<String>,
    },

    /// A running job has been cancelled.
    #[serde(rename = "job_cancelled")]
    JobCancelled {
        job_id: String,
        reason: String,
    },

    /// Runner capabilities have been updated by an admin.
    #[serde(rename = "capabilities_updated")]
    CapabilitiesUpdated {
        runner_id: String,
    },

    /// Keepalive ping.
    #[serde(rename = "ci_ping")]
    Ping { timestamp: String },
}

impl CiEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            CiEvent::JobAvailable { .. } => "job_available",
            CiEvent::JobAssigned { .. } => "job_assigned",
            CiEvent::JobCancelled { .. } => "job_cancelled",
            CiEvent::CapabilitiesUpdated { .. } => "capabilities_updated",
            CiEvent::Ping { .. } => "ci_ping",
        }
    }
}

/// Events sent from runners to adp via REST.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RunnerEvent {
    /// Runner acknowledges a job and starts execution.
    #[serde(rename = "job_started")]
    JobStarted {
        job_id: String,
        started_at: String,
    },

    /// Job completed successfully.
    #[serde(rename = "job_completed")]
    JobCompleted {
        job_id: String,
        completed_at: String,
        artifacts: Vec<super::artifact::ArtifactRef>,
        metrics: super::job::JobMetrics,
    },

    /// Job failed.
    #[serde(rename = "job_failed")]
    JobFailed {
        job_id: String,
        failed_at: String,
        phase: String,
        error_message: String,
        exit_code: i32,
    },

    /// Runner periodic status update.
    #[serde(rename = "runner_status")]
    RunnerStatus {
        runner_id: String,
        status: super::runner::RunnerStatus,
        current_job: Option<String>,
        available_slots: u32,
    },

    /// Log chunk streamed during execution.
    #[serde(rename = "log_chunk")]
    LogChunk {
        job_id: String,
        stream: LogStream,
        data: String,
        offset: u64,
    },
}

/// Log stream type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_event_round_trip() {
        let events = vec![
            CiEvent::JobAvailable {
                job_id: "j-1".into(),
                repo_id: "repo".into(),
                branch: "main".into(),
                commit_sha: "abc".into(),
                job_type: super::super::job::JobType::Build,
                priority: super::super::job::JobPriority::High,
            },
            CiEvent::JobAssigned {
                job_id: "j-1".into(),
                deadline: "2026-04-24T01:00:00Z".into(),
                secret_refs: vec!["refs/secrets/ci/deploy-key".into()],
            },
            CiEvent::JobCancelled {
                job_id: "j-1".into(),
                reason: "user cancelled".into(),
            },
            CiEvent::Ping { timestamp: "t".into() },
        ];

        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let decoded: CiEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event.event_type(), decoded.event_type());
        }
    }

    #[test]
    fn runner_event_round_trip() {
        let event = RunnerEvent::LogChunk {
            job_id: "j-1".into(),
            stream: LogStream::Stdout,
            data: "SGVsbG8=".into(), // base64 "Hello"
            offset: 0,
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: RunnerEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, RunnerEvent::LogChunk { .. }));
    }
}

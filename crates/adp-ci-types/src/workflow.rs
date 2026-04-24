//! CI Workflow types — stored as JSON at `refs/ci/workflows/{name}`.
//!
//! Workflows follow the same pattern as all ADP data: JSON blobs in git refs,
//! validated by the schema registry against the `ci-workflow` schema.
//!
//! ## Schema location
//!
//! Schema definition: `refs/adp/schema/ci-workflow`
//! Workflow data: `refs/ci/workflows/{workflow-name}`
//!
//! ## Example JSON
//!
//! ```json
//! {
//!   "on": {
//!     "push": { "branches": ["main", "develop"] },
//!     "pull_request": { "branches": ["main"], "types": ["opened", "synchronize"] }
//!   },
//!   "env": { "CARGO_TERM_COLOR": "always" },
//!   "jobs": {
//!     "build": {
//!       "environment": "nix",
//!       "steps": [
//!         { "name": "Build", "command": "nix build", "timeout_secs": 3600 }
//!       ],
//!       "artifacts": ["result/bin/adp"],
//!       "secrets": ["refs/secrets/ci/deploy-key"]
//!     },
//!     "test": {
//!       "environment": "nix",
//!       "needs": ["build"],
//!       "steps": [
//!         { "name": "Test", "command": "cargo test", "timeout_secs": 1800 }
//!       ]
//!     }
//!   }
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A CI workflow definition — stored as JSON at `refs/ci/workflows/{name}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiWorkflow {
    /// Trigger configuration — what events cause this workflow to run.
    #[serde(default)]
    pub on: TriggerConfig,

    /// Global environment variables applied to all jobs.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Job definitions.
    pub jobs: HashMap<String, WorkflowJob>,
}

/// Trigger configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Push trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push: Option<PushTrigger>,

    /// Pull request trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequestTrigger>,
}

/// Push trigger — match on branch/tag patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushTrigger {
    /// Branch glob patterns (e.g. ["main", "release/*"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,

    /// Tag glob patterns (e.g. ["v*"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Pull request trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestTrigger {
    /// Branch glob patterns for the target branch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,

    /// PR action types: opened, synchronize, closed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
}

/// A single job within a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowJob {
    /// Build environment (nix, devbox, flux, bazel).
    pub environment: String,

    /// Steps to execute in order.
    pub steps: Vec<WorkflowStep>,

    /// Jobs that must complete before this one (DAG dependency).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,

    /// Artifact paths to collect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,

    /// Secret refs to mount (K8s pattern).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,

    /// Compliance requirements (e.g. ["soc2"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compliance: Vec<String>,

    /// Job-level env vars (override global).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

/// A single step within a workflow job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step name.
    pub name: String,

    /// Command to execute.
    pub command: String,

    /// Working directory relative to workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Timeout in seconds (default: 600).
    #[serde(default = "default_step_timeout")]
    pub timeout_secs: u64,

    /// Step-level env vars.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

fn default_step_timeout() -> u64 {
    600
}

/// Trigger event — used to match against a workflow's TriggerConfig.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerEvent {
    Push { branch: String, tag: Option<String> },
    PullRequest { branch: String, action: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_round_trip() {
        let workflow = CiWorkflow {
            on: TriggerConfig {
                push: Some(PushTrigger {
                    branches: vec!["main".into(), "develop".into()],
                    tags: vec!["v*".into()],
                }),
                pull_request: Some(PullRequestTrigger {
                    branches: vec!["main".into()],
                    types: vec!["opened".into(), "synchronize".into()],
                }),
            },
            env: {
                let mut m = HashMap::new();
                m.insert("CARGO_TERM_COLOR".into(), "always".into());
                m
            },
            jobs: {
                let mut m = HashMap::new();
                m.insert(
                    "build".into(),
                    WorkflowJob {
                        environment: "nix".into(),
                        steps: vec![WorkflowStep {
                            name: "Build".into(),
                            command: "nix build".into(),
                            working_dir: None,
                            timeout_secs: 3600,
                            env: HashMap::new(),
                        }],
                        needs: Vec::new(),
                        artifacts: vec!["result/bin/adp".into()],
                        secrets: vec!["refs/secrets/ci/deploy-key".into()],
                        compliance: Vec::new(),
                        env: HashMap::new(),
                    },
                );
                m.insert(
                    "test".into(),
                    WorkflowJob {
                        environment: "nix".into(),
                        steps: vec![WorkflowStep {
                            name: "Test".into(),
                            command: "cargo test".into(),
                            working_dir: None,
                            timeout_secs: 1800,
                            env: HashMap::new(),
                        }],
                        needs: vec!["build".into()],
                        artifacts: Vec::new(),
                        secrets: Vec::new(),
                        compliance: vec!["soc2".into()],
                        env: HashMap::new(),
                    },
                );
                m
            },
        };

        let json = serde_json::to_string_pretty(&workflow).unwrap();
        let decoded: CiWorkflow = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.jobs.len(), 2);
        assert!(decoded.on.push.is_some());
        assert!(decoded.on.pull_request.is_some());
        assert_eq!(decoded.env.get("CARGO_TERM_COLOR").unwrap(), "always");

        // Verify DAG dependency
        let test_job = decoded.jobs.get("test").unwrap();
        assert_eq!(test_job.needs, vec!["build"]);
        assert_eq!(test_job.compliance, vec!["soc2"]);
    }

    #[test]
    fn minimal_workflow_json() {
        let json = r#"{
            "on": { "push": { "branches": ["main"] } },
            "jobs": {
                "hello": {
                    "environment": "sh",
                    "steps": [{ "name": "Greet", "command": "echo hello" }]
                }
            }
        }"#;

        let workflow: CiWorkflow = serde_json::from_str(json).unwrap();
        assert_eq!(workflow.jobs.len(), 1);
        assert!(workflow.on.push.is_some());
        assert!(workflow.env.is_empty());
    }

    #[test]
    fn trigger_config_default_is_empty() {
        let config = TriggerConfig::default();
        assert!(config.push.is_none());
        assert!(config.pull_request.is_none());
    }
}

//! Workflow types — the deserialized form of adp-ci.yaml.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The top-level workflow file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiWorkflow {
    /// Schema version (must be 1).
    pub version: u32,

    /// Trigger configuration.
    #[serde(default)]
    pub on: TriggerConfig,

    /// Global environment variables.
    #[serde(default)]
    pub env: WorkflowEnv,

    /// Job definitions.
    pub jobs: HashMap<String, JobDefinition>,
}

/// Trigger configuration — what events cause the workflow to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Push trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push: Option<PushTrigger>,

    /// Pull request trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequestTrigger>,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            push: None,
            pull_request: None,
        }
    }
}

/// Push trigger configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushTrigger {
    /// Branch patterns to match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,

    /// Tag patterns to match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Path filters (include).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,

    /// Path filters (exclude).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths_ignore: Vec<String>,
}

/// Pull request trigger configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestTrigger {
    /// Branch patterns to match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,

    /// PR types: opened, synchronize, closed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
}

/// Trigger event for matching.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerEvent {
    Push {
        branch: String,
        tag: Option<String>,
    },
    PullRequest {
        branch: String,
        action: String,
    },
}

/// Global environment variables.
pub type WorkflowEnv = HashMap<String, String>;

/// A single job definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDefinition {
    /// Build environment (nix, devbox, flux, bazel).
    pub environment: String,

    /// Steps to execute.
    pub steps: Vec<StepDefinition>,

    /// Jobs that must complete before this one starts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,

    /// Artifact paths to collect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,

    /// Secret refs to mount.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,

    /// Compliance requirements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compliance: Vec<String>,

    /// Job-level environment variables (override global).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Timeout in seconds (default: 3600).
    #[serde(default = "default_job_timeout")]
    pub timeout: u64,
}

fn default_job_timeout() -> u64 {
    3600
}

/// A single step definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDefinition {
    /// Step name.
    pub name: String,

    /// Command to execute.
    pub command: String,

    /// Working directory relative to workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Timeout in seconds (default: 600).
    #[serde(default = "default_step_timeout")]
    pub timeout: u64,

    /// Step-level environment variables.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

fn default_step_timeout() -> u64 {
    600
}

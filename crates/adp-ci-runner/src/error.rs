//! Runner error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RunnerError {
    #[error("Workspace error: {0}")]
    Workspace(String),

    #[error("Secret mount error: {0}")]
    SecretMount(String),

    #[error("Command failed: {command} — exit code {exit_code}")]
    CommandFailed { command: String, exit_code: i32 },

    #[error("Command timed out: {command} after {timeout_secs}s")]
    CommandTimeout { command: String, timeout_secs: u64 },

    #[error("Step failed: {step} — {error}")]
    StepFailed { step: String, error: String },

    #[error("Artifact not found: {0}")]
    ArtifactNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
}

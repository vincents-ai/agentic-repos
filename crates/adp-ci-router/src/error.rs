//! Router error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RouterError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Runner not found: {0}")]
    RunnerNotFound(String),

    #[error("Job already assigned: {0}")]
    JobAlreadyAssigned(String),

    #[error("No matching runner for job {job_id}: {reason}")]
    NoMatchingRunner { job_id: String, reason: String },

    #[error("Job timeout: {0}")]
    JobTimeout(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Queue full: {0} pending jobs")]
    QueueFull(usize),
}

//! # adp-ci-runner
//!
//! CI runner execution engine — the library that runner binaries link against.
//!
//! ## Responsibilities
//!
//! - **Workspace isolation**: each job gets a clean temp directory
//! - **Git checkout**: clone repo at commit SHA, init submodules
//! - **Secret mounting**: K8s-pattern (tmpfs files + env vars)
//! - **Step execution**: run each JobStep with timeout enforcement
//! - **Log streaming**: capture stdout/stderr, stream to adp via REST
//! - **Artifact collection**: collect build outputs for upload
//!
//! ## Usage
//!
//! ```ignore
//! use adp_ci_runner::{JobExecutor, ExecutorConfig};
//!
//! let executor = JobExecutor::new(ExecutorConfig::default());
//! let result = executor.execute_job(request).await?;
//! ```

mod error;
mod executor;
mod secrets;
mod workspace;
mod log_stream;
mod ref_writer;

pub use error::RunnerError;
pub use executor::{JobExecutor, ExecutorConfig};
pub use secrets::SecretMounter;
pub use workspace::Workspace;
pub use log_stream::LogStreamer;
pub use ref_writer::{RefWriter, RefWriteError};

/// Re-export key types from adp-ci-types.
pub use adp_ci_types::{
    JobRequest, JobResult, JobStep, StepResult, JobStatus, JobMetrics,
    SecretMount, ArtifactRef, LogStream,
};

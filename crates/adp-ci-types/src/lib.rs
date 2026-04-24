//! # adp-ci-types
//!
//! Shared types for the ADP CI runner system.
//!
//! Defines:
//! - Events: CiEvent (adp→runner), RunnerEvent (runner→adp)
//! - Job types: JobRequest, JobResult, JobType, JobPriority, JobStatus
//! - Runner types: RunnerCapabilities, RunnerStatus, ResourceLimits
//! - Artifacts: ArtifactRef, ArtifactStorage
//! - Metrics: JobMetrics
//! - Secrets: SecretMount (Kubernetes pattern)

mod events;
mod job;
mod runner;
mod artifact;
mod secrets;

pub use events::{CiEvent, RunnerEvent, LogStream};
pub use job::{JobRequest, JobResult, JobStatus, JobType, JobPriority, JobTrigger, JobStep, ComplianceRequirements, JobMetrics, StepResult};
pub use runner::{RunnerCapabilities, RunnerStatus, RunnerType, ResourceLimits};
pub use artifact::{ArtifactRef, ArtifactStorage};
pub use secrets::SecretMount;

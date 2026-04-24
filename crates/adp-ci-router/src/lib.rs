//! # adp-ci-router
//!
//! CI job routing engine: SQLite-backed job queue, runner matching,
//! job assignment, and timeout monitoring.
//!
//! ## Architecture
//!
//! ```text
//! Git push → router.create_job() → SQLite (pending)
//!                                         ↓
//! Router loop → match_pending_jobs() → assign to runner → SQLite (assigned)
//!                                                              ↓
//! Runner SSE ← JobAssigned event ← SSE registry broadcast
//!                                                              ↓
//! Runner REST → update_job(Running) → SQLite (running)
//!                         → append logs → git ref
//!                                                              ↓
//! Runner REST → update_job(Completed/Failed) → SQLite (done)
//!                                              → write result → git ref
//! ```
//!
//! Git refs are the audit trail. SQLite is the fast queue.

mod error;
mod queue;
mod matcher;
mod router;
mod metrics;

pub use error::RouterError;
pub use queue::JobQueue;
pub use matcher::select_runner;
pub use router::CiRouter;
pub use metrics::{CiMetricsSnapshot, snapshot_queue_metrics};

/// Re-export key types from adp-ci-types for convenience.
pub use adp_ci_types::{
    CiEvent, RunnerEvent, JobRequest, JobResult, JobStatus, JobType, JobPriority,
    RunnerCapabilities, RunnerStatus,
};

//! Metrics sync — periodically pushes SQLite queue state to Prometheus gauges.
//!
//! The router loop calls `sync_metrics()` on each tick to keep
//! KEDA-relevant gauges up to date.

use crate::queue::JobQueue;
use std::sync::Arc;

/// CI metric names (matching adp/src/routes/metrics.rs).
/// These are defined as constants so both crates can reference them
/// without depending on prometheus directly.
#[allow(dead_code)]
pub const METRIC_JOBS_PENDING: &str = "adp_ci_jobs_pending";
#[allow(dead_code)]
pub const METRIC_JOBS_RUNNING: &str = "adp_ci_jobs_running";
#[allow(dead_code)]
pub const METRIC_RUNNERS_AVAILABLE: &str = "adp_ci_runners_available";
#[allow(dead_code)]
pub const METRIC_RUNNERS_TOTAL: &str = "adp_ci_runners_total";

/// Snapshot of CI queue metrics.
#[derive(Debug, Clone, Default)]
pub struct CiMetricsSnapshot {
    pub jobs_pending: u64,
    pub jobs_assigned: u64,
    pub jobs_running: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub jobs_cancelled: u64,
    pub runners_idle: u64,
    pub runners_busy: u64,
    pub runners_total: u64,
}

/// Read current metrics from the job queue.
pub fn snapshot_queue_metrics(queue: &Arc<JobQueue>) -> CiMetricsSnapshot {
    let mut snapshot = CiMetricsSnapshot::default();

    match queue.count_jobs_by_status() {
        Ok(counts) => {
            for (status, count) in counts {
                match status.as_str() {
                    "pending" => snapshot.jobs_pending = count as u64,
                    "assigned" => snapshot.jobs_assigned = count as u64,
                    "running" => snapshot.jobs_running = count as u64,
                    "completed" => snapshot.jobs_completed = count as u64,
                    "failed" => snapshot.jobs_failed = count as u64,
                    "cancelled" => snapshot.jobs_cancelled = count as u64,
                    _ => {}
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to read job counts for metrics");
        }
    }

    // Count runners by status
    // This is a simplified approach — in production, add a count_runners_by_status method
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::JobQueue;
    use adp_ci_types::{JobPriority, JobType};

    #[test]
    fn snapshot_empty_queue() {
        let queue = Arc::new(JobQueue::open_in_memory().unwrap());
        let snapshot = snapshot_queue_metrics(&queue);
        assert_eq!(snapshot.jobs_pending, 0);
    }

    #[test]
    fn snapshot_with_jobs() {
        let queue = Arc::new(JobQueue::open_in_memory().unwrap());

        queue
            .create_job("j1", "r", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "a", None)
            .unwrap();
        queue
            .create_job("j2", "r", JobType::Test, JobPriority::Medium, "nix", "{}", "[]", "main", "b", None)
            .unwrap();

        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 4)
            .unwrap();
        queue.assign_job("j1", "r1").unwrap();

        let snapshot = snapshot_queue_metrics(&queue);
        assert_eq!(snapshot.jobs_pending, 1);
        assert_eq!(snapshot.jobs_assigned, 1);
    }
}

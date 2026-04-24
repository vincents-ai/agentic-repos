//! CI Router — orchestrates job lifecycle.
//!
//! The router is the central coordinator:
//! 1. Receives job creation requests (from git push hooks)
//! 2. Matches pending jobs to available runners
//! 3. Assigns jobs via SSE broadcast
//! 4. Monitors for assignment timeouts
//! 5. Re-queues timed-out jobs

use crate::error::RouterError;
use crate::matcher::select_runner;
use crate::queue::JobQueue;
use adp_ci_types::{CiEvent, JobPriority, JobStatus, JobType};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing;

/// How long before an assigned job is considered timed out (seconds).
const DEFAULT_ASSIGNMENT_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// How often to check for timed-out jobs (seconds).
const DEFAULT_TIMEOUT_CHECK_INTERVAL_SECS: u64 = 30;

/// CI Router — coordinates job lifecycle.
pub struct CiRouter {
    queue: Arc<JobQueue>,
    event_tx: broadcast::Sender<CiEvent>,
    assignment_timeout_secs: u64,
}

impl CiRouter {
    /// Create a new CI router.
    pub fn new(queue: Arc<JobQueue>, event_tx: broadcast::Sender<CiEvent>) -> Self {
        Self {
            queue,
            event_tx,
            assignment_timeout_secs: DEFAULT_ASSIGNMENT_TIMEOUT_SECS,
        }
    }

    /// Create with custom assignment timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.assignment_timeout_secs = secs;
        self
    }

    /// Submit a new job to the queue.
    ///
    /// This is called when a git push triggers a CI pipeline.
    /// The job is inserted as `pending` and will be matched to a runner
    /// on the next routing cycle.
    pub fn submit_job(
        &self,
        job_id: &str,
        repo_id: &str,
        job_type: JobType,
        priority: JobPriority,
        environment: &str,
        compliance_json: &str,
        steps_json: &str,
        branch: &str,
        commit: &str,
    ) -> Result<(), RouterError> {
        tracing::info!(
            job_id,
            repo_id,
            ?job_type,
            ?priority,
            environment,
            "Submitting CI job"
        );

        self.queue.create_job(
            job_id,
            repo_id,
            job_type,
            priority,
            environment,
            compliance_json,
            steps_json,
            branch,
            commit,
            None,
        )?;

        // Broadcast job available event
        let event = CiEvent::JobAvailable {
            job_id: job_id.to_string(),
            repo_id: repo_id.to_string(),
            branch: branch.to_string(),
            commit_sha: commit.to_string(),
            job_type,
            priority,
        };
        let _ = self.event_tx.send(event);

        Ok(())
    }

    /// Match pending jobs to available runners.
    ///
    /// Returns the number of jobs assigned.
    pub fn match_pending_jobs(&self) -> Result<usize, RouterError> {
        let pending = self.queue.pending_jobs()?;

        let mut assigned = 0;
        for job in pending {
            match select_runner(&self.queue, &job) {
                Ok(runner_id) => {
                    tracing::info!(
                        job_id = %job.job_id,
                        runner_id = %runner_id,
                        "Assigning job to runner"
                    );

                    self.queue.assign_job(&job.job_id, &runner_id)?;

                    let event = CiEvent::JobAssigned {
                        job_id: job.job_id.clone(),
                        deadline: chrono::Utc::now()
                            .checked_add_signed(chrono::Duration::seconds(
                                self.assignment_timeout_secs as i64,
                            ))
                            .unwrap()
                            .to_rfc3339(),
                        secret_refs: Vec::new(), // populated from steps_json
                    };
                    let _ = self.event_tx.send(event);
                    assigned += 1;
                }
                Err(RouterError::NoMatchingRunner { job_id, reason }) => {
                    tracing::debug!(
                        job_id = %job_id,
                        reason = %reason,
                        "No runner available for job, will retry"
                    );
                }
                Err(e) => return Err(e),
            }
        }

        Ok(assigned)
    }

    /// Check for timed-out assignments and re-queue them.
    ///
    /// Returns the number of jobs re-queued.
    pub fn handle_timeouts(&self) -> Result<usize, RouterError> {
        let timed_out = self.queue.timed_out_jobs(self.assignment_timeout_secs)?;

        for job_id in &timed_out {
            tracing::warn!(job_id = %job_id, "Job assignment timed out, re-queuing");
            self.queue.requeue_job(job_id)?;
        }

        Ok(timed_out.len())
    }

    /// Update job status (called by runner via REST).
    pub fn update_job_status(
        &self,
        job_id: &str,
        status: JobStatus,
    ) -> Result<(), RouterError> {
        tracing::info!(job_id, ?status, "Updating job status");
        self.queue.update_job_status(job_id, status)
    }

    /// Cancel a job.
    pub fn cancel_job(&self, job_id: &str, reason: &str) -> Result<(), RouterError> {
        tracing::info!(job_id, reason, "Cancelling job");

        self.queue.update_job_status(job_id, JobStatus::Cancelled)?;

        let event = CiEvent::JobCancelled {
            job_id: job_id.to_string(),
            reason: reason.to_string(),
        };
        let _ = self.event_tx.send(event);

        Ok(())
    }

    /// Get the underlying queue for direct access.
    pub fn queue(&self) -> &Arc<JobQueue> {
        &self.queue
    }

    /// Run the routing loop (blocking).
    ///
    /// Periodically matches pending jobs and checks for timeouts.
    /// Runs until the cancellation token is triggered.
    pub async fn run_routing_loop(
        self: &Arc<Self>,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(
            DEFAULT_TIMEOUT_CHECK_INTERVAL_SECS,
        ));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.match_pending_jobs() {
                        tracing::error!(error = %e, "Error matching pending jobs");
                    }
                    if let Err(e) = self.handle_timeouts() {
                        tracing::error!(error = %e, "Error handling timeouts");
                    }
                }
                _ = cancel.cancelled() => {
                    tracing::info!("CI router loop shutting down");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_and_assign_job() {
        let queue = Arc::new(JobQueue::open_in_memory().unwrap());
        let (tx, _rx) = broadcast::channel(16);
        let router = CiRouter::new(queue.clone(), tx);

        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();

        router
            .submit_job(
                "j1",
                "repo-1",
                JobType::Build,
                JobPriority::High,
                "nix",
                "{}",
                "[]",
                "main",
                "abc123",
            )
            .unwrap();

        assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Pending));

        let assigned = router.match_pending_jobs().unwrap();
        assert_eq!(assigned, 1);
        assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Assigned));
    }

    #[test]
    fn cancel_job() {
        let queue = Arc::new(JobQueue::open_in_memory().unwrap());
        let (tx, _rx) = broadcast::channel(16);
        let router = CiRouter::new(queue.clone(), tx);

        router
            .submit_job("j1", "repo-1", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "abc")
            .unwrap();

        router.cancel_job("j1", "user request").unwrap();
        assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Cancelled));
    }

    #[test]
    fn job_lifecycle_through_router() {
        let queue = Arc::new(JobQueue::open_in_memory().unwrap());
        let (tx, _rx) = broadcast::channel(16);
        let router = CiRouter::new(queue.clone(), tx);

        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 4)
            .unwrap();

        // Submit
        router
            .submit_job("j1", "repo", JobType::Build, JobPriority::Medium, "nix", "{}", "[]", "main", "abc")
            .unwrap();

        // Match
        router.match_pending_jobs().unwrap();
        assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Assigned));

        // Runner starts
        router.update_job_status("j1", JobStatus::Running).unwrap();
        assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Running));

        // Runner completes
        router.update_job_status("j1", JobStatus::Completed).unwrap();
        assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Completed));
    }
}

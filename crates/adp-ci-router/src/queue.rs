//! SQLite-backed job queue.

use crate::error::RouterError;
use adp_ci_types::{JobPriority, JobStatus, JobType};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

/// SQLite-backed job queue.
///
/// Thread-safe via interior mutability (Mutex<Connection>).
/// For high-throughput production use, consider connection pooling.
pub struct JobQueue {
    conn: Mutex<Connection>,
}

impl JobQueue {
    /// Open (or create) the job queue database at the given path.
    pub fn open(path: &Path) -> Result<Self, RouterError> {
        let conn = Connection::open(path)?;
        let queue = Self {
            conn: Mutex::new(conn),
        };
        queue.init_schema()?;
        Ok(queue)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, RouterError> {
        let conn = Connection::open_in_memory()?;
        let queue = Self {
            conn: Mutex::new(conn),
        };
        queue.init_schema()?;
        Ok(queue)
    }

    fn init_schema(&self) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ci_jobs (
                job_id          TEXT PRIMARY KEY,
                repo_id         TEXT NOT NULL,
                job_type        TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'pending',
                priority        INTEGER NOT NULL DEFAULT 1,
                assigned_runner TEXT,
                created_at      TEXT NOT NULL,
                updated_at      TEXT NOT NULL,
                deadline_at     TEXT,
                environment     TEXT,
                compliance      TEXT,
                steps_json      TEXT,
                trigger_branch  TEXT,
                trigger_commit  TEXT,
                artifact_storage TEXT DEFAULT 'git_lfs'
            );

            CREATE INDEX IF NOT EXISTS idx_ci_jobs_status
                ON ci_jobs(status, priority DESC);

            CREATE INDEX IF NOT EXISTS idx_ci_jobs_runner
                ON ci_jobs(assigned_runner);

            CREATE TABLE IF NOT EXISTS ci_runners (
                runner_id       TEXT PRIMARY KEY,
                status          TEXT NOT NULL DEFAULT 'idle',
                runner_type     TEXT NOT NULL,
                environments    TEXT NOT NULL,
                platforms       TEXT NOT NULL,
                compliance      TEXT DEFAULT '[]',
                labels_json     TEXT DEFAULT '{}',
                max_concurrent  INTEGER NOT NULL DEFAULT 1,
                current_jobs    INTEGER NOT NULL DEFAULT 0,
                last_heartbeat  TEXT NOT NULL,
                registered_at   TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_ci_runners_status
                ON ci_runners(status, runner_type);
            ",
        )?;
        Ok(())
    }

    /// Insert a new job into the queue.
    pub fn create_job(
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
        deadline: Option<&str>,
    ) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO ci_jobs (job_id, repo_id, job_type, status, priority, created_at, updated_at, deadline_at, environment, compliance, steps_json, trigger_branch, trigger_commit)
             VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                job_id,
                repo_id,
                job_type.to_string(),
                priority as i32,
                now,
                deadline,
                environment,
                compliance_json,
                steps_json,
                branch,
                commit,
            ],
        )?;
        Ok(())
    }

    /// Get the next pending job, sorted by priority (highest first).
    pub fn next_pending_job(&self) -> Result<Option<PendingJob>, RouterError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT job_id, repo_id, job_type, priority, environment, compliance, steps_json, trigger_branch, trigger_commit
             FROM ci_jobs
             WHERE status = 'pending'
             ORDER BY priority DESC, created_at ASC
             LIMIT 1",
        )?;

        let job = stmt.query_row([], |row| {
            Ok(PendingJob {
                job_id: row.get(0)?,
                repo_id: row.get(1)?,
                job_type: row.get::<_, String>(2)?,
                priority: row.get::<_, i32>(3)?,
                environment: row.get(4)?,
                compliance_json: row.get(5)?,
                steps_json: row.get(6)?,
                branch: row.get(7)?,
                commit: row.get(8)?,
            })
        }).optional()?;

        Ok(job)
    }

    /// Get all pending jobs, ordered by priority.
    pub fn pending_jobs(&self) -> Result<Vec<PendingJob>, RouterError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT job_id, repo_id, job_type, priority, environment, compliance, steps_json, trigger_branch, trigger_commit
             FROM ci_jobs
             WHERE status = 'pending'
             ORDER BY priority DESC, created_at ASC",
        )?;

        let jobs = stmt.query_map([], |row| {
            Ok(PendingJob {
                job_id: row.get(0)?,
                repo_id: row.get(1)?,
                job_type: row.get::<_, String>(2)?,
                priority: row.get::<_, i32>(3)?,
                environment: row.get(4)?,
                compliance_json: row.get(5)?,
                steps_json: row.get(6)?,
                branch: row.get(7)?,
                commit: row.get(8)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(jobs)
    }

    /// Assign a job to a runner.
    pub fn assign_job(&self, job_id: &str, runner_id: &str) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE ci_jobs SET status = 'assigned', assigned_runner = ?1, updated_at = ?2
             WHERE job_id = ?3 AND status = 'pending'",
            params![runner_id, now, job_id],
        )?;
        if rows == 0 {
            return Err(RouterError::JobNotFound(job_id.to_string()));
        }
        // Increment runner's current job count
        conn.execute(
            "UPDATE ci_runners SET current_jobs = current_jobs + 1 WHERE runner_id = ?1",
            params![runner_id],
        )?;
        Ok(())
    }

    /// Update job status (runner callback).
    pub fn update_job_status(&self, job_id: &str, status: JobStatus) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let status_str = match status {
            JobStatus::Pending => "pending",
            JobStatus::Assigned => "assigned",
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        };
        let rows = conn.execute(
            "UPDATE ci_jobs SET status = ?1, updated_at = ?2 WHERE job_id = ?3",
            params![status_str, now, job_id],
        )?;
        if rows == 0 {
            return Err(RouterError::JobNotFound(job_id.to_string()));
        }

        // If job completed/failed/cancelled, decrement runner's job count
        if matches!(status, JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled) {
            conn.execute(
                "UPDATE ci_runners SET current_jobs = MAX(0, current_jobs - 1)
                 WHERE runner_id = (SELECT assigned_runner FROM ci_jobs WHERE job_id = ?1)",
                params![job_id],
            )?;
        }

        Ok(())
    }

    /// Get job status.
    pub fn job_status(&self, job_id: &str) -> Result<Option<JobStatus>, RouterError> {
        let conn = self.conn.lock().unwrap();
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM ci_jobs WHERE job_id = ?1",
                params![job_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(status.and_then(|s| match s.as_str() {
            "pending" => Some(JobStatus::Pending),
            "assigned" => Some(JobStatus::Assigned),
            "running" => Some(JobStatus::Running),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            "cancelled" => Some(JobStatus::Cancelled),
            _ => None,
        }))
    }

    /// Register a runner.
    pub fn register_runner(
        &self,
        runner_id: &str,
        runner_type: &str,
        environments: &str, // JSON array
        platforms: &str,    // JSON array
        compliance: &str,   // JSON array
        labels_json: &str,
        max_concurrent: u32,
    ) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO ci_runners (runner_id, status, runner_type, environments, platforms, compliance, labels_json, max_concurrent, current_jobs, last_heartbeat, registered_at)
             VALUES (?1, 'idle', ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)",
            params![runner_id, runner_type, environments, platforms, compliance, labels_json, max_concurrent, now],
        )?;
        Ok(())
    }

    /// Update runner heartbeat / status.
    pub fn update_runner_status(
        &self,
        runner_id: &str,
        status: &str,
        current_jobs: u32,
    ) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE ci_runners SET status = ?1, current_jobs = ?2, last_heartbeat = ?3
             WHERE runner_id = ?4",
            params![status, current_jobs, now, runner_id],
        )?;
        if rows == 0 {
            return Err(RouterError::RunnerNotFound(runner_id.to_string()));
        }
        Ok(())
    }

    /// Get available runners that match given criteria.
    pub fn available_runners(
        &self,
        environment: &str,
        compliance_tags: &[String],
    ) -> Result<Vec<RunnerRow>, RouterError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT runner_id, runner_type, environments, platforms, compliance, labels_json, max_concurrent, current_jobs
             FROM ci_runners
             WHERE status = 'idle' AND current_jobs < max_concurrent",
        )?;

        let runners = stmt.query_map([], |row| {
            Ok(RunnerRow {
                runner_id: row.get(0)?,
                runner_type: row.get(1)?,
                environments: row.get(2)?,
                platforms: row.get(3)?,
                compliance: row.get(4)?,
                labels_json: row.get(5)?,
                max_concurrent: row.get::<_, u32>(6)?,
                current_jobs: row.get::<_, u32>(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        // Filter in-memory for environment and compliance matching
        let filtered: Vec<RunnerRow> = runners
            .into_iter()
            .filter(|r| {
                // Check environment support
                let envs: Vec<String> = serde_json::from_str(&r.environments).unwrap_or_default();
                if !envs.iter().any(|e| e == environment) {
                    return false;
                }
                // Check compliance
                if !compliance_tags.is_empty() {
                    let runner_compliance: Vec<String> =
                        serde_json::from_str(&r.compliance).unwrap_or_default();
                    for required in compliance_tags {
                        if !runner_compliance.contains(required) {
                            return false;
                        }
                    }
                }
                true
            })
            .collect();

        Ok(filtered)
    }

    /// Count jobs by status.
    pub fn count_jobs_by_status(&self) -> Result<Vec<(String, usize)>, RouterError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT status, COUNT(*) FROM ci_jobs GROUP BY status",
        )?;
        let counts = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(counts)
    }

    /// List jobs, optionally filtered by status, with a limit.
    pub fn list_jobs(
        &self,
        status: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<JobRow>, RouterError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT job_id, repo_id, job_type, status, priority, assigned_runner,
                    created_at, updated_at, deadline_at, environment,
                    trigger_branch, trigger_commit
             FROM ci_jobs
             WHERE (?1 IS NULL OR status = ?1)
             ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows: Vec<JobRow> = stmt
            .query_map(params![status, limit, offset], |row| Ok(job_row_from(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Count total jobs matching a status filter.
    pub fn count_jobs(&self, status: Option<&str>) -> Result<usize, RouterError> {
        let conn = self.conn.lock().unwrap();
        let count: usize = if let Some(s) = status {
            conn.query_row(
                "SELECT COUNT(*) FROM ci_jobs WHERE status = ?1",
                params![s],
                |row| row.get(0),
            )?
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM ci_jobs",
                [],
                |row| row.get(0),
            )?
        };
        Ok(count)
    }

    /// Find timed-out assigned jobs (assigned but not started within deadline).
    pub fn timed_out_jobs(&self, timeout_secs: u64) -> Result<Vec<String>, RouterError> {
        let conn = self.conn.lock().unwrap();
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(timeout_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT job_id FROM ci_jobs
             WHERE status = 'assigned' AND updated_at < ?1",
        )?;
        let ids: Vec<String> = stmt
            .query_map(params![cutoff_str], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Re-queue timed-out jobs (reset status to pending, clear runner).
    pub fn requeue_job(&self, job_id: &str) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE ci_jobs SET status = 'pending', assigned_runner = NULL, updated_at = ?1
             WHERE job_id = ?2",
            params![now, job_id],
        )?;
        if rows == 0 {
            return Err(RouterError::JobNotFound(job_id.to_string()));
        }
        Ok(())
    }

    /// Deregister a runner (set status to offline).
    pub fn deregister_runner(&self, runner_id: &str) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE ci_runners SET status = 'offline' WHERE runner_id = ?1",
            params![runner_id],
        )?;
        if rows == 0 {
            return Err(RouterError::RunnerNotFound(runner_id.to_string()));
        }
        Ok(())
    }

    /// Force-set the updated_at timestamp (for timeout testing).
    pub fn force_set_job_updated_at(&self, job_id: &str, timestamp: &str) -> Result<(), RouterError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE ci_jobs SET updated_at = ?1 WHERE job_id = ?2",
            params![timestamp, job_id],
        )?;
        Ok(())
    }

    /// Get runner details.
    pub fn get_runner(&self, runner_id: &str) -> Result<Option<RunnerRow>, RouterError> {
        let conn = self.conn.lock().unwrap();
        let runner = conn
            .query_row(
                "SELECT runner_id, runner_type, environments, platforms, compliance, labels_json, max_concurrent, current_jobs
                 FROM ci_runners WHERE runner_id = ?1",
                params![runner_id],
                |row| {
                    Ok(RunnerRow {
                        runner_id: row.get(0)?,
                        runner_type: row.get(1)?,
                        environments: row.get(2)?,
                        platforms: row.get(3)?,
                        compliance: row.get(4)?,
                        labels_json: row.get(5)?,
                        max_concurrent: row.get::<_, u32>(6)?,
                        current_jobs: row.get::<_, u32>(7)?,
                    })
                },
            )
            .optional()?;
        Ok(runner)
    }
}

/// A pending job from the queue.
#[derive(Debug, Clone)]
pub struct PendingJob {
    pub job_id: String,
    pub repo_id: String,
    pub job_type: String,
    pub priority: i32,
    pub environment: String,
    pub compliance_json: String,
    pub steps_json: String,
    pub branch: String,
    pub commit: String,
}

/// A runner row from the database.
#[derive(Debug, Clone)]
pub struct RunnerRow {
    pub runner_id: String,
    pub runner_type: String,
    pub environments: String,
    pub platforms: String,
    pub compliance: String,
    pub labels_json: String,
    pub max_concurrent: u32,
    pub current_jobs: u32,
}

/// A job row from the queue database.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JobRow {
    pub job_id: String,
    pub repo_id: String,
    pub job_type: String,
    pub status: String,
    pub priority: i32,
    pub assigned_runner: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deadline_at: Option<String>,
    pub environment: Option<String>,
    pub trigger_branch: Option<String>,
    pub trigger_commit: Option<String>,
}

fn job_row_from(row: &rusqlite::Row<'_>) -> JobRow {
    JobRow {
        job_id: row.get(0).unwrap_or_default(),
        repo_id: row.get(1).unwrap_or_default(),
        job_type: row.get(2).unwrap_or_default(),
        status: row.get(3).unwrap_or_default(),
        priority: row.get(4).unwrap_or(1),
        assigned_runner: row.get(5).unwrap_or(None),
        created_at: row.get(6).unwrap_or_default(),
        updated_at: row.get(7).unwrap_or_default(),
        deadline_at: row.get(8).unwrap_or(None),
        environment: row.get(9).unwrap_or(None),
        trigger_branch: row.get(10).unwrap_or(None),
        trigger_commit: row.get(11).unwrap_or(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_query_job() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .create_job(
                "job-1",
                "repo-1",
                JobType::Build,
                JobPriority::High,
                "nix",
                "{}",
                "[]",
                "main",
                "abc123",
                None,
            )
            .unwrap();

        let pending = queue.pending_jobs().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].job_id, "job-1");
        assert_eq!(pending[0].repo_id, "repo-1");
        assert_eq!(pending[0].environment, "nix");
    }

    #[test]
    fn assign_job_to_runner() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .register_runner(
                "runner-1",
                "build_worker",
                "[\"nix\"]",
                "[\"linux/amd64\"]",
                "[]",
                "{}",
                4,
            )
            .unwrap();

        queue
            .create_job(
                "job-1",
                "repo-1",
                JobType::Build,
                JobPriority::Medium,
                "nix",
                "{}",
                "[]",
                "main",
                "abc123",
                None,
            )
            .unwrap();

        queue.assign_job("job-1", "runner-1").unwrap();

        let status = queue.job_status("job-1").unwrap().unwrap();
        assert_eq!(status, JobStatus::Assigned);

        let runner = queue.get_runner("runner-1").unwrap().unwrap();
        assert_eq!(runner.current_jobs, 1);
    }

    #[test]
    fn job_lifecycle() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .create_job(
                "job-1",
                "repo-1",
                JobType::Test,
                JobPriority::Low,
                "nix",
                "{}",
                "[]",
                "main",
                "abc",
                None,
            )
            .unwrap();

        assert_eq!(queue.job_status("job-1").unwrap(), Some(JobStatus::Pending));

        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();
        queue.assign_job("job-1", "r1").unwrap();
        assert_eq!(queue.job_status("job-1").unwrap(), Some(JobStatus::Assigned));

        queue.update_job_status("job-1", JobStatus::Running).unwrap();
        assert_eq!(queue.job_status("job-1").unwrap(), Some(JobStatus::Running));

        queue.update_job_status("job-1", JobStatus::Completed).unwrap();
        assert_eq!(queue.job_status("job-1").unwrap(), Some(JobStatus::Completed));

        // Runner job count should decrement
        let runner = queue.get_runner("r1").unwrap().unwrap();
        assert_eq!(runner.current_jobs, 0);
    }

    #[test]
    fn priority_ordering() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .create_job("low", "r", JobType::Lint, JobPriority::Low, "nix", "{}", "[]", "main", "a", None)
            .unwrap();
        queue
            .create_job("critical", "r", JobType::Build, JobPriority::Critical, "nix", "{}", "[]", "main", "b", None)
            .unwrap();
        queue
            .create_job("medium", "r", JobType::Test, JobPriority::Medium, "nix", "{}", "[]", "main", "c", None)
            .unwrap();

        let pending = queue.pending_jobs().unwrap();
        assert_eq!(pending[0].job_id, "critical");
        assert_eq!(pending[1].job_id, "medium");
        assert_eq!(pending[2].job_id, "low");
    }

    #[test]
    fn available_runners_filter_by_environment() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();
        queue
            .register_runner("r2", "build_worker", "[\"docker\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();

        let nix_runners = queue.available_runners("nix", &[]).unwrap();
        assert_eq!(nix_runners.len(), 1);
        assert_eq!(nix_runners[0].runner_id, "r1");
    }

    #[test]
    fn available_runners_filter_by_compliance() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[\"soc2\"]", "{}", 2)
            .unwrap();
        queue
            .register_runner("r2", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();

        let soc2_runners = queue
            .available_runners("nix", &[String::from("soc2")])
            .unwrap();
        assert_eq!(soc2_runners.len(), 1);
        assert_eq!(soc2_runners[0].runner_id, "r1");
    }

    #[test]
    fn runner_at_capacity_not_returned() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 1)
            .unwrap();
        queue.update_runner_status("r1", "idle", 1).unwrap();

        let runners = queue.available_runners("nix", &[]).unwrap();
        assert!(runners.is_empty());
    }

    #[test]
    fn timed_out_jobs_detected() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .create_job("j1", "r", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "a", None)
            .unwrap();
        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();
        queue.assign_job("j1", "r1").unwrap();

        // Manually set updated_at to past
        let conn = queue.conn.lock().unwrap();
        let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        conn.execute(
            "UPDATE ci_jobs SET updated_at = ?1 WHERE job_id = 'j1'",
            params![past],
        )
        .unwrap();
        drop(conn);

        let timed_out = queue.timed_out_jobs(300).unwrap();
        assert_eq!(timed_out, vec!["j1"]);
    }

    #[test]
    fn requeue_job() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue
            .create_job("j1", "r", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "a", None)
            .unwrap();
        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();
        queue.assign_job("j1", "r1").unwrap();

        queue.requeue_job("j1").unwrap();
        assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Pending));
    }

    #[test]
    fn deregister_runner() {
        let queue = JobQueue::open_in_memory().unwrap();
        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();
        queue.deregister_runner("r1").unwrap();

        let runner = queue.get_runner("r1").unwrap().unwrap();
        // Runner still exists but is offline
        assert_eq!(runner.runner_id, "r1");
        // Available runners should not include offline ones
        let avail = queue.available_runners("nix", &[]).unwrap();
        assert!(avail.is_empty());
    }

    #[test]
    fn count_by_status() {
        let queue = JobQueue::open_in_memory().unwrap();

        queue.create_job("j1", "r", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "a", None).unwrap();
        queue.create_job("j2", "r", JobType::Test, JobPriority::Medium, "nix", "{}", "[]", "main", "b", None).unwrap();
        queue.create_job("j3", "r", JobType::Lint, JobPriority::Low, "nix", "{}", "[]", "main", "c", None).unwrap();

        queue.register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 4).unwrap();
        queue.assign_job("j1", "r1").unwrap();

        let counts = queue.count_jobs_by_status().unwrap();
        let pending = counts.iter().find(|(s, _)| s == "pending").map(|(_, c)| *c).unwrap_or(0);
        let assigned = counts.iter().find(|(s, _)| s == "assigned").map(|(_, c)| *c).unwrap_or(0);
        assert_eq!(pending, 2);
        assert_eq!(assigned, 1);
    }
}

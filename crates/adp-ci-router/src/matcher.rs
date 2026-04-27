//! Runner matching — score and select the best runner for a job.

use crate::error::RouterError;
use crate::queue::{JobQueue, PendingJob, RunnerRow};

/// Score a runner's suitability for a job. Higher is better.
///
/// Factors:
/// - Available slots (more = better)
/// - Compliance match (exact = better)
/// - Labels match (more overlap = better)
pub fn score_runner(runner: &RunnerRow, job: &PendingJob) -> u64 {
    let mut score: u64 = 0;

    // Available slots: up to 100 points
    let available = runner.max_concurrent.saturating_sub(runner.current_jobs);
    score += available as u64 * 25;

    // Label matching: parse labels_json, check for region matches
    let job_labels: serde_json::Value = serde_json::from_str(&job.compliance_json)
        .unwrap_or(serde_json::Value::Null);
    let runner_labels: serde_json::Value = serde_json::from_str(&runner.labels_json)
        .unwrap_or(serde_json::Value::Null);

    if let (serde_json::Value::Object(jl), serde_json::Value::Object(rl)) = (job_labels, runner_labels)
    {
        for (key, value) in &jl {
            if let Some(rv) = rl.get(key) {
                if rv == value {
                    score += 10;
                }
            }
        }
    }

    score
}

/// Select the best runner for a pending job.
///
/// Returns the runner_id of the best match, or an error if no runner is available.
pub fn select_runner(
    queue: &JobQueue,
    job: &PendingJob,
) -> Result<String, RouterError> {
    let compliance: Vec<String> = serde_json::from_str::<Vec<String>>(&job.compliance_json)
        .unwrap_or_default();

    let runners = queue.available_runners(&job.environment, &compliance)?;

    if runners.is_empty() {
        return Err(RouterError::NoMatchingRunner {
            job_id: job.job_id.clone(),
            reason: format!(
                "No idle runner with environment '{}' and compliance {:?}",
                job.environment, compliance
            ),
        });
    }

    // Score each runner and pick the best
    let best = runners
        .iter()
        .max_by_key(|r| score_runner(r, job))
        .unwrap();

    Ok(best.runner_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::RunnerRow;

    fn make_runner(id: &str, envs: &[&str], slots: u32, current: u32, labels: &str) -> RunnerRow {
        RunnerRow {
            runner_id: id.to_string(),
            runner_type: "build_worker".to_string(),
            environments: serde_json::to_string(envs).unwrap(),
            platforms: "[\"linux/amd64\"]".to_string(),
            compliance: "[]".to_string(),
            labels_json: labels.to_string(),
            max_concurrent: slots,
            current_jobs: current,
        }
    }

    fn make_job(id: &str, env: &str) -> PendingJob {
        PendingJob {
            job_id: id.to_string(),
            repo_id: "repo".to_string(),
            job_type: "build".to_string(),
            priority: 1,
            environment: env.to_string(),
            compliance_json: "{}".to_string(),
            steps_json: "[]".to_string(),
            branch: "main".to_string(),
            commit: "abc".to_string(),
        }
    }

    #[test]
    fn score_prefers_more_available_slots() {
        let r1 = make_runner("r1", &["nix"], 4, 0, "{}");
        let r2 = make_runner("r2", &["nix"], 2, 0, "{}");

        let job = make_job("j1", "nix");
        let s1 = score_runner(&r1, &job);
        let s2 = score_runner(&r2, &job);
        assert!(s1 > s2);
    }

    #[test]
    fn score_prefers_label_match() {
        let r1 = make_runner("r1", &["nix"], 4, 0, r#"{"region":"eu"}"#);
        let r2 = make_runner("r2", &["nix"], 4, 0, r#"{"region":"us"}"#);

        let job = PendingJob {
            compliance_json: r#"{"region":"eu"}"#.to_string(),
            ..make_job("j1", "nix")
        };

        let s1 = score_runner(&r1, &job);
        let s2 = score_runner(&r2, &job);
        assert!(s1 > s2);
    }

    #[test]
    fn select_runner_picks_best() {
        let queue = JobQueue::open_in_memory().unwrap();
        queue
            .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 2)
            .unwrap();
        queue
            .register_runner("r2", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", r#"{"region":"eu"}"#, 4)
            .unwrap();

        let job = PendingJob {
            compliance_json: r#"{"region":"eu"}"#.to_string(),
            ..make_job("j1", "nix")
        };

        let selected = select_runner(&queue, &job).unwrap();
        assert_eq!(selected, "r2");
    }

    #[test]
    fn select_runner_returns_error_when_no_match() {
        let queue = JobQueue::open_in_memory().unwrap();
        // No runners registered

        let job = make_job("j1", "nix");
        let result = select_runner(&queue, &job);
        assert!(result.is_err());
    }
}

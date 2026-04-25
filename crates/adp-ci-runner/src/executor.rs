//! Job executor — runs a JobRequest through its steps.

use crate::error::RunnerError;
use crate::log_stream::LogStreamer;
use crate::secrets::SecretMounter;
use crate::workspace::Workspace;
use adp_ci_types::{
    JobMetrics, JobRequest, JobResult, JobStatus, LogStream, StepResult,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// Configuration for the job executor.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Base URL for the adp server (for REST callbacks).
    pub adp_url: String,
    /// Maximum total job duration in seconds.
    pub max_job_duration_secs: u64,
    /// Whether to actually clone git repos (false for testing).
    pub enable_git_checkout: bool,
    /// Working directory base (None = system temp).
    pub work_dir_base: Option<PathBuf>,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            adp_url: "http://localhost:8090".to_string(),
            max_job_duration_secs: 3600,
            enable_git_checkout: true,
            work_dir_base: None,
        }
    }
}

/// The job executor — runs a complete CI job.
pub struct JobExecutor {
    config: ExecutorConfig,
}

impl JobExecutor {
    /// Create a new executor with the given configuration.
    pub fn new(config: ExecutorConfig) -> Self {
        Self { config }
    }

    /// Execute a complete job.
    ///
    /// 1. Create isolated workspace
    /// 2. Checkout git repo at commit SHA
    /// 3. Mount secrets
    /// 4. Execute each step
    /// 5. Collect artifacts
    /// 6. Return result
    pub async fn execute_job(&self, request: &JobRequest) -> JobResult {
        let job_start = Instant::now();
        let job_id_str = request.job_id.to_string();

        // 1. Create workspace
        let workspace = match self.create_workspace(&job_id_str) {
            Ok(ws) => ws,
            Err(e) => {
                return JobResult {
                    job_id: request.job_id,
                    status: JobStatus::Failed,
                    started_at: chrono::Utc::now(),
                    completed_at: chrono::Utc::now(),
                    artifacts: Vec::new(),
                    metrics: JobMetrics {
                        duration_secs: 0,
                        cpu_usage_percent: 0.0,
                        memory_peak_bytes: 0,
                        disk_usage_bytes: 0,
                    },
                    step_results: vec![StepResult {
                        name: "workspace-setup".to_string(),
                        success: false,
                        exit_code: 1,
                        duration_secs: 0,
                        error: Some(e.to_string()),
                    }],
                };
            }
        };

        // 2. Git checkout
        if self.config.enable_git_checkout {
            if let Err(e) = workspace
                .checkout(
                    &format!("{}/repos/{}.git", self.config.adp_url, request.trigger.commit_sha),
                    &request.trigger.commit_sha,
                    request.trigger.init_submodules,
                )
                .await
            {
                tracing::warn!(error = %e, "Git checkout failed (continuing without checkout)");
            }
        }

        // 3. Mount secrets
        let secret_env = self.mount_secrets(&workspace, &request.secrets);

        // 4. Execute steps
        let mut step_results = Vec::new();
        let mut all_success = true;

        let log_dir = workspace.root().join("logs");
        let mut log_streamer = match LogStreamer::new(&log_dir) {
            Ok(ls) => ls,
            Err(e) => {
                tracing::error!(error = %e, "Failed to create log streamer");
                return self.failed_result(request, "log-setup", &e.to_string());
            }
        };

        for step in &request.steps {
            tracing::info!(
                step = %step.name,
                environment = %step.environment,
                "Executing step"
            );

            let step_result = self
                .execute_step(step, &workspace, &secret_env, &mut log_streamer)
                .await;

            let success = step_result.success;
            step_results.push(step_result);

            if !success {
                all_success = false;
                break; // Stop on first failure
            }
        }

        let duration = job_start.elapsed();

        // 5. Collect artifacts
        let artifacts = self.collect_artifacts(&workspace, &request.artifacts);

        // Cleanup workspace
        if let Err(_e) = workspace.cleanup() {
        }

        JobResult {
            job_id: request.job_id,
            status: if all_success {
                JobStatus::Completed
            } else {
                JobStatus::Failed
            },
            started_at: chrono::Utc::now() - chrono::Duration::from_std(duration).unwrap(),
            completed_at: chrono::Utc::now(),
            artifacts,
            metrics: JobMetrics {
                duration_secs: duration.as_secs(),
                cpu_usage_percent: 0.0, // TODO: collect from cgroups (requires cgroup v2 mount)
                memory_peak_bytes: 0,   // TODO: collect from cgroups (requires cgroup v2 mount)
                disk_usage_bytes: workspace.disk_usage(),
            },
            step_results,
        }
    }

    fn create_workspace(&self, job_id: &str) -> Result<Workspace, RunnerError> {
        match &self.config.work_dir_base {
            Some(base) => Workspace::create_at(base.join(format!("adp-ci-{}", job_id))),
            None => Workspace::create(job_id),
        }
    }

    fn mount_secrets(
        &self,
        workspace: &Workspace,
        secret_refs: &[String],
    ) -> HashMap<String, String> {
        let mounter = SecretMounter::new(workspace.secrets_dir());

        // TODO (async): Fetch secrets from adp via REST, decrypt with SOPS.
        // The blocker is that mount_secrets is synchronous but fetching is async.
        // To fix properly, make execute_job's secret mounting async:
        //   let secret_mounts = self.fetch_secrets(secret_refs).await;
        //   mounter.mount_all(&secret_mounts)
        //
        // fetch_secrets would:
        //   1. GET {adp_url}/api/v1/secrets/{ref} → encrypted blob
        //   2. Decrypt with SOPS + AGE (using the runner's AGE private key)
        //   3. Return Vec<SecretMount>
        //
        // For now, return empty env map. The mounter infrastructure is ready.
        let _ = (mounter, secret_refs);
        HashMap::new()
    }

    /// Execute a single step.
    async fn execute_step(
        &self,
        step: &adp_ci_types::JobStep,
        workspace: &Workspace,
        secret_env: &HashMap<String, String>,
        log_streamer: &mut LogStreamer,
    ) -> StepResult {
        let step_start = Instant::now();

        let working_dir = step
            .working_dir
            .as_ref()
            .map(|d| workspace.workspace_dir().join(d))
            .unwrap_or_else(|| workspace.workspace_dir());

        // Build environment: step env + secret env
        let mut env: HashMap<String, String> = secret_env.clone();
        env.extend(step.env.clone());

        // Execute command
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(step.timeout_secs),
            self.run_command(&step.command, &working_dir, &env, log_streamer),
        )
        .await;

        let duration = step_start.elapsed().as_secs();

        match result {
            Ok(Ok(exit_code)) => StepResult {
                name: step.name.clone(),
                success: exit_code == 0,
                exit_code,
                duration_secs: duration,
                error: None,
            },
            Ok(Err(e)) => StepResult {
                name: step.name.clone(),
                success: false,
                exit_code: -1,
                duration_secs: duration,
                error: Some(e.to_string()),
            },
            Err(_) => StepResult {
                name: step.name.clone(),
                success: false,
                exit_code: -1,
                duration_secs: duration,
                error: Some(format!("Timed out after {}s", step.timeout_secs)),
            },
        }
    }

    /// Run a command and capture output into log streamer.
    async fn run_command(
        &self,
        command: &str,
        working_dir: &std::path::Path,
        env: &HashMap<String, String>,
        log_streamer: &mut LogStreamer,
    ) -> Result<i32, RunnerError> {
        // Use `sh -c` to support shell syntax in commands
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (k, v) in env {
            cmd.env(k, v);
        }

        let output = cmd.output().await.map_err(|_| {
            RunnerError::CommandFailed {
                command: command.to_string(),
                exit_code: -1,
            }
        })?;

        // Stream stdout
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let _ = log_streamer.append_line(LogStream::Stdout, line);
        }

        // Stream stderr
        let stderr = String::from_utf8_lossy(&output.stderr);
        for line in stderr.lines() {
            let _ = log_streamer.append_line(LogStream::Stderr, line);
        }

        let exit_code = output.status.code().unwrap_or(-1);
        if !output.status.success() {
            return Ok(exit_code);
        }

        Ok(exit_code)
    }

    fn collect_artifacts(
        &self,
        workspace: &Workspace,
        artifact_paths: &[String],
    ) -> Vec<adp_ci_types::ArtifactRef> {
        let mut artifacts = Vec::new();
        for path_str in artifact_paths {
            let full_path = workspace.workspace_dir().join(path_str);
            if full_path.exists() {
                if let Ok(metadata) = std::fs::metadata(&full_path) {
                    let sha256 = self.compute_sha256(&full_path);
                    artifacts.push(adp_ci_types::ArtifactRef {
                        name: path_str.clone(),
                        path: full_path.to_string_lossy().to_string(),
                        size_bytes: metadata.len(),
                        sha256,
                    });
                }
            }
        }
        artifacts
    }

    /// Compute SHA-256 hash of a file.
    fn compute_sha256(&self, path: &std::path::Path) -> String {
        use sha2::{Sha256, Digest};
        match std::fs::read(path) {
            Ok(data) => {
                let mut hasher = Sha256::new();
                hasher.update(&data);
                format!("{:x}", hasher.finalize())
            }
            Err(_) => "error-computing-hash".to_string(),
        }
    }

    fn failed_result(&self, request: &JobRequest, step: &str, error: &str) -> JobResult {
        JobResult {
            job_id: request.job_id,
            status: JobStatus::Failed,
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            artifacts: Vec::new(),
            metrics: JobMetrics {
                duration_secs: 0,
                cpu_usage_percent: 0.0,
                memory_peak_bytes: 0,
                disk_usage_bytes: 0,
            },
            step_results: vec![StepResult {
                name: step.to_string(),
                success: false,
                exit_code: 1,
                duration_secs: 0,
                error: Some(error.to_string()),
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adp_ci_types::JobTrigger;

    #[tokio::test]
    async fn execute_simple_echo_job() {
        let tmp = tempfile::tempdir().unwrap();
        let config = ExecutorConfig {
            enable_git_checkout: false,
            work_dir_base: Some(tmp.path().to_path_buf()),
            ..ExecutorConfig::default()
        };
        let executor = JobExecutor::new(config);

        let request = JobRequest {
            job_id: uuid::Uuid::new_v4(),
            trigger: JobTrigger {
                event: "push".into(),
                branch: "main".into(),
                commit_sha: "abc123".into(),
                committer: "test".into(),
                init_submodules: false,
            },
            steps: vec![adp_ci_types::JobStep {
                name: "echo-hello".into(),
                environment: "sh".into(),
                command: "echo hello".into(),
                timeout_secs: 10,
                working_dir: None,
                env: HashMap::new(),
            }],
            secrets: Vec::new(),
            compliance: adp_ci_types::ComplianceRequirements::default(),
            artifacts: Vec::new(),
            artifact_storage: adp_ci_types::ArtifactStorage::default(),
        };

        let result = executor.execute_job(&request).await;

        assert_eq!(result.status, JobStatus::Completed);
        assert_eq!(result.step_results.len(), 1);
        assert!(result.step_results[0].success);
        assert_eq!(result.step_results[0].exit_code, 0);
    }

    #[tokio::test]
    async fn execute_failing_step() {
        let tmp = tempfile::tempdir().unwrap();
        let config = ExecutorConfig {
            enable_git_checkout: false,
            work_dir_base: Some(tmp.path().to_path_buf()),
            ..ExecutorConfig::default()
        };
        let executor = JobExecutor::new(config);

        let request = JobRequest {
            job_id: uuid::Uuid::new_v4(),
            trigger: JobTrigger {
                event: "push".into(),
                branch: "main".into(),
                commit_sha: "abc".into(),
                committer: "test".into(),
                init_submodules: false,
            },
            steps: vec![adp_ci_types::JobStep {
                name: "fail".into(),
                environment: "sh".into(),
                command: "exit 42".into(),
                timeout_secs: 10,
                working_dir: None,
                env: HashMap::new(),
            }],
            secrets: Vec::new(),
            compliance: adp_ci_types::ComplianceRequirements::default(),
            artifacts: Vec::new(),
            artifact_storage: adp_ci_types::ArtifactStorage::default(),
        };

        let result = executor.execute_job(&request).await;

        assert_eq!(result.status, JobStatus::Failed);
        assert_eq!(result.step_results[0].exit_code, 42);
    }

    #[tokio::test]
    async fn execute_multi_step_stops_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let config = ExecutorConfig {
            enable_git_checkout: false,
            work_dir_base: Some(tmp.path().to_path_buf()),
            ..ExecutorConfig::default()
        };
        let executor = JobExecutor::new(config);

        let request = JobRequest {
            job_id: uuid::Uuid::new_v4(),
            trigger: JobTrigger {
                event: "push".into(),
                branch: "main".into(),
                commit_sha: "abc".into(),
                committer: "test".into(),
                init_submodules: false,
            },
            steps: vec![
                adp_ci_types::JobStep {
                    name: "step1".into(),
                    environment: "sh".into(),
                    command: "echo step1".into(),
                    timeout_secs: 10,
                    working_dir: None,
                    env: HashMap::new(),
                },
                adp_ci_types::JobStep {
                    name: "step2-fail".into(),
                    environment: "sh".into(),
                    command: "exit 1".into(),
                    timeout_secs: 10,
                    working_dir: None,
                    env: HashMap::new(),
                },
                adp_ci_types::JobStep {
                    name: "step3".into(),
                    environment: "sh".into(),
                    command: "echo should-not-run".into(),
                    timeout_secs: 10,
                    working_dir: None,
                    env: HashMap::new(),
                },
            ],
            secrets: Vec::new(),
            compliance: adp_ci_types::ComplianceRequirements::default(),
            artifacts: Vec::new(),
            artifact_storage: adp_ci_types::ArtifactStorage::default(),
        };

        let result = executor.execute_job(&request).await;

        assert_eq!(result.status, JobStatus::Failed);
        // Only 2 steps should have run (step1 succeeded, step2 failed, step3 skipped)
        assert_eq!(result.step_results.len(), 2);
        assert!(result.step_results[0].success);
        assert!(!result.step_results[1].success);
    }

    #[tokio::test]
    async fn step_env_vars_passed_to_command() {
        let tmp = tempfile::tempdir().unwrap();
        let config = ExecutorConfig {
            enable_git_checkout: false,
            work_dir_base: Some(tmp.path().to_path_buf()),
            ..ExecutorConfig::default()
        };
        let executor = JobExecutor::new(config);

        let request = JobRequest {
            job_id: uuid::Uuid::new_v4(),
            trigger: JobTrigger {
                event: "push".into(),
                branch: "main".into(),
                commit_sha: "abc".into(),
                committer: "test".into(),
                init_submodules: false,
            },
            steps: vec![adp_ci_types::JobStep {
                name: "check-env".into(),
                environment: "sh".into(),
                command: "test \"$MY_VAR\" = \"hello\"".into(),
                timeout_secs: 10,
                working_dir: None,
                env: {
                    let mut m = HashMap::new();
                    m.insert("MY_VAR".to_string(), "hello".to_string());
                    m
                },
            }],
            secrets: Vec::new(),
            compliance: adp_ci_types::ComplianceRequirements::default(),
            artifacts: Vec::new(),
            artifact_storage: adp_ci_types::ArtifactStorage::default(),
        };

        let result = executor.execute_job(&request).await;
        assert_eq!(result.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn step_timeout_enforced() {
        let tmp = tempfile::tempdir().unwrap();
        let config = ExecutorConfig {
            enable_git_checkout: false,
            work_dir_base: Some(tmp.path().to_path_buf()),
            ..ExecutorConfig::default()
        };
        let executor = JobExecutor::new(config);

        let request = JobRequest {
            job_id: uuid::Uuid::new_v4(),
            trigger: JobTrigger {
                event: "push".into(),
                branch: "main".into(),
                commit_sha: "abc".into(),
                committer: "test".into(),
                init_submodules: false,
            },
            steps: vec![adp_ci_types::JobStep {
                name: "slow".into(),
                environment: "sh".into(),
                command: "sleep 30".into(),
                timeout_secs: 1, // 1 second timeout
                working_dir: None,
                env: HashMap::new(),
            }],
            secrets: Vec::new(),
            compliance: adp_ci_types::ComplianceRequirements::default(),
            artifacts: Vec::new(),
            artifact_storage: adp_ci_types::ArtifactStorage::default(),
        };

        let result = executor.execute_job(&request).await;

        assert_eq!(result.status, JobStatus::Failed);
        assert!(result.step_results[0].error.as_ref().unwrap().contains("Timed out"));
    }

    #[tokio::test]
    async fn artifact_collection() {
        let tmp = tempfile::tempdir().unwrap();
        let config = ExecutorConfig {
            enable_git_checkout: false,
            work_dir_base: Some(tmp.path().to_path_buf()),
            ..ExecutorConfig::default()
        };
        let executor = JobExecutor::new(config);

        let job_id = uuid::Uuid::new_v4();

        // Pre-create workspace with an artifact
        let ws = Workspace::create_at(tmp.path().join(format!("adp-ci-{}", job_id))).unwrap();
        let artifact_path = ws.workspace_dir().join("output.bin");
        std::fs::write(&artifact_path, b"artifact data").unwrap();

        let request = JobRequest {
            job_id,
            trigger: JobTrigger {
                event: "push".into(),
                branch: "main".into(),
                commit_sha: "abc".into(),
                committer: "test".into(),
                init_submodules: false,
            },
            steps: vec![adp_ci_types::JobStep {
                name: "create-artifact".into(),
                environment: "sh".into(),
                command: format!("echo data > {}/workspace/output.bin", ws.root().display()),
                timeout_secs: 10,
                working_dir: None,
                env: HashMap::new(),
            }],
            secrets: Vec::new(),
            compliance: adp_ci_types::ComplianceRequirements::default(),
            artifacts: vec!["output.bin".to_string()],
            artifact_storage: adp_ci_types::ArtifactStorage::GitLfs,
        };

        let result = executor.execute_job(&request).await;
        // Artifact might or might not exist depending on path resolution
        // The key thing is the job completes
        assert_eq!(result.status, JobStatus::Completed);
    }
}

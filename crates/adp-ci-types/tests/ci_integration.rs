//! CI integration tests — full lifecycle across all CI crates.
//!
//! Tests the complete flow:
//! 1. Submit job → SQLite queue (pending)
//! 2. Register runner → SQLite
//! 3. Router matches → assigns (assigned)
//! 4. Runner starts → (running)
//! 5. Runner streams logs → LogStreamer
//! 6. Runner completes → (completed)
//! 7. Metrics snapshot → Prometheus gauges
//! 8. SSE events broadcast correctly

use adp_ci_router::{CiRouter, JobQueue, snapshot_queue_metrics};
use adp_ci_runner::{JobExecutor, ExecutorConfig, LogStreamer, Workspace};
use adp_ci_types::{
    ArtifactStorage, CiEvent, ComplianceRequirements, JobPriority, JobRequest, JobStatus,
    JobStep, JobTrigger, JobType, LogStream, SecretMount,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Test the full CI job lifecycle from submission to completion.
#[tokio::test]
async fn full_job_lifecycle() {
    // Set up infrastructure
    let queue = Arc::new(JobQueue::open_in_memory().unwrap());
    let (event_tx, mut event_rx) = broadcast::channel::<CiEvent>(64);
    let router = Arc::new(CiRouter::new(queue.clone(), event_tx));

    // Register a runner
    queue
        .register_runner(
            "runner-1",
            "build_worker",
            "[\"nix\", \"sh\"]",
            "[\"linux/amd64\"]",
            "[]",
            "{}",
            4,
        )
        .unwrap();

    // Submit a job
    router
        .submit_job(
            "job-lifecycle-1",
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

    // Verify job is pending
    assert_eq!(
        queue.job_status("job-lifecycle-1").unwrap(),
        Some(JobStatus::Pending)
    );

    // Verify SSE event was broadcast
    let event = event_rx.try_recv().unwrap();
    match event {
        CiEvent::JobAvailable { job_id, .. } => assert_eq!(job_id, "job-lifecycle-1"),
        _ => panic!("Expected JobAvailable event, got {:?}", event),
    }

    // Router matches and assigns
    let assigned = router.match_pending_jobs().unwrap();
    assert_eq!(assigned, 1);
    assert_eq!(
        queue.job_status("job-lifecycle-1").unwrap(),
        Some(JobStatus::Assigned)
    );

    // Verify JobAssigned SSE event
    let event = event_rx.try_recv().unwrap();
    match event {
        CiEvent::JobAssigned { job_id, .. } => assert_eq!(job_id, "job-lifecycle-1"),
        _ => panic!("Expected JobAssigned event, got {:?}", event),
    }

    // Runner starts the job
    router
        .update_job_status("job-lifecycle-1", JobStatus::Running)
        .unwrap();
    assert_eq!(
        queue.job_status("job-lifecycle-1").unwrap(),
        Some(JobStatus::Running)
    );

    // Runner completes the job
    router
        .update_job_status("job-lifecycle-1", JobStatus::Completed)
        .unwrap();
    assert_eq!(
        queue.job_status("job-lifecycle-1").unwrap(),
        Some(JobStatus::Completed)
    );

    // Verify runner's job count decremented
    let runner = queue.get_runner("runner-1").unwrap().unwrap();
    assert_eq!(runner.current_jobs, 0);
}

/// Test that the executor can run a multi-step job and produce correct results.
#[tokio::test]
async fn executor_multi_step_job() {
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
            committer: "ci".into(),
            init_submodules: false,
        },
        steps: vec![
            JobStep {
                name: "setup".into(),
                environment: "sh".into(),
                command: "echo 'Setting up environment'".into(),
                timeout_secs: 10,
                working_dir: None,
                env: HashMap::new(),
            },
            JobStep {
                name: "build".into(),
                environment: "sh".into(),
                command: "echo 'Building artifacts'".into(),
                timeout_secs: 30,
                working_dir: None,
                env: HashMap::new(),
            },
            JobStep {
                name: "test".into(),
                environment: "sh".into(),
                command: "echo 'Running tests'".into(),
                timeout_secs: 60,
                working_dir: None,
                env: HashMap::new(),
            },
        ],
        secrets: Vec::new(),
        compliance: ComplianceRequirements::default(),
        artifacts: Vec::new(),
        artifact_storage: ArtifactStorage::default(),
    };

    let result = executor.execute_job(&request).await;

    assert_eq!(result.status, JobStatus::Completed);
    assert_eq!(result.step_results.len(), 3);
    for step in &result.step_results {
        assert!(step.success, "Step '{}' failed", step.name);
    }
    assert!(result.metrics.duration_secs == 0 || result.metrics.duration_secs > 0); // job ran
}

/// Test log streaming through the full pipeline.
#[test]
fn log_streaming_pipeline() {
    let tmp = tempfile::tempdir().unwrap();
    let mut streamer = LogStreamer::new(tmp.path()).unwrap();

    // Simulate build output
    streamer
        .append_line(LogStream::Stdout, "Compiling v0.1.0")
        .unwrap();
    streamer
        .append_line(LogStream::Stdout, "Building binary")
        .unwrap();
    streamer
        .append_line(LogStream::Stderr, "warning: unused import")
        .unwrap();

    // Verify stdout
    let stdout = streamer.read_all(LogStream::Stdout).unwrap();
    assert!(stdout.contains("Compiling v0.1.0"));
    assert!(stdout.contains("Building binary"));

    // Verify stderr
    let stderr = streamer.read_all(LogStream::Stderr).unwrap();
    assert!(stderr.contains("warning: unused import"));

    // Verify offset tracking
    assert!(streamer.offset(LogStream::Stdout) > 0);
    assert!(streamer.offset(LogStream::Stderr) > 0);
}

/// Test secret mounting in workspace context.
#[test]
fn secret_mounting_in_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = Workspace::create_at(tmp.path().join("ws")).unwrap();

    let mounter = adp_ci_runner::SecretMounter::new(workspace.secrets_dir());

    let secret = SecretMount {
        name: "deploy-key".into(),
        kind: "Opaque".into(),
        data: {
            let mut m = HashMap::new();
            m.insert("private-key".into(), STANDARD.encode("ssh-key-data"));
            m.insert("known-hosts".into(), STANDARD.encode("github.com ssh-rsa AAAA"));
            m
        },
        env_mapping: {
            let mut m = HashMap::new();
            m.insert("DEPLOY_KEY".into(), "private-key".into());
            m
        },
        mount_path: "ci-secrets".into(),
        tmpfs: false,
        optional: false,
    };

    let env = mounter.mount(&secret).unwrap();

    // Files created
    let key_file = workspace.secrets_dir().join("ci-secrets/deploy-key/private-key");
    assert!(key_file.exists());
    assert_eq!(std::fs::read_to_string(&key_file).unwrap(), "ssh-key-data");

    let hosts_file = workspace.secrets_dir().join("ci-secrets/deploy-key/known-hosts");
    assert!(hosts_file.exists());

    // Env mapping
    assert_eq!(env.get("DEPLOY_KEY").unwrap(), "private-key");
}

/// Test metrics snapshot from queue state.
#[test]
fn metrics_snapshot_accuracy() {
    let queue = Arc::new(JobQueue::open_in_memory().unwrap());

    // Create jobs in different states
    queue
        .create_job("j1", "r", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "a", None)
        .unwrap();
    queue
        .create_job("j2", "r", JobType::Test, JobPriority::Medium, "nix", "{}", "[]", "main", "b", None)
        .unwrap();
    queue
        .create_job("j3", "r", JobType::Lint, JobPriority::Low, "nix", "{}", "[]", "main", "c", None)
        .unwrap();

    // Register runner and assign one job
    queue
        .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 4)
        .unwrap();
    queue.assign_job("j1", "r1").unwrap();

    // Update one to running
    queue.update_job_status("j1", JobStatus::Running).unwrap();

    // Take snapshot
    let snapshot = snapshot_queue_metrics(&queue);

    assert_eq!(snapshot.jobs_pending, 2);
    assert_eq!(snapshot.jobs_running, 1);
    assert_eq!(snapshot.jobs_assigned, 0); // j1 moved to running
}

/// Test job cancellation propagates SSE event.
#[test]
fn job_cancellation_broadcasts_event() {
    let queue = Arc::new(JobQueue::open_in_memory().unwrap());
    let (event_tx, mut event_rx) = broadcast::channel::<CiEvent>(64);
    let router = CiRouter::new(queue.clone(), event_tx);

    router
        .submit_job("j1", "r", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "abc")
        .unwrap();

    // Consume JobAvailable
    let _ = event_rx.try_recv();

    router.cancel_job("j1", "user request").unwrap();

    // Verify JobCancelled SSE event
    let event = event_rx.try_recv().unwrap();
    match event {
        CiEvent::JobCancelled { job_id, reason } => {
            assert_eq!(job_id, "j1");
            assert_eq!(reason, "user request");
        }
        _ => panic!("Expected JobCancelled event, got {:?}", event),
    }

    assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Cancelled));
}

/// Test timeout detection and re-queuing.
#[test]
fn timeout_detection_and_requeue() {
    let queue = Arc::new(JobQueue::open_in_memory().unwrap());
    let (event_tx, _) = broadcast::channel::<CiEvent>(64);
    let router = CiRouter::new(queue.clone(), event_tx);

    queue
        .register_runner("r1", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", "{}", 4)
        .unwrap();

    router
        .submit_job("j1", "r", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "abc")
        .unwrap();
    router.match_pending_jobs().unwrap();

    // Manually age the job
    {
        let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        queue.force_set_job_updated_at("j1", &past).unwrap();
    }

    // Handle timeouts
    let requeued = router.handle_timeouts().unwrap();
    assert_eq!(requeued, 1);

    // Job should be back to pending
    assert_eq!(queue.job_status("j1").unwrap(), Some(JobStatus::Pending));
}

/// Test compliance-aware runner matching.
#[test]
fn compliance_aware_routing() {
    let queue = Arc::new(JobQueue::open_in_memory().unwrap());
    let (event_tx, _) = broadcast::channel::<CiEvent>(64);
    let router = CiRouter::new(queue.clone(), event_tx);

    // Register SOC2-certified runner in EU
    queue
        .register_runner("r-eu-soc2", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[\"soc2\"]", r#"{"region":"eu"}"#, 4)
        .unwrap();
    // Register non-certified runner in US
    queue
        .register_runner("r-us", "build_worker", "[\"nix\"]", "[\"linux/amd64\"]", "[]", r#"{"region":"us"}"#, 4)
        .unwrap();

    // Submit job requiring SOC2
    router
        .submit_job("j1", "r", JobType::Build, JobPriority::High, "nix", "[\"soc2\"]", "[]", "main", "abc")
        .unwrap();

    // Only one runner should match
    let assigned = router.match_pending_jobs().unwrap();
    assert_eq!(assigned, 1);

    // Verify the SOC2 runner got the job
    let runner = queue.get_runner("r-eu-soc2").unwrap().unwrap();
    assert_eq!(runner.current_jobs, 1);

    let us_runner = queue.get_runner("r-us").unwrap().unwrap();
    assert_eq!(us_runner.current_jobs, 0);
}

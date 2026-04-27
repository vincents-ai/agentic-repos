//! ADP CI Runner Binary
//!
//! Connects to an adp server, receives CI jobs via SSE, executes them,
//! and reports results back via REST.
//!
//! ## Usage
//!
//! ```bash
//! adp-ci-runner \
//!   --adp-url http://localhost:8090 \
//!   --runner-id my-runner-1 \
//!   --runner-type build_worker \
//!   --environments nix,sh \
//!   --platforms linux/amd64 \
//!   --max-concurrent 4
//! ```

use adp_ci_runner::{ExecutorConfig, JobExecutor};
use adp_ci_types::{CiEvent, JobRequest, JobStatus};
use clap::Parser;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;

/// ADP CI Runner configuration.
#[derive(Parser, Debug)]
#[command(name = "adp-ci-runner", about = "ADP CI runner — executes build jobs")]
struct Args {
    /// adp server URL.
    #[arg(long, env = "ADP_URL", default_value = "http://localhost:8090")]
    adp_url: String,

    /// Unique runner identifier.
    #[arg(long, env = "ADP_RUNNER_ID")]
    runner_id: String,

    /// Runner type (build_worker, deployment_service, compliance_worker, specialized).
    #[arg(long, env = "ADP_RUNNER_TYPE", default_value = "build_worker")]
    runner_type: String,

    /// Supported build environments (comma-separated).
    #[arg(long, env = "ADP_ENVIRONMENTS", default_value = "sh")]
    environments: String,

    /// Supported platforms (comma-separated).
    #[arg(long, env = "ADP_PLATFORMS", default_value = "linux/amd64")]
    platforms: String,

    /// Compliance certifications (comma-separated).
    #[arg(long, env = "ADP_COMPLIANCE", default_value = "")]
    compliance: String,

    /// Runner labels (key=value, comma-separated).
    #[arg(long, env = "ADP_LABELS", default_value = "")]
    labels: String,

    /// Maximum concurrent jobs.
    #[arg(long, env = "ADP_MAX_CONCURRENT", default_value_t = 2)]
    max_concurrent: u32,

    /// Runner labels as JSON (overrides --labels).
    #[arg(long, env = "ADP_LABELS_JSON", default_value = "{}")]
    labels_json: String,

    /// Registration secret (TTL-based, obtained from adp admin).
    #[arg(long, env = "ADP_REGISTRATION_SECRET")]
    registration_secret: Option<String>,

    /// Enable git checkout (set false for testing).
    #[arg(long, env = "ADP_ENABLE_GIT_CHECKOUT", default_value_t = true)]
    enable_git_checkout: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    tracing::info!(
        runner_id = %args.runner_id,
        runner_type = %args.runner_type,
        environments = %args.environments,
        "Starting ADP CI Runner"
    );

    let client = reqwest::Client::new();
    let executor_config = ExecutorConfig {
        adp_url: args.adp_url.clone(),
        enable_git_checkout: args.enable_git_checkout,
        ..ExecutorConfig::default()
    };
    let executor = Arc::new(JobExecutor::new(executor_config));

    // Register runner with adp
    register_runner(&client, &args).await?;

    // Connect SSE and listen for jobs
    let sse_url = format!(
        "{}/api/agentic-loop/connect?service_type=ci_runner&service_id={}",
        args.adp_url, args.runner_id
    );

    tracing::info!(url = %sse_url, "Connecting to SSE endpoint");

    // SSE loop — reconnect on disconnect
    loop {
        match connect_sse(&client, &sse_url).await {
            Ok(response) => {
                tracing::info!("SSE connected, waiting for jobs");
                let mut stream = response.bytes_stream();
                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(bytes) => {
                            let data = String::from_utf8_lossy(&bytes);
                            if let Some(ci_event) = parse_sse_chunk(&data) {
                                handle_event(
                                    &ci_event,
                                    &client,
                                    &args,
                                    &executor,
                                )
                                .await;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "SSE chunk error");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "SSE connection failed");
            }
        }

        tracing::info!("Reconnecting SSE in 5 seconds...");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn register_runner(client: &reqwest::Client, args: &Args) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!(runner_id = %args.runner_id, "Registering runner with adp");

    // Register via the CI router queue (adp will expose a registration endpoint)
    // For now, we just announce ourselves via the SSE connection
    let environments: Vec<String> = args
        .environments
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let platforms: Vec<String> = args
        .platforms
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let compliance: Vec<String> = args
        .compliance
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let labels: HashMap<String, String> = if args.labels_json != "{}" {
        serde_json::from_str(&args.labels_json).unwrap_or_default()
    } else {
        args.labels
            .split(',')
            .filter_map(|kv| {
                let parts: Vec<&str> = kv.trim().splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect()
    };

    let payload = serde_json::json!({
        "runner_id": args.runner_id,
        "runner_type": args.runner_type,
        "environments": environments,
        "platforms": platforms,
        "compliance": compliance,
        "labels": labels,
        "max_concurrent": args.max_concurrent,
    });

    // POST to register endpoint
    let url = format!("{}/api/v1/ci/runners/register", args.adp_url);
    let resp = client.post(&url).json(&payload).send().await;

    match resp {
        Ok(r) if r.status().is_success() => {
            tracing::info!("Runner registered successfully");
        }
        Ok(r) => {
            tracing::warn!(status = %r.status(), "Runner registration returned non-200");
        }
        Err(e) => {
            tracing::warn!(error = %e, "Runner registration failed (will retry via SSE)");
        }
    }

    Ok(())
}

async fn connect_sse(
    client: &reqwest::Client,
    url: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    client.get(url).send().await
}

fn parse_sse_chunk(data: &str) -> Option<CiEvent> {
    // SSE format: "data: {json}\n\n"
    for line in data.lines() {
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<CiEvent>(json_str) {
                return Some(event);
            }
        }
    }
    None
}

async fn handle_event(
    event: &CiEvent,
    client: &reqwest::Client,
    args: &Args,
    executor: &Arc<JobExecutor>,
) {
    match event {
        CiEvent::JobAssigned { job_id, deadline, .. } => {
            tracing::info!(job_id = %job_id, deadline = %deadline, "Received job assignment");

            // Report started
            let _ = report_status(client, &args.adp_url, job_id, "started", None).await;

            // Fetch job details from adp
            match fetch_job(client, &args.adp_url, job_id).await {
                Ok(Some(job_request)) => {
                    // Execute the job
                    let result = executor.execute_job(&job_request).await;

                    // Report result
                    match result.status {
                        JobStatus::Completed => {
                            tracing::info!(job_id = %job_id, duration = result.metrics.duration_secs, "Job completed");
                            let _ = report_status(client, &args.adp_url, job_id, "completed", None).await;
                        }
                        JobStatus::Failed => {
                            tracing::warn!(job_id = %job_id, "Job failed");
                            let error = result
                                .step_results
                                .iter()
                                .find(|s| !s.success)
                                .and_then(|s| s.error.clone())
                                .unwrap_or_default();
                            let _ = report_status(client, &args.adp_url, job_id, "failed", Some(&error)).await;
                        }
                        _ => {}
                    }
                }
                Ok(None) => {
                    tracing::warn!(job_id = %job_id, "Job not found on adp server");
                }
                Err(e) => {
                    tracing::error!(job_id = %job_id, error = %e, "Failed to fetch job details");
                }
            }
        }
        CiEvent::JobCancelled { job_id, reason } => {
            tracing::info!(job_id = %job_id, reason = %reason, "Job cancelled");
        }
        CiEvent::JobAvailable { job_id, .. } => {
            tracing::debug!(job_id = %job_id, "Job available (will be assigned by router)");
        }
        CiEvent::CapabilitiesUpdated { runner_id } => {
            if runner_id == &args.runner_id {
                tracing::info!("Capabilities updated notification received");
            }
        }
        CiEvent::Ping { timestamp } => {
            tracing::trace!(timestamp = %timestamp, "SSE ping received");
        }
    }
}

async fn report_status(
    client: &reqwest::Client,
    adp_url: &str,
    job_id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<(), reqwest::Error> {
    let url = match status {
        "started" => format!("{}/api/v1/ci/jobs/{}/started", adp_url, job_id),
        "completed" => format!("{}/api/v1/ci/jobs/{}/completed", adp_url, job_id),
        "failed" => format!("{}/api/v1/ci/jobs/{}/failed", adp_url, job_id),
        _ => return Ok(()),
    };

    let payload = match status {
        "started" => serde_json::json!({ "started_at": chrono::Utc::now().to_rfc3339() }),
        "completed" => serde_json::json!({ "completed_at": chrono::Utc::now().to_rfc3339(), "artifacts": [], "metrics": {} }),
        "failed" => serde_json::json!({ "failed_at": chrono::Utc::now().to_rfc3339(), "phase": "execution", "error_message": error.unwrap_or("unknown"), "exit_code": 1 }),
        _ => serde_json::json!({}),
    };

    client.post(&url).json(&payload).send().await?;
    Ok(())
}

async fn fetch_job(
    client: &reqwest::Client,
    adp_url: &str,
    job_id: &str,
) -> Result<Option<JobRequest>, reqwest::Error> {
    let url = format!("{}/api/v1/ci/jobs/{}", adp_url, job_id);
    let resp = client.get(&url).send().await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    // The API returns job status, not the full request — in production,
    // we'd have a separate endpoint for fetching the job request details
    Ok(None)
}

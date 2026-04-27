//! SeaweedFS fixture — manages a podman container running SeaweedFS with S3 gateway.
//!
//! SeaweedFS exposes an S3-compatible API on port 8333. This fixture starts it
//! via podman, waits for readiness, and creates a test bucket.

use std::process::Command;
use std::time::{Duration, Instant};
use tracing::info;

fn random_suffix() -> String {
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:08x}", d.as_nanos() % 0x100000000)
}

/// A running SeaweedFS container managed by podman.
pub struct SeaweedFs {
    container_name: String,
    s3_port: u16,
}

impl SeaweedFs {
    /// Start a new SeaweedFS container with S3 gateway enabled.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let container_name = format!("e2e-swfs-{}", random_suffix());
        let s3_port = portpicker::pick_unused_port().ok_or("no unused port for SeaweedFS S3")?;

        info!(
            "Starting SeaweedFS container {} (S3 :{})",
            container_name, s3_port
        );

        let output = Command::new("podman")
            .args([
                "run",
                "-d",
                "--name",
                &container_name,
                "-p",
                &format!("127.0.0.1:{}:8333", s3_port),
                "docker.io/chrislusf/seaweedfs:latest",
                "server",
                "-s3",
                "-filer",
                "-dir=/tmp",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("podman run seaweedfs failed: {}", stderr).into());
        }

        let swfs = Self {
            container_name,
            s3_port,
        };

        swfs.wait_ready().await?;
        info!("SeaweedFS ready on S3 port {}", s3_port);
        Ok(swfs)
    }

    /// Wait for the S3 endpoint to respond to ListBuckets (max 30s).
    async fn wait_ready(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let start = Instant::now();
        let timeout = Duration::from_secs(30);
        let url = format!("http://127.0.0.1:{}/", self.s3_port);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?;

        loop {
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() || resp.status().as_u16() == 403 {
                    // Extra wait for S3 gateway to fully initialise after HTTP responds
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    return Ok(());
                }
            }
            if start.elapsed() > timeout {
                return Err(format!("SeaweedFS not ready after {:?}", timeout).into());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// The S3 endpoint URL (e.g., "http://127.0.0.1:12345").
    pub fn s3_endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.s3_port)
    }

    /// Build an aws_sdk_s3::Client pointed at this SeaweedFS instance.
    ///
    /// Uses path-style addressing and anonymous credentials — SeaweedFS accepts
    /// unsigned requests when no IAM config is present.
    pub async fn s3_client(&self) -> aws_sdk_s3::Client {
        let config = aws_sdk_s3::config::Builder::new()
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .endpoint_url(self.s3_endpoint())
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                "anonymous",
                "anonymous",
                None,
                None,
                "static",
            ))
            .force_path_style(true)
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .build();

        aws_sdk_s3::Client::from_conf(config)
    }

    /// Create a bucket in SeaweedFS.
    pub async fn create_bucket(
        &self,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.s3_client().await;
        client.create_bucket().bucket(name).send().await?;
        info!("Created bucket '{}' in SeaweedFS", name);
        Ok(())
    }

    /// Stop and remove the container.
    pub fn stop(&self) {
        info!("Stopping SeaweedFS container {}", self.container_name);
        let _ = Command::new("podman")
            .args(["stop", "-t", "3", &self.container_name])
            .output();
        let _ = Command::new("podman")
            .args(["rm", "-f", &self.container_name])
            .output();
    }
}

impl Drop for SeaweedFs {
    fn drop(&mut self) {
        self.stop();
    }
}

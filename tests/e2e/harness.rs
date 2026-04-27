//! E2E test harness — manages agentic-repos server subprocess lifecycle.
//!
//! Spins up a real agentic-repos binary and runs tests against it over HTTP.
//! All services use in-memory stores — no external database needed.
//!
//! Usage:
//! ```no_run
//! let harness = E2eHarness::new().await.unwrap();
//! let resp = harness.get("/api/v1/health").await;
//! ```

use reqwest::StatusCode;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tracing::{debug, error, info, warn};

/// Full-stack E2E test harness.
///
/// Manages the lifecycle of a agentic-repos server subprocess with temporary storage.
/// Each test should create its own harness instance for full isolation.
pub struct E2eHarness {
    child: Mutex<Option<Child>>,
    pub http_port: u16,
    pub ssh_port: u16,
    pub storage_root: PathBuf,
    /// Directory containing the test SSH keypair (id_ed25519 + id_ed25519.pub)
    pub ssh_keys_dir: PathBuf,
    _config_dir: TempDir,
}

impl E2eHarness {
    /// Create a new harness: build binary, bootstrap system repo, start server, wait for healthy.
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Init tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::INFO.into()),
            )
            .try_init();

        let http_port =
            portpicker::pick_unused_port().ok_or("no unused port available for HTTP")?;
        let ssh_port = portpicker::pick_unused_port().ok_or("no unused port available for SSH")?;

        let config_dir = TempDir::new()?;
        let storage_root = config_dir.path().join("repositories");
        std::fs::create_dir_all(&storage_root)?;

        // Build the binary
        let binary_path = Self::build_binary()?;

        // Bootstrap the system repo
        info!("Bootstrapping system repo at {:?}", storage_root);
        let init_output = Command::new(&binary_path)
            .args(["init", "--storage"])
            .arg(&storage_root)
            .output()?;

        if !init_output.status.success() {
            let stderr = String::from_utf8_lossy(&init_output.stderr);
            return Err(format!("agentic-repos init failed: {}", stderr).into());
        }

        // Generate SSH test keypair directly in config_dir and write authorized_keys
        let ak_path = config_dir.path().join("authorized_keys");
        let host_key_path = config_dir.path().join("ssh_host_key");
        let ssh_private_key = config_dir.path().join("id_ed25519");
        let ssh_public_key = config_dir.path().join("id_ed25519.pub");

        // Generate keypair via ssh-keygen
        let keygen_output = Command::new("ssh-keygen")
            .args([
                "-t",
                "ed25519",
                "-N",
                "",
                "-f",
                ssh_private_key.to_str().unwrap(),
                "-C",
                "git@e2e-test",
            ])
            .output()?;
        if !keygen_output.status.success() {
            return Err(format!(
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&keygen_output.stderr)
            )
            .into());
        }

        // Write authorized_keys in ADP format: "username:ssh-type base64 comment"
        let pub_key_line = std::fs::read_to_string(&ssh_public_key)?.trim().to_string();
        info!("Generated SSH test key: {}", pub_key_line);
        std::fs::write(&ak_path, format!("git:{}\n", pub_key_line))?;

        let ssh_keys_dir = config_dir.path().to_path_buf();

        // Update refs/adp/config/server in the system repo with our ports and paths
        crate::ssh_keys::configure_ssh_in_system_repo(
            &storage_root,
            http_port,
            ssh_port,
            &ak_path,
            &host_key_path,
        )?;
        info!(
            "Configured git-refs: HTTP={}, SSH={}, ak={:?}, host_key={:?}",
            http_port, ssh_port, ak_path, host_key_path
        );

        // Start the server subprocess — config comes from git-refs in the system repo
        info!(
            "Starting agentic-repos server on HTTP :{}, SSH :{}",
            http_port, ssh_port
        );
        let child = Command::new(&binary_path)
            .env("AGENTIC_REPOS_STORAGE_ROOT", &storage_root)
            .env("RUST_LOG", "info,agentic_repos=debug")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        let mut harness = Self {
            child: Mutex::new(Some(child)),
            http_port,
            ssh_port,
            storage_root,
            ssh_keys_dir,
            _config_dir: config_dir,
        };

        // Wait for health
        harness.wait_healthy().await?;
        Ok(harness)
    }

    fn build_binary() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let binary_path = PathBuf::from("target/debug/agentic-repos");
        info!("Building agentic-repos binary...");
        let build_output = Command::new("cargo")
            .args(["build", "--bin", "agentic-repos"])
            .output()?;

        if !build_output.status.success() {
            let stderr = String::from_utf8_lossy(&build_output.stderr);
            return Err(format!("cargo build failed: {}", stderr).into());
        }

        if !binary_path.exists() {
            return Err(format!("Binary not found at {:?}", binary_path).into());
        }

        Ok(binary_path)
    }

    /// Poll GET /health until 200 (max 30s).
    async fn wait_healthy(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;

        let url = format!("http://127.0.0.1:{}/api/v1/health", self.http_port);
        let start = Instant::now();
        let timeout = Duration::from_secs(30);

        loop {
            match client.get(&url).send().await {
                Ok(resp) if resp.status() == StatusCode::OK => {
                    info!(
                        "Server healthy on port {} after {:?}",
                        self.http_port,
                        start.elapsed()
                    );
                    return Ok(());
                }
                Ok(resp) => {
                    debug!("Health check returned {}, retrying...", resp.status());
                }
                Err(e) => {
                    debug!("Health check connection failed: {}, retrying...", e);
                }
            }

            // Check if child is still alive
            if let Some(ref mut child) = self.child.get_mut().unwrap() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        return Err(
                            format!("Server exited prematurely with status: {}", status).into()
                        );
                    }
                    Ok(None) => {}
                    Err(e) => {
                        return Err(format!("Failed to check server status: {}", e).into());
                    }
                }
            }

            if start.elapsed() > timeout {
                return Err(format!(
                    "Server not healthy after {:?} on port {}",
                    timeout, self.http_port
                )
                .into());
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Base URL for HTTP requests.
    pub fn http_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.http_port)
    }

    /// Make a GET request to the server.
    pub async fn get(&self, path: &str) -> reqwest::Response {
        let url = format!("{}{}", self.http_url(), path);
        self.client().get(&url).send().await.unwrap()
    }

    /// Make a POST request with a JSON body.
    pub async fn post_json(&self, path: &str, body: &serde_json::Value) -> reqwest::Response {
        let url = format!("{}{}", self.http_url(), path);
        self.client().post(&url).json(body).send().await.unwrap()
    }

    /// Make a POST request with no body.
    pub async fn post(&self, path: &str) -> reqwest::Response {
        let url = format!("{}{}", self.http_url(), path);
        self.client().post(&url).send().await.unwrap()
    }

    pub async fn delete(&self, path: &str) -> reqwest::Response {
        let url = format!("{}{}", self.http_url(), path);
        self.client().delete(&url).send().await.unwrap()
    }

    fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client")
    }

    /// Stop the server gracefully.
    pub fn stop(&self) {
        let mut guard = self.child.lock().unwrap();
        if let Some(ref mut child) = *guard {
            let pid = child.id();
            info!("Stopping agentic-repos server (PID: {:?})", pid);

            // SIGTERM via kill command
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .output();

            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        info!("Server stopped gracefully");
                        break;
                    }
                    Ok(None) if Instant::now() > deadline => {
                        warn!("Server did not stop in 10s, killing");
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    Ok(None) => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        error!("Error checking server status: {}", e);
                        let _ = child.kill();
                        break;
                    }
                }
            }
        }
    }
}

impl Drop for E2eHarness {
    fn drop(&mut self) {
        self.stop();
    }
}

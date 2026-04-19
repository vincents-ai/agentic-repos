//! SSH key helpers for E2E tests.
//!
//! Generates ed25519 keypairs, writes authorized_keys in ADP format, scans
//! the server's host key into a known_hosts file, and updates refs/adp/config/server
//! in the system repo so the server picks up the test SSH port and key paths.

use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Test SSH keypair written to disk for use with system git.
pub struct SshTestKeys {
    pub username: String,
    pub private_key_path: PathBuf,
    pub authorized_keys_path: PathBuf,
    pub _dir: TempDir,
}

impl SshTestKeys {
    /// Generate an ed25519 keypair using ssh-keygen (available in NixOS devshell).
    pub fn generate(username: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let dir = TempDir::new()?;
        let private_key_path = dir.path().join("id_ed25519");
        let authorized_keys_path = dir.path().join("authorized_keys");

        // Generate keypair via ssh-keygen (no passphrase)
        let output = Command::new("ssh-keygen")
            .args([
                "-t",
                "ed25519",
                "-N",
                "", // no passphrase
                "-f",
                private_key_path.to_str().unwrap(),
                "-C",
                &format!("{}@e2e-test", username),
            ])
            .output()?;

        if !output.status.success() {
            return Err(format!(
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }

        // Read the generated public key
        let pub_key_path = dir.path().join("id_ed25519.pub");
        let pub_key_line = std::fs::read_to_string(&pub_key_path)?;
        let pub_key_line = pub_key_line.trim();

        // Write authorized_keys in ADP format: "username:<full openssh public key line>"
        let ak_line = format!("{}:{}\n", username, pub_key_line);
        std::fs::write(&authorized_keys_path, &ak_line)?;

        Ok(Self {
            username: username.to_string(),
            private_key_path,
            authorized_keys_path,
            _dir: dir,
        })
    }
}

/// Write an authorized_keys file for the given username to the given path.
/// Used by the harness after init to set up the server's key store.
pub fn generate_e2e_authorized_keys(
    username: &str,
    ak_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // We just write an empty authorized_keys so the server starts cleanly.
    // Individual SSH tests generate their own keys and append them.
    let content = format!("# E2E test authorized_keys for user: {}\n", username);
    std::fs::write(ak_path, content)?;
    Ok(())
}

/// Update refs/adp/config/server in the system repo with all E2E test settings.
/// Called by the harness after `agentic-repos init`.
pub fn configure_ssh_in_system_repo(
    storage_root: &Path,
    http_port: u16,
    ssh_port: u16,
    authorized_keys_path: &Path,
    host_key_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use agentic_repos::adp::bootstrap::SYSTEM_REPO_PATH;
    use agentic_repos::adp::config::ServerConfig;
    use agentic_repos::adp::storage::AdpStore;
    use agentic_repos::adp::SERVER_CONFIG_REF;

    let system_repo = storage_root.join(SYSTEM_REPO_PATH);

    // Read the config written by agentic-repos init
    let mut cfg: ServerConfig = AdpStore::read_ref_blob(&system_repo, SERVER_CONFIG_REF)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    // Set all E2E test overrides
    cfg.http_host = "127.0.0.1".to_string();
    cfg.http_port = http_port;
    cfg.ssh_port = ssh_port;
    cfg.ssh_authorized_keys_path = authorized_keys_path.to_path_buf();
    cfg.ssh_host_key_path = host_key_path.to_path_buf();
    cfg.feature_flags.insert("web_ui".to_string(), true);
    cfg.feature_flags.insert("rbac".to_string(), true);
    cfg.feature_flags.insert("git_lfs".to_string(), true);
    cfg.feature_flags.insert("webhooks".to_string(), true);
    cfg.feature_flags.insert("monitoring".to_string(), true);

    // Write back to refs/adp/config/server
    let value = serde_json::to_value(&cfg)?;
    AdpStore::write_ref_blob(&system_repo, SERVER_CONFIG_REF, &value)?;

    // Verify the write took effect
    let verify: ServerConfig = AdpStore::read_ref_blob(&system_repo, SERVER_CONFIG_REF)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    if verify.http_port != http_port || verify.ssh_port != ssh_port {
        return Err(format!(
            "git-refs config write failed: expected http={}, ssh={}, got http={}, ssh={}",
            http_port, ssh_port, verify.http_port, verify.ssh_port
        )
        .into());
    }

    // Write SSH keys to git-refs: refs/adp/users/git/ssh_keys
    let ak_content = std::fs::read_to_string(authorized_keys_path)?;
    let mut ssh_keys: Vec<String> = Vec::new();
    for line in ak_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(key_part) = line.splitn(2, ':').nth(1) {
            let key_part = key_part.trim();
            if key_part.starts_with("ssh-") {
                ssh_keys.push(key_part.to_string());
            }
        }
    }
    if !ssh_keys.is_empty() {
        let keys_value = serde_json::to_value(&ssh_keys)?;
        AdpStore::write_ref_blob(&system_repo, "refs/adp/users/git/ssh_keys", &keys_value)?;
    }

    // Also enable LFS in refs/adp/config/lfs
    use agentic_repos::adp::lfs_config::{LfsConfig, LFS_CONFIG_REF};
    let mut lfs_cfg: LfsConfig = AdpStore::read_ref_blob(&system_repo, LFS_CONFIG_REF)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    lfs_cfg.enabled = true;
    lfs_cfg.base_url = format!("http://127.0.0.1:{}", http_port);
    lfs_cfg.jwt_secret = Some("e2e-test-lfs-secret".to_string());
    let lfs_value = serde_json::to_value(&lfs_cfg)?;
    AdpStore::write_ref_blob(&system_repo, LFS_CONFIG_REF, &lfs_value)?;

    Ok(())
}

/// Scan the server's SSH host key and write it to a known_hosts file.
/// Must be called AFTER the server is up and listening on the SSH port.
/// `known_hosts_path` should be in a temp dir managed by the caller.
pub fn scan_host_key(
    ssh_port: u16,
    known_hosts_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Use ssh-keyscan to get the server's host key
    let output = Command::new("ssh-keyscan")
        .args(["-p", &ssh_port.to_string(), "-T", "5", "127.0.0.1"])
        .output()?;

    if !output.status.success() || output.stdout.is_empty() {
        return Err(format!(
            "ssh-keyscan failed (port {}): {}",
            ssh_port,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    std::fs::write(known_hosts_path, &output.stdout)?;
    Ok(())
}

/// Run a git command over SSH against the test server.
/// Uses GIT_SSH_COMMAND to inject the test key and point at a known_hosts file.
pub fn git_ssh(
    keys: &SshTestKeys,
    ssh_port: u16,
    known_hosts_path: &Path,
    args: &[&str],
    cwd: &Path,
) -> std::process::Output {
    let ssh_cmd = format!(
        "ssh -i {} -p {} -o UserKnownHostsFile={} -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=5",
        keys.private_key_path.display(),
        ssh_port,
        known_hosts_path.display(),
    );

    let mut child = Command::new("git")
        .env("GIT_SSH_COMMAND", &ssh_cmd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("git command failed to spawn");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                return std::process::Output {
                    status,
                    stdout,
                    stderr,
                };
            }
            Ok(None) if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                return std::process::Output {
                    status: std::process::ExitStatus::from_raw(1 << 8), // exit code 1 on Unix
                    stdout,
                    stderr,
                };
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => panic!("failed to wait on git: {}", e),
        }
    }
}

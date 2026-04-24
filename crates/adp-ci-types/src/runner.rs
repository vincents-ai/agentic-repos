//! Runner types — capabilities, status, resources.

use serde::{Deserialize, Serialize};

/// Runner classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerType {
    /// General build worker.
    BuildWorker,
    /// Deployment service.
    DeploymentService,
    /// Compliance/certification runner.
    ComplianceWorker,
    /// Specialized runner (custom).
    Specialized,
}

/// Runner lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStatus {
    /// Ready to accept jobs.
    Idle,
    /// Currently executing one or more jobs.
    Busy,
    /// Under maintenance, not accepting jobs.
    Maintenance,
    /// Finishing current jobs, not accepting new ones.
    Draining,
    /// Disconnected.
    Offline,
}

/// Runner capabilities — stored at `refs/runners/{runner_id}/capabilities`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerCapabilities {
    /// Runner type.
    pub runner_type: RunnerType,
    /// Supported build environments (nix, devbox, flux, bazel).
    pub environments: Vec<String>,
    /// Supported platforms (e.g. "linux/amd64", "linux/arm64").
    pub platforms: Vec<String>,
    /// Compliance certifications this runner holds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compliance: Vec<String>,
    /// Arbitrary labels for routing (region, zone, etc.).
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub labels: std::collections::HashMap<String, String>,
    /// Maximum concurrent jobs this runner can handle.
    pub max_concurrent_jobs: u32,
    /// Resource limits for the runner.
    pub resource_limits: ResourceLimits,
}

/// Resource limits for a runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU cores available.
    pub cpu_cores: u32,
    /// Memory in MB.
    pub memory_mb: u64,
    /// Disk space in MB.
    pub disk_mb: u64,
    /// Maximum PIDs per job.
    #[serde(default = "default_max_pids")]
    pub max_pids_per_job: u32,
    /// Whether network is allowed during build.
    /// SASL certification requires this to be false.
    #[serde(default = "default_true")]
    pub network_allowed: bool,
}

fn default_max_pids() -> u32 {
    4096
}

fn default_true() -> bool {
    true
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_cores: 4,
            memory_mb: 8192,
            disk_mb: 51200,
            max_pids_per_job: 4096,
            network_allowed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_capabilities_round_trip() {
        let caps = RunnerCapabilities {
            runner_type: RunnerType::BuildWorker,
            environments: vec!["nix".into(), "devbox".into()],
            platforms: vec!["linux/amd64".into()],
            compliance: vec!["soc2".into()],
            labels: {
                let mut m = std::collections::HashMap::new();
                m.insert("region".into(), "eu-west-1".into());
                m
            },
            max_concurrent_jobs: 4,
            resource_limits: ResourceLimits {
                cpu_cores: 8,
                memory_mb: 16384,
                disk_mb: 102400,
                max_pids_per_job: 8192,
                network_allowed: false,
            },
        };

        let json = serde_json::to_string(&caps).unwrap();
        let decoded: RunnerCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.runner_type, RunnerType::BuildWorker);
        assert_eq!(decoded.environments, vec!["nix", "devbox"]);
        assert!(!decoded.resource_limits.network_allowed);
    }

    #[test]
    fn runner_status_round_trip() {
        for status in [
            RunnerStatus::Idle,
            RunnerStatus::Busy,
            RunnerStatus::Maintenance,
            RunnerStatus::Draining,
            RunnerStatus::Offline,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: RunnerStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }
}

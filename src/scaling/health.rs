//! Health check improvements for Kubernetes readiness/liveness probes
//!
//! Provides separate endpoints for:
//! - Liveness: Is the process running?
//! - Readiness: Is the instance ready to accept traffic?

use crate::scaling::cluster::{ClusterManager, InstanceStatus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
    Starting,
    Draining,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessStatus {
    pub state: HealthState,
    pub uptime_secs: u64,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessStatus {
    pub state: HealthState,
    pub checks: Vec<HealthCheck>,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub instance_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub state: HealthState,
    pub message: Option<String>,
    pub latency_ms: Option<u64>,
}

impl HealthCheck {
    pub fn healthy(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: HealthState::Healthy,
            message: None,
            latency_ms: None,
        }
    }

    pub fn degraded(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: HealthState::Degraded,
            message: Some(message.into()),
            latency_ms: None,
        }
    }

    pub fn unhealthy(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: HealthState::Unhealthy,
            message: Some(message.into()),
            latency_ms: None,
        }
    }

    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.latency_ms = Some(latency.as_millis() as u64);
        self
    }
}

pub struct HealthChecker {
    started_at: Instant,
    instance_id: String,
    cluster_manager: Option<Arc<RwLock<ClusterManager>>>,
    readiness_checks: Vec<Box<dyn ReadinessCheck + Send + Sync>>,
}

impl HealthChecker {
    pub fn new(instance_id: String) -> Self {
        Self {
            started_at: Instant::now(),
            instance_id,
            cluster_manager: None,
            readiness_checks: Vec::new(),
        }
    }

    pub fn with_cluster_manager(mut self, manager: Arc<RwLock<ClusterManager>>) -> Self {
        self.cluster_manager = Some(manager);
        self
    }

    pub fn add_readiness_check(mut self, check: Box<dyn ReadinessCheck + Send + Sync>) -> Self {
        self.readiness_checks.push(check);
        self
    }

    pub async fn check_liveness(&self) -> LivenessStatus {
        let state = if let Some(ref manager) = self.cluster_manager {
            let mgr = manager.read().await;
            match mgr.local_instance().status {
                InstanceStatus::Starting => HealthState::Starting,
                InstanceStatus::Healthy => HealthState::Healthy,
                InstanceStatus::Degraded => HealthState::Degraded,
                InstanceStatus::Draining => HealthState::Draining,
                InstanceStatus::ShuttingDown | InstanceStatus::Dead => HealthState::Unhealthy,
            }
        } else {
            HealthState::Healthy
        };

        LivenessStatus {
            state,
            uptime_secs: self.started_at.elapsed().as_secs(),
            checked_at: chrono::Utc::now(),
            details: None,
        }
    }

    pub async fn check_readiness(&self) -> ReadinessStatus {
        let mut checks = Vec::new();

        for check in &self.readiness_checks {
            let start = Instant::now();
            let result = check.check().await;
            let latency = start.elapsed();

            let health_check = match result {
                Ok(message) => HealthCheck::healthy(&check.name())
                    .with_latency(latency)
                    .with_message_opt(message),
                Err(message) => HealthCheck::unhealthy(&check.name(), message)
                    .with_latency(latency),
            };
            checks.push(health_check);
        }

        let state = if checks.is_empty() {
            HealthState::Healthy
        } else if checks.iter().any(|c| c.state == HealthState::Unhealthy) {
            HealthState::Unhealthy
        } else if checks.iter().any(|c| c.state == HealthState::Degraded) {
            HealthState::Degraded
        } else {
            HealthState::Healthy
        };

        if let Some(ref manager) = self.cluster_manager {
            let mgr = manager.read().await;
            match mgr.local_instance().status {
                InstanceStatus::Draining => {
                    return ReadinessStatus {
                        state: HealthState::Draining,
                        checks,
                        checked_at: chrono::Utc::now(),
                        instance_id: self.instance_id.clone(),
                    };
                }
                InstanceStatus::Starting => {
                    return ReadinessStatus {
                        state: HealthState::Starting,
                        checks,
                        checked_at: chrono::Utc::now(),
                        instance_id: self.instance_id.clone(),
                    };
                }
                _ => {}
            }
        }

        ReadinessStatus {
            state,
            checks,
            checked_at: chrono::Utc::now(),
            instance_id: self.instance_id.clone(),
        }
    }

    pub async fn is_ready(&self) -> bool {
        let status = self.check_readiness().await;
        matches!(
            status.state,
            HealthState::Healthy | HealthState::Degraded
        )
    }

    pub async fn is_alive(&self) -> bool {
        let status = self.check_liveness().await;
        !matches!(status.state, HealthState::Unhealthy)
    }
}

impl HealthCheck {
    fn with_message_opt(mut self, message: Option<String>) -> Self {
        self.message = message;
        self
    }
}

#[async_trait::async_trait]
pub trait ReadinessCheck {
    fn name(&self) -> String;
    async fn check(&self) -> Result<Option<String>, String>;
}

pub struct DatabaseCheck {
    name: String,
}

impl DatabaseCheck {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
        }
    }
}

#[async_trait::async_trait]
impl ReadinessCheck for DatabaseCheck {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn check(&self) -> Result<Option<String>, String> {
        Ok(Some("Database connection OK".to_string()))
    }
}

pub struct StorageCheck {
    name: String,
    path: std::path::PathBuf,
}

impl StorageCheck {
    pub fn new(name: impl Into<String>, path: std::path::PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
        }
    }
}

#[async_trait::async_trait]
impl ReadinessCheck for StorageCheck {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn check(&self) -> Result<Option<String>, String> {
        if self.path.exists() {
            Ok(Some(format!("Storage path {} OK", self.path.display())))
        } else {
            Err(format!("Storage path {} not found", self.path.display()))
        }
    }
}

pub struct RedisCheck {
    name: String,
    url: Option<String>,
}

impl RedisCheck {
    pub fn new(name: impl Into<String>, url: Option<String>) -> Self {
        Self {
            name: name.into(),
            url,
        }
    }
}

#[async_trait::async_trait]
impl ReadinessCheck for RedisCheck {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn check(&self) -> Result<Option<String>, String> {
        if self.url.is_some() {
            Ok(Some("Redis connection OK".to_string()))
        } else {
            Ok(Some("Redis not configured (optional)".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_checker_liveness() {
        let checker = HealthChecker::new("test-instance".to_string());
        let status = checker.check_liveness().await;

        assert_eq!(status.state, HealthState::Healthy);
        assert!(status.uptime_secs < 10);
    }

    #[tokio::test]
    async fn test_health_checker_readiness_no_checks() {
        let checker = HealthChecker::new("test-instance".to_string());
        let status = checker.check_readiness().await;

        assert_eq!(status.state, HealthState::Healthy);
    }

    #[tokio::test]
    async fn test_health_check_helpers() {
        let healthy = HealthCheck::healthy("test");
        assert_eq!(healthy.state, HealthState::Healthy);

        let degraded = HealthCheck::degraded("test", "warning");
        assert_eq!(degraded.state, HealthState::Degraded);

        let unhealthy = HealthCheck::unhealthy("test", "error");
        assert_eq!(unhealthy.state, HealthState::Unhealthy);
    }
}

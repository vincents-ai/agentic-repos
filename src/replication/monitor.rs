//! Replication monitoring — health checks, lag detection, alerting thresholds.

use crate::replication::models::*;
use crate::replication::service::{ReplicationError, ReplicationService};
use std::sync::Arc;

/// Overall replication health summary across all targets.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplicationDashboard {
    pub total_targets: usize,
    pub healthy_targets: usize,
    pub degraded_targets: usize,
    pub failed_targets: usize,
    pub pending_conflicts: usize,
    pub total_objects_replicated: u64,
    pub total_bytes_replicated: u64,
    pub targets: Vec<ReplicationHealth>,
}

/// Alert threshold configuration for replication monitoring.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AlertThresholds {
    /// Maximum acceptable replication latency in milliseconds.
    pub max_latency_ms: f64,
    /// Maximum acceptable number of pending objects.
    pub max_pending_objects: u64,
    /// Maximum acceptable age of last replication (seconds).
    pub max_staleness_secs: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_latency_ms: 5000.0,
            max_pending_objects: 10_000,
            max_staleness_secs: 3600,
        }
    }
}

/// A monitoring alert.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplicationAlert {
    pub target_id: uuid::Uuid,
    pub target_name: String,
    pub alert_type: AlertType,
    pub message: String,
    pub detected_at: chrono::DateTime<chrono::Utc>,
}

/// Type of replication alert.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    HighLatency,
    TooManyPending,
    StaleReplication,
    TargetFailed,
    ConflictDetected,
}

/// Replication monitor — checks health against thresholds and generates alerts.
pub struct ReplicationMonitor {
    service: Arc<ReplicationService>,
    thresholds: AlertThresholds,
}

impl ReplicationMonitor {
    pub fn new(service: Arc<ReplicationService>, thresholds: AlertThresholds) -> Self {
        Self {
            service,
            thresholds,
        }
    }

    /// Build a dashboard summary of all replication targets.
    pub fn dashboard(&self) -> Result<ReplicationDashboard, ReplicationError> {
        let health = self.service.get_all_health()?;
        let conflicts = self.service.list_conflicts()?;
        let pending_conflicts = conflicts
            .iter()
            .filter(|c| c.resolution == ConflictResolution::Pending)
            .count();

        let total_objects: u64 = health.iter().map(|h| h.objects_replicated).sum();
        let total_bytes: u64 = health.iter().map(|h| h.bytes_replicated).sum();

        let healthy = health
            .iter()
            .filter(|h| {
                h.status == ReplicationStatus::Idle || h.status == ReplicationStatus::Completed
            })
            .count();
        let degraded = health
            .iter()
            .filter(|h| h.status == ReplicationStatus::Degraded)
            .count();
        let failed = health
            .iter()
            .filter(|h| h.status == ReplicationStatus::Failed)
            .count();

        Ok(ReplicationDashboard {
            total_targets: health.len(),
            healthy_targets: healthy,
            degraded_targets: degraded,
            failed_targets: failed,
            pending_conflicts,
            total_objects_replicated: total_objects,
            total_bytes_replicated: total_bytes,
            targets: health,
        })
    }

    /// Check all targets against alert thresholds and return any alerts.
    pub fn check_alerts(&self) -> Result<Vec<ReplicationAlert>, ReplicationError> {
        let health = self.service.get_all_health()?;
        let mut alerts = Vec::new();
        let now = chrono::Utc::now();

        for h in &health {
            // High latency
            if let Some(latency) = h.avg_latency_ms {
                if latency > self.thresholds.max_latency_ms {
                    alerts.push(ReplicationAlert {
                        target_id: h.target_id,
                        target_name: h.target_name.clone(),
                        alert_type: AlertType::HighLatency,
                        message: format!(
                            "Latency {:.0}ms exceeds threshold {:.0}ms",
                            latency, self.thresholds.max_latency_ms
                        ),
                        detected_at: now,
                    });
                }
            }

            // Too many pending objects
            if h.pending_objects > self.thresholds.max_pending_objects {
                alerts.push(ReplicationAlert {
                    target_id: h.target_id,
                    target_name: h.target_name.clone(),
                    alert_type: AlertType::TooManyPending,
                    message: format!(
                        "Pending objects {} exceeds threshold {}",
                        h.pending_objects, self.thresholds.max_pending_objects
                    ),
                    detected_at: now,
                });
            }

            // Stale replication
            if let Some(last) = h.last_replicated_at {
                let staleness = (now - last).num_seconds();
                if staleness > self.thresholds.max_staleness_secs as i64 {
                    alerts.push(ReplicationAlert {
                        target_id: h.target_id,
                        target_name: h.target_name.clone(),
                        alert_type: AlertType::StaleReplication,
                        message: format!(
                            "Last replication {}s ago exceeds threshold {}s",
                            staleness, self.thresholds.max_staleness_secs
                        ),
                        detected_at: now,
                    });
                }
            }

            // Target failed
            if h.status == ReplicationStatus::Failed {
                alerts.push(ReplicationAlert {
                    target_id: h.target_id,
                    target_name: h.target_name.clone(),
                    alert_type: AlertType::TargetFailed,
                    message: h
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "Unknown failure".to_string()),
                    detected_at: now,
                });
            }

            // Conflict detected
            if h.status == ReplicationStatus::Conflict {
                alerts.push(ReplicationAlert {
                    target_id: h.target_id,
                    target_name: h.target_name.clone(),
                    alert_type: AlertType::ConflictDetected,
                    message: "Replication conflict requires resolution".to_string(),
                    detected_at: now,
                });
            }
        }

        Ok(alerts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication::service::{InMemoryReplicationStore, ReplicationService};

    fn make_monitor() -> (Arc<ReplicationService>, ReplicationMonitor) {
        let svc = Arc::new(ReplicationService::new(InMemoryReplicationStore::shared()));
        let monitor = ReplicationMonitor::new(svc.clone(), AlertThresholds::default());
        (svc, monitor)
    }

    #[test]
    fn test_dashboard_empty() {
        let (_, monitor) = make_monitor();
        let dash = monitor.dashboard().unwrap();
        assert_eq!(dash.total_targets, 0);
        assert_eq!(dash.healthy_targets, 0);
    }

    #[test]
    fn test_dashboard_with_healthy_target() {
        let (svc, monitor) = make_monitor();
        let t = ReplicationTarget::new("mirror", "us-east-1", "bucket");
        svc.register_target(t).unwrap();

        // Start and complete replication to set health to Completed
        let t_list = svc.list_targets().unwrap();
        let target = &t_list[0];
        let op = svc
            .start_replication(target.id, "org/repo", 10, 1024)
            .unwrap();
        svc.complete_replication(op.id, target.id).unwrap();

        let dash = monitor.dashboard().unwrap();
        assert_eq!(dash.total_targets, 1);
        assert_eq!(dash.healthy_targets, 1);
        assert_eq!(dash.total_objects_replicated, 10);
        assert_eq!(dash.total_bytes_replicated, 1024);
    }

    #[test]
    fn test_alerts_for_failed_target() {
        let (svc, monitor) = make_monitor();
        let t = ReplicationTarget::new("failing", "us-east-1", "bucket");
        svc.register_target(t).unwrap();

        let t_list = svc.list_targets().unwrap();
        let target = &t_list[0];
        let op = svc.start_replication(target.id, "org/repo", 1, 1).unwrap();
        svc.fail_replication(op.id, target.id, "network timeout")
            .unwrap();

        let alerts = monitor.check_alerts().unwrap();
        assert!(!alerts.is_empty());
        assert!(alerts
            .iter()
            .any(|a| a.alert_type == AlertType::TargetFailed));
    }

    #[test]
    fn test_alerts_clear_when_healthy() {
        let (svc, monitor) = make_monitor();
        let t = ReplicationTarget::new("ok", "us-east-1", "bucket");
        svc.register_target(t).unwrap();

        let t_list = svc.list_targets().unwrap();
        let target = &t_list[0];
        let op = svc.start_replication(target.id, "org/repo", 1, 1).unwrap();
        svc.complete_replication(op.id, target.id).unwrap();

        let alerts = monitor.check_alerts().unwrap();
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_custom_thresholds() {
        let svc = Arc::new(ReplicationService::new(InMemoryReplicationStore::shared()));
        let thresholds = AlertThresholds {
            max_latency_ms: 100.0,
            max_pending_objects: 5,
            max_staleness_secs: 60,
        };
        let _monitor = ReplicationMonitor::new(svc, thresholds);
        // Monitor created with custom thresholds
    }
}

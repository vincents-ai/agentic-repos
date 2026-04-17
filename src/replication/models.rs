//! Replication data models — region targets, replication status, conflict records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A replication target — a remote S3 bucket in another region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationTarget {
    pub id: Uuid,
    /// Human-readable name (e.g. "us-east-1-mirror").
    pub name: String,
    /// AWS region (e.g. "us-east-1").
    pub region: String,
    /// S3 bucket ARN or name.
    pub bucket: String,
    /// Optional IAM role ARN for cross-account replication.
    pub role_arn: Option<String>,
    /// Whether this target is currently active.
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ReplicationTarget {
    pub fn new(
        name: impl Into<String>,
        region: impl Into<String>,
        bucket: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            region: region.into(),
            bucket: bucket.into(),
            role_arn: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_role_arn(mut self, arn: impl Into<String>) -> Self {
        self.role_arn = Some(arn.into());
        self
    }
}

/// Status of a replication target's sync state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationStatus {
    /// Replication is idle — nothing to replicate.
    Idle,
    /// Replication is in progress.
    Replicating,
    /// Replication completed successfully.
    Completed,
    /// Replication failed — check `last_error`.
    Failed,
    /// Target is temporarily unreachable — automatic retry pending.
    Degraded,
    /// Conflict detected — requires manual or automatic resolution.
    Conflict,
}

impl std::fmt::Display for ReplicationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ReplicationStatus::Idle => "idle",
            ReplicationStatus::Replicating => "replicating",
            ReplicationStatus::Completed => "completed",
            ReplicationStatus::Failed => "failed",
            ReplicationStatus::Degraded => "degraded",
            ReplicationStatus::Conflict => "conflict",
        };
        write!(f, "{}", s)
    }
}

/// Per-target replication health snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationHealth {
    pub target_id: Uuid,
    pub target_name: String,
    pub status: ReplicationStatus,
    /// Number of objects successfully replicated.
    pub objects_replicated: u64,
    /// Total bytes replicated.
    pub bytes_replicated: u64,
    /// Number of pending objects awaiting replication.
    pub pending_objects: u64,
    /// Last successful replication timestamp.
    pub last_replicated_at: Option<DateTime<Utc>>,
    /// Last error message (if any).
    pub last_error: Option<String>,
    /// Average replication latency in milliseconds.
    pub avg_latency_ms: Option<f64>,
    pub checked_at: DateTime<Utc>,
}

impl ReplicationHealth {
    pub fn healthy(target: &ReplicationTarget) -> Self {
        Self {
            target_id: target.id,
            target_name: target.name.clone(),
            status: ReplicationStatus::Idle,
            objects_replicated: 0,
            bytes_replicated: 0,
            pending_objects: 0,
            last_replicated_at: None,
            last_error: None,
            avg_latency_ms: None,
            checked_at: Utc::now(),
        }
    }
}

/// A replication conflict record — two divergent versions of the same object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConflict {
    pub id: Uuid,
    /// Repository that has the conflict.
    pub repository_id: String,
    /// Object key (path) in conflict.
    pub object_key: String,
    /// Source region version hash.
    pub source_version: String,
    /// Target region version hash.
    pub target_version: String,
    /// Source region.
    pub source_region: String,
    /// Target region.
    pub target_region: String,
    /// How the conflict was (or will be) resolved.
    pub resolution: ConflictResolution,
    pub detected_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl ReplicationConflict {
    pub fn new(
        repository_id: impl Into<String>,
        object_key: impl Into<String>,
        source_version: impl Into<String>,
        target_version: impl Into<String>,
        source_region: impl Into<String>,
        target_region: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            repository_id: repository_id.into(),
            object_key: object_key.into(),
            source_version: source_version.into(),
            target_version: target_version.into(),
            source_region: source_region.into(),
            target_region: target_region.into(),
            resolution: ConflictResolution::Pending,
            detected_at: Utc::now(),
            resolved_at: None,
        }
    }
}

/// Strategy for resolving a replication conflict.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Not yet resolved.
    Pending,
    /// Source region wins — overwrite target.
    SourceWins,
    /// Target region wins — overwrite source.
    TargetWins,
    /// Most recent timestamp wins.
    LastWriteWins,
    /// Merged both versions (requires application-specific logic).
    Merged,
    /// Manually resolved by an operator.
    Manual,
}

/// A replication operation — tracks a single sync batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationOp {
    pub id: Uuid,
    pub target_id: Uuid,
    pub repository_id: String,
    /// Number of objects in this batch.
    pub object_count: u64,
    /// Total bytes in this batch.
    pub total_bytes: u64,
    pub status: ReplicationStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

/// Configuration for region failover behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfig {
    /// Maximum consecutive failures before triggering failover.
    pub max_failures: u32,
    /// Seconds to wait between retry attempts.
    pub retry_interval_secs: u64,
    /// Whether to automatically promote a replica to primary on failover.
    pub auto_promote: bool,
    /// Preferred failover target order (target names).
    pub failover_order: Vec<String>,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            max_failures: 3,
            retry_interval_secs: 30,
            auto_promote: false,
            failover_order: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replication_target_builder() {
        let target = ReplicationTarget::new("eu-mirror", "eu-west-1", "my-bucket-eu")
            .with_role_arn("arn:aws:iam::123456:role/replication");
        assert_eq!(target.name, "eu-mirror");
        assert_eq!(target.region, "eu-west-1");
        assert!(target.enabled);
        assert_eq!(
            target.role_arn.as_deref(),
            Some("arn:aws:iam::123456:role/replication")
        );
    }

    #[test]
    fn test_replication_status_display() {
        assert_eq!(ReplicationStatus::Idle.to_string(), "idle");
        assert_eq!(ReplicationStatus::Failed.to_string(), "failed");
        assert_eq!(ReplicationStatus::Conflict.to_string(), "conflict");
    }

    #[test]
    fn test_replication_conflict_new() {
        let conflict = ReplicationConflict::new(
            "org/repo",
            "refs/heads/main",
            "abc123",
            "def456",
            "us-east-1",
            "eu-west-1",
        );
        assert_eq!(conflict.resolution, ConflictResolution::Pending);
        assert!(conflict.resolved_at.is_none());
    }

    #[test]
    fn test_failover_config_default() {
        let config = FailoverConfig::default();
        assert_eq!(config.max_failures, 3);
        assert_eq!(config.retry_interval_secs, 30);
        assert!(!config.auto_promote);
        assert!(config.failover_order.is_empty());
    }
}

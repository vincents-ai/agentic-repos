//! Replication service — manages targets, triggers sync, tracks health.

use crate::replication::models::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ReplicationError {
    #[error("Target not found: {0}")]
    TargetNotFound(Uuid),
    #[error("Target already exists: {0}")]
    TargetExists(String),
    #[error("Replication failed: {0}")]
    ReplicationFailed(String),
    #[error(
        "Conflict detected for {object_key}: source={source_version}, target={target_version}"
    )]
    ConflictDetected {
        object_key: String,
        source_version: String,
        target_version: String,
    },
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Lock poisoned")]
    LockPoisoned,
}

/// Trait for replication state persistence.
pub trait ReplicationStore: Send + Sync {
    fn list_targets(&self) -> Result<Vec<ReplicationTarget>, ReplicationError>;
    fn get_target(&self, id: Uuid) -> Result<Option<ReplicationTarget>, ReplicationError>;
    fn add_target(&self, target: ReplicationTarget) -> Result<(), ReplicationError>;
    fn remove_target(&self, id: Uuid) -> Result<(), ReplicationError>;
    fn update_target(&self, target: ReplicationTarget) -> Result<(), ReplicationError>;

    fn list_health(&self) -> Result<Vec<ReplicationHealth>, ReplicationError>;
    fn get_health(&self, target_id: Uuid) -> Result<Option<ReplicationHealth>, ReplicationError>;
    fn update_health(&self, health: ReplicationHealth) -> Result<(), ReplicationError>;

    fn list_conflicts(&self) -> Result<Vec<ReplicationConflict>, ReplicationError>;
    fn add_conflict(&self, conflict: ReplicationConflict) -> Result<(), ReplicationError>;
    fn resolve_conflict(
        &self,
        id: Uuid,
        resolution: ConflictResolution,
    ) -> Result<(), ReplicationError>;

    fn list_ops(&self, target_id: Uuid) -> Result<Vec<ReplicationOp>, ReplicationError>;
    fn add_op(&self, op: ReplicationOp) -> Result<(), ReplicationError>;
    fn update_op(&self, op: ReplicationOp) -> Result<(), ReplicationError>;

    /// Atomically find an operation by ID and update its status.
    /// Returns the updated operation. Prevents TOCTOU races between read and write.
    fn update_op_status(
        &self,
        op_id: Uuid,
        status: ReplicationStatus,
        error: Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<ReplicationOp, ReplicationError> {
        // Default implementation: read-modify-write (subclasses should override)
        let mut ops = self.list_ops(op_id)?;
        let op = ops
            .iter_mut()
            .find(|o| o.id == op_id)
            .ok_or_else(|| ReplicationError::Storage("operation not found".into()))?;
        op.status = status;
        op.error = error;
        op.completed_at = completed_at;
        let updated = op.clone();
        self.update_op(updated.clone())?;
        Ok(updated)
    }
}

/// In-memory replication store (for development and testing).
pub struct InMemoryReplicationStore {
    targets: RwLock<HashMap<Uuid, ReplicationTarget>>,
    health: RwLock<HashMap<Uuid, ReplicationHealth>>,
    conflicts: RwLock<HashMap<Uuid, ReplicationConflict>>,
    ops: RwLock<HashMap<Uuid, Vec<ReplicationOp>>>,
}

impl InMemoryReplicationStore {
    pub fn new() -> Self {
        Self {
            targets: RwLock::new(HashMap::new()),
            health: RwLock::new(HashMap::new()),
            conflicts: RwLock::new(HashMap::new()),
            ops: RwLock::new(HashMap::new()),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl Default for InMemoryReplicationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplicationStore for InMemoryReplicationStore {
    fn list_targets(&self) -> Result<Vec<ReplicationTarget>, ReplicationError> {
        let targets = self
            .targets
            .read()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        Ok(targets.values().cloned().collect())
    }

    fn get_target(&self, id: Uuid) -> Result<Option<ReplicationTarget>, ReplicationError> {
        let targets = self
            .targets
            .read()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        Ok(targets.get(&id).cloned())
    }

    fn add_target(&self, target: ReplicationTarget) -> Result<(), ReplicationError> {
        let mut targets = self
            .targets
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        if targets.contains_key(&target.id) {
            return Err(ReplicationError::TargetExists(target.name.clone()));
        }
        targets.insert(target.id, target.clone());
        // Initialize health
        let mut health = self
            .health
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        health.insert(target.id, ReplicationHealth::healthy(&target));
        Ok(())
    }

    fn remove_target(&self, id: Uuid) -> Result<(), ReplicationError> {
        let mut targets = self
            .targets
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        targets
            .remove(&id)
            .ok_or(ReplicationError::TargetNotFound(id))?;
        let mut health = self
            .health
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        health.remove(&id);
        Ok(())
    }

    fn update_target(&self, target: ReplicationTarget) -> Result<(), ReplicationError> {
        let mut targets = self
            .targets
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        if !targets.contains_key(&target.id) {
            return Err(ReplicationError::TargetNotFound(target.id));
        }
        targets.insert(target.id, target);
        Ok(())
    }

    fn list_health(&self) -> Result<Vec<ReplicationHealth>, ReplicationError> {
        let health = self
            .health
            .read()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        Ok(health.values().cloned().collect())
    }

    fn get_health(&self, target_id: Uuid) -> Result<Option<ReplicationHealth>, ReplicationError> {
        let health = self
            .health
            .read()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        Ok(health.get(&target_id).cloned())
    }

    fn update_health(&self, h: ReplicationHealth) -> Result<(), ReplicationError> {
        let mut health = self
            .health
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        health.insert(h.target_id, h);
        Ok(())
    }

    fn list_conflicts(&self) -> Result<Vec<ReplicationConflict>, ReplicationError> {
        let conflicts = self
            .conflicts
            .read()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        Ok(conflicts.values().cloned().collect())
    }

    fn add_conflict(&self, conflict: ReplicationConflict) -> Result<(), ReplicationError> {
        let mut conflicts = self
            .conflicts
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        conflicts.insert(conflict.id, conflict);
        Ok(())
    }

    fn resolve_conflict(
        &self,
        id: Uuid,
        resolution: ConflictResolution,
    ) -> Result<(), ReplicationError> {
        let mut conflicts = self
            .conflicts
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        let conflict = conflicts
            .get_mut(&id)
            .ok_or(ReplicationError::Storage("conflict not found".into()))?;
        conflict.resolution = resolution;
        conflict.resolved_at = Some(chrono::Utc::now());
        Ok(())
    }

    fn list_ops(&self, target_id: Uuid) -> Result<Vec<ReplicationOp>, ReplicationError> {
        let ops = self
            .ops
            .read()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        Ok(ops.get(&target_id).cloned().unwrap_or_default())
    }

    fn add_op(&self, op: ReplicationOp) -> Result<(), ReplicationError> {
        let mut ops = self
            .ops
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        ops.entry(op.target_id).or_default().push(op);
        Ok(())
    }

    fn update_op(&self, op: ReplicationOp) -> Result<(), ReplicationError> {
        let mut ops = self
            .ops
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        let target_ops = ops
            .get_mut(&op.target_id)
            .ok_or_else(|| ReplicationError::Storage("target ops not found".into()))?;
        if let Some(existing) = target_ops.iter_mut().find(|o| o.id == op.id) {
            *existing = op;
        }
        Ok(())
    }

    /// Atomic find-and-update under a single write lock.
    fn update_op_status(
        &self,
        op_id: Uuid,
        status: ReplicationStatus,
        error: Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<ReplicationOp, ReplicationError> {
        let mut all_ops = self
            .ops
            .write()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        for (_, target_ops) in all_ops.iter_mut() {
            if let Some(op) = target_ops.iter_mut().find(|o| o.id == op_id) {
                op.status = status;
                op.error = error;
                op.completed_at = completed_at;
                return Ok(op.clone());
            }
        }
        Err(ReplicationError::Storage("operation not found".into()))
    }
}

/// High-level replication service.
pub struct ReplicationService {
    store: Arc<dyn ReplicationStore>,
    failover_config: RwLock<FailoverConfig>,
}

impl ReplicationService {
    pub fn new(store: Arc<dyn ReplicationStore>) -> Self {
        Self {
            store,
            failover_config: RwLock::new(FailoverConfig::default()),
        }
    }

    pub fn with_failover_config(store: Arc<dyn ReplicationStore>, config: FailoverConfig) -> Self {
        Self {
            store,
            failover_config: RwLock::new(config),
        }
    }

    // ── Target management ──

    pub fn register_target(&self, target: ReplicationTarget) -> Result<(), ReplicationError> {
        self.store.add_target(target)
    }

    pub fn remove_target(&self, id: Uuid) -> Result<(), ReplicationError> {
        self.store.remove_target(id)
    }

    pub fn list_targets(&self) -> Result<Vec<ReplicationTarget>, ReplicationError> {
        self.store.list_targets()
    }

    pub fn get_target(&self, id: Uuid) -> Result<Option<ReplicationTarget>, ReplicationError> {
        self.store.get_target(id)
    }

    pub fn enable_target(&self, id: Uuid) -> Result<(), ReplicationError> {
        let mut target = self
            .store
            .get_target(id)?
            .ok_or(ReplicationError::TargetNotFound(id))?;
        target.enabled = true;
        target.updated_at = chrono::Utc::now();
        self.store.update_target(target)
    }

    pub fn disable_target(&self, id: Uuid) -> Result<(), ReplicationError> {
        let mut target = self
            .store
            .get_target(id)?
            .ok_or(ReplicationError::TargetNotFound(id))?;
        target.enabled = false;
        target.updated_at = chrono::Utc::now();
        self.store.update_target(target)
    }

    // ── Replication operations ──

    /// Start a replication operation for a repository to a specific target.
    pub fn start_replication(
        &self,
        target_id: Uuid,
        repository_id: &str,
        object_count: u64,
        total_bytes: u64,
    ) -> Result<ReplicationOp, ReplicationError> {
        let target = self
            .store
            .get_target(target_id)?
            .ok_or(ReplicationError::TargetNotFound(target_id))?;
        if !target.enabled {
            return Err(ReplicationError::ReplicationFailed(format!(
                "target {} is disabled",
                target.name
            )));
        }

        let op = ReplicationOp {
            id: Uuid::new_v4(),
            target_id,
            repository_id: repository_id.to_string(),
            object_count,
            total_bytes,
            status: ReplicationStatus::Replicating,
            started_at: chrono::Utc::now(),
            completed_at: None,
            error: None,
        };

        // Update health to reflect in-progress replication
        if let Some(mut health) = self.store.get_health(target_id)? {
            health.status = ReplicationStatus::Replicating;
            health.pending_objects += object_count;
            self.store.update_health(health)?;
        }

        self.store.add_op(op.clone())?;
        Ok(op)
    }

    /// Mark a replication operation as completed.
    pub fn complete_replication(
        &self,
        op_id: Uuid,
        target_id: Uuid,
    ) -> Result<(), ReplicationError> {
        let now = chrono::Utc::now();
        let updated_op =
            self.store
                .update_op_status(op_id, ReplicationStatus::Completed, None, Some(now))?;

        // Update health atomically
        if let Some(mut health) = self.store.get_health(target_id)? {
            health.status = ReplicationStatus::Completed;
            health.objects_replicated += updated_op.object_count;
            health.bytes_replicated += updated_op.total_bytes;
            health.pending_objects = health
                .pending_objects
                .saturating_sub(updated_op.object_count);
            health.last_replicated_at = Some(now);
            self.store.update_health(health)?;
        }

        Ok(())
    }

    /// Mark a replication operation as failed.
    pub fn fail_replication(
        &self,
        op_id: Uuid,
        target_id: Uuid,
        error: &str,
    ) -> Result<(), ReplicationError> {
        let now = chrono::Utc::now();
        self.store.update_op_status(
            op_id,
            ReplicationStatus::Failed,
            Some(error.to_string()),
            Some(now),
        )?;

        if let Some(mut health) = self.store.get_health(target_id)? {
            health.status = ReplicationStatus::Failed;
            health.last_error = Some(error.to_string());
            self.store.update_health(health)?;
        }

        Ok(())
    }

    // ── Health monitoring ──

    pub fn get_all_health(&self) -> Result<Vec<ReplicationHealth>, ReplicationError> {
        self.store.list_health()
    }

    pub fn get_target_health(
        &self,
        target_id: Uuid,
    ) -> Result<Option<ReplicationHealth>, ReplicationError> {
        self.store.get_health(target_id)
    }

    // ── Conflict management ──

    pub fn list_conflicts(&self) -> Result<Vec<ReplicationConflict>, ReplicationError> {
        self.store.list_conflicts()
    }

    pub fn report_conflict(&self, conflict: ReplicationConflict) -> Result<(), ReplicationError> {
        // Update health for the target in the conflict's target region
        let targets = self.store.list_targets()?;
        if let Some(target) = targets.iter().find(|t| t.region == conflict.target_region) {
            if let Some(mut health) = self.store.get_health(target.id)? {
                health.status = ReplicationStatus::Conflict;
                self.store.update_health(health)?;
            }
        }
        self.store.add_conflict(conflict)
    }

    pub fn resolve_conflict(
        &self,
        conflict_id: Uuid,
        resolution: ConflictResolution,
    ) -> Result<(), ReplicationError> {
        self.store.resolve_conflict(conflict_id, resolution)
    }

    // ── Failover ──

    pub fn get_failover_config(&self) -> FailoverConfig {
        self.failover_config
            .read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    pub fn set_failover_config(&self, config: FailoverConfig) {
        if let Ok(mut c) = self.failover_config.write() {
            *c = config;
        }
    }

    /// Check if failover should be triggered for a given target based on health.
    pub fn should_failover(&self, target_id: Uuid) -> Result<bool, ReplicationError> {
        let config = self
            .failover_config
            .read()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        let health = self.store.get_health(target_id)?;
        match health {
            Some(h) if h.status == ReplicationStatus::Failed => {
                // Count recent failed ops
                let ops = self.store.list_ops(target_id)?;
                let consecutive_failures = ops
                    .iter()
                    .rev()
                    .take_while(|o| o.status == ReplicationStatus::Failed)
                    .count() as u32;
                Ok(consecutive_failures >= config.max_failures)
            }
            _ => Ok(false),
        }
    }

    /// Get the next failover target from the configured order.
    pub fn failover_target(&self) -> Result<Option<ReplicationTarget>, ReplicationError> {
        let config = self
            .failover_config
            .read()
            .map_err(|_| ReplicationError::LockPoisoned)?;
        let targets = self.store.list_targets()?;
        for name in &config.failover_order {
            if let Some(t) = targets.iter().find(|t| &t.name == name && t.enabled) {
                return Ok(Some(t.clone()));
            }
        }
        // Fallback: return first enabled target
        Ok(targets.into_iter().find(|t| t.enabled))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> ReplicationService {
        ReplicationService::new(InMemoryReplicationStore::shared())
    }

    fn make_target(name: &str) -> ReplicationTarget {
        ReplicationTarget::new(name, "us-east-1", format!("bucket-{}", name))
    }

    #[test]
    fn test_register_and_list_targets() {
        let svc = make_service();
        let t1 = make_target("mirror-1");
        let t2 = make_target("mirror-2");
        svc.register_target(t1).unwrap();
        svc.register_target(t2).unwrap();
        let targets = svc.list_targets().unwrap();
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn test_remove_target() {
        let svc = make_service();
        let t = make_target("to-remove");
        svc.register_target(t.clone()).unwrap();
        svc.remove_target(t.id).unwrap();
        assert!(svc.get_target(t.id).unwrap().is_none());
    }

    #[test]
    fn test_enable_disable_target() {
        let svc = make_service();
        let t = make_target("toggle");
        svc.register_target(t.clone()).unwrap();
        svc.disable_target(t.id).unwrap();
        let disabled = svc.get_target(t.id).unwrap().unwrap();
        assert!(!disabled.enabled);
        svc.enable_target(t.id).unwrap();
        let enabled = svc.get_target(t.id).unwrap().unwrap();
        assert!(enabled.enabled);
    }

    #[test]
    fn test_start_and_complete_replication() {
        let svc = make_service();
        let t = make_target("repl-target");
        svc.register_target(t.clone()).unwrap();

        let op = svc.start_replication(t.id, "org/repo", 100, 4096).unwrap();
        assert_eq!(op.status, ReplicationStatus::Replicating);
        assert_eq!(op.object_count, 100);

        svc.complete_replication(op.id, t.id).unwrap();

        let health = svc.get_target_health(t.id).unwrap().unwrap();
        assert_eq!(health.status, ReplicationStatus::Completed);
        assert_eq!(health.objects_replicated, 100);
        assert_eq!(health.bytes_replicated, 4096);
    }

    #[test]
    fn test_fail_replication() {
        let svc = make_service();
        let t = make_target("fail-target");
        svc.register_target(t.clone()).unwrap();

        let op = svc.start_replication(t.id, "org/repo", 50, 2048).unwrap();
        svc.fail_replication(op.id, t.id, "connection refused")
            .unwrap();

        let health = svc.get_target_health(t.id).unwrap().unwrap();
        assert_eq!(health.status, ReplicationStatus::Failed);
        assert_eq!(health.last_error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn test_replication_disabled_target() {
        let svc = make_service();
        let t = make_target("disabled");
        svc.register_target(t.clone()).unwrap();
        svc.disable_target(t.id).unwrap();

        let result = svc.start_replication(t.id, "org/repo", 10, 512);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }

    #[test]
    fn test_conflict_report_and_resolve() {
        let svc = make_service();
        let t = make_target("conflict-target");
        svc.register_target(t.clone()).unwrap();

        let conflict = ReplicationConflict::new(
            "org/repo",
            "refs/heads/main",
            "abc123",
            "def456",
            "us-east-1",
            "eu-west-1",
        );
        svc.report_conflict(conflict).unwrap();

        let conflicts = svc.list_conflicts().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].resolution, ConflictResolution::Pending);

        svc.resolve_conflict(conflicts[0].id, ConflictResolution::SourceWins)
            .unwrap();
        let resolved = svc.list_conflicts().unwrap();
        assert_eq!(resolved[0].resolution, ConflictResolution::SourceWins);
        assert!(resolved[0].resolved_at.is_some());
    }

    #[test]
    fn test_failover_trigger() {
        let svc = ReplicationService::with_failover_config(
            InMemoryReplicationStore::shared(),
            FailoverConfig {
                max_failures: 2,
                ..Default::default()
            },
        );
        let t = make_target("failing");
        svc.register_target(t.clone()).unwrap();

        // First failure — not enough
        let op1 = svc.start_replication(t.id, "org/repo", 1, 1).unwrap();
        svc.fail_replication(op1.id, t.id, "err1").unwrap();
        assert!(!svc.should_failover(t.id).unwrap());

        // Second failure — triggers
        let op2 = svc.start_replication(t.id, "org/repo", 1, 1).unwrap();
        svc.fail_replication(op2.id, t.id, "err2").unwrap();
        assert!(svc.should_failover(t.id).unwrap());
    }

    #[test]
    fn test_failover_target_selection() {
        let svc = ReplicationService::with_failover_config(
            InMemoryReplicationStore::shared(),
            FailoverConfig {
                failover_order: vec!["backup-b".to_string(), "backup-a".to_string()],
                ..Default::default()
            },
        );
        svc.register_target(make_target("backup-a")).unwrap();
        svc.register_target(make_target("backup-b")).unwrap();

        // Should prefer backup-b from failover_order
        let target = svc.failover_target().unwrap().unwrap();
        assert_eq!(target.name, "backup-b");
    }

    #[test]
    fn test_remove_nonexistent_target() {
        let svc = make_service();
        let result = svc.remove_target(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_target_not_found_replication() {
        let svc = make_service();
        let result = svc.start_replication(Uuid::new_v4(), "org/repo", 1, 1);
        assert!(result.is_err());
    }
}

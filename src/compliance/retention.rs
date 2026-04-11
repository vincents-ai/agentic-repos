use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use super::audit::ComplianceAuditStorage;

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum RetentionError {
    #[error("Retention policy error: {0}")]
    Policy(String),
    #[error("Enforcement error: {0}")]
    Enforcement(String),
    #[error("Storage error: {0}")]
    Storage(String),
}

pub type RetentionResult<T> = std::result::Result<T, RetentionError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRetentionPolicy {
    pub policy_id: String,
    pub name: String,
    pub description: String,
    pub data_type: String,
    pub retention_period_days: u32,
    pub action: RetentionAction,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub exceptions: Vec<RetentionException>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionAction {
    Delete,
    Anonymize,
    Archive,
    Notify,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionException {
    pub exception_id: String,
    pub reason: String,
    pub data_identifier: String,
    pub extended_retention_days: u32,
    pub expires_at: DateTime<Utc>,
    pub approved_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionExecution {
    pub execution_id: String,
    pub policy_id: String,
    pub tenant_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: RetentionExecutionStatus,
    pub items_processed: u64,
    pub items_affected: u64,
    pub errors: Vec<RetentionError>,
    pub details: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionExecutionStatus {
    Pending,
    Running,
    Completed,
    PartiallyCompleted,
    Failed,
}

pub struct DataRetentionService<S: ComplianceAuditStorage> {
    audit_storage: Arc<S>,
    policies: tokio::sync::RwLock<Vec<DataRetentionPolicy>>,
    executions: tokio::sync::RwLock<Vec<RetentionExecution>>,
}

impl<S: ComplianceAuditStorage> DataRetentionService<S> {
    pub fn new(audit_storage: Arc<S>) -> Self {
        Self {
            audit_storage,
            policies: tokio::sync::RwLock::new(Vec::new()),
            executions: tokio::sync::RwLock::new(Vec::new()),
        }
    }

    pub async fn add_policy(&self, policy: DataRetentionPolicy) -> RetentionResult<String> {
        let policy_id = policy.policy_id.clone();
        let mut policies = self.policies.write().await;

        if policies.iter().any(|p| p.policy_id == policy.policy_id) {
            return Err(RetentionError::Policy(format!(
                "Policy with ID {} already exists",
                policy.policy_id
            )));
        }

        policies.push(policy);
        Ok(policy_id)
    }

    pub async fn update_policy(
        &self,
        policy_id: &str,
        updates: PolicyUpdates,
    ) -> RetentionResult<DataRetentionPolicy> {
        let mut policies = self.policies.write().await;

        let policy = policies
            .iter_mut()
            .find(|p| p.policy_id == policy_id)
            .ok_or_else(|| RetentionError::Policy(format!("Policy {} not found", policy_id)))?;

        if let Some(name) = updates.name {
            policy.name = name;
        }
        if let Some(description) = updates.description {
            policy.description = description;
        }
        if let Some(retention_days) = updates.retention_period_days {
            policy.retention_period_days = retention_days;
        }
        if let Some(action) = updates.action {
            policy.action = action;
        }
        if let Some(enabled) = updates.enabled {
            policy.enabled = enabled;
        }
        policy.updated_at = Utc::now();

        Ok(policy.clone())
    }

    pub async fn get_policy(
        &self,
        policy_id: &str,
    ) -> RetentionResult<Option<DataRetentionPolicy>> {
        let policies = self.policies.read().await;
        Ok(policies.iter().find(|p| p.policy_id == policy_id).cloned())
    }

    pub async fn list_policies(&self) -> RetentionResult<Vec<DataRetentionPolicy>> {
        let policies = self.policies.read().await;
        Ok(policies.clone())
    }

    pub async fn delete_policy(&self, policy_id: &str) -> RetentionResult<bool> {
        let mut policies = self.policies.write().await;
        let initial_len = policies.len();
        policies.retain(|p| p.policy_id != policy_id);
        Ok(policies.len() < initial_len)
    }

    pub async fn add_exception(
        &self,
        policy_id: &str,
        exception: RetentionException,
    ) -> RetentionResult<()> {
        let mut policies = self.policies.write().await;

        let policy = policies
            .iter_mut()
            .find(|p| p.policy_id == policy_id)
            .ok_or_else(|| RetentionError::Policy(format!("Policy {} not found", policy_id)))?;

        policy.exceptions.push(exception);
        policy.updated_at = Utc::now();

        Ok(())
    }

    pub async fn enforce_retention(
        &self,
        tenant_id: &str,
        policy_id: &str,
        dry_run: bool,
    ) -> RetentionResult<RetentionExecution> {
        // Get policy info and clone what we need before releasing the lock
        let retention_period_days = {
            let policies = self.policies.read().await;
            let policy = policies
                .iter()
                .find(|p| p.policy_id == policy_id)
                .ok_or_else(|| RetentionError::Policy(format!("Policy {} not found", policy_id)))?;

            if !policy.enabled {
                return Err(RetentionError::Enforcement(format!(
                    "Policy {} is disabled",
                    policy_id
                )));
            }

            // Clone the value we need
            policy.retention_period_days
        };
        // policies lock is now released

        let execution_id = uuid::Uuid::new_v4().to_string();
        let execution = RetentionExecution {
            execution_id: execution_id.clone(),
            policy_id: policy_id.to_string(),
            tenant_id: tenant_id.to_string(),
            started_at: Utc::now(),
            completed_at: None,
            status: RetentionExecutionStatus::Running,
            items_processed: 0,
            items_affected: 0,
            errors: Vec::new(),
            details: HashMap::new(),
        };

        self.executions.write().await.push(execution.clone());

        let cutoff = Utc::now() - chrono::Duration::days(retention_period_days as i64);

        let deleted_count = if dry_run {
            self.audit_storage
                .count(Some(tenant_id), None, None, Some(cutoff))
                .await
                .map_err(|e| RetentionError::Storage(e.to_string()))?
        } else {
            self.audit_storage
                .delete_before(cutoff, Some(tenant_id))
                .await
                .map_err(|e| RetentionError::Storage(e.to_string()))?
        };

        let mut executions = self.executions.write().await;
        if let Some(exec) = executions
            .iter_mut()
            .find(|e| e.execution_id == execution_id)
        {
            exec.items_processed = deleted_count;
            exec.items_affected = if dry_run { 0 } else { deleted_count };
            exec.completed_at = Some(Utc::now());
            exec.status = RetentionExecutionStatus::Completed;
            exec.details
                .insert("dry_run".to_string(), serde_json::json!(dry_run));
            exec.details.insert(
                "cutoff_date".to_string(),
                serde_json::json!(cutoff.to_rfc3339()),
            );

            return Ok(exec.clone());
        }

        Err(RetentionError::Enforcement(
            "Execution record not found".to_string(),
        ))
    }

    pub async fn enforce_all_policies(
        &self,
        tenant_id: &str,
        dry_run: bool,
    ) -> RetentionResult<Vec<RetentionExecution>> {
        let policies = self.policies.read().await;
        let mut results = Vec::new();

        for policy in policies.iter().filter(|p| p.enabled) {
            let result = self
                .enforce_retention(tenant_id, &policy.policy_id, dry_run)
                .await;
            match result {
                Ok(execution) => results.push(execution),
                Err(e) => {
                    tracing::warn!("Failed to enforce policy {}: {}", policy.policy_id, e);
                }
            }
        }

        Ok(results)
    }

    pub async fn get_execution_history(
        &self,
        tenant_id: Option<&str>,
        policy_id: Option<&str>,
        limit: usize,
    ) -> RetentionResult<Vec<RetentionExecution>> {
        let executions = self.executions.read().await;

        let filtered: Vec<RetentionExecution> = executions
            .iter()
            .filter(|e| {
                if let Some(t) = tenant_id {
                    if e.tenant_id != t {
                        return false;
                    }
                }
                if let Some(p) = policy_id {
                    if e.policy_id != p {
                        return false;
                    }
                }
                true
            })
            .take(limit)
            .cloned()
            .collect();

        Ok(filtered)
    }

    pub async fn get_retention_summary(
        &self,
        tenant_id: &str,
    ) -> RetentionResult<RetentionSummary> {
        let policies = self.policies.read().await;

        let mut summary = RetentionSummary {
            tenant_id: tenant_id.to_string(),
            total_policies: policies.len() as u64,
            enabled_policies: policies.iter().filter(|p| p.enabled).count() as u64,
            disabled_policies: policies.iter().filter(|p| !p.enabled).count() as u64,
            policies_by_data_type: HashMap::new(),
            overdue_enforcement: 0,
            last_enforcement: None,
        };

        for policy in policies.iter() {
            *summary
                .policies_by_data_type
                .entry(policy.data_type.clone())
                .or_insert(0) += 1;
        }

        let executions = self.executions.read().await;
        if let Some(last) = executions
            .iter()
            .filter(|e| e.tenant_id == tenant_id)
            .max_by_key(|e| e.started_at)
        {
            summary.last_enforcement = Some(last.started_at);
        }

        Ok(summary)
    }

    pub async fn create_default_policies(&self) -> RetentionResult<()> {
        let default_policies = vec![
            DataRetentionPolicy {
                policy_id: "default-audit-logs".to_string(),
                name: "Audit Logs Retention".to_string(),
                description: "Default retention policy for audit logs".to_string(),
                data_type: "audit_logs".to_string(),
                retention_period_days: 365,
                action: RetentionAction::Delete,
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                exceptions: Vec::new(),
            },
            DataRetentionPolicy {
                policy_id: "default-repository-data".to_string(),
                name: "Repository Data Retention".to_string(),
                description: "Default retention policy for repository data".to_string(),
                data_type: "repository_data".to_string(),
                retention_period_days: 2555,
                action: RetentionAction::Archive,
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                exceptions: Vec::new(),
            },
            DataRetentionPolicy {
                policy_id: "default-session-data".to_string(),
                name: "Session Data Retention".to_string(),
                description: "Default retention policy for session data".to_string(),
                data_type: "session_data".to_string(),
                retention_period_days: 30,
                action: RetentionAction::Delete,
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                exceptions: Vec::new(),
            },
        ];

        for policy in default_policies {
            self.add_policy(policy).await?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyUpdates {
    pub name: Option<String>,
    pub description: Option<String>,
    pub retention_period_days: Option<u32>,
    pub action: Option<RetentionAction>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionSummary {
    pub tenant_id: String,
    pub total_policies: u64,
    pub enabled_policies: u64,
    pub disabled_policies: u64,
    pub policies_by_data_type: HashMap<String, u64>,
    pub overdue_enforcement: u64,
    pub last_enforcement: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::audit::InMemoryComplianceAuditStorage;

    #[tokio::test]
    async fn test_add_policy() {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(10000));
        let service = DataRetentionService::new(audit_storage);

        let policy = DataRetentionPolicy {
            policy_id: "test-policy".to_string(),
            name: "Test Policy".to_string(),
            description: "Test description".to_string(),
            data_type: "test_data".to_string(),
            retention_period_days: 90,
            action: RetentionAction::Delete,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            exceptions: Vec::new(),
        };

        let result = service.add_policy(policy).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-policy");
    }

    #[tokio::test]
    async fn test_enforce_retention_dry_run() {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(10000));
        let service = DataRetentionService::new(audit_storage);

        service.create_default_policies().await.unwrap();

        let execution = service
            .enforce_retention("tenant-1", "default-audit-logs", true)
            .await
            .unwrap();

        assert_eq!(execution.status, RetentionExecutionStatus::Completed);
        assert_eq!(execution.items_affected, 0);
    }

    #[tokio::test]
    async fn test_list_policies() {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(10000));
        let service = DataRetentionService::new(audit_storage);

        service.create_default_policies().await.unwrap();

        let policies = service.list_policies().await.unwrap();
        assert_eq!(policies.len(), 3);
    }

    #[tokio::test]
    async fn test_get_retention_summary() {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(10000));
        let service = DataRetentionService::new(audit_storage);

        service.create_default_policies().await.unwrap();

        let summary = service.get_retention_summary("tenant-1").await.unwrap();
        assert_eq!(summary.total_policies, 3);
        assert_eq!(summary.enabled_policies, 3);
    }
}

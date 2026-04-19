use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum AuditAggregationError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Query error: {0}")]
    Query(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceEventType {
    UserAuthentication,
    UserAuthorization,
    DataAccess,
    DataModification,
    DataDeletion,
    DataExport,
    ConfigurationChange,
    SecurityIncident,
    PolicyViolation,
    ApiAccess,
    RepositoryAccess,
    SshKeyOperation,
    PermissionChange,
    RoleAssignment,
    AuditLogAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAuditEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: ComplianceEventType,
    pub tenant_id: String,
    pub actor: ActorInfo,
    pub resource: ResourceInfo,
    pub action: String,
    pub outcome: EventOutcome,
    pub details: HashMap<String, serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<String>,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub actor_type: ActorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    ServiceAccount,
    System,
    ApiKey,
    Anonymous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub resource_type: String,
    pub resource_id: String,
    pub resource_name: Option<String>,
    pub parent_resource: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventOutcome {
    Success,
    Failure,
    Denied,
    Partial,
}

impl ComplianceAuditEvent {
    pub fn new(
        event_type: ComplianceEventType,
        tenant_id: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            tenant_id: tenant_id.into(),
            actor: ActorInfo {
                user_id: None,
                username: None,
                email: None,
                actor_type: ActorType::Anonymous,
            },
            resource: ResourceInfo {
                resource_type: "unknown".to_string(),
                resource_id: "unknown".to_string(),
                resource_name: None,
                parent_resource: None,
            },
            action: action.into(),
            outcome: EventOutcome::Success,
            details: HashMap::new(),
            ip_address: None,
            user_agent: None,
            session_id: None,
            correlation_id: None,
        }
    }

    pub fn with_actor(mut self, user_id: impl Into<String>, username: impl Into<String>) -> Self {
        self.actor.user_id = Some(user_id.into());
        self.actor.username = Some(username.into());
        self.actor.actor_type = ActorType::User;
        self
    }

    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.actor.email = Some(email.into());
        self
    }

    pub fn with_actor_type(mut self, actor_type: ActorType) -> Self {
        self.actor.actor_type = actor_type;
        self
    }

    pub fn with_resource(
        mut self,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        self.resource.resource_type = resource_type.into();
        self.resource.resource_id = resource_id.into();
        self
    }

    pub fn with_resource_name(mut self, name: impl Into<String>) -> Self {
        self.resource.resource_name = Some(name.into());
        self
    }

    pub fn with_outcome(mut self, outcome: EventOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_correlation(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }
}

#[allow(clippy::needless_borrows_for_generic_args, clippy::too_many_arguments)]
#[async_trait::async_trait]
pub trait ComplianceAuditStorage: Send + Sync {
    async fn store(&self, event: ComplianceAuditEvent) -> Result<(), AuditAggregationError>;

    async fn query(
        &self,
        tenant_id: Option<&str>,
        event_types: Option<&[ComplianceEventType]>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        user_id: Option<&str>,
        resource_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError>;

    async fn count(
        &self,
        tenant_id: Option<&str>,
        event_types: Option<&[ComplianceEventType]>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<u64, AuditAggregationError>;

    async fn delete_before(
        &self,
        before: DateTime<Utc>,
        tenant_id: Option<&str>,
    ) -> Result<u64, AuditAggregationError>;

    async fn get_events_for_user(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError>;

    async fn get_events_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
        limit: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError>;
}

pub struct InMemoryComplianceAuditStorage {
    events: Arc<RwLock<Vec<ComplianceAuditEvent>>>,
    max_events: usize,
}

impl InMemoryComplianceAuditStorage {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            max_events,
        }
    }
}

#[async_trait::async_trait]
impl ComplianceAuditStorage for InMemoryComplianceAuditStorage {
    async fn store(&self, event: ComplianceAuditEvent) -> Result<(), AuditAggregationError> {
        let mut events = self.events.write().await;
        events.push(event);
        if events.len() > self.max_events {
            let drain_count = events.len() - self.max_events;
            events.drain(0..drain_count);
        }
        Ok(())
    }

    async fn query(
        &self,
        tenant_id: Option<&str>,
        event_types: Option<&[ComplianceEventType]>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        user_id: Option<&str>,
        resource_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError> {
        let events = self.events.read().await;
        let filtered: Vec<ComplianceAuditEvent> = events
            .iter()
            .filter(|e| {
                if let Some(t) = tenant_id {
                    if e.tenant_id != t {
                        return false;
                    }
                }
                if let Some(types) = event_types {
                    if !types.contains(&e.event_type) {
                        return false;
                    }
                }
                if let Some(start) = start_time {
                    if e.timestamp < start {
                        return false;
                    }
                }
                if let Some(end) = end_time {
                    if e.timestamp > end {
                        return false;
                    }
                }
                if let Some(uid) = user_id {
                    if e.actor.user_id.as_deref() != Some(uid) {
                        return false;
                    }
                }
                if let Some(rt) = resource_type {
                    if e.resource.resource_type != rt {
                        return false;
                    }
                }
                true
            })
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn count(
        &self,
        tenant_id: Option<&str>,
        event_types: Option<&[ComplianceEventType]>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<u64, AuditAggregationError> {
        let events = self.events.read().await;
        let count = events
            .iter()
            .filter(|e| {
                if let Some(t) = tenant_id {
                    if e.tenant_id != t {
                        return false;
                    }
                }
                if let Some(types) = event_types {
                    if !types.contains(&e.event_type) {
                        return false;
                    }
                }
                if let Some(start) = start_time {
                    if e.timestamp < start {
                        return false;
                    }
                }
                if let Some(end) = end_time {
                    if e.timestamp > end {
                        return false;
                    }
                }
                true
            })
            .count();
        Ok(count as u64)
    }

    async fn delete_before(
        &self,
        before: DateTime<Utc>,
        tenant_id: Option<&str>,
    ) -> Result<u64, AuditAggregationError> {
        let mut events = self.events.write().await;
        let initial_len = events.len();
        events.retain(|e| {
            if e.timestamp < before {
                if let Some(t) = tenant_id {
                    return e.tenant_id != t;
                }
                return false;
            }
            true
        });
        Ok((initial_len - events.len()) as u64)
    }

    async fn get_events_for_user(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError> {
        self.query(tenant_id, None, None, None, Some(user_id), None, limit, 0)
            .await
    }

    async fn get_events_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
        limit: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError> {
        let events = self.events.read().await;
        let filtered: Vec<ComplianceAuditEvent> = events
            .iter()
            .filter(|e| {
                e.resource.resource_type == resource_type && e.resource.resource_id == resource_id
            })
            .take(limit)
            .cloned()
            .collect();
        Ok(filtered)
    }
}

/// File-based append-only audit log storage.
/// Each event is written as a single JSON line (JSONL) to the log file.
/// Reads scan the file sequentially — suitable for moderate volumes.
/// For high-volume production, replace with a database-backed implementation.
pub struct FileBasedAuditStorage {
    path: std::path::PathBuf,
    max_file_size_bytes: u64,
}

impl FileBasedAuditStorage {
    pub fn new(path: std::path::PathBuf, max_file_size_bytes: u64) -> Self {
        Self {
            path,
            max_file_size_bytes,
        }
    }

    fn read_all_events(&self) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| AuditAggregationError::Storage(format!("Failed to read audit log: {}", e)))?;
        let mut events = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<ComplianceAuditEvent>(line) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!("Skipping malformed audit event: {}", e);
                }
            }
        }
        Ok(events)
    }

    fn matches_filter(
        e: &ComplianceAuditEvent,
        tenant_id: Option<&str>,
        event_types: Option<&[ComplianceEventType]>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        user_id: Option<&str>,
        resource_type: Option<&str>,
    ) -> bool {
        if let Some(t) = tenant_id {
            if e.tenant_id != t {
                return false;
            }
        }
        if let Some(types) = event_types {
            if !types.contains(&e.event_type) {
                return false;
            }
        }
        if let Some(start) = start_time {
            if e.timestamp < start {
                return false;
            }
        }
        if let Some(end) = end_time {
            if e.timestamp > end {
                return false;
            }
        }
        if let Some(uid) = user_id {
            if e.actor.user_id.as_deref() != Some(uid) {
                return false;
            }
        }
        if let Some(rt) = resource_type {
            if e.resource.resource_type != rt {
                return false;
            }
        }
        true
    }
}

#[async_trait::async_trait]
impl ComplianceAuditStorage for FileBasedAuditStorage {
    async fn store(&self, event: ComplianceAuditEvent) -> Result<(), AuditAggregationError> {
        // Check file size limit before appending
        if self.path.exists() {
            if let Ok(metadata) = std::fs::metadata(&self.path) {
                if metadata.len() > self.max_file_size_bytes {
                    tracing::warn!(
                        "Audit log file exceeds {} bytes, rotating",
                        self.max_file_size_bytes
                    );
                    // Rotate: rename current file and start fresh
                    let rotated_path = format!("{}.{}", self.path.display(), chrono::Utc::now().format("%Y%m%d%H%M%S"));
                    let _ = std::fs::rename(&self.path, &rotated_path);
                }
            }
        }

        let mut line = serde_json::to_string(&event)
            .map_err(|e| AuditAggregationError::Storage(format!("Serialization failed: {}", e)))?;
        line.push('\n');

        // Use OpenOptions::append for atomic appends
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| AuditAggregationError::Storage(format!("Failed to open audit log: {}", e)))?;

        file.write_all(line.as_bytes())
            .map_err(|e| AuditAggregationError::Storage(format!("Failed to write audit event: {}", e)))?;

        file.sync_all()
            .map_err(|e| AuditAggregationError::Storage(format!("Failed to sync audit log: {}", e)))?;

        Ok(())
    }

    async fn query(
        &self,
        tenant_id: Option<&str>,
        event_types: Option<&[ComplianceEventType]>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        user_id: Option<&str>,
        resource_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError> {
        let events = self.read_all_events()?;
        let filtered: Vec<ComplianceAuditEvent> = events
            .iter()
            .filter(|e| {
                Self::matches_filter(
                    e,
                    tenant_id,
                    event_types,
                    start_time,
                    end_time,
                    user_id,
                    resource_type,
                )
            })
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn count(
        &self,
        tenant_id: Option<&str>,
        event_types: Option<&[ComplianceEventType]>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<u64, AuditAggregationError> {
        let events = self.read_all_events()?;
        let count = events
            .iter()
            .filter(|e| {
                Self::matches_filter(e, tenant_id, event_types, start_time, end_time, None, None)
            })
            .count();
        Ok(count as u64)
    }

    async fn delete_before(
        &self,
        before: DateTime<Utc>,
        tenant_id: Option<&str>,
    ) -> Result<u64, AuditAggregationError> {
        let events = self.read_all_events()?;
        let initial_count = events.len();
        let retained: Vec<ComplianceAuditEvent> = events
            .into_iter()
            .filter(|e| {
                if e.timestamp >= before {
                    return true;
                }
                if let Some(t) = tenant_id {
                    return e.tenant_id != t;
                }
                false
            })
            .collect();
        let deleted = initial_count - retained.len();

        // Rewrite the file with retained events
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&self.path)
            .map_err(|e| AuditAggregationError::Storage(format!("Failed to rewrite audit log: {}", e)))?;

        for event in &retained {
            let mut line = serde_json::to_string(event)
                .map_err(|e| AuditAggregationError::Storage(format!("Serialization failed: {}", e)))?;
            line.push('\n');
            file.write_all(line.as_bytes())
                .map_err(|e| AuditAggregationError::Storage(format!("Failed to write audit event: {}", e)))?;
        }
        file.sync_all()
            .map_err(|e| AuditAggregationError::Storage(format!("Failed to sync audit log: {}", e)))?;

        Ok(deleted as u64)
    }

    async fn get_events_for_user(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError> {
        self.query(tenant_id, None, None, None, Some(user_id), None, limit, 0)
            .await
    }

    async fn get_events_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
        limit: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, AuditAggregationError> {
        let events = self.read_all_events()?;
        let filtered: Vec<ComplianceAuditEvent> = events
            .iter()
            .filter(|e| {
                e.resource.resource_type == resource_type && e.resource.resource_id == resource_id
            })
            .take(limit)
            .cloned()
            .collect();
        Ok(filtered)
    }
}

pub struct AuditAggregator<S: ComplianceAuditStorage> {
    storage: Arc<S>,
}

impl<S: ComplianceAuditStorage> AuditAggregator<S> {
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    pub async fn log(&self, event: ComplianceAuditEvent) -> Result<(), AuditAggregationError> {
        self.storage.store(event).await
    }

    pub async fn get_user_activity_summary(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<UserActivitySummary, AuditAggregationError> {
        let events = self
            .storage
            .query(
                tenant_id,
                None,
                start_time,
                end_time,
                Some(user_id),
                None,
                10000,
                0,
            )
            .await?;

        let mut summary = UserActivitySummary {
            user_id: user_id.to_string(),
            total_events: events.len() as u64,
            ..Default::default()
        };

        for event in &events {
            match event.outcome {
                EventOutcome::Success => summary.successful_actions += 1,
                EventOutcome::Failure => summary.failed_actions += 1,
                EventOutcome::Denied => summary.denied_actions += 1,
                EventOutcome::Partial => summary.partial_actions += 1,
            }
            *summary
                .events_by_type
                .entry(format!("{:?}", event.event_type))
                .or_insert(0) += 1;
        }

        if let Some(first) = events.first() {
            summary.first_activity = Some(first.timestamp);
        }
        if let Some(last) = events.last() {
            summary.last_activity = Some(last.timestamp);
        }

        Ok(summary)
    }

    pub async fn get_tenant_audit_summary(
        &self,
        tenant_id: &str,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<TenantAuditSummary, AuditAggregationError> {
        let events = self
            .storage
            .query(
                Some(tenant_id),
                None,
                start_time,
                end_time,
                None,
                None,
                100000,
                0,
            )
            .await?;

        let mut summary = TenantAuditSummary {
            tenant_id: tenant_id.to_string(),
            total_events: events.len() as u64,
            ..Default::default()
        };

        let mut unique_users = std::collections::HashSet::new();
        for event in &events {
            if let Some(ref user_id) = event.actor.user_id {
                unique_users.insert(user_id.clone());
            }
            match event.outcome {
                EventOutcome::Success => summary.successful_actions += 1,
                EventOutcome::Failure => summary.failed_actions += 1,
                EventOutcome::Denied => summary.denied_actions += 1,
                EventOutcome::Partial => summary.partial_actions += 1,
            }
            *summary
                .events_by_type
                .entry(format!("{:?}", event.event_type))
                .or_insert(0) += 1;
        }
        summary.unique_users = unique_users.len() as u64;

        Ok(summary)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserActivitySummary {
    pub user_id: String,
    pub total_events: u64,
    pub successful_actions: u64,
    pub failed_actions: u64,
    pub denied_actions: u64,
    pub partial_actions: u64,
    pub events_by_type: HashMap<String, u64>,
    pub first_activity: Option<DateTime<Utc>>,
    pub last_activity: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TenantAuditSummary {
    pub tenant_id: String,
    pub total_events: u64,
    pub unique_users: u64,
    pub successful_actions: u64,
    pub failed_actions: u64,
    pub denied_actions: u64,
    pub partial_actions: u64,
    pub events_by_type: HashMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compliance_audit_event_creation() {
        let event = ComplianceAuditEvent::new(
            ComplianceEventType::DataAccess,
            "tenant-1",
            "read_repository",
        )
        .with_actor("user-1", "testuser")
        .with_email("test@example.com")
        .with_resource("repository", "repo-1")
        .with_resource_name("my-repo")
        .with_ip("192.168.1.1")
        .with_outcome(EventOutcome::Success);

        assert_eq!(event.tenant_id, "tenant-1");
        assert_eq!(event.actor.user_id, Some("user-1".to_string()));
        assert_eq!(event.resource.resource_type, "repository");
        assert_eq!(event.outcome, EventOutcome::Success);
    }

    #[tokio::test]
    async fn test_in_memory_storage() {
        let storage = InMemoryComplianceAuditStorage::new(1000);
        let event =
            ComplianceAuditEvent::new(ComplianceEventType::UserAuthentication, "tenant-1", "login");

        storage.store(event).await.unwrap();

        let results = storage
            .query(Some("tenant-1"), None, None, None, None, None, 10, 0)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_audit_aggregator() {
        let storage = Arc::new(InMemoryComplianceAuditStorage::new(1000));
        let aggregator = AuditAggregator::new(storage.clone());

        let event = ComplianceAuditEvent::new(
            ComplianceEventType::DataAccess,
            "tenant-1",
            "read_repository",
        )
        .with_actor("user-1", "testuser");

        aggregator.log(event).await.unwrap();

        let summary = aggregator
            .get_user_activity_summary("user-1", Some("tenant-1"), None, None)
            .await
            .unwrap();

        assert_eq!(summary.total_events, 1);
        assert_eq!(summary.successful_actions, 1);
    }
}

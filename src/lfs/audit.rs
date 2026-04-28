use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum AuditError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditEventType {
    LfsBatchRequest,
    LfsUpload,
    LfsDownload,
    LfsDelete,
    LfsQuotaExceeded,
    LfsVerifyFailed,
    LfsError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub tenant: String,
    pub repo: String,
    pub user: Option<String>,
    pub ip_address: Option<String>,
    pub oid: Option<String>,
    pub size: Option<u64>,
    pub success: bool,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl AuditEvent {
    pub fn new(event_type: AuditEventType, tenant: String, repo: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            tenant,
            repo,
            user: None,
            ip_address: None,
            oid: None,
            size: None,
            success: true,
            error_message: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    pub fn with_oid(mut self, oid: impl Into<String>) -> Self {
        self.oid = Some(oid.into());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.success = false;
        self.error_message = Some(error.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

pub struct AuditLogConfig {
    pub enabled: bool,
    pub retention_days: u32,
    pub include_ip_addresses: bool,
    pub log_successful_operations: bool,
    pub log_failed_operations: bool,
}

impl From<crate::config::LfsAuditConfig> for AuditLogConfig {
    fn from(config: crate::config::LfsAuditConfig) -> Self {
        Self {
            enabled: config.enabled,
            retention_days: config.retention_days,
            include_ip_addresses: config.include_ip_addresses,
            log_successful_operations: config.log_successful_operations,
            log_failed_operations: config.log_failed_operations,
        }
    }
}

impl Default for AuditLogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: 90,
            include_ip_addresses: true,
            log_successful_operations: true,
            log_failed_operations: true,
        }
    }
}

#[async_trait::async_trait]
pub trait AuditStorage: Send + Sync {
    async fn store(&self, event: AuditEvent) -> Result<(), AuditError>;
    async fn query(
        &self,
        tenant: Option<&str>,
        repo: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, AuditError>;
}

pub struct InMemoryAuditStorage {
    events: Arc<RwLock<Vec<AuditEvent>>>,
    max_events: usize,
}

impl InMemoryAuditStorage {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            max_events,
        }
    }
}

#[async_trait::async_trait]
impl AuditStorage for InMemoryAuditStorage {
    async fn store(&self, event: AuditEvent) -> Result<(), AuditError> {
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
        tenant: Option<&str>,
        repo: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        let events = self.events.read().await;

        let filtered: Vec<AuditEvent> = events
            .iter()
            .filter(|e| {
                if let Some(t) = tenant {
                    if e.tenant != t {
                        return false;
                    }
                }
                if let Some(r) = repo {
                    if e.repo != r {
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
            .rev()
            .take(limit)
            .cloned()
            .collect();

        Ok(filtered)
    }
}

pub struct AuditLogger<S: AuditStorage> {
    storage: Arc<S>,
    config: AuditLogConfig,
}

impl<S: AuditStorage> AuditLogger<S> {
    pub fn new(storage: Arc<S>, config: AuditLogConfig) -> Self {
        Self { storage, config }
    }

    pub async fn log(&self, event: AuditEvent) {
        if !self.config.enabled {
            return;
        }

        if !self.config.log_successful_operations && event.success {
            return;
        }

        if !self.config.log_failed_operations && !event.success {
            return;
        }

        if let Err(e) = self.storage.store(event).await {
            tracing::error!("Failed to store audit event: {}", e);
        }
    }

    pub async fn query(
        &self,
        tenant: Option<&str>,
        repo: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        self.storage
            .query(tenant, repo, start_time, end_time, limit)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_builder() {
        let event = AuditEvent::new(
            AuditEventType::LfsUpload,
            "tenant1".to_string(),
            "repo1".to_string(),
        )
        .with_user("user1")
        .with_ip("192.168.1.1")
        .with_oid("abc123")
        .with_size(1024)
        .with_metadata("operation", "upload");

        assert_eq!(event.event_type, AuditEventType::LfsUpload);
        assert_eq!(event.tenant, "tenant1");
        assert_eq!(event.repo, "repo1");
        assert_eq!(event.user, Some("user1".to_string()));
        assert_eq!(event.ip_address, Some("192.168.1.1".to_string()));
        assert_eq!(event.oid, Some("abc123".to_string()));
        assert_eq!(event.size, Some(1024));
        assert!(event.success);
        assert_eq!(event.metadata.get("operation"), Some(&"upload".to_string()));
    }

    #[test]
    fn test_audit_event_with_error() {
        let event = AuditEvent::new(
            AuditEventType::LfsError,
            "tenant1".to_string(),
            "repo1".to_string(),
        )
        .with_error("Hash mismatch");

        assert!(!event.success);
        assert_eq!(event.error_message, Some("Hash mismatch".to_string()));
    }

    #[tokio::test]
    async fn test_in_memory_storage() {
        let storage = InMemoryAuditStorage::new(1000);

        let event = AuditEvent::new(
            AuditEventType::LfsUpload,
            "tenant1".to_string(),
            "repo1".to_string(),
        );

        storage.store(event.clone()).await.unwrap();

        let results = storage
            .query(Some("tenant1"), Some("repo1"), None, None, 10)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tenant, "tenant1");
    }

    #[tokio::test]
    async fn test_storage_query_filtering() {
        let storage = InMemoryAuditStorage::new(1000);

        storage
            .store(AuditEvent::new(
                AuditEventType::LfsUpload,
                "tenant1".to_string(),
                "repo1".to_string(),
            ))
            .await
            .unwrap();

        storage
            .store(AuditEvent::new(
                AuditEventType::LfsDownload,
                "tenant1".to_string(),
                "repo2".to_string(),
            ))
            .await
            .unwrap();

        storage
            .store(AuditEvent::new(
                AuditEventType::LfsUpload,
                "tenant2".to_string(),
                "repo1".to_string(),
            ))
            .await
            .unwrap();

        let results = storage
            .query(Some("tenant1"), None, None, None, 10)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.tenant == "tenant1"));
    }

    #[tokio::test]
    async fn test_storage_max_events() {
        let storage = InMemoryAuditStorage::new(5);

        for i in 0..10 {
            storage
                .store(AuditEvent::new(
                    AuditEventType::LfsUpload,
                    format!("tenant{}", i),
                    "repo1".to_string(),
                ))
                .await
                .unwrap();
        }

        let results = storage.query(None, None, None, None, 100).await.unwrap();

        assert_eq!(results.len(), 5);
    }

    #[tokio::test]
    async fn test_audit_logger_disabled() {
        let storage = Arc::new(InMemoryAuditStorage::new(1000));
        let config = AuditLogConfig {
            enabled: false,
            ..Default::default()
        };
        let logger = AuditLogger::new(storage.clone(), config);

        let event = AuditEvent::new(
            AuditEventType::LfsUpload,
            "tenant1".to_string(),
            "repo1".to_string(),
        );

        logger.log(event).await;

        let results = storage.query(None, None, None, None, 10).await.unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_audit_logger_filter_success() {
        let storage = Arc::new(InMemoryAuditStorage::new(1000));
        let config = AuditLogConfig {
            enabled: true,
            log_successful_operations: false,
            log_failed_operations: true,
            ..Default::default()
        };
        let logger = AuditLogger::new(storage.clone(), config);

        logger
            .log(AuditEvent::new(
                AuditEventType::LfsUpload,
                "tenant1".to_string(),
                "repo1".to_string(),
            ))
            .await;

        logger
            .log(
                AuditEvent::new(
                    AuditEventType::LfsError,
                    "tenant1".to_string(),
                    "repo1".to_string(),
                )
                .with_error("Test error"),
            )
            .await;

        let results = storage.query(None, None, None, None, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
    }
}

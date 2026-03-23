use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use super::audit::{ComplianceAuditEvent, ComplianceAuditStorage, ComplianceEventType, EventOutcome};

#[derive(Debug, Error)]
pub enum PrivacyError {
    #[error("Data export failed: {0}")]
    ExportFailed(String),
    #[error("Data anonymization failed: {0}")]
    AnonymizationFailed(String),
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type PrivacyResult<T> = std::result::Result<T, PrivacyError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDataExport {
    pub export_id: String,
    pub user_id: String,
    pub tenant_id: String,
    pub exported_at: DateTime<Utc>,
    pub data_categories: Vec<DataCategoryExport>,
    pub export_format: ExportFormat,
    pub expires_at: DateTime<Utc>,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataCategoryExport {
    pub category: String,
    pub description: String,
    pub record_count: u64,
    pub data: serde_json::Value,
    pub exported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Csv,
    Xml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnonymizationResult {
    pub anonymization_id: String,
    pub performed_at: DateTime<Utc>,
    pub user_id: String,
    pub tenant_id: String,
    pub categories_anonymized: Vec<String>,
    pub records_affected: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataDeletionRequest {
    pub request_id: String,
    pub user_id: String,
    pub tenant_id: String,
    pub requested_at: DateTime<Utc>,
    pub status: DeletionStatus,
    pub completed_at: Option<DateTime<Utc>>,
    pub categories_deleted: Vec<String>,
    pub records_deleted: u64,
    pub anonymized_instead: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeletionStatus {
    Pending,
    Processing,
    Completed,
    PartiallyCompleted,
    Failed,
}

pub struct DataAnonymizer {
    strategies: HashMap<String, Box<dyn AnonymizationStrategy + Send + Sync>>,
}

impl DataAnonymizer {
    pub fn new() -> Self {
        let mut strategies: HashMap<String, Box<dyn AnonymizationStrategy + Send + Sync>> =
            HashMap::new();
        strategies.insert("email".to_string(), Box::new(EmailAnonymizer));
        strategies.insert("name".to_string(), Box::new(NameAnonymizer));
        strategies.insert("ip_address".to_string(), Box::new(IpAddressAnonymizer));
        strategies.insert("phone".to_string(), Box::new(PhoneAnonymizer));
        strategies.insert("default".to_string(), Box::new(HashAnonymizer));

        Self { strategies }
    }

    pub fn anonymize(
        &self,
        field_type: &str,
        value: &str,
    ) -> Result<String, PrivacyError> {
        let strategy = self
            .strategies
            .get(field_type)
            .or(self.strategies.get("default"))
            .ok_or_else(|| {
                PrivacyError::AnonymizationFailed(format!(
                    "No strategy for field type: {}",
                    field_type
                ))
            })?;

        strategy.anonymize(value)
    }

    pub fn anonymize_record(
        &self,
        mut record: serde_json::Value,
        field_mappings: &HashMap<String, String>,
    ) -> Result<serde_json::Value, PrivacyError> {
        if let Some(obj) = record.as_object_mut() {
            for (field, field_type) in field_mappings {
                if let Some(value) = obj.get(field).and_then(|v| v.as_str()) {
                    let anonymized = self.anonymize(field_type, value)?;
                    obj.insert(field.clone(), serde_json::json!(anonymized));
                }
            }
        }
        Ok(record)
    }
}

impl Default for DataAnonymizer {
    fn default() -> Self {
        Self::new()
    }
}

trait AnonymizationStrategy {
    fn anonymize(&self, value: &str) -> Result<String, PrivacyError>;
}

struct EmailAnonymizer;

impl AnonymizationStrategy for EmailAnonymizer {
    fn anonymize(&self, value: &str) -> Result<String, PrivacyError> {
        let parts: Vec<&str> = value.split('@').collect();
        if parts.len() == 2 {
            let local = parts[0];
            let domain = parts[1];
            let anonymized_local: String = local
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    if i == 0 || i >= local.len() - 1 {
                        c
                    } else {
                        '*'
                    }
                })
                .collect();
            Ok(format!("{}@{}", anonymized_local, domain))
        } else {
            Ok("anonymized@example.com".to_string())
        }
    }
}

struct NameAnonymizer;

impl AnonymizationStrategy for NameAnonymizer {
    fn anonymize(&self, value: &str) -> Result<String, PrivacyError> {
        let chars: Vec<char> = value.chars().collect();
        if chars.is_empty() {
            return Ok("[REDACTED]".to_string());
        }
        let first = chars[0];
        let anonymized = format!("{}***", first);
        Ok(anonymized)
    }
}

struct IpAddressAnonymizer;

impl AnonymizationStrategy for IpAddressAnonymizer {
    fn anonymize(&self, value: &str) -> Result<String, PrivacyError> {
        let parts: Vec<&str> = value.split('.').collect();
        if parts.len() == 4 {
            Ok(format!("{}.{}.{}.0", parts[0], parts[1], parts[2]))
        } else {
            let v6_parts: Vec<&str> = value.split(':').collect();
            if v6_parts.len() >= 4 {
                Ok(format!(
                    "{}:{}::xxxx",
                    v6_parts[0], v6_parts[1]
                ))
            } else {
                Ok("0.0.0.0".to_string())
            }
        }
    }
}

struct PhoneAnonymizer;

impl AnonymizationStrategy for PhoneAnonymizer {
    fn anonymize(&self, value: &str) -> Result<String, PrivacyError> {
        let digits: String = value.chars().filter(|c| c.is_numeric()).collect();
        if digits.len() >= 4 {
            let last_four = &digits[digits.len() - 4..];
            Ok(format!("***-***-{}", last_four))
        } else {
            Ok("***-***-****".to_string())
        }
    }
}

struct HashAnonymizer;

impl AnonymizationStrategy for HashAnonymizer {
    fn anonymize(&self, value: &str) -> Result<String, PrivacyError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        let hash = hasher.finish();
        Ok(format!("anon_{}", hash))
    }
}

pub struct GdprDataExporter<S: ComplianceAuditStorage> {
    audit_storage: Arc<S>,
    anonymizer: DataAnonymizer,
}

impl<S: ComplianceAuditStorage> GdprDataExporter<S> {
    pub fn new(audit_storage: Arc<S>) -> Self {
        Self {
            audit_storage,
            anonymizer: DataAnonymizer::new(),
        }
    }

    pub async fn export_user_data(
        &self,
        user_id: &str,
        tenant_id: &str,
        anonymize: bool,
    ) -> Result<UserDataExport, PrivacyError> {
        let events = self
            .audit_storage
            .get_events_for_user(user_id, Some(tenant_id), 100000)
            .await
            .map_err(|e| PrivacyError::ExportFailed(e.to_string()))?;

        let mut data_categories = Vec::new();

        let mut auth_events: Vec<serde_json::Value> = Vec::new();
        let mut access_events: Vec<serde_json::Value> = Vec::new();
        let mut other_events: Vec<serde_json::Value> = Vec::new();

        for event in events {
            let event_json = serde_json::to_value(&event)?;

            if matches!(
                event.event_type,
                ComplianceEventType::UserAuthentication
                    | ComplianceEventType::UserAuthorization
            ) {
                auth_events.push(if anonymize {
                    self.anonymizer.anonymize_record(
                        event_json,
                        &self.get_anonymization_fields(),
                    )?
                } else {
                    event_json
                });
            } else if matches!(
                event.event_type,
                ComplianceEventType::DataAccess | ComplianceEventType::RepositoryAccess
            ) {
                access_events.push(if anonymize {
                    self.anonymizer.anonymize_record(
                        event_json,
                        &self.get_anonymization_fields(),
                    )?
                } else {
                    event_json
                });
            } else {
                other_events.push(if anonymize {
                    self.anonymizer.anonymize_record(
                        event_json,
                        &self.get_anonymization_fields(),
                    )?
                } else {
                    event_json
                });
            }
        }

        data_categories.push(DataCategoryExport {
            category: "authentication_events".to_string(),
            description: "Authentication and authorization events".to_string(),
            record_count: auth_events.len() as u64,
            data: serde_json::json!(auth_events),
            exported_at: Utc::now(),
        });

        data_categories.push(DataCategoryExport {
            category: "access_events".to_string(),
            description: "Data and resource access events".to_string(),
            record_count: access_events.len() as u64,
            data: serde_json::json!(access_events),
            exported_at: Utc::now(),
        });

        data_categories.push(DataCategoryExport {
            category: "other_events".to_string(),
            description: "Other user activities".to_string(),
            record_count: other_events.len() as u64,
            data: serde_json::json!(other_events),
            exported_at: Utc::now(),
        });

        let export = UserDataExport {
            export_id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            tenant_id: tenant_id.to_string(),
            exported_at: Utc::now(),
            data_categories,
            export_format: ExportFormat::Json,
            expires_at: Utc::now() + chrono::Duration::days(30),
            download_url: None,
        };

        Ok(export)
    }

    pub async fn request_data_deletion(
        &self,
        user_id: &str,
        tenant_id: &str,
        anonymize_instead: bool,
    ) -> Result<DataDeletionRequest, PrivacyError> {
        let request = DataDeletionRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            tenant_id: tenant_id.to_string(),
            requested_at: Utc::now(),
            status: DeletionStatus::Processing,
            completed_at: None,
            categories_deleted: Vec::new(),
            records_deleted: 0,
            anonymized_instead,
        };

        Ok(request)
    }

    fn get_anonymization_fields(&self) -> HashMap<String, String> {
        let mut fields = HashMap::new();
        fields.insert("ip_address".to_string(), "ip_address".to_string());
        fields.insert("email".to_string(), "email".to_string());
        fields.insert("username".to_string(), "name".to_string());
        fields
    }
}

pub struct PrivacyControls<S: ComplianceAuditStorage> {
    audit_storage: Arc<S>,
    exporter: GdprDataExporter<S>,
    anonymizer: DataAnonymizer,
}

impl<S: ComplianceAuditStorage> PrivacyControls<S> {
    pub fn new(audit_storage: Arc<S>) -> Self {
        let exporter = GdprDataExporter::new(audit_storage.clone());
        Self {
            audit_storage,
            exporter,
            anonymizer: DataAnonymizer::new(),
        }
    }

    pub async fn export_user_data(
        &self,
        user_id: &str,
        tenant_id: &str,
        anonymize: bool,
    ) -> Result<UserDataExport, PrivacyError> {
        let export = self.exporter.export_user_data(user_id, tenant_id, anonymize).await?;

        let event = ComplianceAuditEvent::new(
            ComplianceEventType::DataExport,
            tenant_id,
            "gdpr_data_export",
        )
        .with_actor(user_id, "user")
        .with_resource("data_export", &export.export_id)
        .with_detail("user_id", serde_json::json!(user_id))
        .with_detail("anonymized", serde_json::json!(anonymize));

        if let Err(e) = self.audit_storage.store(event).await {
            tracing::warn!("Failed to log data export event: {}", e);
        }

        Ok(export)
    }

    pub async fn delete_user_data(
        &self,
        user_id: &str,
        tenant_id: &str,
        anonymize_instead: bool,
    ) -> Result<DataDeletionRequest, PrivacyError> {
        let request = self
            .exporter
            .request_data_deletion(user_id, tenant_id, anonymize_instead)
            .await?;

        let event = ComplianceAuditEvent::new(
            ComplianceEventType::DataDeletion,
            tenant_id,
            "gdpr_data_deletion",
        )
        .with_actor(user_id, "user")
        .with_resource("deletion_request", &request.request_id)
        .with_detail("user_id", serde_json::json!(user_id))
        .with_detail("anonymize_instead", serde_json::json!(anonymize_instead));

        if let Err(e) = self.audit_storage.store(event).await {
            tracing::warn!("Failed to log data deletion event: {}", e);
        }

        Ok(request)
    }

    pub fn anonymize_data(
        &self,
        field_type: &str,
        value: &str,
    ) -> Result<String, PrivacyError> {
        self.anonymizer.anonymize(field_type, value)
    }

    pub async fn get_user_activity_log(
        &self,
        user_id: &str,
        tenant_id: &str,
        limit: usize,
    ) -> Result<Vec<ComplianceAuditEvent>, PrivacyError> {
        self.audit_storage
            .get_events_for_user(user_id, Some(tenant_id), limit)
            .await
            .map_err(|e| PrivacyError::Storage(e.to_string()))
    }

    pub async fn check_data_retention_compliance(
        &self,
        tenant_id: &str,
        retention_days: u32,
    ) -> Result<RetentionComplianceCheck, PrivacyError> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);

        let total_events = self
            .audit_storage
            .count(Some(tenant_id), None, None, None)
            .await
            .map_err(|e| PrivacyError::Storage(e.to_string()))?;

        let old_events = self
            .audit_storage
            .count(Some(tenant_id), None, None, Some(cutoff))
            .await
            .map_err(|e| PrivacyError::Storage(e.to_string()))?;

        Ok(RetentionComplianceCheck {
            tenant_id: tenant_id.to_string(),
            total_records: total_events,
            overdue_records: old_events,
            retention_days,
            cutoff_date: cutoff,
            compliant: old_events == 0,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionComplianceCheck {
    pub tenant_id: String,
    pub total_records: u64,
    pub overdue_records: u64,
    pub retention_days: u32,
    pub cutoff_date: DateTime<Utc>,
    pub compliant: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::audit::InMemoryComplianceAuditStorage;

    #[test]
    fn test_email_anonymization() {
        let anonymizer = DataAnonymizer::new();

        let result = anonymizer.anonymize("email", "test.user@example.com").unwrap();
        assert!(result.contains('@'));
        assert!(result.contains('*'));
        assert!(result.ends_with("@example.com"));
    }

    #[test]
    fn test_name_anonymization() {
        let anonymizer = DataAnonymizer::new();

        let result = anonymizer.anonymize("name", "John Doe").unwrap();
        assert!(result.starts_with('J'));
        assert!(result.contains('*'));
    }

    #[test]
    fn test_ip_address_anonymization() {
        let anonymizer = DataAnonymizer::new();

        let result = anonymizer.anonymize("ip_address", "192.168.1.100").unwrap();
        assert_eq!(result, "192.168.1.0");
    }

    #[test]
    fn test_phone_anonymization() {
        let anonymizer = DataAnonymizer::new();

        let result = anonymizer.anonymize("phone", "+1-555-123-4567").unwrap();
        assert!(result.contains("4567"));
        assert!(result.contains('*'));
    }

    #[tokio::test]
    async fn test_export_user_data() {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(10000));
        let exporter = GdprDataExporter::new(audit_storage);

        let export = exporter
            .export_user_data("user-1", "tenant-1", true)
            .await
            .unwrap();

        assert_eq!(export.user_id, "user-1");
        assert_eq!(export.tenant_id, "tenant-1");
        assert!(!export.data_categories.is_empty());
    }

    #[tokio::test]
    async fn test_privacy_controls() {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(10000));
        let controls = PrivacyControls::new(audit_storage);

        let export = controls
            .export_user_data("user-1", "tenant-1", true)
            .await
            .unwrap();

        assert_eq!(export.user_id, "user-1");
    }
}

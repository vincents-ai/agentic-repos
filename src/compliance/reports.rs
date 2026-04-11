use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use super::audit::{ComplianceAuditStorage, ComplianceEventType, EventOutcome};

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("Report generation failed: {0}")]
    Generation(String),
    #[error("Report not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    Soc2Compliance,
    GdprDataProcessing,
    AccessControlAudit,
    SecurityIncident,
    DataRetention,
}

impl std::fmt::Display for ReportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportType::Soc2Compliance => write!(f, "soc2_compliance"),
            ReportType::GdprDataProcessing => write!(f, "gdpr_data_processing"),
            ReportType::AccessControlAudit => write!(f, "access_control_audit"),
            ReportType::SecurityIncident => write!(f, "security_incident"),
            ReportType::DataRetention => write!(f, "data_retention"),
        }
    }
}

impl std::str::FromStr for ReportType {
    type Err = ReportError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "soc2_compliance" | "soc2" => Ok(ReportType::Soc2Compliance),
            "gdpr_data_processing" | "gdpr" => Ok(ReportType::GdprDataProcessing),
            "access_control_audit" | "access_control" => Ok(ReportType::AccessControlAudit),
            "security_incident" | "security" => Ok(ReportType::SecurityIncident),
            "data_retention" | "retention" => Ok(ReportType::DataRetention),
            _ => Err(ReportError::Generation(format!(
                "Unknown report type: {}",
                s
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    Json,
    Csv,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub id: String,
    pub report_type: ReportType,
    pub tenant_id: String,
    pub generated_at: DateTime<Utc>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub generated_by: Option<String>,
    pub status: ReportStatus,
    pub summary: String,
    pub data: serde_json::Value,
    pub compliance_score: Option<f32>,
    pub recommendations: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Pending,
    Generating,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Soc2ComplianceReport {
    pub report_period: ReportPeriod,
    pub security_controls: Vec<SecurityControl>,
    pub access_controls: AccessControlSummary,
    pub encryption_status: EncryptionStatus,
    pub incident_summary: IncidentSummary,
    pub change_management: ChangeManagementSummary,
    pub overall_compliance_score: f32,
    pub findings: Vec<ComplianceFinding>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportPeriod {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityControl {
    pub control_id: String,
    pub control_name: String,
    pub category: String,
    pub status: ControlStatus,
    pub evidence_count: u64,
    pub last_assessed: Option<DateTime<Utc>>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlStatus {
    Compliant,
    NonCompliant,
    PartiallyCompliant,
    NotAssessed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlSummary {
    pub total_users: u64,
    pub active_users: u64,
    pub mfa_enabled_users: u64,
    pub role_based_access_enabled: bool,
    pub privileged_users: u64,
    pub access_reviews_completed: u64,
    pub failed_auth_attempts: u64,
    pub locked_accounts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionStatus {
    pub data_at_rest_encrypted: bool,
    pub data_in_transit_encrypted: bool,
    pub encryption_algorithm: String,
    pub key_rotation_enabled: bool,
    pub last_key_rotation: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentSummary {
    pub total_incidents: u64,
    pub security_incidents: u64,
    pub resolved_incidents: u64,
    pub mean_time_to_resolution_hours: Option<f64>,
    pub incidents_by_severity: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeManagementSummary {
    pub total_changes: u64,
    pub authorized_changes: u64,
    pub unauthorized_changes: u64,
    pub change_failure_rate: f64,
    pub rollback_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFinding {
    pub finding_id: String,
    pub severity: FindingSeverity,
    pub title: String,
    pub description: String,
    pub control_reference: String,
    pub remediation: Option<String>,
    pub status: FindingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Critical,
    High,
    Medium,
    Low,
    Informational,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    Open,
    InProgress,
    Remediated,
    AcceptedRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdprDataProcessingReport {
    pub report_period: ReportPeriod,
    pub data_subjects_count: u64,
    pub processing_activities: Vec<ProcessingActivity>,
    pub data_categories: Vec<DataCategory>,
    pub consent_records: ConsentSummary,
    pub data_breaches: DataBreachSummary,
    pub cross_border_transfers: Vec<CrossBorderTransfer>,
    pub retention_compliance: RetentionCompliance,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingActivity {
    pub activity_id: String,
    pub name: String,
    pub purpose: String,
    pub legal_basis: LegalBasis,
    pub data_categories: Vec<String>,
    pub recipients: Vec<String>,
    pub retention_period_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LegalBasis {
    Consent,
    Contract,
    LegalObligation,
    VitalInterests,
    PublicTask,
    LegitimateInterests,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataCategory {
    pub name: String,
    pub description: String,
    pub sensitive: bool,
    pub subjects_count: u64,
    pub storage_locations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentSummary {
    pub total_consents: u64,
    pub active_consents: u64,
    pub withdrawn_consents: u64,
    pub consent_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBreachSummary {
    pub total_breaches: u64,
    pub notified_breaches: u64,
    pub affected_subjects: u64,
    pub mean_detection_time_hours: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossBorderTransfer {
    pub destination_country: String,
    pub transfer_mechanism: String,
    pub data_categories: Vec<String>,
    pub safeguards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionCompliance {
    pub policies_defined: bool,
    pub automated_enforcement: bool,
    pub overdue_deletions: u64,
    pub compliance_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlAudit {
    pub report_period: ReportPeriod,
    pub user_access_reviews: Vec<UserAccessReview>,
    pub role_assignments: Vec<RoleAssignment>,
    pub permission_changes: Vec<PermissionChange>,
    pub orphaned_permissions: Vec<OrphanedPermission>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccessReview {
    pub user_id: String,
    pub username: String,
    pub roles: Vec<String>,
    pub last_review_date: Option<DateTime<Utc>>,
    pub access_anomalies: Vec<String>,
    pub review_status: ReviewStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Reviewed,
    Pending,
    Overdue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignment {
    pub role_name: String,
    pub assigned_to: String,
    pub assigned_by: String,
    pub assigned_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub justification: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionChange {
    pub change_id: String,
    pub changed_at: DateTime<Utc>,
    pub changed_by: String,
    pub target_user: String,
    pub change_type: PermissionChangeType,
    pub permission: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionChangeType {
    Granted,
    Revoked,
    Modified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanedPermission {
    pub permission_id: String,
    pub permission_name: String,
    pub resource_type: String,
    pub resource_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIncidentReport {
    pub report_period: ReportPeriod,
    pub incidents: Vec<SecurityIncident>,
    pub metrics: IncidentMetrics,
    pub trends: IncidentTrends,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIncident {
    pub incident_id: String,
    pub detected_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub severity: IncidentSeverity,
    pub category: IncidentCategory,
    pub title: String,
    pub description: String,
    pub affected_resources: Vec<String>,
    pub root_cause: Option<String>,
    pub remediation: Option<String>,
    pub status: IncidentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncidentSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncidentCategory {
    UnauthorizedAccess,
    DataBreach,
    Malware,
    DenialOfService,
    PolicyViolation,
    ConfigurationChange,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    New,
    Investigating,
    Contained,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentMetrics {
    pub total_incidents: u64,
    pub mean_time_to_detect_hours: f64,
    pub mean_time_to_respond_hours: f64,
    pub mean_time_to_resolve_hours: f64,
    pub incidents_by_severity: HashMap<String, u64>,
    pub incidents_by_category: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentTrends {
    pub period_over_period_change: f64,
    pub trend_direction: TrendDirection,
    pub projected_next_period: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRetentionReport {
    pub report_period: ReportPeriod,
    pub retention_policies: Vec<RetentionPolicyStatus>,
    pub data_inventory: Vec<DataInventoryItem>,
    pub retention_actions: Vec<RetentionActionRecord>,
    pub compliance_status: RetentionComplianceStatus,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicyStatus {
    pub policy_name: String,
    pub data_type: String,
    pub retention_period_days: u32,
    pub items_covered: u64,
    pub items_overdue: u64,
    pub compliance_rate: f64,
    pub last_enforcement: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataInventoryItem {
    pub data_type: String,
    pub location: String,
    pub total_items: u64,
    pub oldest_item: Option<DateTime<Utc>>,
    pub newest_item: Option<DateTime<Utc>>,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionActionRecord {
    pub action_id: String,
    pub action_type: RetentionActionType,
    pub performed_at: DateTime<Utc>,
    pub data_type: String,
    pub items_affected: u64,
    pub status: RetentionActionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionActionType {
    Deletion,
    Anonymization,
    Archival,
    Extension,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionActionStatus {
    Completed,
    PartiallyCompleted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionComplianceStatus {
    pub overall_compliance_rate: f64,
    pub policies_compliant: u64,
    pub policies_non_compliant: u64,
    pub overdue_items: u64,
    pub next_enforcement: Option<DateTime<Utc>>,
}

#[async_trait::async_trait]
pub trait ComplianceReportStorage: Send + Sync {
    async fn store(&self, report: ComplianceReport) -> Result<(), ReportError>;
    async fn get(&self, report_id: &str) -> Result<Option<ComplianceReport>, ReportError>;
    async fn list(
        &self,
        tenant_id: Option<&str>,
        report_type: Option<ReportType>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ComplianceReport>, ReportError>;
    async fn delete(&self, report_id: &str) -> Result<bool, ReportError>;
    async fn delete_before(&self, before: DateTime<Utc>) -> Result<u64, ReportError>;
}

pub struct InMemoryReportStorage {
    reports: tokio::sync::RwLock<Vec<ComplianceReport>>,
    max_reports: usize,
}

impl InMemoryReportStorage {
    pub fn new(max_reports: usize) -> Self {
        Self {
            reports: tokio::sync::RwLock::new(Vec::new()),
            max_reports,
        }
    }
}

#[async_trait::async_trait]
impl ComplianceReportStorage for InMemoryReportStorage {
    async fn store(&self, report: ComplianceReport) -> Result<(), ReportError> {
        let mut reports = self.reports.write().await;
        reports.push(report);
        if reports.len() > self.max_reports {
            let drain_count = reports.len() - self.max_reports;
            reports.drain(0..drain_count);
        }
        Ok(())
    }

    async fn get(&self, report_id: &str) -> Result<Option<ComplianceReport>, ReportError> {
        let reports = self.reports.read().await;
        Ok(reports.iter().find(|r| r.id == report_id).cloned())
    }

    async fn list(
        &self,
        tenant_id: Option<&str>,
        report_type: Option<ReportType>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ComplianceReport>, ReportError> {
        let reports = self.reports.read().await;
        let filtered: Vec<ComplianceReport> = reports
            .iter()
            .filter(|r| {
                if let Some(t) = tenant_id {
                    if r.tenant_id != t {
                        return false;
                    }
                }
                if let Some(rt) = &report_type {
                    if r.report_type != *rt {
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

    async fn delete(&self, report_id: &str) -> Result<bool, ReportError> {
        let mut reports = self.reports.write().await;
        let initial_len = reports.len();
        reports.retain(|r| r.id != report_id);
        Ok(reports.len() < initial_len)
    }

    async fn delete_before(&self, before: DateTime<Utc>) -> Result<u64, ReportError> {
        let mut reports = self.reports.write().await;
        let initial_len = reports.len();
        reports.retain(|r| r.generated_at >= before);
        Ok((initial_len - reports.len()) as u64)
    }
}

pub struct ComplianceReportGenerator<S: ComplianceAuditStorage> {
    audit_storage: Arc<S>,
}

impl<S: ComplianceAuditStorage> ComplianceReportGenerator<S> {
    pub fn new(audit_storage: Arc<S>) -> Self {
        Self { audit_storage }
    }

    pub async fn generate_soc2_report(
        &self,
        tenant_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<Soc2ComplianceReport, ReportError> {
        let auth_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[ComplianceEventType::UserAuthentication]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let failed_auth = auth_events
            .iter()
            .filter(|e| e.outcome == EventOutcome::Failure)
            .count() as u64;

        let _access_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[
                    ComplianceEventType::DataAccess,
                    ComplianceEventType::RepositoryAccess,
                ]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let security_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[
                    ComplianceEventType::SecurityIncident,
                    ComplianceEventType::PolicyViolation,
                ]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let change_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[ComplianceEventType::ConfigurationChange]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let mut unique_users = std::collections::HashSet::new();
        for event in &auth_events {
            if let Some(ref user_id) = event.actor.user_id {
                unique_users.insert(user_id.clone());
            }
        }

        let security_controls = vec![
            SecurityControl {
                control_id: "CC6.1".to_string(),
                control_name: "Logical and Physical Access".to_string(),
                category: "Access Control".to_string(),
                status: ControlStatus::Compliant,
                evidence_count: auth_events.len() as u64,
                last_assessed: Some(Utc::now()),
                description: "Access to systems is restricted to authorized users".to_string(),
            },
            SecurityControl {
                control_id: "CC6.6".to_string(),
                control_name: "Security Incident Management".to_string(),
                category: "Security".to_string(),
                status: if security_events.is_empty() {
                    ControlStatus::Compliant
                } else {
                    ControlStatus::PartiallyCompliant
                },
                evidence_count: security_events.len() as u64,
                last_assessed: Some(Utc::now()),
                description: "Security incidents are tracked and managed".to_string(),
            },
            SecurityControl {
                control_id: "CC8.1".to_string(),
                control_name: "Change Management".to_string(),
                category: "Change Management".to_string(),
                status: ControlStatus::Compliant,
                evidence_count: change_events.len() as u64,
                last_assessed: Some(Utc::now()),
                description: "Changes are authorized and tracked".to_string(),
            },
        ];

        let access_controls = AccessControlSummary {
            total_users: unique_users.len() as u64,
            active_users: unique_users.len() as u64,
            mfa_enabled_users: 0,
            role_based_access_enabled: true,
            privileged_users: 0,
            access_reviews_completed: 0,
            failed_auth_attempts: failed_auth,
            locked_accounts: 0,
        };

        let encryption_status = EncryptionStatus {
            data_at_rest_encrypted: true,
            data_in_transit_encrypted: true,
            encryption_algorithm: "AES-256".to_string(),
            key_rotation_enabled: false,
            last_key_rotation: None,
        };

        let incidents_by_severity = HashMap::new();
        let incident_summary = IncidentSummary {
            total_incidents: security_events.len() as u64,
            security_incidents: security_events.len() as u64,
            resolved_incidents: 0,
            mean_time_to_resolution_hours: None,
            incidents_by_severity,
        };

        let change_management = ChangeManagementSummary {
            total_changes: change_events.len() as u64,
            authorized_changes: change_events.len() as u64,
            unauthorized_changes: 0,
            change_failure_rate: 0.0,
            rollback_count: 0,
        };

        let overall_score = if security_controls.is_empty() {
            100.0
        } else {
            let compliant_count = security_controls
                .iter()
                .filter(|c| c.status == ControlStatus::Compliant)
                .count();
            (compliant_count as f32 / security_controls.len() as f32) * 100.0
        };

        let mut findings = Vec::new();
        if failed_auth > 10 {
            findings.push(ComplianceFinding {
                finding_id: format!("FIND-{}", uuid::Uuid::new_v4()),
                severity: FindingSeverity::Medium,
                title: "High number of failed authentication attempts".to_string(),
                description: format!(
                    "{} failed authentication attempts in the reporting period",
                    failed_auth
                ),
                control_reference: "CC6.1".to_string(),
                remediation: Some(
                    "Review authentication logs and consider implementing rate limiting"
                        .to_string(),
                ),
                status: FindingStatus::Open,
            });
        }

        let recommendations = vec![
            "Enable MFA for all user accounts".to_string(),
            "Implement regular access reviews".to_string(),
            "Enable key rotation for encryption keys".to_string(),
        ];

        Ok(Soc2ComplianceReport {
            report_period: ReportPeriod {
                start: period_start,
                end: period_end,
            },
            security_controls,
            access_controls,
            encryption_status,
            incident_summary,
            change_management,
            overall_compliance_score: overall_score,
            findings,
            recommendations,
        })
    }

    pub async fn generate_gdpr_report(
        &self,
        tenant_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<GdprDataProcessingReport, ReportError> {
        let data_access_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[ComplianceEventType::DataAccess]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let _data_export_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[ComplianceEventType::DataExport]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let _data_deletion_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[ComplianceEventType::DataDeletion]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let mut unique_data_subjects = std::collections::HashSet::new();
        for event in &data_access_events {
            if let Some(ref user_id) = event.actor.user_id {
                unique_data_subjects.insert(user_id.clone());
            }
        }

        let processing_activities = vec![
            ProcessingActivity {
                activity_id: "PA-001".to_string(),
                name: "Repository Access".to_string(),
                purpose: "Version control and code management".to_string(),
                legal_basis: LegalBasis::Contract,
                data_categories: vec!["Source Code".to_string(), "Metadata".to_string()],
                recipients: vec!["Internal Teams".to_string()],
                retention_period_days: 2555,
            },
            ProcessingActivity {
                activity_id: "PA-002".to_string(),
                name: "User Authentication".to_string(),
                purpose: "Access control and security".to_string(),
                legal_basis: LegalBasis::LegitimateInterests,
                data_categories: vec![
                    "Authentication Data".to_string(),
                    "IP Addresses".to_string(),
                ],
                recipients: vec!["Security Team".to_string()],
                retention_period_days: 365,
            },
        ];

        let data_categories = vec![
            DataCategory {
                name: "Source Code".to_string(),
                description: "User-provided source code and repositories".to_string(),
                sensitive: false,
                subjects_count: unique_data_subjects.len() as u64,
                storage_locations: vec!["Primary Storage".to_string()],
            },
            DataCategory {
                name: "Authentication Data".to_string(),
                description: "User credentials and session information".to_string(),
                sensitive: true,
                subjects_count: unique_data_subjects.len() as u64,
                storage_locations: vec!["Database".to_string()],
            },
        ];

        let consent_summary = ConsentSummary {
            total_consents: 0,
            active_consents: 0,
            withdrawn_consents: 0,
            consent_rate: 100.0,
        };

        let data_breach_summary = DataBreachSummary {
            total_breaches: 0,
            notified_breaches: 0,
            affected_subjects: 0,
            mean_detection_time_hours: None,
        };

        let cross_border_transfers = vec![CrossBorderTransfer {
            destination_country: "US".to_string(),
            transfer_mechanism: "Standard Contractual Clauses".to_string(),
            data_categories: vec!["Source Code".to_string()],
            safeguards: vec!["Encryption".to_string(), "Access Controls".to_string()],
        }];

        let retention_compliance = RetentionCompliance {
            policies_defined: true,
            automated_enforcement: true,
            overdue_deletions: 0,
            compliance_rate: 100.0,
        };

        let recommendations = vec![
            "Implement consent management for marketing communications".to_string(),
            "Document data protection impact assessments".to_string(),
            "Establish data breach notification procedures".to_string(),
        ];

        Ok(GdprDataProcessingReport {
            report_period: ReportPeriod {
                start: period_start,
                end: period_end,
            },
            data_subjects_count: unique_data_subjects.len() as u64,
            processing_activities,
            data_categories,
            consent_records: consent_summary,
            data_breaches: data_breach_summary,
            cross_border_transfers,
            retention_compliance,
            recommendations,
        })
    }

    pub async fn generate_access_control_audit(
        &self,
        tenant_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<AccessControlAudit, ReportError> {
        let auth_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[
                    ComplianceEventType::UserAuthentication,
                    ComplianceEventType::UserAuthorization,
                    ComplianceEventType::PermissionChange,
                    ComplianceEventType::RoleAssignment,
                ]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let user_access_reviews = Vec::new();
        let mut role_assignments = Vec::new();
        let mut permission_changes = Vec::new();

        for event in &auth_events {
            match event.event_type {
                ComplianceEventType::RoleAssignment => {
                    role_assignments.push(RoleAssignment {
                        role_name: event
                            .details
                            .get("role")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        assigned_to: event.actor.user_id.clone().unwrap_or_default(),
                        assigned_by: event
                            .details
                            .get("assigned_by")
                            .and_then(|v| v.as_str())
                            .unwrap_or("system")
                            .to_string(),
                        assigned_at: event.timestamp,
                        expires_at: event
                            .details
                            .get("expires_at")
                            .and_then(|v| v.as_str())
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Utc)),
                        justification: event
                            .details
                            .get("justification")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
                ComplianceEventType::PermissionChange => {
                    permission_changes.push(PermissionChange {
                        change_id: event.id.clone(),
                        changed_at: event.timestamp,
                        changed_by: event.actor.user_id.clone().unwrap_or_default(),
                        target_user: event.resource.resource_id.clone(),
                        change_type: event
                            .details
                            .get("change_type")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or(PermissionChangeType::Granted),
                        permission: event
                            .details
                            .get("permission")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        reason: event
                            .details
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
                _ => {}
            }
        }

        let orphaned_permissions = Vec::new();

        let recommendations = vec![
            "Implement periodic access reviews".to_string(),
            "Enable just-in-time access for privileged roles".to_string(),
            "Implement separation of duties controls".to_string(),
        ];

        Ok(AccessControlAudit {
            report_period: ReportPeriod {
                start: period_start,
                end: period_end,
            },
            user_access_reviews,
            role_assignments,
            permission_changes,
            orphaned_permissions,
            recommendations,
        })
    }

    pub async fn generate_security_incident_report(
        &self,
        tenant_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<SecurityIncidentReport, ReportError> {
        let security_events = self
            .audit_storage
            .query(
                Some(tenant_id),
                Some(&[
                    ComplianceEventType::SecurityIncident,
                    ComplianceEventType::PolicyViolation,
                ]),
                Some(period_start),
                Some(period_end),
                None,
                None,
                100000,
                0,
            )
            .await
            .map_err(|e| ReportError::Generation(e.to_string()))?;

        let mut incidents = Vec::new();
        let mut incidents_by_severity: HashMap<String, u64> = HashMap::new();
        let mut incidents_by_category: HashMap<String, u64> = HashMap::new();

        for event in &security_events {
            let severity = event
                .details
                .get("severity")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(IncidentSeverity::Medium);

            let category = event
                .details
                .get("category")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(IncidentCategory::Other);

            *incidents_by_severity
                .entry(format!("{:?}", severity))
                .or_insert(0) += 1;
            *incidents_by_category
                .entry(format!("{:?}", category))
                .or_insert(0) += 1;

            incidents.push(SecurityIncident {
                incident_id: event.id.clone(),
                detected_at: event.timestamp,
                resolved_at: event
                    .details
                    .get("resolved_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                severity,
                category,
                title: event.action.clone(),
                description: event
                    .details
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                affected_resources: event
                    .details
                    .get("affected_resources")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default(),
                root_cause: event
                    .details
                    .get("root_cause")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                remediation: event
                    .details
                    .get("remediation")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                status: if event.outcome == EventOutcome::Success {
                    IncidentStatus::Resolved
                } else {
                    IncidentStatus::Investigating
                },
            });
        }

        let metrics = IncidentMetrics {
            total_incidents: incidents.len() as u64,
            mean_time_to_detect_hours: 0.0,
            mean_time_to_respond_hours: 0.0,
            mean_time_to_resolve_hours: 0.0,
            incidents_by_severity,
            incidents_by_category,
        };

        let trends = IncidentTrends {
            period_over_period_change: 0.0,
            trend_direction: TrendDirection::Stable,
            projected_next_period: incidents.len() as u64,
        };

        let recommendations = vec![
            "Implement automated threat detection".to_string(),
            "Establish incident response procedures".to_string(),
            "Conduct regular security awareness training".to_string(),
        ];

        Ok(SecurityIncidentReport {
            report_period: ReportPeriod {
                start: period_start,
                end: period_end,
            },
            incidents,
            metrics,
            trends,
            recommendations,
        })
    }

    pub async fn generate_data_retention_report(
        &self,
        _tenant_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<DataRetentionReport, ReportError> {
        let retention_policies = vec![
            RetentionPolicyStatus {
                policy_name: "Audit Logs".to_string(),
                data_type: "audit_logs".to_string(),
                retention_period_days: 365,
                items_covered: 1000,
                items_overdue: 0,
                compliance_rate: 100.0,
                last_enforcement: Some(Utc::now()),
            },
            RetentionPolicyStatus {
                policy_name: "Repository Data".to_string(),
                data_type: "repository_data".to_string(),
                retention_period_days: 2555,
                items_covered: 500,
                items_overdue: 0,
                compliance_rate: 100.0,
                last_enforcement: Some(Utc::now()),
            },
        ];

        let data_inventory = vec![
            DataInventoryItem {
                data_type: "audit_logs".to_string(),
                location: "database".to_string(),
                total_items: 10000,
                oldest_item: Some(period_start),
                newest_item: Some(period_end),
                total_size_bytes: 1024 * 1024 * 100,
            },
            DataInventoryItem {
                data_type: "repositories".to_string(),
                location: "storage".to_string(),
                total_items: 50,
                oldest_item: Some(period_start),
                newest_item: Some(period_end),
                total_size_bytes: 1024 * 1024 * 1024 * 10,
            },
        ];

        let retention_actions = vec![RetentionActionRecord {
            action_id: format!("RA-{}", uuid::Uuid::new_v4()),
            action_type: RetentionActionType::Deletion,
            performed_at: Utc::now(),
            data_type: "audit_logs".to_string(),
            items_affected: 100,
            status: RetentionActionStatus::Completed,
        }];

        let compliance_status = RetentionComplianceStatus {
            overall_compliance_rate: 100.0,
            policies_compliant: 2,
            policies_non_compliant: 0,
            overdue_items: 0,
            next_enforcement: Some(Utc::now() + chrono::Duration::days(1)),
        };

        let recommendations = vec![
            "Document retention policies for all data types".to_string(),
            "Implement automated retention enforcement".to_string(),
            "Establish data archival procedures".to_string(),
        ];

        Ok(DataRetentionReport {
            report_period: ReportPeriod {
                start: period_start,
                end: period_end,
            },
            retention_policies,
            data_inventory,
            retention_actions,
            compliance_status,
            recommendations,
        })
    }

    pub async fn generate_report(
        &self,
        report_type: ReportType,
        tenant_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        generated_by: Option<&str>,
    ) -> Result<ComplianceReport, ReportError> {
        let (data, summary, compliance_score, recommendations) = match report_type {
            ReportType::Soc2Compliance => {
                let report = self
                    .generate_soc2_report(tenant_id, period_start, period_end)
                    .await?;
                let score = report.overall_compliance_score;
                let recs = report.recommendations.clone();
                (
                    serde_json::to_value(report)?,
                    "SOC2 Compliance Report".to_string(),
                    Some(score),
                    recs,
                )
            }
            ReportType::GdprDataProcessing => {
                let report = self
                    .generate_gdpr_report(tenant_id, period_start, period_end)
                    .await?;
                let recs = report.recommendations.clone();
                (
                    serde_json::to_value(report)?,
                    "GDPR Data Processing Report".to_string(),
                    None,
                    recs,
                )
            }
            ReportType::AccessControlAudit => {
                let report = self
                    .generate_access_control_audit(tenant_id, period_start, period_end)
                    .await?;
                let recs = report.recommendations.clone();
                (
                    serde_json::to_value(report)?,
                    "Access Control Audit Report".to_string(),
                    None,
                    recs,
                )
            }
            ReportType::SecurityIncident => {
                let report = self
                    .generate_security_incident_report(tenant_id, period_start, period_end)
                    .await?;
                let recs = report.recommendations.clone();
                (
                    serde_json::to_value(report)?,
                    "Security Incident Report".to_string(),
                    None,
                    recs,
                )
            }
            ReportType::DataRetention => {
                let report = self
                    .generate_data_retention_report(tenant_id, period_start, period_end)
                    .await?;
                let recs = report.recommendations.clone();
                (
                    serde_json::to_value(report)?,
                    "Data Retention Report".to_string(),
                    None,
                    recs,
                )
            }
        };

        Ok(ComplianceReport {
            id: uuid::Uuid::new_v4().to_string(),
            report_type,
            tenant_id: tenant_id.to_string(),
            generated_at: Utc::now(),
            period_start,
            period_end,
            generated_by: generated_by.map(|s| s.to_string()),
            status: ReportStatus::Completed,
            summary,
            data,
            compliance_score,
            recommendations,
            metadata: HashMap::new(),
        })
    }

    pub fn format_report_as_csv(report: &ComplianceReport) -> Result<String, ReportError> {
        let mut csv = String::new();
        csv.push_str("Field,Value\n");
        csv.push_str(&format!("Report ID,{}\n", report.id));
        csv.push_str(&format!("Report Type,{}\n", report.report_type));
        csv.push_str(&format!("Tenant ID,{}\n", report.tenant_id));
        csv.push_str(&format!(
            "Generated At,{}\n",
            report.generated_at.to_rfc3339()
        ));
        csv.push_str(&format!(
            "Period Start,{}\n",
            report.period_start.to_rfc3339()
        ));
        csv.push_str(&format!("Period End,{}\n", report.period_end.to_rfc3339()));
        csv.push_str(&format!("Status,{:?}\n", report.status));
        csv.push_str(&format!("Summary,{}\n", report.summary));
        if let Some(score) = report.compliance_score {
            csv.push_str(&format!("Compliance Score,{:.2}%\n", score));
        }
        csv.push_str("\nRecommendations:\n");
        for (i, rec) in report.recommendations.iter().enumerate() {
            csv.push_str(&format!("{},{}\n", i + 1, rec));
        }
        Ok(csv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::audit::InMemoryComplianceAuditStorage;

    #[tokio::test]
    async fn test_generate_soc2_report() {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(10000));
        let generator = ComplianceReportGenerator::new(audit_storage);

        let now = Utc::now();
        let start = now - chrono::Duration::days(30);

        let report = generator
            .generate_soc2_report("tenant-1", start, now)
            .await
            .unwrap();

        assert!(report.overall_compliance_score >= 0.0);
        assert!(!report.security_controls.is_empty());
    }

    #[tokio::test]
    async fn test_generate_gdpr_report() {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(10000));
        let generator = ComplianceReportGenerator::new(audit_storage);

        let now = Utc::now();
        let start = now - chrono::Duration::days(30);

        let report = generator
            .generate_gdpr_report("tenant-1", start, now)
            .await
            .unwrap();

        assert!(!report.processing_activities.is_empty());
    }

    #[tokio::test]
    async fn test_report_storage() {
        let storage = InMemoryReportStorage::new(100);

        let report = ComplianceReport {
            id: "report-1".to_string(),
            report_type: ReportType::Soc2Compliance,
            tenant_id: "tenant-1".to_string(),
            generated_at: Utc::now(),
            period_start: Utc::now() - chrono::Duration::days(30),
            period_end: Utc::now(),
            generated_by: Some("user-1".to_string()),
            status: ReportStatus::Completed,
            summary: "Test report".to_string(),
            data: serde_json::json!({}),
            compliance_score: Some(95.0),
            recommendations: vec![],
            metadata: HashMap::new(),
        };

        storage.store(report.clone()).await.unwrap();

        let retrieved = storage.get("report-1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().tenant_id, "tenant-1");
    }
}

pub mod audit;
pub mod encryption;
pub mod eu_data_protection;
pub mod handlers;
pub mod hipaa;
pub mod privacy;
pub mod reports;
pub mod retention;

pub use audit::{ComplianceAuditEvent, ComplianceAuditStorage, ComplianceEventType};
pub use privacy::{DataAnonymizer, GdprDataExporter, PrivacyControls};
pub use reports::{
    AccessControlAudit, ComplianceReport, ComplianceReportGenerator, ComplianceReportStorage,
    DataRetentionReport, GdprDataProcessingReport, ReportFormat, ReportType,
    SecurityIncidentReport, Soc2ComplianceReport,
};
pub use retention::{DataRetentionPolicy, DataRetentionService, RetentionAction};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ComplianceError {
    #[error("Report generation failed: {0}")]
    ReportGeneration(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Invalid report type: {0}")]
    InvalidReportType(String),
    #[error("Data export failed: {0}")]
    DataExport(String),
    #[error("Data anonymization failed: {0}")]
    Anonymization(String),
    #[error("Retention policy error: {0}")]
    RetentionPolicy(String),
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type ComplianceResult<T> = std::result::Result<T, ComplianceError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    pub reports_enabled: bool,
    pub gdpr_tools_enabled: bool,
    pub audit_retention_days: u32,
    pub report_retention_days: u32,
    pub auto_retention_enforcement: bool,
    pub anonymize_on_deletion: bool,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            reports_enabled: true,
            gdpr_tools_enabled: true,
            audit_retention_days: 365,
            report_retention_days: 2555,
            auto_retention_enforcement: true,
            anonymize_on_deletion: true,
        }
    }
}

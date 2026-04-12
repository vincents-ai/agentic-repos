//! EU Data Protection controls — enhanced GDPR administrative compliance.
//!
//! Covers: Article 30 Record of Processing Activities (RoPA),
//! Data Protection Impact Assessments (DPIA), Data Protection Officer management,
//! cross-border data transfer mechanisms, and supervisory authority tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Article 30: Record of Processing Activities ──

/// A single processing activity record (Article 30).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingActivity {
    pub id: Uuid,
    /// Name of the processing operation.
    pub name: String,
    /// Purpose of processing.
    pub purpose: String,
    /// Lawful basis (Article 6).
    pub lawful_basis: LawfulBasis,
    /// Categories of data subjects.
    pub data_subject_categories: Vec<String>,
    /// Categories of personal data.
    pub data_categories: Vec<String>,
    /// Recipients of the data.
    pub recipients: Vec<String>,
    /// Whether data is transferred to third countries.
    pub third_country_transfers: Vec<ThirdCountryTransfer>,
    /// Data retention period.
    pub retention_period: String,
    /// Technical and organisational security measures.
    pub security_measures: Vec<String>,
    /// Controller name.
    pub controller: String,
    /// DPO contact (if applicable).
    pub dpo_contact: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProcessingActivity {
    pub fn new(
        name: impl Into<String>,
        purpose: impl Into<String>,
        lawful_basis: LawfulBasis,
        controller: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            purpose: purpose.into(),
            lawful_basis,
            data_subject_categories: Vec::new(),
            data_categories: Vec::new(),
            recipients: Vec::new(),
            third_country_transfers: Vec::new(),
            retention_period: String::new(),
            security_measures: Vec::new(),
            controller: controller.into(),
            dpo_contact: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// GDPR Article 6 lawful bases for processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LawfulBasis {
    Consent,
    Contract,
    LegalObligation,
    VitalInterests,
    PublicTask,
    LegitimateInterests,
}

impl std::fmt::Display for LawfulBasis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LawfulBasis::Consent => write!(f, "consent"),
            LawfulBasis::Contract => write!(f, "contract"),
            LawfulBasis::LegalObligation => write!(f, "legal_obligation"),
            LawfulBasis::VitalInterests => write!(f, "vital_interests"),
            LawfulBasis::PublicTask => write!(f, "public_task"),
            LawfulBasis::LegitimateInterests => write!(f, "legitimate_interests"),
        }
    }
}

/// A third-country data transfer mechanism.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThirdCountryTransfer {
    pub country: String,
    pub safeguard: TransferSafeguard,
}

/// Legal mechanism for third-country transfers (Article 44-49).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferSafeguard {
    /// EU adequacy decision (Article 45).
    AdequacyDecision,
    /// Standard Contractual Clauses (Article 46(2)(c)).
    StandardContractualClauses,
    /// Binding Corporate Rules (Article 47).
    BindingCorporateRules,
    /// Explicit consent of data subject (Article 49(1)(a)).
    ExplicitConsent,
    /// Derogation — necessary for contract (Article 49(1)(b)).
    ContractDerogation,
}

// ── Article 35: Data Protection Impact Assessment ──

/// A DPIA record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataProtectionImpactAssessment {
    pub id: Uuid,
    /// The processing activity this DPIA covers.
    pub activity_name: String,
    /// Description of the processing.
    pub description: String,
    /// Why a DPIA is required.
    pub necessity_reason: String,
    /// Assessment of necessity and proportionality.
    pub proportionality_assessment: String,
    /// Identified risks to data subjects.
    pub risks: Vec<DpiaRisk>,
    /// Measures to mitigate risks.
    pub mitigation_measures: Vec<String>,
    /// Whether the DPO was consulted.
    pub dpo_consulted: bool,
    /// DPO recommendation.
    pub dpo_recommendation: Option<String>,
    /// Whether supervisory authority consultation is required (Article 36).
    pub supervisory_consultation_required: bool,
    /// Current status.
    pub status: DpiaStatus,
    /// Who authored the DPIA.
    pub author: String,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
}

impl DataProtectionImpactAssessment {
    pub fn new(activity_name: impl Into<String>, author: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            activity_name: activity_name.into(),
            description: String::new(),
            necessity_reason: String::new(),
            proportionality_assessment: String::new(),
            risks: Vec::new(),
            mitigation_measures: Vec::new(),
            dpo_consulted: false,
            dpo_recommendation: None,
            supervisory_consultation_required: false,
            status: DpiaStatus::Draft,
            author: author.into(),
            created_at: Utc::now(),
            reviewed_at: None,
            approved_at: None,
        }
    }

    /// Check if the DPIA is complete and approved.
    pub fn is_approved(&self) -> bool {
        self.status == DpiaStatus::Approved
    }

    /// Check if there are any high risks without mitigation.
    pub fn has_unmitigated_high_risks(&self) -> bool {
        self.risks
            .iter()
            .any(|r| r.severity >= RiskSeverity::High && !r.mitigated)
    }
}

/// Status of a DPIA.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DpiaStatus {
    Draft,
    UnderReview,
    DpoConsultation,
    SupervisoryConsultation,
    Approved,
    Rejected,
}

/// A risk identified in a DPIA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpiaRisk {
    pub description: String,
    pub severity: RiskSeverity,
    pub likelihood: RiskLikelihood,
    pub affected_rights: Vec<DataSubjectRight>,
    pub mitigated: bool,
}

/// Risk severity levels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Risk likelihood levels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLikelihood {
    Unlikely,
    Possible,
    Likely,
    AlmostCertain,
}

/// GDPR data subject rights (Article 12-23).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataSubjectRight {
    Access,             // Article 15
    Rectification,      // Article 16
    Erasure,            // Article 17
    RestrictProcessing, // Article 18
    DataPortability,    // Article 20
    Object,             // Article 21
    AutomatedDecisions, // Article 22
}

// ── Data Protection Officer ──

/// Data Protection Officer record (Article 37-39).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataProtectionOfficer {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub organisation: String,
    /// Whether the DPO appointment is required (Article 37).
    pub appointment_required: bool,
    pub appointed_at: DateTime<Utc>,
}

impl DataProtectionOfficer {
    pub fn new(
        name: impl Into<String>,
        email: impl Into<String>,
        organisation: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            email: email.into(),
            phone: None,
            organisation: organisation.into(),
            appointment_required: true,
            appointed_at: Utc::now(),
        }
    }
}

// ── Supervisory Authority ──

/// A supervisory authority record (Article 51-54).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisoryAuthority {
    pub id: Uuid,
    pub name: String,
    pub country: String,
    pub email: String,
    pub website: Option<String>,
}

impl SupervisoryAuthority {
    pub fn new(
        name: impl Into<String>,
        country: impl Into<String>,
        email: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            country: country.into(),
            email: email.into(),
            website: None,
        }
    }
}

// ── Storage ──

#[derive(Debug, thiserror::Error)]
pub enum EuComplianceError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("DPIA not approved for activity: {0}")]
    DpiaNotApproved(String),
    #[error("DPO not appointed")]
    DpoNotAppointed,
    #[error("Unmitigated high risks in DPIA: {0}")]
    UnmitigatedRisks(String),
    #[error("Lock poisoned")]
    LockPoisoned,
}

/// In-memory store for all EU compliance records.
pub struct InMemoryEuComplianceStore {
    activities: std::sync::RwLock<HashMap<Uuid, ProcessingActivity>>,
    dpias: std::sync::RwLock<HashMap<Uuid, DataProtectionImpactAssessment>>,
    dpo: std::sync::RwLock<Option<DataProtectionOfficer>>,
    authorities: std::sync::RwLock<HashMap<Uuid, SupervisoryAuthority>>,
}

impl InMemoryEuComplianceStore {
    pub fn new() -> Self {
        Self {
            activities: std::sync::RwLock::new(HashMap::new()),
            dpias: std::sync::RwLock::new(HashMap::new()),
            dpo: std::sync::RwLock::new(None),
            authorities: std::sync::RwLock::new(HashMap::new()),
        }
    }

    pub fn shared() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::new())
    }
}

impl Default for InMemoryEuComplianceStore {
    fn default() -> Self {
        Self::new()
    }
}

/// EU compliance service — manages RoPA, DPIAs, DPO, and supervisory authorities.
pub struct EuComplianceService {
    store: std::sync::Arc<InMemoryEuComplianceStore>,
}

impl EuComplianceService {
    pub fn new(store: std::sync::Arc<InMemoryEuComplianceStore>) -> Self {
        Self { store }
    }

    // ── Processing Activities (Article 30) ──

    pub fn add_activity(&self, activity: ProcessingActivity) -> Result<(), EuComplianceError> {
        let mut activities = self
            .store
            .activities
            .write()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        activities.insert(activity.id, activity);
        Ok(())
    }

    pub fn list_activities(&self) -> Result<Vec<ProcessingActivity>, EuComplianceError> {
        let activities = self
            .store
            .activities
            .read()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        Ok(activities.values().cloned().collect())
    }

    pub fn get_activity(&self, id: Uuid) -> Result<Option<ProcessingActivity>, EuComplianceError> {
        let activities = self
            .store
            .activities
            .read()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        Ok(activities.get(&id).cloned())
    }

    // ── DPIA (Article 35) ──

    pub fn add_dpia(&self, dpia: DataProtectionImpactAssessment) -> Result<(), EuComplianceError> {
        let mut dpias = self
            .store
            .dpias
            .write()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        dpias.insert(dpia.id, dpia);
        Ok(())
    }

    pub fn list_dpias(&self) -> Result<Vec<DataProtectionImpactAssessment>, EuComplianceError> {
        let dpias = self
            .store
            .dpias
            .read()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        Ok(dpias.values().cloned().collect())
    }

    pub fn approve_dpia(
        &self,
        id: Uuid,
    ) -> Result<DataProtectionImpactAssessment, EuComplianceError> {
        let mut dpias = self
            .store
            .dpias
            .write()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        let dpia = dpias
            .get_mut(&id)
            .ok_or_else(|| EuComplianceError::NotFound(format!("DPIA {}", id)))?;

        if dpia.has_unmitigated_high_risks() {
            return Err(EuComplianceError::UnmitigatedRisks(
                dpia.activity_name.clone(),
            ));
        }

        dpia.status = DpiaStatus::Approved;
        dpia.approved_at = Some(Utc::now());
        Ok(dpias.get(&id).unwrap().clone())
    }

    // ── DPO (Article 37-39) ──

    pub fn appoint_dpo(&self, dpo: DataProtectionOfficer) -> Result<(), EuComplianceError> {
        let mut dpo_slot = self
            .store
            .dpo
            .write()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        *dpo_slot = Some(dpo);
        Ok(())
    }

    pub fn get_dpo(&self) -> Result<Option<DataProtectionOfficer>, EuComplianceError> {
        let dpo_slot = self
            .store
            .dpo
            .read()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        Ok(dpo_slot.clone())
    }

    // ── Supervisory Authorities ──

    pub fn add_authority(&self, authority: SupervisoryAuthority) -> Result<(), EuComplianceError> {
        let mut authorities = self
            .store
            .authorities
            .write()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        authorities.insert(authority.id, authority);
        Ok(())
    }

    pub fn list_authorities(&self) -> Result<Vec<SupervisoryAuthority>, EuComplianceError> {
        let authorities = self
            .store
            .authorities
            .read()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        Ok(authorities.values().cloned().collect())
    }

    // ── Compliance checks ──

    /// Check if Article 30 records exist.
    pub fn has_processing_records(&self) -> Result<bool, EuComplianceError> {
        let activities = self
            .store
            .activities
            .read()
            .map_err(|_| EuComplianceError::LockPoisoned)?;
        Ok(!activities.is_empty())
    }

    /// Check if a DPO has been appointed.
    pub fn has_dpo(&self) -> Result<bool, EuComplianceError> {
        Ok(self.get_dpo()?.is_some())
    }

    /// Run a GDPR readiness assessment.
    pub fn gdpr_readiness_check(&self) -> GdprReadinessReport {
        let mut report = GdprReadinessReport {
            checked_at: Utc::now(),
            ..Default::default()
        };

        // Article 30: RoPA
        match self.list_activities() {
            Ok(activities) => {
                report.articles_ready.insert(
                    "Article 30 — Record of Processing".to_string(),
                    !activities.is_empty(),
                );
                report.processing_activities_count = activities.len();
            }
            Err(_) => {
                report
                    .articles_ready
                    .insert("Article 30 — Record of Processing".to_string(), false);
            }
        }

        // Article 35: DPIAs
        match self.list_dpias() {
            Ok(dpias) => {
                let all_approved = dpias.iter().all(|d| d.is_approved());
                report.articles_ready.insert(
                    "Article 35 — Impact Assessments".to_string(),
                    dpias.is_empty() || all_approved,
                );
                report.dpia_count = dpias.len();
                report.unapproved_dpias = dpias.iter().filter(|d| !d.is_approved()).count();
            }
            Err(_) => {
                report
                    .articles_ready
                    .insert("Article 35 — Impact Assessments".to_string(), false);
            }
        }

        // Article 37: DPO
        report.articles_ready.insert(
            "Article 37 — Data Protection Officer".to_string(),
            self.has_dpo().unwrap_or(false),
        );

        report.overall_ready = report.articles_ready.values().all(|&v| v);
        report
    }
}

/// GDPR readiness report.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GdprReadinessReport {
    pub overall_ready: bool,
    pub articles_ready: HashMap<String, bool>,
    pub processing_activities_count: usize,
    pub dpia_count: usize,
    pub unapproved_dpias: usize,
    pub checked_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> EuComplianceService {
        EuComplianceService::new(InMemoryEuComplianceStore::shared())
    }

    #[test]
    fn test_add_and_list_activities() {
        let svc = make_service();
        let activity = ProcessingActivity::new(
            "User analytics",
            "Track repository access patterns",
            LawfulBasis::LegitimateInterests,
            "ACME Corp",
        );
        svc.add_activity(activity).unwrap();
        let activities = svc.list_activities().unwrap();
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].lawful_basis, LawfulBasis::LegitimateInterests);
    }

    #[test]
    fn test_lawful_basis_display() {
        assert_eq!(LawfulBasis::Consent.to_string(), "consent");
        assert_eq!(LawfulBasis::LegalObligation.to_string(), "legal_obligation");
    }

    #[test]
    fn test_dpia_creation() {
        let svc = make_service();
        let dpia = DataProtectionImpactAssessment::new("Profiling users", "privacy-team");
        svc.add_dpia(dpia).unwrap();
        let dpias = svc.list_dpias().unwrap();
        assert_eq!(dpias.len(), 1);
        assert_eq!(dpias[0].status, DpiaStatus::Draft);
    }

    #[test]
    fn test_dpia_approve() {
        let svc = make_service();
        let dpia = DataProtectionImpactAssessment::new("Low-risk processing", "admin");
        svc.add_dpia(dpia.clone()).unwrap();
        let approved = svc.approve_dpia(dpia.id).unwrap();
        assert_eq!(approved.status, DpiaStatus::Approved);
        assert!(approved.approved_at.is_some());
    }

    #[test]
    fn test_dpia_reject_unmitigated_risks() {
        let svc = make_service();
        let mut dpia = DataProtectionImpactAssessment::new("High-risk profiling", "admin");
        dpia.risks.push(DpiaRisk {
            description: "Re-identification of anonymised data".to_string(),
            severity: RiskSeverity::High,
            likelihood: RiskLikelihood::Possible,
            affected_rights: vec![DataSubjectRight::Object],
            mitigated: false,
        });
        svc.add_dpia(dpia.clone()).unwrap();
        let result = svc.approve_dpia(dpia.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unmitigated"));
    }

    #[test]
    fn test_dpia_approve_with_mitigated_risks() {
        let svc = make_service();
        let mut dpia = DataProtectionImpactAssessment::new("Processing with mitigations", "admin");
        dpia.risks.push(DpiaRisk {
            description: "Data breach via insider".to_string(),
            severity: RiskSeverity::High,
            likelihood: RiskLikelihood::Unlikely,
            affected_rights: vec![DataSubjectRight::Access],
            mitigated: true,
        });
        svc.add_dpia(dpia.clone()).unwrap();
        let approved = svc.approve_dpia(dpia.id).unwrap();
        assert_eq!(approved.status, DpiaStatus::Approved);
    }

    #[test]
    fn test_dpo_appointment() {
        let svc = make_service();
        assert!(!svc.has_dpo().unwrap());

        let dpo = DataProtectionOfficer::new("Jane Smith", "dpo@example.eu", "ACME Corp");
        svc.appoint_dpo(dpo).unwrap();
        assert!(svc.has_dpo().unwrap());

        let retrieved = svc.get_dpo().unwrap().unwrap();
        assert_eq!(retrieved.name, "Jane Smith");
    }

    #[test]
    fn test_supervisory_authority() {
        let svc = make_service();
        let ico = SupervisoryAuthority::new(
            "Information Commissioner's Office",
            "United Kingdom",
            "ico@ico.org.uk",
        );
        svc.add_authority(ico).unwrap();
        let authorities = svc.list_authorities().unwrap();
        assert_eq!(authorities.len(), 1);
        assert_eq!(authorities[0].country, "United Kingdom");
    }

    #[test]
    fn test_gdpr_readiness_empty() {
        let svc = make_service();
        let report = svc.gdpr_readiness_check();
        assert!(!report.overall_ready);
        assert_eq!(report.processing_activities_count, 0);
    }

    #[test]
    fn test_gdpr_readiness_fully_ready() {
        let svc = make_service();
        svc.add_activity(ProcessingActivity::new(
            "Git hosting",
            "Repository management",
            LawfulBasis::Contract,
            "ACME",
        ))
        .unwrap();
        let dpia = DataProtectionImpactAssessment::new("Git hosting", "admin");
        svc.add_dpia(dpia.clone()).unwrap();
        svc.approve_dpia(dpia.id).unwrap();
        svc.appoint_dpo(DataProtectionOfficer::new("DPO", "dpo@acme.eu", "ACME"))
            .unwrap();

        let report = svc.gdpr_readiness_check();
        assert!(report.overall_ready);
        assert_eq!(report.processing_activities_count, 1);
        assert_eq!(report.dpia_count, 1);
        assert_eq!(report.unapproved_dpias, 0);
    }

    #[test]
    fn test_third_country_transfer() {
        let transfer = ThirdCountryTransfer {
            country: "United States".to_string(),
            safeguard: TransferSafeguard::StandardContractualClauses,
        };
        assert_eq!(
            transfer.safeguard,
            TransferSafeguard::StandardContractualClauses
        );
    }

    #[test]
    fn test_processing_activity_builder() {
        let activity = ProcessingActivity::new(
            "Email marketing",
            "Send product updates",
            LawfulBasis::Consent,
            "ACME Corp",
        );
        assert_eq!(activity.name, "Email marketing");
        assert_eq!(activity.lawful_basis, LawfulBasis::Consent);
        assert!(activity.data_subject_categories.is_empty());
    }
}

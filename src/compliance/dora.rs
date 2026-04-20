//! DORA (Digital Operational Resilience Act) ICT risk management framework.
//!
//! Implements EU Regulation 2022/2554 (DORA) requirements for ICT risk management
//! governance, incident reporting, digital operational resilience testing,
//! third-party ICT risk management, and information sharing arrangements.
//!
//! EU precedence applies — where EU and US requirements conflict, EU standards win.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════════
// Chapter II: ICT Risk Management (Articles 5-16)
// ═══════════════════════════════════════════════════════════════════════════════

/// ICT risk management framework (Article 6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IctRiskManagementFramework {
    pub id: Uuid,
    pub version: String,
    pub approved_at: DateTime<Utc>,
    pub approved_by: String,
    /// Areas covered by the framework.
    pub areas: Vec<IctRiskArea>,
    /// Strategies for each risk area.
    pub strategies: HashMap<IctRiskArea, RiskStrategy>,
    /// Risk appetite statement.
    pub risk_appetite: RiskAppetiteStatement,
    /// Last review date.
    pub last_reviewed_at: DateTime<Utc>,
    /// Review frequency (months).
    pub review_frequency_months: u32,
    pub created_at: DateTime<Utc>,
}

impl IctRiskManagementFramework {
    pub fn new(approved_by: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            version: "1.0".into(),
            approved_at: now,
            approved_by: approved_by.into(),
            areas: vec![
                IctRiskArea::InformationSecurity,
                IctRiskArea::Availability,
                IctRiskArea::DataIntegrity,
                IctRiskArea::BusinessContinuity,
            ],
            strategies: HashMap::new(),
            risk_appetite: RiskAppetiteStatement::default(),
            last_reviewed_at: now,
            review_frequency_months: 12,
            created_at: now,
        }
    }

    pub fn with_strategy(mut self, area: IctRiskArea, strategy: RiskStrategy) -> Self {
        self.strategies.insert(area, strategy);
        self
    }

    /// Whether the framework is due for review.
    pub fn is_review_overdue(&self) -> bool {
        let months_since = (Utc::now() - self.last_reviewed_at).num_days() / 30;
        months_since as u32 >= self.review_frequency_months
    }
}

/// ICT risk areas (Article 6(2)).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IctRiskArea {
    InformationSecurity,
    Availability,
    DataIntegrity,
    BusinessContinuity,
    ThirdPartyRisk,
    IncidentManagement,
    ChangeManagement,
    DataProcessingIntegrity,
}

impl std::fmt::Display for IctRiskArea {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        write!(f, "{}", s)
    }
}

/// Risk mitigation strategy for an ICT risk area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskStrategy {
    pub area: IctRiskArea,
    pub risk_level: RiskLevel,
    pub mitigation_controls: Vec<String>,
    pub residual_risk: RiskLevel,
    pub owner: String,
    pub review_date: DateTime<Utc>,
}

/// Risk appetite statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAppetiteStatement {
    pub max_acceptable_downtime_hours: f64,
    pub max_acceptable_data_loss_percentage: f64,
    pub max_acceptable_incident_frequency_per_quarter: u32,
    pub risk_tolerance_narrative: String,
}

impl Default for RiskAppetiteStatement {
    fn default() -> Self {
        Self {
            max_acceptable_downtime_hours: 4.0,
            max_acceptable_data_loss_percentage: 0.01,
            max_acceptable_incident_frequency_per_quarter: 2,
            risk_tolerance_narrative: "Low risk tolerance for availability and data integrity"
                .into(),
        }
    }
}

/// Risk level classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Negligible,
    Low,
    Medium,
    High,
    Critical,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Chapter III: ICT-Related Incident Reporting (Articles 17-23)
// ═══════════════════════════════════════════════════════════════════════════════

/// ICT incident classification (Article 18).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IctIncident {
    pub id: Uuid,
    pub classification: IncidentClassification,
    pub severity: IncidentSeverity,
    pub status: IncidentStatus,
    pub title: String,
    pub description: String,
    /// When the incident was detected.
    pub detected_at: DateTime<Utc>,
    /// When the entity was notified.
    pub notified_at: Option<DateTime<Utc>>,
    /// Competent authority reference.
    pub authority_reference: Option<String>,
    /// Root cause analysis.
    pub root_cause: Option<String>,
    /// Impact assessment.
    pub impact: IncidentImpact,
    /// Remediation actions taken.
    pub remediation_actions: Vec<String>,
    /// Timeline of events.
    pub timeline: Vec<IncidentTimelineEntry>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl IctIncident {
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        severity: IncidentSeverity,
        classification: IncidentClassification,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            classification,
            severity,
            status: IncidentStatus::Detected,
            title: title.into(),
            description: description.into(),
            detected_at: now,
            notified_at: None,
            authority_reference: None,
            root_cause: None,
            impact: IncidentImpact::default(),
            remediation_actions: Vec::new(),
            timeline: vec![IncidentTimelineEntry {
                timestamp: now,
                event: "Incident detected".into(),
            }],
            created_at: now,
            updated_at: now,
        }
    }

    /// Whether DORA reporting deadline is breached (4 hours for major incidents).
    pub fn is_reporting_deadline_breached(&self) -> bool {
        if self.notified_at.is_some() {
            return false;
        }
        match self.severity {
            IncidentSeverity::Major | IncidentSeverity::Critical => {
                let deadline = self.detected_at + chrono::Duration::hours(4);
                Utc::now() > deadline
            }
            _ => false,
        }
    }

    /// Submit initial notification to competent authority.
    pub fn submit_notification(&mut self, authority_ref: impl Into<String>) {
        self.authority_reference = Some(authority_ref.into());
        self.notified_at = Some(Utc::now());
        self.status = IncidentStatus::Reported;
        self.timeline.push(IncidentTimelineEntry {
            timestamp: Utc::now(),
            event: "Initial notification submitted to competent authority".into(),
        });
        self.updated_at = Utc::now();
    }
}

/// Incident classification (Article 18(2)).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncidentClassification {
    /// Unauthorised access or system compromise.
    CyberAttack,
    /// System failure or degradation.
    SystemFailure,
    /// Data breach or corruption.
    DataBreach,
    /// Loss of availability.
    AvailabilityIncident,
    /// Third-party caused incident.
    ThirdPartyIncident,
    /// Change or deployment failure.
    ChangeFailure,
    /// Natural disaster or physical incident.
    PhysicalIncident,
}

/// Incident severity (Article 18(3)).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum IncidentSeverity {
    Minor,
    Major,
    Critical,
}

/// Incident lifecycle status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Detected,
    Analysing,
    Contained,
    Reported,
    Remediated,
    Closed,
}

/// Impact assessment for an incident.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentImpact {
    /// Number of affected clients/users.
    pub affected_entities: u32,
    /// Geographic scope of impact.
    pub geographic_scope: GeographicScope,
    /// Duration of service disruption (hours).
    pub disruption_duration_hours: Option<f64>,
    /// Data loss estimate (records).
    pub data_loss_records: u64,
    /// Financial impact estimate (EUR).
    pub financial_impact_eur: Option<f64>,
    /// Whether critical services are affected.
    pub critical_services_affected: bool,
}

impl Default for IncidentImpact {
    fn default() -> Self {
        Self {
            affected_entities: 0,
            geographic_scope: GeographicScope::SingleMemberState,
            disruption_duration_hours: None,
            data_loss_records: 0,
            financial_impact_eur: None,
            critical_services_affected: false,
        }
    }
}

/// Geographic scope of an incident's impact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GeographicScope {
    SingleMemberState,
    MultipleMemberStates,
    CrossBorder,
    Global,
}

/// Entry in an incident timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentTimelineEntry {
    pub timestamp: DateTime<Utc>,
    pub event: String,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Chapter IV: Digital Operational Resilience Testing (Articles 24-27)
// ═══════════════════════════════════════════════════════════════════════════════

/// Digital operational resilience testing programme (Article 24).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilienceTestingProgramme {
    pub id: Uuid,
    pub year: i32,
    pub tests: Vec<ResilienceTest>,
    pub next_review_at: DateTime<Utc>,
    pub approved_by: String,
    pub created_at: DateTime<Utc>,
}

impl ResilienceTestingProgramme {
    pub fn new(year: i32, approved_by: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            year,
            tests: Vec::new(),
            next_review_at: now + chrono::Duration::days(365),
            approved_by: approved_by.into(),
            created_at: now,
        }
    }

    pub fn with_test(mut self, test: ResilienceTest) -> Self {
        self.tests.push(test);
        self
    }

    /// Whether TLPT (threat-led penetration testing) is required.
    /// DORA Article 26: Systematically important entities must perform TLPT.
    pub fn requires_tlpt(&self, is_systemically_important: bool) -> bool {
        is_systemically_important
    }
}

/// A single resilience test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilienceTest {
    pub id: Uuid,
    pub test_type: ResilienceTestType,
    pub scope: String,
    pub status: ResilienceTestStatus,
    pub findings: Vec<TestFinding>,
    pub conducted_at: Option<DateTime<Utc>>,
    pub conducted_by: String,
    pub next_scheduled_at: Option<DateTime<Utc>>,
}

impl ResilienceTest {
    pub fn new(
        test_type: ResilienceTestType,
        scope: impl Into<String>,
        conducted_by: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            test_type,
            scope: scope.into(),
            status: ResilienceTestStatus::Planned,
            findings: Vec::new(),
            conducted_at: None,
            conducted_by: conducted_by.into(),
            next_scheduled_at: None,
        }
    }

    pub fn with_finding(mut self, finding: TestFinding) -> Self {
        self.findings.push(finding);
        self
    }
}

/// Types of resilience tests (Article 24(1)).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResilienceTestType {
    /// Vulnerability assessments and open-source analyses.
    VulnerabilityAssessment,
    /// Network security assessments.
    NetworkSecurityAssessment,
    /// Scenario-based testing.
    ScenarioTesting,
    /// End-to-end testing of critical functions.
    EndToEndTesting,
    /// Threat-led penetration testing (Article 26).
    ThreatLedPenetrationTest,
    /// Performance and load testing.
    PerformanceTesting,
    /// Disaster recovery testing.
    DisasterRecoveryTest,
}

/// Status of a resilience test.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResilienceTestStatus {
    Planned,
    InProgress,
    Completed,
    RemediationInProgress,
    Closed,
}

/// A finding from a resilience test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFinding {
    pub id: Uuid,
    pub severity: RiskLevel,
    pub title: String,
    pub description: String,
    pub remediation: String,
    pub status: FindingStatus,
    pub deadline: DateTime<Utc>,
}

impl TestFinding {
    pub fn new(
        severity: RiskLevel,
        title: impl Into<String>,
        description: impl Into<String>,
        remediation: impl Into<String>,
        deadline: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            severity,
            title: title.into(),
            description: description.into(),
            remediation: remediation.into(),
            status: FindingStatus::Open,
            deadline,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    Open,
    Accepted,
    Remediated,
    Closed,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Chapter V: Third-Party ICT Risk (Articles 28-44)
// ═══════════════════════════════════════════════════════════════════════════════

/// Third-party ICT service provider registry (Article 28).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyProvider {
    pub id: Uuid,
    pub name: String,
    pub service_type: ThirdPartyServiceType,
    pub criticality: ProviderCriticality,
    /// Contract reference.
    pub contract_ref: String,
    /// Services provided.
    pub services: Vec<String>,
    /// Risk assessment.
    pub risk_assessment: Option<ThirdPartyRiskAssessment>,
    /// Last audit date.
    pub last_audited_at: Option<DateTime<Utc>>,
    /// Whether the provider is in the EU/EEA.
    pub is_eu_based: bool,
    /// Subcontractors used.
    pub subcontractors: Vec<String>,
    /// Exit strategy documented.
    pub has_exit_strategy: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ThirdPartyProvider {
    pub fn new(
        name: impl Into<String>,
        service_type: ThirdPartyServiceType,
        criticality: ProviderCriticality,
        contract_ref: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            service_type,
            criticality,
            contract_ref: contract_ref.into(),
            services: Vec::new(),
            risk_assessment: None,
            last_audited_at: None,
            is_eu_based: false,
            subcontractors: Vec::new(),
            has_exit_strategy: false,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Third-party service classification (Article 28(2)).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThirdPartyServiceType {
    CloudInfrastructure,
    ManagedServices,
    DataProcessing,
    IdentityAndAccess,
    CommunicationServices,
    SecurityServices,
    SoftwareAsAService,
    Other,
}

/// Criticality of a third-party provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCriticality {
    Low,
    Medium,
    High,
    Critical,
}

/// Risk assessment for a third-party provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyRiskAssessment {
    pub overall_risk: RiskLevel,
    pub security_risk: RiskLevel,
    pub availability_risk: RiskLevel,
    pub data_protection_risk: RiskLevel,
    /// Whether the provider holds certifications (SOC 2, ISO 27001, etc.).
    pub certifications: Vec<String>,
    /// Assessment date.
    pub assessed_at: DateTime<Utc>,
    /// Next assessment due.
    pub next_assessment_at: DateTime<Utc>,
    pub assessor: String,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Information Sharing (Article 45)
// ═══════════════════════════════════════════════════════════════════════════════

/// Information sharing arrangement (Article 45).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformationSharingArrangement {
    pub id: Uuid,
    pub partner_name: String,
    pub arrangement_type: SharingType,
    pub scope: String,
    /// Threat intelligence shared.
    pub threat_intelligence_categories: Vec<ThreatCategory>,
    /// Non-disclosure agreement reference.
    pub nda_reference: Option<String>,
    pub established_at: DateTime<Utc>,
    pub review_at: DateTime<Utc>,
    pub status: SharingStatus,
}

impl InformationSharingArrangement {
    pub fn new(
        partner_name: impl Into<String>,
        arrangement_type: SharingType,
        scope: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            partner_name: partner_name.into(),
            arrangement_type,
            scope: scope.into(),
            threat_intelligence_categories: Vec::new(),
            nda_reference: None,
            established_at: Utc::now(),
            review_at: Utc::now() + chrono::Duration::days(365),
            status: SharingStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharingType {
    /// ISAC (Information Sharing Analysis Centre).
    Isac,
    Bilateral,
    Multilateral,
    Government,
    EuInstitution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharingStatus {
    Active,
    Suspended,
    Terminated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreatCategory {
    Malware,
    Phishing,
    Ddos,
    ZeroDay,
    InsiderThreat,
    SupplyChain,
    Ransomware,
    Atp,
}

// ═══════════════════════════════════════════════════════════════════════════════
// DORA Service
// ═══════════════════════════════════════════════════════════════════════════════

/// Storage trait for DORA compliance data.
pub trait DoraStore: Send + Sync {
    // Framework
    fn save_framework(&self, framework: &IctRiskManagementFramework) -> Result<(), String>;
    fn get_framework(&self) -> Result<Option<IctRiskManagementFramework>, String>;

    // Incidents
    fn create_incident(&self, incident: &IctIncident) -> Result<(), String>;
    fn update_incident(&self, incident: &IctIncident) -> Result<(), String>;
    fn list_incidents(&self) -> Result<Vec<IctIncident>, String>;
    fn get_incident(&self, id: Uuid) -> Result<Option<IctIncident>, String>;

    // Testing
    fn save_testing_programme(&self, programme: &ResilienceTestingProgramme) -> Result<(), String>;
    fn get_testing_programme(
        &self,
        year: i32,
    ) -> Result<Option<ResilienceTestingProgramme>, String>;

    // Third-party
    fn register_provider(&self, provider: &ThirdPartyProvider) -> Result<(), String>;
    fn list_providers(&self) -> Result<Vec<ThirdPartyProvider>, String>;
    fn update_provider(&self, provider: &ThirdPartyProvider) -> Result<(), String>;

    // Information sharing
    fn save_sharing_arrangement(
        &self,
        arrangement: &InformationSharingArrangement,
    ) -> Result<(), String>;
    fn list_sharing_arrangements(&self) -> Result<Vec<InformationSharingArrangement>, String>;
}

/// In-memory DORA store.
pub struct InMemoryDoraStore {
    framework: std::sync::RwLock<Option<IctRiskManagementFramework>>,
    incidents: std::sync::RwLock<HashMap<Uuid, IctIncident>>,
    testing_programmes: std::sync::RwLock<HashMap<i32, ResilienceTestingProgramme>>,
    providers: std::sync::RwLock<HashMap<Uuid, ThirdPartyProvider>>,
    sharing: std::sync::RwLock<Vec<InformationSharingArrangement>>,
}

impl InMemoryDoraStore {
    pub fn new() -> Self {
        Self {
            framework: std::sync::RwLock::new(None),
            incidents: std::sync::RwLock::new(HashMap::new()),
            testing_programmes: std::sync::RwLock::new(HashMap::new()),
            providers: std::sync::RwLock::new(HashMap::new()),
            sharing: std::sync::RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemoryDoraStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DoraStore for InMemoryDoraStore {
    fn save_framework(&self, framework: &IctRiskManagementFramework) -> Result<(), String> {
        let mut fw = self.framework.write().map_err(|e| e.to_string())?;
        *fw = Some(framework.clone());
        Ok(())
    }

    fn get_framework(&self) -> Result<Option<IctRiskManagementFramework>, String> {
        let fw = self.framework.read().map_err(|e| e.to_string())?;
        Ok(fw.clone())
    }

    fn create_incident(&self, incident: &IctIncident) -> Result<(), String> {
        let mut incidents = self.incidents.write().map_err(|e| e.to_string())?;
        incidents.insert(incident.id, incident.clone());
        Ok(())
    }

    fn update_incident(&self, incident: &IctIncident) -> Result<(), String> {
        let mut incidents = self.incidents.write().map_err(|e| e.to_string())?;
        incidents.insert(incident.id, incident.clone());
        Ok(())
    }

    fn list_incidents(&self) -> Result<Vec<IctIncident>, String> {
        let incidents = self.incidents.read().map_err(|e| e.to_string())?;
        Ok(incidents.values().cloned().collect())
    }

    fn get_incident(&self, id: Uuid) -> Result<Option<IctIncident>, String> {
        let incidents = self.incidents.read().map_err(|e| e.to_string())?;
        Ok(incidents.get(&id).cloned())
    }

    fn save_testing_programme(&self, programme: &ResilienceTestingProgramme) -> Result<(), String> {
        let mut progs = self.testing_programmes.write().map_err(|e| e.to_string())?;
        progs.insert(programme.year, programme.clone());
        Ok(())
    }

    fn get_testing_programme(
        &self,
        year: i32,
    ) -> Result<Option<ResilienceTestingProgramme>, String> {
        let progs = self.testing_programmes.read().map_err(|e| e.to_string())?;
        Ok(progs.get(&year).cloned())
    }

    fn register_provider(&self, provider: &ThirdPartyProvider) -> Result<(), String> {
        let mut providers = self.providers.write().map_err(|e| e.to_string())?;
        providers.insert(provider.id, provider.clone());
        Ok(())
    }

    fn list_providers(&self) -> Result<Vec<ThirdPartyProvider>, String> {
        let providers = self.providers.read().map_err(|e| e.to_string())?;
        Ok(providers.values().cloned().collect())
    }

    fn update_provider(&self, provider: &ThirdPartyProvider) -> Result<(), String> {
        let mut providers = self.providers.write().map_err(|e| e.to_string())?;
        providers.insert(provider.id, provider.clone());
        Ok(())
    }

    fn save_sharing_arrangement(
        &self,
        arrangement: &InformationSharingArrangement,
    ) -> Result<(), String> {
        let mut sharing = self.sharing.write().map_err(|e| e.to_string())?;
        sharing.push(arrangement.clone());
        Ok(())
    }

    fn list_sharing_arrangements(&self) -> Result<Vec<InformationSharingArrangement>, String> {
        let sharing = self.sharing.read().map_err(|e| e.to_string())?;
        Ok(sharing.clone())
    }
}

/// DORA compliance service.
pub struct DoraService {
    store: std::sync::Arc<dyn DoraStore>,
}

impl DoraService {
    pub fn new(store: std::sync::Arc<dyn DoraStore>) -> Self {
        Self { store }
    }

    // ── Framework ──

    pub fn save_framework(&self, framework: &IctRiskManagementFramework) -> Result<(), String> {
        self.store.save_framework(framework)
    }

    pub fn get_framework(&self) -> Result<Option<IctRiskManagementFramework>, String> {
        self.store.get_framework()
    }

    // ── Incidents ──

    pub fn report_incident(&self, incident: &IctIncident) -> Result<(), String> {
        self.store.create_incident(incident)
    }

    pub fn list_incidents(&self) -> Result<Vec<IctIncident>, String> {
        self.store.list_incidents()
    }

    /// Get incidents that breach DORA reporting deadlines.
    pub fn get_overdue_reports(&self) -> Result<Vec<IctIncident>, String> {
        let incidents = self.store.list_incidents()?;
        Ok(incidents
            .into_iter()
            .filter(|i| i.is_reporting_deadline_breached())
            .collect())
    }

    /// Submit notification for an incident.
    pub fn notify_authority(
        &self,
        incident_id: Uuid,
        authority_ref: impl Into<String>,
    ) -> Result<(), String> {
        let mut incident = self
            .store
            .get_incident(incident_id)?
            .ok_or_else(|| format!("Incident {} not found", incident_id))?;
        incident.submit_notification(authority_ref);
        self.store.update_incident(&incident)
    }

    // ── Testing ──

    pub fn save_testing_programme(
        &self,
        programme: &ResilienceTestingProgramme,
    ) -> Result<(), String> {
        self.store.save_testing_programme(programme)
    }

    pub fn get_testing_programme(
        &self,
        year: i32,
    ) -> Result<Option<ResilienceTestingProgramme>, String> {
        self.store.get_testing_programme(year)
    }

    // ── Third-Party ──

    pub fn register_provider(&self, provider: &ThirdPartyProvider) -> Result<(), String> {
        self.store.register_provider(provider)
    }

    pub fn list_providers(&self) -> Result<Vec<ThirdPartyProvider>, String> {
        self.store.list_providers()
    }

    /// Get providers that are missing exit strategies (DORA Article 30(3)(g)).
    pub fn get_providers_missing_exit_strategy(&self) -> Result<Vec<ThirdPartyProvider>, String> {
        let providers = self.store.list_providers()?;
        Ok(providers
            .into_iter()
            .filter(|p| !p.has_exit_strategy)
            .collect())
    }

    /// Get critical providers without recent audits.
    pub fn get_critical_providers_needing_audit(&self) -> Result<Vec<ThirdPartyProvider>, String> {
        let providers = self.store.list_providers()?;
        let ninety_days_ago = Utc::now() - chrono::Duration::days(90);
        Ok(providers
            .into_iter()
            .filter(|p| {
                p.criticality >= ProviderCriticality::High
                    && p.last_audited_at.is_none_or(|d| d < ninety_days_ago)
            })
            .collect())
    }

    // ── Information Sharing ──

    pub fn save_sharing_arrangement(
        &self,
        arrangement: &InformationSharingArrangement,
    ) -> Result<(), String> {
        self.store.save_sharing_arrangement(arrangement)
    }

    pub fn list_sharing_arrangements(&self) -> Result<Vec<InformationSharingArrangement>, String> {
        self.store.list_sharing_arrangements()
    }

    /// Generate a DORA compliance status summary.
    pub fn compliance_status(&self) -> DoraComplianceStatus {
        let framework = self.store.get_framework().ok().flatten();
        let incidents = self.store.list_incidents().unwrap_or_default();
        let providers = self.store.list_providers().unwrap_or_default();
        let sharing = self.store.list_sharing_arrangements().unwrap_or_default();

        let overdue_reports = incidents
            .iter()
            .filter(|i| i.is_reporting_deadline_breached())
            .count();
        let open_incidents = incidents
            .iter()
            .filter(|i| !matches!(i.status, IncidentStatus::Closed))
            .count();
        let missing_exit_strategies = providers.iter().filter(|p| !p.has_exit_strategy).count();

        DoraComplianceStatus {
            has_framework: framework.is_some(),
            framework_review_overdue: framework.as_ref().is_none_or(|f| f.is_review_overdue()),
            total_incidents: incidents.len(),
            open_incidents,
            overdue_reports,
            total_providers: providers.len(),
            missing_exit_strategies,
            total_sharing_arrangements: sharing.len(),
            active_sharing_arrangements: sharing
                .iter()
                .filter(|s| s.status == SharingStatus::Active)
                .count(),
        }
    }
}

/// DORA compliance status summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoraComplianceStatus {
    pub has_framework: bool,
    pub framework_review_overdue: bool,
    pub total_incidents: usize,
    pub open_incidents: usize,
    pub overdue_reports: usize,
    pub total_providers: usize,
    pub missing_exit_strategies: usize,
    pub total_sharing_arrangements: usize,
    pub active_sharing_arrangements: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_service() -> DoraService {
        DoraService::new(Arc::new(InMemoryDoraStore::new()))
    }

    // ── Framework tests ──

    #[test]
    fn test_framework_creation() {
        let fw = IctRiskManagementFramework::new("CTO");
        assert_eq!(fw.areas.len(), 4);
        assert!(!fw.is_review_overdue());
    }

    #[test]
    fn test_framework_with_strategy() {
        let fw = IctRiskManagementFramework::new("CTO").with_strategy(
            IctRiskArea::InformationSecurity,
            RiskStrategy {
                area: IctRiskArea::InformationSecurity,
                risk_level: RiskLevel::Medium,
                mitigation_controls: vec!["Encryption at rest".into(), "MFA".into()],
                residual_risk: RiskLevel::Low,
                owner: "Security Team".into(),
                review_date: Utc::now(),
            },
        );
        assert!(fw
            .strategies
            .contains_key(&IctRiskArea::InformationSecurity));
    }

    #[test]
    fn test_save_and_get_framework() {
        let svc = make_service();
        let fw = IctRiskManagementFramework::new("CTO");
        svc.save_framework(&fw).unwrap();

        let retrieved = svc.get_framework().unwrap().unwrap();
        assert_eq!(retrieved.approved_by, "CTO");
    }

    // ── Incident tests ──

    #[test]
    fn test_incident_creation() {
        let incident = IctIncident::new(
            "Git server outage",
            "SSH service unavailable for 2 hours",
            IncidentSeverity::Major,
            IncidentClassification::AvailabilityIncident,
        );
        assert_eq!(incident.status, IncidentStatus::Detected);
        assert!(!incident.is_reporting_deadline_breached());
    }

    #[test]
    fn test_incident_notification() {
        let svc = make_service();
        let incident = IctIncident::new(
            "Data breach",
            "Unauthorised access to user data",
            IncidentSeverity::Critical,
            IncidentClassification::DataBreach,
        );
        svc.report_incident(&incident).unwrap();

        svc.notify_authority(incident.id, "EU-FCA-2026-001")
            .unwrap();

        let updated = svc.store.get_incident(incident.id).unwrap().unwrap();
        assert_eq!(updated.status, IncidentStatus::Reported);
        assert!(updated.notified_at.is_some());
        assert_eq!(
            updated.authority_reference.as_deref(),
            Some("EU-FCA-2026-001")
        );
    }

    #[test]
    fn test_list_incidents() {
        let svc = make_service();
        let i1 = IctIncident::new(
            "i1",
            "desc",
            IncidentSeverity::Minor,
            IncidentClassification::SystemFailure,
        );
        let i2 = IctIncident::new(
            "i2",
            "desc",
            IncidentSeverity::Major,
            IncidentClassification::CyberAttack,
        );
        svc.report_incident(&i1).unwrap();
        svc.report_incident(&i2).unwrap();

        let incidents = svc.list_incidents().unwrap();
        assert_eq!(incidents.len(), 2);
    }

    // ── Testing programme tests ──

    #[test]
    fn test_testing_programme() {
        let svc = make_service();
        let programme = ResilienceTestingProgramme::new(2026, "CISO")
            .with_test(ResilienceTest::new(
                ResilienceTestType::VulnerabilityAssessment,
                "All production systems",
                "Security Team",
            ))
            .with_test(ResilienceTest::new(
                ResilienceTestType::ScenarioTesting,
                "Git server failover",
                "Ops Team",
            ));

        svc.save_testing_programme(&programme).unwrap();

        let retrieved = svc.get_testing_programme(2026).unwrap().unwrap();
        assert_eq!(retrieved.tests.len(), 2);
    }

    #[test]
    fn test_tlpt_required_for_systemically_important() {
        let programme = ResilienceTestingProgramme::new(2026, "CISO");
        assert!(programme.requires_tlpt(true));
        assert!(!programme.requires_tlpt(false));
    }

    #[test]
    fn test_test_finding() {
        let finding = TestFinding::new(
            RiskLevel::High,
            "Unencrypted data at rest",
            "LFS objects stored without encryption",
            "Enable AES-256-GCM encryption",
            Utc::now() + chrono::Duration::days(30),
        );
        assert_eq!(finding.status, FindingStatus::Open);
        assert_eq!(finding.severity, RiskLevel::High);
    }

    // ── Third-party provider tests ──

    #[test]
    fn test_register_provider() {
        let svc = make_service();
        let provider = ThirdPartyProvider::new(
            "AWS",
            ThirdPartyServiceType::CloudInfrastructure,
            ProviderCriticality::Critical,
            "AWS-ENT-2026-001",
        );
        svc.register_provider(&provider).unwrap();

        let providers = svc.list_providers().unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name, "AWS");
    }

    #[test]
    fn test_providers_missing_exit_strategy() {
        let svc = make_service();
        let p1 = ThirdPartyProvider::new(
            "AWS",
            ThirdPartyServiceType::CloudInfrastructure,
            ProviderCriticality::Critical,
            "ref1",
        );
        let mut p2 = ThirdPartyProvider::new(
            "GitHub",
            ThirdPartyServiceType::SoftwareAsAService,
            ProviderCriticality::High,
            "ref2",
        );
        p2.has_exit_strategy = true;

        svc.register_provider(&p1).unwrap();
        svc.register_provider(&p2).unwrap();

        let missing = svc.get_providers_missing_exit_strategy().unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].name, "AWS");
    }

    #[test]
    fn test_critical_providers_needing_audit() {
        let svc = make_service();
        let mut p = ThirdPartyProvider::new(
            "AWS",
            ThirdPartyServiceType::CloudInfrastructure,
            ProviderCriticality::Critical,
            "ref",
        );
        p.last_audited_at = Some(Utc::now() - chrono::Duration::days(120));

        svc.register_provider(&p).unwrap();

        let needs_audit = svc.get_critical_providers_needing_audit().unwrap();
        assert_eq!(needs_audit.len(), 1);
    }

    // ── Information sharing tests ──

    #[test]
    fn test_sharing_arrangement() {
        let svc = make_service();
        let arrangement = InformationSharingArrangement::new(
            "FS-ISAC",
            SharingType::Isac,
            "Cyber threat intelligence",
        );
        svc.save_sharing_arrangement(&arrangement).unwrap();

        let arrangements = svc.list_sharing_arrangements().unwrap();
        assert_eq!(arrangements.len(), 1);
        assert_eq!(arrangements[0].status, SharingStatus::Active);
    }

    // ── Compliance status tests ──

    #[test]
    fn test_compliance_status_empty() {
        let svc = make_service();
        let status = svc.compliance_status();
        assert!(!status.has_framework);
        assert!(status.framework_review_overdue);
        assert_eq!(status.total_incidents, 0);
        assert_eq!(status.total_providers, 0);
    }

    #[test]
    fn test_compliance_status_with_data() {
        let svc = make_service();

        // Save framework
        let fw = IctRiskManagementFramework::new("CTO");
        svc.save_framework(&fw).unwrap();

        // Report incident
        let incident = IctIncident::new(
            "test",
            "desc",
            IncidentSeverity::Minor,
            IncidentClassification::SystemFailure,
        );
        svc.report_incident(&incident).unwrap();

        // Register provider
        let mut provider = ThirdPartyProvider::new(
            "AWS",
            ThirdPartyServiceType::CloudInfrastructure,
            ProviderCriticality::High,
            "ref",
        );
        provider.has_exit_strategy = true;
        svc.register_provider(&provider).unwrap();

        // Add sharing arrangement
        let sharing =
            InformationSharingArrangement::new("FS-ISAC", SharingType::Isac, "Threat intel");
        svc.save_sharing_arrangement(&sharing).unwrap();

        let status = svc.compliance_status();
        assert!(status.has_framework);
        assert!(!status.framework_review_overdue);
        assert_eq!(status.total_incidents, 1);
        assert_eq!(status.open_incidents, 1);
        assert_eq!(status.total_providers, 1);
        assert_eq!(status.missing_exit_strategies, 0);
        assert_eq!(status.active_sharing_arrangements, 1);
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
        assert!(RiskLevel::Low > RiskLevel::Negligible);
    }

    #[test]
    fn test_incident_severity_ordering() {
        assert!(IncidentSeverity::Critical > IncidentSeverity::Major);
        assert!(IncidentSeverity::Major > IncidentSeverity::Minor);
    }
}

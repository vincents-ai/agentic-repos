//! NIS2 (Network and Information Security Directive 2) compliance framework.
//!
//! Implements EU Directive 2022/2555 (NIS2) requirements for:
//! - Entity classification (essential vs important)
//! - Security measures (Article 21)
//! - Incident notification to CSIRT (Article 23): initial 24h, detailed 72h, final 1 month
//! - Supply chain security (Article 21(2)(d))
//! - Risk management obligations
//!
//! EU precedence applies — where EU and US requirements conflict, EU standards win.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════════
// Entity Classification (Article 2)
// ═══════════════════════════════════════════════════════════════════════════════

/// NIS2 entity classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    /// Essential entities — stricter obligations, proactive supervision.
    Essential,
    /// Important entities — standard obligations, reactive supervision.
    Important,
}

/// Sector classification (Annex I & II).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Nis2Sector {
    // Annex I — Essential entities
    Energy,
    Transport,
    Banking,
    FinancialMarketInfrastructure,
    Health,
    DrinkingWater,
    Wastewater,
    DigitalInfrastructure,
    PublicAdministration,
    Space,
    // Annex II — Important entities
    PostalServices,
    WasteManagement,
    Chemicals,
    FoodProduction,
    Manufacturing,
    DigitalProviders,
    Research,
}

impl std::fmt::Display for Nis2Sector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        write!(f, "{}", s)
    }
}

/// Entity profile for NIS2 classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nis2EntityProfile {
    pub id: Uuid,
    pub name: String,
    pub entity_type: EntityType,
    pub sector: Nis2Sector,
    /// Sub-sectors if applicable.
    pub sub_sectors: Vec<String>,
    /// Number of employees (threshold for classification).
    pub employee_count: u32,
    /// Annual turnover in EUR (threshold for classification).
    pub annual_turnover_eur: Option<f64>,
    /// Whether the entity operates in multiple member states.
    pub cross_border: bool,
    /// Competent authority / CSIRT assigned.
    pub competent_authority: Option<String>,
    /// Single point of contact.
    pub single_point_of_contact: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Nis2EntityProfile {
    pub fn new(
        name: impl Into<String>,
        entity_type: EntityType,
        sector: Nis2Sector,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            entity_type,
            sector,
            sub_sectors: Vec::new(),
            employee_count: 0,
            annual_turnover_eur: None,
            cross_border: false,
            competent_authority: None,
            single_point_of_contact: None,
            created_at: now,
            updated_at: now,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Security Measures (Article 21)
// ═══════════════════════════════════════════════════════════════════════════════

/// Security measures required under Article 21.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityMeasures {
    pub id: Uuid,
    /// Entity this applies to.
    pub entity_id: Uuid,
    /// Policies for risk analysis and information system security.
    pub risk_analysis_policies: Vec<String>,
    /// Incident handling procedures.
    pub incident_handling: Vec<String>,
    /// Business continuity and crisis management.
    pub business_continuity: BusinessContinuityMeasures,
    /// Supply chain security measures.
    pub supply_chain_security: SupplyChainSecurity,
    /// Network and information systems security.
    pub systems_security: SystemsSecurityMeasures,
    /// Training and cyber hygiene practices.
    pub training_measures: Vec<String>,
    /// Cryptography and encryption policies.
    pub crypto_policies: Vec<String>,
    /// Multi-factor authentication deployment.
    pub mfa_deployed: bool,
    /// Last assessment date.
    pub assessed_at: DateTime<Utc>,
    /// Next assessment due.
    pub next_assessment_at: DateTime<Utc>,
}

impl SecurityMeasures {
    pub fn new(entity_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            entity_id,
            risk_analysis_policies: Vec::new(),
            incident_handling: Vec::new(),
            business_continuity: BusinessContinuityMeasures::default(),
            supply_chain_security: SupplyChainSecurity::default(),
            systems_security: SystemsSecurityMeasures::default(),
            training_measures: Vec::new(),
            crypto_policies: Vec::new(),
            mfa_deployed: false,
            assessed_at: now,
            next_assessment_at: now + chrono::Duration::days(365),
        }
    }

    /// Check if assessment is overdue.
    pub fn is_assessment_overdue(&self) -> bool {
        Utc::now() > self.next_assessment_at
    }
}

/// Business continuity measures (Article 21(2)(c)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessContinuityMeasures {
    pub has_bcp: bool,
    pub has_dr_plan: bool,
    pub backup_frequency_days: u32,
    pub last_dr_test: Option<DateTime<Utc>>,
    pub dr_test_frequency_months: u32,
    pub rto_hours: f64,
    pub rpo_hours: f64,
}

impl Default for BusinessContinuityMeasures {
    fn default() -> Self {
        Self {
            has_bcp: false,
            has_dr_plan: false,
            backup_frequency_days: 1,
            last_dr_test: None,
            dr_test_frequency_months: 6,
            rto_hours: 4.0,
            rpo_hours: 1.0,
        }
    }
}

/// Supply chain security measures (Article 21(2)(d)).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SupplyChainSecurity {
    /// Supplier security assessment programme in place.
    pub has_supplier_assessment: bool,
    /// Contracts include security requirements.
    pub contracts_include_security: bool,
    /// Audited suppliers.
    pub audited_suppliers: u32,
    /// Total critical suppliers.
    pub total_critical_suppliers: u32,
    /// Security levels required for suppliers.
    pub supplier_security_levels: HashMap<String, SupplierSecurityLevel>,
}

/// Security level required of suppliers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SupplierSecurityLevel {
    Basic,
    Standard,
    Enhanced,
    Critical,
}

/// Systems security measures (Article 21(2)(g)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemsSecurityMeasures {
    pub vulnerability_scanning_enabled: bool,
    pub network_segmentation: bool,
    pub endpoint_detection: bool,
    pub log_retention_days: u32,
    pub siem_deployed: bool,
    pub patch_management_policy: bool,
    pub last_vulnerability_scan: Option<DateTime<Utc>>,
}

impl Default for SystemsSecurityMeasures {
    fn default() -> Self {
        Self {
            vulnerability_scanning_enabled: false,
            network_segmentation: false,
            endpoint_detection: false,
            log_retention_days: 90,
            siem_deployed: false,
            patch_management_policy: false,
            last_vulnerability_scan: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Incident Notification (Article 23)
// ═══════════════════════════════════════════════════════════════════════════════

/// NIS2 significant incident (Article 23).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nis2Incident {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub title: String,
    pub description: String,
    pub severity: Nis2IncidentSeverity,
    pub status: Nis2IncidentStatus,
    pub category: Nis2IncidentCategory,
    /// Whether the incident is "significant" per Article 23 criteria.
    pub is_significant: bool,
    /// When the incident was detected.
    pub detected_at: DateTime<Utc>,
    /// Initial notification (24-hour deadline).
    pub initial_notification: Option<NotificationRecord>,
    /// Detailed notification (72-hour deadline).
    pub detailed_notification: Option<NotificationRecord>,
    /// Final report (1-month deadline).
    pub final_report: Option<NotificationRecord>,
    /// Impact assessment.
    pub impact: Nis2ImpactAssessment,
    /// Remediation measures taken.
    pub remediation: Vec<String>,
    /// Cross-border impact.
    pub cross_border_impact: bool,
    /// Affected member states.
    pub affected_member_states: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Nis2Incident {
    pub fn new(
        entity_id: Uuid,
        title: impl Into<String>,
        description: impl Into<String>,
        severity: Nis2IncidentSeverity,
        category: Nis2IncidentCategory,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            entity_id,
            title: title.into(),
            description: description.into(),
            is_significant: matches!(severity, Nis2IncidentSeverity::Significant | Nis2IncidentSeverity::Major),
            severity,
            status: Nis2IncidentStatus::Detected,
            category,
            detected_at: now,
            initial_notification: None,
            detailed_notification: None,
            final_report: None,
            impact: Nis2ImpactAssessment::default(),
            remediation: Vec::new(),
            cross_border_impact: false,
            affected_member_states: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Deadline for initial notification (24 hours from detection).
    pub fn initial_notification_deadline(&self) -> DateTime<Utc> {
        self.detected_at + chrono::Duration::hours(24)
    }

    /// Deadline for detailed notification (72 hours from detection).
    pub fn detailed_notification_deadline(&self) -> DateTime<Utc> {
        self.detected_at + chrono::Duration::hours(72)
    }

    /// Deadline for final report (1 month from detection).
    pub fn final_report_deadline(&self) -> DateTime<Utc> {
        self.detected_at + chrono::Duration::days(30)
    }

    /// Whether the initial notification is overdue.
    pub fn is_initial_notification_overdue(&self) -> bool {
        self.initial_notification.is_none() && Utc::now() > self.initial_notification_deadline()
    }

    /// Whether the detailed notification is overdue.
    pub fn is_detailed_notification_overdue(&self) -> bool {
        self.initial_notification.is_some()
            && self.detailed_notification.is_none()
            && Utc::now() > self.detailed_notification_deadline()
    }

    /// Whether the final report is overdue.
    pub fn is_final_report_overdue(&self) -> bool {
        self.detailed_notification.is_some()
            && self.final_report.is_none()
            && Utc::now() > self.final_report_deadline()
    }

    /// Submit initial notification to CSIRT.
    pub fn submit_initial_notification(
        &mut self,
        csirt_ref: impl Into<String>,
        summary: impl Into<String>,
    ) {
        self.initial_notification = Some(NotificationRecord {
            id: Uuid::new_v4(),
            notification_type: NotificationType::Initial,
            csirt_reference: csirt_ref.into(),
            summary: summary.into(),
            submitted_at: Utc::now(),
            submitted_by: String::new(),
        });
        self.status = Nis2IncidentStatus::InitialNotified;
        self.updated_at = Utc::now();
    }

    /// Submit detailed notification to CSIRT.
    pub fn submit_detailed_notification(
        &mut self,
        csirt_ref: impl Into<String>,
        summary: impl Into<String>,
    ) {
        self.detailed_notification = Some(NotificationRecord {
            id: Uuid::new_v4(),
            notification_type: NotificationType::Detailed,
            csirt_reference: csirt_ref.into(),
            summary: summary.into(),
            submitted_at: Utc::now(),
            submitted_by: String::new(),
        });
        self.status = Nis2IncidentStatus::DetailedNotified;
        self.updated_at = Utc::now();
    }

    /// Submit final report to CSIRT.
    pub fn submit_final_report(
        &mut self,
        csirt_ref: impl Into<String>,
        summary: impl Into<String>,
        _root_cause: impl Into<String>,
    ) {
        self.final_report = Some(NotificationRecord {
            id: Uuid::new_v4(),
            notification_type: NotificationType::Final,
            csirt_reference: csirt_ref.into(),
            summary: summary.into(),
            submitted_at: Utc::now(),
            submitted_by: String::new(),
        });
        self.status = Nis2IncidentStatus::Closed;
        self.updated_at = Utc::now();
    }
}

/// Incident severity (NIS2 Article 23 criteria).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Nis2IncidentSeverity {
    Low,
    Medium,
    Significant,
    Major,
}

/// Incident lifecycle status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Nis2IncidentStatus {
    Detected,
    Analysing,
    InitialNotified,
    DetailedNotified,
    Remediated,
    Closed,
}

/// Incident category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Nis2IncidentCategory {
    SystemCompromise,
    DataBreach,
    DenialOfService,
    Malware,
    Phishing,
    Ransomware,
    InsiderThreat,
    SupplyChainAttack,
    Misconfiguration,
    PhysicalIncident,
    Other,
}

/// Impact assessment per NIS2 Article 23(2) criteria.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nis2ImpactAssessment {
    /// Number of affected users/customers.
    pub affected_users: u64,
    /// Geographic scope.
    pub geographic_scope: GeographicScope,
    /// Duration of service disruption.
    pub disruption_duration_hours: Option<f64>,
    /// Whether essential services are affected.
    pub essential_services_affected: bool,
    /// Economic impact estimate (EUR).
    pub economic_impact_eur: Option<f64>,
    /// Whether the incident caused physical or material damage.
    pub physical_damage: bool,
}

impl Default for Nis2ImpactAssessment {
    fn default() -> Self {
        Self {
            affected_users: 0,
            geographic_scope: GeographicScope::SingleMemberState,
            disruption_duration_hours: None,
            essential_services_affected: false,
            economic_impact_eur: None,
            physical_damage: false,
        }
    }
}

/// Geographic scope of impact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GeographicScope {
    SingleMemberState,
    MultipleMemberStates,
    CrossBorder,
    Global,
}

/// A notification record submitted to CSIRT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub id: Uuid,
    pub notification_type: NotificationType,
    pub csirt_reference: String,
    pub summary: String,
    pub submitted_at: DateTime<Utc>,
    pub submitted_by: String,
}

/// Type of notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    Initial,
    Detailed,
    Final,
}

// ═══════════════════════════════════════════════════════════════════════════════
// NIS2 Service
// ═══════════════════════════════════════════════════════════════════════════════

/// Storage trait for NIS2 compliance data.
pub trait Nis2Store: Send + Sync {
    // Entity profiles
    fn save_entity(&self, entity: &Nis2EntityProfile) -> Result<(), String>;
    fn get_entity(&self, id: Uuid) -> Result<Option<Nis2EntityProfile>, String>;
    fn list_entities(&self) -> Result<Vec<Nis2EntityProfile>, String>;

    // Security measures
    fn save_security_measures(&self, measures: &SecurityMeasures) -> Result<(), String>;
    fn get_security_measures(&self, entity_id: Uuid) -> Result<Option<SecurityMeasures>, String>;

    // Incidents
    fn create_incident(&self, incident: &Nis2Incident) -> Result<(), String>;
    fn update_incident(&self, incident: &Nis2Incident) -> Result<(), String>;
    fn list_incidents(&self) -> Result<Vec<Nis2Incident>, String>;
    fn get_incident(&self, id: Uuid) -> Result<Option<Nis2Incident>, String>;
}

/// In-memory NIS2 store.
pub struct InMemoryNis2Store {
    entities: std::sync::RwLock<HashMap<Uuid, Nis2EntityProfile>>,
    security_measures: std::sync::RwLock<HashMap<Uuid, SecurityMeasures>>,
    incidents: std::sync::RwLock<HashMap<Uuid, Nis2Incident>>,
}

impl InMemoryNis2Store {
    pub fn new() -> Self {
        Self {
            entities: std::sync::RwLock::new(HashMap::new()),
            security_measures: std::sync::RwLock::new(HashMap::new()),
            incidents: std::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryNis2Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Nis2Store for InMemoryNis2Store {
    fn save_entity(&self, entity: &Nis2EntityProfile) -> Result<(), String> {
        let mut entities = self.entities.write().map_err(|e| e.to_string())?;
        entities.insert(entity.id, entity.clone());
        Ok(())
    }

    fn get_entity(&self, id: Uuid) -> Result<Option<Nis2EntityProfile>, String> {
        let entities = self.entities.read().map_err(|e| e.to_string())?;
        Ok(entities.get(&id).cloned())
    }

    fn list_entities(&self) -> Result<Vec<Nis2EntityProfile>, String> {
        let entities = self.entities.read().map_err(|e| e.to_string())?;
        Ok(entities.values().cloned().collect())
    }

    fn save_security_measures(&self, measures: &SecurityMeasures) -> Result<(), String> {
        let mut store = self.security_measures.write().map_err(|e| e.to_string())?;
        store.insert(measures.entity_id, measures.clone());
        Ok(())
    }

    fn get_security_measures(&self, entity_id: Uuid) -> Result<Option<SecurityMeasures>, String> {
        let store = self.security_measures.read().map_err(|e| e.to_string())?;
        Ok(store.get(&entity_id).cloned())
    }

    fn create_incident(&self, incident: &Nis2Incident) -> Result<(), String> {
        let mut incidents = self.incidents.write().map_err(|e| e.to_string())?;
        incidents.insert(incident.id, incident.clone());
        Ok(())
    }

    fn update_incident(&self, incident: &Nis2Incident) -> Result<(), String> {
        let mut incidents = self.incidents.write().map_err(|e| e.to_string())?;
        incidents.insert(incident.id, incident.clone());
        Ok(())
    }

    fn list_incidents(&self) -> Result<Vec<Nis2Incident>, String> {
        let incidents = self.incidents.read().map_err(|e| e.to_string())?;
        Ok(incidents.values().cloned().collect())
    }

    fn get_incident(&self, id: Uuid) -> Result<Option<Nis2Incident>, String> {
        let incidents = self.incidents.read().map_err(|e| e.to_string())?;
        Ok(incidents.get(&id).cloned())
    }
}

/// NIS2 compliance service.
pub struct Nis2Service {
    store: std::sync::Arc<dyn Nis2Store>,
}

impl Nis2Service {
    pub fn new(store: std::sync::Arc<dyn Nis2Store>) -> Self {
        Self { store }
    }

    // ── Entity management ──

    pub fn register_entity(&self, entity: &Nis2EntityProfile) -> Result<(), String> {
        self.store.save_entity(entity)
    }

    pub fn get_entity(&self, id: Uuid) -> Result<Option<Nis2EntityProfile>, String> {
        self.store.get_entity(id)
    }

    pub fn list_entities(&self) -> Result<Vec<Nis2EntityProfile>, String> {
        self.store.list_entities()
    }

    // ── Security measures ──

    pub fn save_security_measures(&self, measures: &SecurityMeasures) -> Result<(), String> {
        self.store.save_security_measures(measures)
    }

    pub fn get_security_measures(&self, entity_id: Uuid) -> Result<Option<SecurityMeasures>, String> {
        self.store.get_security_measures(entity_id)
    }

    /// Check which entities have overdue security assessments.
    pub fn get_entities_with_overdue_assessments(&self) -> Result<Vec<Nis2EntityProfile>, String> {
        let entities = self.store.list_entities()?;
        let mut overdue = Vec::new();
        for entity in entities {
            if let Some(measures) = self.store.get_security_measures(entity.id)? {
                if measures.is_assessment_overdue() {
                    overdue.push(entity);
                }
            }
        }
        Ok(overdue)
    }

    // ── Incident management ──

    pub fn report_incident(&self, incident: &Nis2Incident) -> Result<(), String> {
        self.store.create_incident(incident)
    }

    pub fn list_incidents(&self) -> Result<Vec<Nis2Incident>, String> {
        self.store.list_incidents()
    }

    /// Submit initial notification (24h deadline).
    pub fn submit_initial_notification(
        &self,
        incident_id: Uuid,
        csirt_ref: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<(), String> {
        let mut incident = self
            .store
            .get_incident(incident_id)?
            .ok_or_else(|| format!("Incident {} not found", incident_id))?;
        incident.submit_initial_notification(csirt_ref, summary);
        self.store.update_incident(&incident)
    }

    /// Submit detailed notification (72h deadline).
    pub fn submit_detailed_notification(
        &self,
        incident_id: Uuid,
        csirt_ref: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<(), String> {
        let mut incident = self
            .store
            .get_incident(incident_id)?
            .ok_or_else(|| format!("Incident {} not found", incident_id))?;
        incident.submit_detailed_notification(csirt_ref, summary);
        self.store.update_incident(&incident)
    }

    /// Submit final report (1 month deadline).
    pub fn submit_final_report(
        &self,
        incident_id: Uuid,
        csirt_ref: impl Into<String>,
        summary: impl Into<String>,
        _root_cause: impl Into<String>,
    ) -> Result<(), String> {
        let mut incident = self
            .store
            .get_incident(incident_id)?
            .ok_or_else(|| format!("Incident {} not found", incident_id))?;
        incident.submit_final_report(csirt_ref, summary, _root_cause);
        self.store.update_incident(&incident)
    }

    /// Get incidents with overdue notifications.
    pub fn get_overdue_notifications(&self) -> Result<Nis2OverdueReport, String> {
        let incidents = self.store.list_incidents()?;
        let mut initial_overdue = Vec::new();
        let mut detailed_overdue = Vec::new();
        let mut final_overdue = Vec::new();

        for incident in &incidents {
            if incident.is_initial_notification_overdue() {
                initial_overdue.push(incident.id);
            }
            if incident.is_detailed_notification_overdue() {
                detailed_overdue.push(incident.id);
            }
            if incident.is_final_report_overdue() {
                final_overdue.push(incident.id);
            }
        }

        Ok(Nis2OverdueReport {
            initial_notification_overdue: initial_overdue,
            detailed_notification_overdue: detailed_overdue,
            final_report_overdue: final_overdue,
        })
    }

    /// Generate NIS2 compliance status summary.
    pub fn compliance_status(&self) -> Nis2ComplianceStatus {
        let entities = self.store.list_entities().unwrap_or_default();
        let incidents = self.store.list_incidents().unwrap_or_default();

        let overdue = self.get_overdue_notifications().unwrap_or_default();

        Nis2ComplianceStatus {
            total_entities: entities.len(),
            essential_entities: entities.iter().filter(|e| e.entity_type == EntityType::Essential).count(),
            important_entities: entities.iter().filter(|e| e.entity_type == EntityType::Important).count(),
            total_incidents: incidents.len(),
            significant_incidents: incidents.iter().filter(|i| i.is_significant).count(),
            open_incidents: incidents.iter().filter(|i| i.status != Nis2IncidentStatus::Closed).count(),
            overdue_initial_notifications: overdue.initial_notification_overdue.len(),
            overdue_detailed_notifications: overdue.detailed_notification_overdue.len(),
            overdue_final_reports: overdue.final_report_overdue.len(),
        }
    }
}

/// Overdue notification report.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Nis2OverdueReport {
    pub initial_notification_overdue: Vec<Uuid>,
    pub detailed_notification_overdue: Vec<Uuid>,
    pub final_report_overdue: Vec<Uuid>,
}

/// NIS2 compliance status summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nis2ComplianceStatus {
    pub total_entities: usize,
    pub essential_entities: usize,
    pub important_entities: usize,
    pub total_incidents: usize,
    pub significant_incidents: usize,
    pub open_incidents: usize,
    pub overdue_initial_notifications: usize,
    pub overdue_detailed_notifications: usize,
    pub overdue_final_reports: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_service() -> Nis2Service {
        Nis2Service::new(Arc::new(InMemoryNis2Store::new()))
    }

    // ── Entity tests ──

    #[test]
    fn test_entity_creation() {
        let entity = Nis2EntityProfile::new("GitCorp", EntityType::Essential, Nis2Sector::DigitalInfrastructure);
        assert_eq!(entity.entity_type, EntityType::Essential);
        assert_eq!(entity.sector, Nis2Sector::DigitalInfrastructure);
    }

    #[test]
    fn test_register_and_get_entity() {
        let svc = make_service();
        let entity = Nis2EntityProfile::new("GitCorp", EntityType::Essential, Nis2Sector::DigitalInfrastructure);
        let id = entity.id;
        svc.register_entity(&entity).unwrap();

        let retrieved = svc.get_entity(id).unwrap().unwrap();
        assert_eq!(retrieved.name, "GitCorp");
    }

    #[test]
    fn test_list_entities() {
        let svc = make_service();
        let e1 = Nis2EntityProfile::new("A", EntityType::Essential, Nis2Sector::Energy);
        let e2 = Nis2EntityProfile::new("B", EntityType::Important, Nis2Sector::DigitalProviders);
        svc.register_entity(&e1).unwrap();
        svc.register_entity(&e2).unwrap();

        let entities = svc.list_entities().unwrap();
        assert_eq!(entities.len(), 2);
    }

    // ── Security measures tests ──

    #[test]
    fn test_security_measures_creation() {
        let entity = Nis2EntityProfile::new("Test", EntityType::Essential, Nis2Sector::Banking);
        let measures = SecurityMeasures::new(entity.id);
        assert!(!measures.is_assessment_overdue());
    }

    #[test]
    fn test_save_and_get_security_measures() {
        let svc = make_service();
        let entity = Nis2EntityProfile::new("Test", EntityType::Essential, Nis2Sector::Banking);
        svc.register_entity(&entity).unwrap();

        let measures = SecurityMeasures::new(entity.id);
        svc.save_security_measures(&measures).unwrap();

        let retrieved = svc.get_security_measures(entity.id).unwrap().unwrap();
        assert!(!retrieved.mfa_deployed);
    }

    // ── Incident tests ──

    #[test]
    fn test_incident_creation() {
        let entity_id = Uuid::new_v4();
        let incident = Nis2Incident::new(
            entity_id,
            "Ransomware attack",
            "Critical systems encrypted",
            Nis2IncidentSeverity::Major,
            Nis2IncidentCategory::Ransomware,
        );
        assert!(incident.is_significant);
        assert_eq!(incident.status, Nis2IncidentStatus::Detected);
    }

    #[test]
    fn test_incident_non_significant() {
        let incident = Nis2Incident::new(
            Uuid::new_v4(),
            "Minor outage",
            "Brief service degradation",
            Nis2IncidentSeverity::Low,
            Nis2IncidentCategory::Misconfiguration,
        );
        assert!(!incident.is_significant);
    }

    #[test]
    fn test_notification_deadlines() {
        let incident = Nis2Incident::new(
            Uuid::new_v4(),
            "Test",
            "Test",
            Nis2IncidentSeverity::Significant,
            Nis2IncidentCategory::DataBreach,
        );
        let initial = incident.initial_notification_deadline();
        let detailed = incident.detailed_notification_deadline();
        let final_dl = incident.final_report_deadline();

        assert!(detailed > initial);
        assert!(final_dl > detailed);
        assert_eq!((detailed - incident.detected_at).num_hours(), 72);
        assert_eq!((initial - incident.detected_at).num_hours(), 24);
    }

    #[test]
    fn test_incident_notification_lifecycle() {
        let svc = make_service();
        let entity_id = Uuid::new_v4();

        let incident = Nis2Incident::new(
            entity_id,
            "Data breach",
            "Customer data exposed",
            Nis2IncidentSeverity::Significant,
            Nis2IncidentCategory::DataBreach,
        );
        let id = incident.id;
        svc.report_incident(&incident).unwrap();

        // Initial notification
        svc.submit_initial_notification(id, "CSIRT-EU-2026-001", "Initial: data breach detected").unwrap();
        let i = svc.store.get_incident(id).unwrap().unwrap();
        assert_eq!(i.status, Nis2IncidentStatus::InitialNotified);
        assert!(i.initial_notification.is_some());

        // Detailed notification
        svc.submit_detailed_notification(id, "CSIRT-EU-2026-001", "Detailed: 50k records affected").unwrap();
        let i = svc.store.get_incident(id).unwrap().unwrap();
        assert_eq!(i.status, Nis2IncidentStatus::DetailedNotified);
        assert!(i.detailed_notification.is_some());

        // Final report
        svc.submit_final_report(id, "CSIRT-EU-2026-001", "Final: remediated, root cause identified", "Unpatched server").unwrap();
        let i = svc.store.get_incident(id).unwrap().unwrap();
        assert_eq!(i.status, Nis2IncidentStatus::Closed);
        assert!(i.final_report.is_some());
    }

    #[test]
    fn test_overdue_notifications() {
        let svc = make_service();
        // Create an incident with a manually set old detection time
        let mut incident = Nis2Incident::new(
            Uuid::new_v4(),
            "Old incident",
            "Detected long ago",
            Nis2IncidentSeverity::Significant,
            Nis2IncidentCategory::Malware,
        );
        // Backdate detection by 48 hours
        incident.detected_at = Utc::now() - chrono::Duration::hours(48);
        let id = incident.id;
        svc.report_incident(&incident).unwrap();

        let overdue = svc.get_overdue_notifications().unwrap();
        assert!(overdue.initial_notification_overdue.contains(&id));
    }

    // ── Compliance status tests ──

    #[test]
    fn test_compliance_status_empty() {
        let svc = make_service();
        let status = svc.compliance_status();
        assert_eq!(status.total_entities, 0);
        assert_eq!(status.total_incidents, 0);
    }

    #[test]
    fn test_compliance_status_with_data() {
        let svc = make_service();

        let e1 = Nis2EntityProfile::new("Essential Corp", EntityType::Essential, Nis2Sector::Energy);
        let e2 = Nis2EntityProfile::new("Important Corp", EntityType::Important, Nis2Sector::DigitalProviders);
        svc.register_entity(&e1).unwrap();
        svc.register_entity(&e2).unwrap();

        let incident = Nis2Incident::new(
            e1.id,
            "Test incident",
            "desc",
            Nis2IncidentSeverity::Significant,
            Nis2IncidentCategory::DataBreach,
        );
        svc.report_incident(&incident).unwrap();

        let status = svc.compliance_status();
        assert_eq!(status.total_entities, 2);
        assert_eq!(status.essential_entities, 1);
        assert_eq!(status.important_entities, 1);
        assert_eq!(status.total_incidents, 1);
        assert_eq!(status.significant_incidents, 1);
        assert_eq!(status.open_incidents, 1);
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Nis2IncidentSeverity::Major > Nis2IncidentSeverity::Significant);
        assert!(Nis2IncidentSeverity::Significant > Nis2IncidentSeverity::Medium);
        assert!(Nis2IncidentSeverity::Medium > Nis2IncidentSeverity::Low);
    }

    #[test]
    fn test_supplier_security_level_ordering() {
        assert!(SupplierSecurityLevel::Critical > SupplierSecurityLevel::Enhanced);
        assert!(SupplierSecurityLevel::Enhanced > SupplierSecurityLevel::Standard);
        assert!(SupplierSecurityLevel::Standard > SupplierSecurityLevel::Basic);
    }

    #[test]
    fn test_entities_with_overdue_assessments() {
        let svc = make_service();
        let entity = Nis2EntityProfile::new("Test", EntityType::Essential, Nis2Sector::Banking);
        svc.register_entity(&entity).unwrap();

        let mut measures = SecurityMeasures::new(entity.id);
        measures.next_assessment_at = Utc::now() - chrono::Duration::days(30);
        svc.save_security_measures(&measures).unwrap();

        let overdue = svc.get_entities_with_overdue_assessments().unwrap();
        assert_eq!(overdue.len(), 1);
    }
}

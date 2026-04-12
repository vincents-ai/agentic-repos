//! HIPAA compliance module — BAA tracking, compliance verification, and audit.
//!
//! US healthcare compliance. When EU and US rules conflict, EU takes precedence
//! (per project policy). This module handles HIPAA-specific requirements that
//! don't overlap with GDPR/DORA/NIS2.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Business Associate Agreement (BAA) record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessAssociateAgreement {
    pub id: Uuid,
    pub associate_name: String,
    pub associate_type: AssociateType,
    pub executed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub status: BaaStatus,
    pub contact_email: Option<String>,
    pub services_description: String,
    pub security_training_completed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BusinessAssociateAgreement {
    pub fn new(
        associate_name: impl Into<String>,
        associate_type: AssociateType,
        services_description: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            associate_name: associate_name.into(),
            associate_type,
            executed_at: None,
            expires_at: None,
            status: BaaStatus::Draft,
            contact_email: None,
            services_description: services_description.into(),
            security_training_completed: false,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_valid(&self) -> bool {
        match self.status {
            BaaStatus::Executed => {
                if let Some(expires) = self.expires_at {
                    expires > Utc::now()
                } else {
                    true
                }
            }
            _ => false,
        }
    }

    pub fn expiring_within_days(&self, days: i64) -> bool {
        if let Some(expires) = self.expires_at {
            let threshold = Utc::now() + chrono::Duration::days(days);
            expires < threshold
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssociateType {
    CloudProvider,
    SaasVendor,
    HealthcareProvider,
    HealthPlan,
    ThirdPartyAdmin,
    Consultant,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaaStatus {
    Draft,
    UnderReview,
    Executed,
    Expired,
    Terminated,
}

/// HIPAA compliance verification result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HipaaComplianceCheck {
    pub id: Uuid,
    pub check_type: HipaaCheckType,
    pub passed: bool,
    pub findings: Vec<ComplianceFinding>,
    pub checked_at: DateTime<Utc>,
}

impl HipaaComplianceCheck {
    pub fn new(check_type: HipaaCheckType) -> Self {
        Self {
            id: Uuid::new_v4(),
            check_type,
            passed: true,
            findings: Vec::new(),
            checked_at: Utc::now(),
        }
    }

    pub fn add_finding(mut self, finding: ComplianceFinding) -> Self {
        if finding.severity == FindingSeverity::Critical
            || finding.severity == FindingSeverity::High
        {
            self.passed = false;
        }
        self.findings.push(finding);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HipaaCheckType {
    AccessControl,
    AuditControls,
    IntegrityControls,
    AuthenticationControls,
    TransmissionSecurity,
    BusinessAssociateContracts,
    UsesAndDisclosures,
    AdministrativeRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFinding {
    pub rule: String,
    pub description: String,
    pub severity: FindingSeverity,
    pub remediation: String,
}

impl ComplianceFinding {
    pub fn critical(
        rule: impl Into<String>,
        description: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            rule: rule.into(),
            description: description.into(),
            severity: FindingSeverity::Critical,
            remediation: remediation.into(),
        }
    }

    pub fn high(
        rule: impl Into<String>,
        description: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            rule: rule.into(),
            description: description.into(),
            severity: FindingSeverity::High,
            remediation: remediation.into(),
        }
    }

    pub fn info(rule: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            description: description.into(),
            severity: FindingSeverity::Info,
            remediation: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, thiserror::Error)]
pub enum HipaaError {
    #[error("BAA not found: {0}")]
    BaaNotFound(Uuid),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("BAA expired for associate: {0}")]
    BaaExpired(String),
    #[error("Compliance check failed: {0}")]
    CheckFailed(String),
    #[error("Lock poisoned")]
    LockPoisoned,
}

/// In-memory BAA store.
pub struct InMemoryBaaStore {
    baas: std::sync::RwLock<std::collections::HashMap<Uuid, BusinessAssociateAgreement>>,
}

impl InMemoryBaaStore {
    pub fn new() -> Self {
        Self {
            baas: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn shared() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::new())
    }
}

impl Default for InMemoryBaaStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for BAA storage.
pub trait BaaStore: Send + Sync {
    fn add_baa(&self, baa: BusinessAssociateAgreement) -> Result<(), HipaaError>;
    fn get_baa(&self, id: Uuid) -> Result<Option<BusinessAssociateAgreement>, HipaaError>;
    fn list_baas(&self) -> Result<Vec<BusinessAssociateAgreement>, HipaaError>;
    fn update_baa(&self, baa: BusinessAssociateAgreement) -> Result<(), HipaaError>;
    fn remove_baa(&self, id: Uuid) -> Result<(), HipaaError>;
}

impl BaaStore for InMemoryBaaStore {
    fn add_baa(&self, baa: BusinessAssociateAgreement) -> Result<(), HipaaError> {
        let mut baas = self.baas.write().map_err(|_| HipaaError::LockPoisoned)?;
        baas.insert(baa.id, baa);
        Ok(())
    }

    fn get_baa(&self, id: Uuid) -> Result<Option<BusinessAssociateAgreement>, HipaaError> {
        let baas = self.baas.read().map_err(|_| HipaaError::LockPoisoned)?;
        Ok(baas.get(&id).cloned())
    }

    fn list_baas(&self) -> Result<Vec<BusinessAssociateAgreement>, HipaaError> {
        let baas = self.baas.read().map_err(|_| HipaaError::LockPoisoned)?;
        Ok(baas.values().cloned().collect())
    }

    fn update_baa(&self, baa: BusinessAssociateAgreement) -> Result<(), HipaaError> {
        let mut baas = self.baas.write().map_err(|_| HipaaError::LockPoisoned)?;
        if !baas.contains_key(&baa.id) {
            return Err(HipaaError::BaaNotFound(baa.id));
        }
        baas.insert(baa.id, baa);
        Ok(())
    }

    fn remove_baa(&self, id: Uuid) -> Result<(), HipaaError> {
        let mut baas = self.baas.write().map_err(|_| HipaaError::LockPoisoned)?;
        baas.remove(&id).ok_or(HipaaError::BaaNotFound(id))?;
        Ok(())
    }
}

/// HIPAA compliance service.
pub struct HipaaService {
    baa_store: std::sync::Arc<dyn BaaStore>,
}

impl HipaaService {
    pub fn new(baa_store: std::sync::Arc<dyn BaaStore>) -> Self {
        Self { baa_store }
    }

    pub fn create_baa(&self, baa: BusinessAssociateAgreement) -> Result<(), HipaaError> {
        self.baa_store.add_baa(baa)
    }

    pub fn get_baa(&self, id: Uuid) -> Result<Option<BusinessAssociateAgreement>, HipaaError> {
        self.baa_store.get_baa(id)
    }

    pub fn list_baas(&self) -> Result<Vec<BusinessAssociateAgreement>, HipaaError> {
        self.baa_store.list_baas()
    }

    pub fn execute_baa(&self, id: Uuid) -> Result<BusinessAssociateAgreement, HipaaError> {
        let mut baa = self
            .baa_store
            .get_baa(id)?
            .ok_or(HipaaError::BaaNotFound(id))?;
        baa.status = BaaStatus::Executed;
        baa.executed_at = Some(Utc::now());
        baa.updated_at = Utc::now();
        self.baa_store.update_baa(baa.clone())?;
        Ok(baa)
    }

    pub fn terminate_baa(&self, id: Uuid) -> Result<BusinessAssociateAgreement, HipaaError> {
        let mut baa = self
            .baa_store
            .get_baa(id)?
            .ok_or(HipaaError::BaaNotFound(id))?;
        baa.status = BaaStatus::Terminated;
        baa.updated_at = Utc::now();
        self.baa_store.update_baa(baa.clone())?;
        Ok(baa)
    }

    pub fn expiring_baas(
        &self,
        within_days: i64,
    ) -> Result<Vec<BusinessAssociateAgreement>, HipaaError> {
        let baas = self.baa_store.list_baas()?;
        Ok(baas
            .into_iter()
            .filter(|b| b.status == BaaStatus::Executed && b.expiring_within_days(within_days))
            .collect())
    }

    pub fn all_baas_valid(&self) -> Result<bool, HipaaError> {
        let baas = self.baa_store.list_baas()?;
        Ok(baas
            .iter()
            .all(|b| b.status != BaaStatus::Executed || b.is_valid()))
    }

    pub fn run_compliance_check(&self) -> HipaaComplianceCheck {
        let mut check = HipaaComplianceCheck::new(HipaaCheckType::AccessControl);
        let baas = self.list_baas().unwrap_or_default();
        let executed: Vec<_> = baas
            .iter()
            .filter(|b| b.status == BaaStatus::Executed)
            .collect();

        if executed.is_empty() {
            check = check.add_finding(ComplianceFinding::high(
                "§164.314(b)",
                "No executed Business Associate Agreements on file",
                "Execute BAAs with all business associates who handle PHI",
            ));
        }

        for baa in &executed {
            if !baa.security_training_completed {
                check = check.add_finding(ComplianceFinding::info(
                    "§164.308(a)(5)",
                    format!("Security training not completed for {}", baa.associate_name),
                ));
            }
        }
        check
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> HipaaService {
        HipaaService::new(InMemoryBaaStore::shared())
    }

    fn make_baa(name: &str) -> BusinessAssociateAgreement {
        BusinessAssociateAgreement::new(name, AssociateType::SaasVendor, "Cloud hosting")
    }

    #[test]
    fn test_create_and_list_baas() {
        let svc = make_service();
        svc.create_baa(make_baa("Vendor A")).unwrap();
        svc.create_baa(make_baa("Vendor B")).unwrap();
        assert_eq!(svc.list_baas().unwrap().len(), 2);
    }

    #[test]
    fn test_execute_baa() {
        let svc = make_service();
        let baa = make_baa("Test Vendor");
        svc.create_baa(baa.clone()).unwrap();
        let executed = svc.execute_baa(baa.id).unwrap();
        assert_eq!(executed.status, BaaStatus::Executed);
        assert!(executed.executed_at.is_some());
    }

    #[test]
    fn test_baa_validity() {
        let mut baa = make_baa("Valid");
        baa.status = BaaStatus::Executed;
        assert!(baa.is_valid());
        baa.status = BaaStatus::Draft;
        assert!(!baa.is_valid());
    }

    #[test]
    fn test_baa_expiry() {
        let mut baa = make_baa("Expiring");
        baa.status = BaaStatus::Executed;
        baa.expires_at = Some(Utc::now() + chrono::Duration::days(5));
        assert!(baa.expiring_within_days(30));
        assert!(!baa.expiring_within_days(3));
    }

    #[test]
    fn test_baa_expired() {
        let mut baa = make_baa("Expired");
        baa.status = BaaStatus::Executed;
        baa.expires_at = Some(Utc::now() - chrono::Duration::days(1));
        assert!(!baa.is_valid());
    }

    #[test]
    fn test_terminate_baa() {
        let svc = make_service();
        let baa = make_baa("Terminated");
        svc.create_baa(baa.clone()).unwrap();
        let terminated = svc.terminate_baa(baa.id).unwrap();
        assert_eq!(terminated.status, BaaStatus::Terminated);
    }

    #[test]
    fn test_compliance_check_no_baas() {
        let svc = make_service();
        let check = svc.run_compliance_check();
        assert!(!check.passed);
    }

    #[test]
    fn test_compliance_check_with_executed_baa() {
        let svc = make_service();
        let baa = make_baa("Good Vendor");
        svc.create_baa(baa).unwrap();
        svc.execute_baa(svc.list_baas().unwrap()[0].id).unwrap();
        let check = svc.run_compliance_check();
        assert!(check.passed);
    }

    #[test]
    fn test_remove_nonexistent_baa() {
        let svc = make_service();
        assert!(svc.baa_store.remove_baa(Uuid::new_v4()).is_err());
    }
}

//! SDLC monitoring service — metric collection, gate evaluation, assessments.

use crate::sdlc::models::*;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SdlcError {
    #[error("Gate not found: {0}")]
    GateNotFound(Uuid),
    #[error("Assessment not found: {0}")]
    AssessmentNotFound(Uuid),
    #[error("Metric not found: {0}")]
    MetricNotFound(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Lock poisoned")]
    LockPoisoned,
}

/// Trait for SDLC data persistence.
pub trait SdlcStore: Send + Sync {
    fn record_metric(&self, metric: QualityMetric) -> Result<(), SdlcError>;
    fn get_latest_metrics(&self) -> Result<Vec<QualityMetric>, SdlcError>;
    fn get_metrics_by_category(
        &self,
        category: &MetricCategory,
    ) -> Result<Vec<QualityMetric>, SdlcError>;
    fn clear_metrics(&self) -> Result<(), SdlcError>;

    fn add_gate(&self, gate: QualityGate) -> Result<(), SdlcError>;
    fn get_gate(&self, id: Uuid) -> Result<Option<QualityGate>, SdlcError>;
    fn list_gates(&self) -> Result<Vec<QualityGate>, SdlcError>;
    fn update_gate(&self, gate: QualityGate) -> Result<(), SdlcError>;
    fn remove_gate(&self, id: Uuid) -> Result<(), SdlcError>;

    fn add_assessment(&self, assessment: SdlcAssessment) -> Result<(), SdlcError>;
    fn get_latest_assessment(&self) -> Result<Option<SdlcAssessment>, SdlcError>;
    fn list_assessments(&self) -> Result<Vec<SdlcAssessment>, SdlcError>;
}

/// In-memory SDLC store.
pub struct InMemorySdlcStore {
    metrics: RwLock<Vec<QualityMetric>>,
    gates: RwLock<HashMap<Uuid, QualityGate>>,
    assessments: RwLock<Vec<SdlcAssessment>>,
}

impl InMemorySdlcStore {
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(Vec::new()),
            gates: RwLock::new(HashMap::new()),
            assessments: RwLock::new(Vec::new()),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl Default for InMemorySdlcStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SdlcStore for InMemorySdlcStore {
    fn record_metric(&self, metric: QualityMetric) -> Result<(), SdlcError> {
        let mut metrics = self.metrics.write().map_err(|_| SdlcError::LockPoisoned)?;
        // Keep only the latest metric per name
        metrics.retain(|m| m.name != metric.name);
        metrics.push(metric);
        Ok(())
    }

    fn get_latest_metrics(&self) -> Result<Vec<QualityMetric>, SdlcError> {
        let metrics = self.metrics.read().map_err(|_| SdlcError::LockPoisoned)?;
        Ok(metrics.iter().cloned().collect())
    }

    fn get_metrics_by_category(
        &self,
        category: &MetricCategory,
    ) -> Result<Vec<QualityMetric>, SdlcError> {
        let metrics = self.metrics.read().map_err(|_| SdlcError::LockPoisoned)?;
        Ok(metrics
            .iter()
            .filter(|m| m.category == *category)
            .cloned()
            .collect())
    }

    fn clear_metrics(&self) -> Result<(), SdlcError> {
        let mut metrics = self.metrics.write().map_err(|_| SdlcError::LockPoisoned)?;
        metrics.clear();
        Ok(())
    }

    fn add_gate(&self, gate: QualityGate) -> Result<(), SdlcError> {
        let mut gates = self.gates.write().map_err(|_| SdlcError::LockPoisoned)?;
        gates.insert(gate.id, gate);
        Ok(())
    }

    fn get_gate(&self, id: Uuid) -> Result<Option<QualityGate>, SdlcError> {
        let gates = self.gates.read().map_err(|_| SdlcError::LockPoisoned)?;
        Ok(gates.get(&id).cloned())
    }

    fn list_gates(&self) -> Result<Vec<QualityGate>, SdlcError> {
        let gates = self.gates.read().map_err(|_| SdlcError::LockPoisoned)?;
        Ok(gates.values().cloned().collect())
    }

    fn update_gate(&self, gate: QualityGate) -> Result<(), SdlcError> {
        let mut gates = self.gates.write().map_err(|_| SdlcError::LockPoisoned)?;
        gates.insert(gate.id, gate);
        Ok(())
    }

    fn remove_gate(&self, id: Uuid) -> Result<(), SdlcError> {
        let mut gates = self.gates.write().map_err(|_| SdlcError::LockPoisoned)?;
        gates.remove(&id).ok_or(SdlcError::GateNotFound(id))?;
        Ok(())
    }

    fn add_assessment(&self, assessment: SdlcAssessment) -> Result<(), SdlcError> {
        let mut assessments = self
            .assessments
            .write()
            .map_err(|_| SdlcError::LockPoisoned)?;
        assessments.push(assessment);
        Ok(())
    }

    fn get_latest_assessment(&self) -> Result<Option<SdlcAssessment>, SdlcError> {
        let assessments = self
            .assessments
            .read()
            .map_err(|_| SdlcError::LockPoisoned)?;
        Ok(assessments.last().cloned())
    }

    fn list_assessments(&self) -> Result<Vec<SdlcAssessment>, SdlcError> {
        let assessments = self
            .assessments
            .read()
            .map_err(|_| SdlcError::LockPoisoned)?;
        Ok(assessments.iter().cloned().collect())
    }
}

/// High-level SDLC monitoring service.
pub struct SdlcService {
    store: Arc<dyn SdlcStore>,
}

impl SdlcService {
    pub fn new(store: Arc<dyn SdlcStore>) -> Self {
        Self { store }
    }

    // ── Metrics ──

    pub fn record_metric(&self, metric: QualityMetric) -> Result<(), SdlcError> {
        self.store.record_metric(metric)
    }

    pub fn get_latest_metrics(&self) -> Result<Vec<QualityMetric>, SdlcError> {
        self.store.get_latest_metrics()
    }

    pub fn record_dora_metrics(&self, dora: &DoraMetrics) -> Result<(), SdlcError> {
        if let Some(freq) = dora.deployment_frequency {
            self.store.record_metric(QualityMetric::new(
                "deployment_frequency",
                MetricCategory::DeploymentFrequency,
                freq,
                1.0,
                "per_week",
            ))?;
        }
        if let Some(lead) = dora.lead_time_hours {
            self.store.record_metric(QualityMetric::new(
                "lead_time",
                MetricCategory::MTTD,
                lead,
                168.0,
                "hours",
            ))?;
        }
        if let Some(mttr) = dora.mttr_hours {
            self.store.record_metric(QualityMetric::new(
                "mttr",
                MetricCategory::MTTR,
                mttr,
                24.0,
                "hours",
            ))?;
        }
        if let Some(cfr) = dora.change_failure_rate {
            self.store.record_metric(QualityMetric::new(
                "change_failure_rate",
                MetricCategory::ChangeFailureRate,
                100.0 - cfr,
                90.0,
                "%", // invert: 95% success rate = 5% failure
            ))?;
        }
        Ok(())
    }

    // ── Gates ──

    pub fn create_gate(&self, gate: QualityGate) -> Result<(), SdlcError> {
        self.store.add_gate(gate)
    }

    pub fn list_gates(&self) -> Result<Vec<QualityGate>, SdlcError> {
        self.store.list_gates()
    }

    pub fn evaluate_gate(&self, id: Uuid) -> Result<GateStatus, SdlcError> {
        let mut gate = self
            .store
            .get_gate(id)?
            .ok_or(SdlcError::GateNotFound(id))?;
        let metrics = self.store.get_latest_metrics()?;
        let status = gate.evaluate(&metrics);
        self.store.update_gate(gate)?;
        Ok(status)
    }

    pub fn evaluate_all_gates(&self) -> Result<Vec<(Uuid, GateStatus)>, SdlcError> {
        let metrics = self.store.get_latest_metrics()?;
        let gates = self.store.list_gates()?;
        let mut results = Vec::new();
        for mut gate in gates {
            let id = gate.id;
            let status = gate.evaluate(&metrics);
            self.store.update_gate(gate)?;
            results.push((id, status));
        }
        Ok(results)
    }

    /// Check if any blocking gate is failing — used to block deployments.
    pub fn any_blocking_gates_failed(&self) -> Result<bool, SdlcError> {
        let gates = self.store.list_gates()?;
        Ok(gates.iter().any(|g| g.is_blocking()))
    }

    // ── Assessments ──

    pub fn create_assessment(&self, assessment: SdlcAssessment) -> Result<(), SdlcError> {
        self.store.add_assessment(assessment)
    }

    pub fn get_latest_assessment(&self) -> Result<Option<SdlcAssessment>, SdlcError> {
        self.store.get_latest_assessment()
    }

    pub fn list_assessments(&self) -> Result<Vec<SdlcAssessment>, SdlcError> {
        self.store.list_assessments()
    }

    // ── Dashboard ──

    pub fn build_dashboard(&self) -> Result<SdlcDashboard, SdlcError> {
        let metrics = self.store.get_latest_metrics()?;
        let gates = self.store.list_gates()?;
        let latest = self.store.get_latest_assessment()?;

        // Build DORA metrics from recorded values
        let mut dora = DoraMetrics::default();
        for m in &metrics {
            match m.name.as_str() {
                "deployment_frequency" => dora.deployment_frequency = Some(m.value),
                "lead_time" => dora.lead_time_hours = Some(m.value),
                "mttr" => dora.mttr_hours = Some(m.value),
                "change_failure_rate" => dora.change_failure_rate = Some(100.0 - m.value),
                _ => {}
            }
        }
        dora.classify();

        Ok(SdlcDashboard {
            metrics,
            gates,
            latest_assessment: latest,
            dora_metrics: dora,
            generated_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> SdlcService {
        SdlcService::new(InMemorySdlcStore::shared())
    }

    #[test]
    fn test_record_and_get_metrics() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "test_coverage",
            MetricCategory::TestCoverage,
            87.5,
            80.0,
            "%",
        ))
        .unwrap();
        let metrics = svc.get_latest_metrics().unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].value, 87.5);
    }

    #[test]
    fn test_metric_latest_overwrites() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            50.0,
            80.0,
            "%",
        ))
        .unwrap();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            90.0,
            80.0,
            "%",
        ))
        .unwrap();
        let metrics = svc.get_latest_metrics().unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].value, 90.0);
    }

    #[test]
    fn test_create_and_evaluate_gate() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            85.0,
            80.0,
            "%",
        ))
        .unwrap();

        let gate = QualityGate::new("merge", SdlcPhase::Implementation).with_condition(
            GateCondition::new(
                "cov",
                ComparisonOperator::GreaterEqual,
                80.0,
                "Coverage >= 80%",
            ),
        );
        svc.create_gate(gate.clone()).unwrap();

        let status = svc.evaluate_gate(gate.id).unwrap();
        assert_eq!(status, GateStatus::Passed);
    }

    #[test]
    fn test_evaluate_gate_fails() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            40.0,
            80.0,
            "%",
        ))
        .unwrap();

        let gate = QualityGate::new("release", SdlcPhase::Deployment)
            .with_condition(GateCondition::new(
                "cov",
                ComparisonOperator::GreaterEqual,
                80.0,
                "cov >= 80",
            ))
            .enforced();
        svc.create_gate(gate.clone()).unwrap();

        let status = svc.evaluate_gate(gate.id).unwrap();
        assert_eq!(status, GateStatus::Failed);
        assert!(svc.any_blocking_gates_failed().unwrap());
    }

    #[test]
    fn test_evaluate_all_gates() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            90.0,
            80.0,
            "%",
        ))
        .unwrap();

        let g1 = QualityGate::new("g1", SdlcPhase::Testing).with_condition(GateCondition::new(
            "cov",
            ComparisonOperator::GreaterEqual,
            80.0,
            "c",
        ));
        let g2 = QualityGate::new("g2", SdlcPhase::Deployment).with_condition(GateCondition::new(
            "cov",
            ComparisonOperator::GreaterEqual,
            95.0,
            "c",
        ));

        svc.create_gate(g1).unwrap();
        svc.create_gate(g2).unwrap();

        let results = svc.evaluate_all_gates().unwrap();
        assert_eq!(results.len(), 2);
        let statuses: Vec<_> = results.iter().map(|(_, s)| s).collect();
        assert!(statuses.contains(&&GateStatus::Passed));
        assert!(statuses.contains(&&GateStatus::Failed));
    }

    #[test]
    fn test_dora_metrics_recording() {
        let svc = make_service();
        let dora = DoraMetrics {
            deployment_frequency: Some(5.0),
            lead_time_hours: Some(48.0),
            mttr_hours: Some(6.0),
            change_failure_rate: Some(8.0),
            ..Default::default()
        };
        svc.record_dora_metrics(&dora).unwrap();
        let metrics = svc.get_latest_metrics().unwrap();
        assert_eq!(metrics.len(), 4);
    }

    #[test]
    fn test_dashboard_builds() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            85.0,
            80.0,
            "%",
        ))
        .unwrap();

        let gate = QualityGate::new("merge", SdlcPhase::Implementation).with_condition(
            GateCondition::new("cov", ComparisonOperator::GreaterEqual, 80.0, "c"),
        );
        svc.create_gate(gate).unwrap();
        svc.evaluate_all_gates().unwrap();

        let assessment = SdlcAssessment::new("Q1", AssessmentPeriod::q(2026, 1), "bot")
            .with_score(MetricCategory::TestCoverage, 85.0);
        svc.create_assessment(assessment).unwrap();

        let dash = svc.build_dashboard().unwrap();
        assert_eq!(dash.metrics.len(), 1);
        assert_eq!(dash.gates.len(), 1);
        assert!(dash.latest_assessment.is_some());
        assert_eq!(
            dash.dora_metrics.performance_level,
            DoraPerformanceLevel::Medium
        );
    }

    #[test]
    fn test_assessment_crud() {
        let svc = make_service();
        let a1 = SdlcAssessment::new("Q1", AssessmentPeriod::q(2026, 1), "bot");
        let a2 = SdlcAssessment::new("Q2", AssessmentPeriod::q(2026, 2), "bot");
        svc.create_assessment(a1).unwrap();
        svc.create_assessment(a2).unwrap();

        let latest = svc.get_latest_assessment().unwrap().unwrap();
        assert!(latest.title.contains("Q2"));
        assert_eq!(svc.list_assessments().unwrap().len(), 2);
    }

    #[test]
    fn test_remove_gate() {
        let svc = make_service();
        let gate = QualityGate::new("temp", SdlcPhase::Planning);
        svc.create_gate(gate.clone()).unwrap();
        svc.store.remove_gate(gate.id).unwrap();
        assert!(svc.store.get_gate(gate.id).unwrap().is_none());
    }

    #[test]
    fn test_no_blocking_gates_initially() {
        let svc = make_service();
        assert!(!svc.any_blocking_gates_failed().unwrap());
    }
}

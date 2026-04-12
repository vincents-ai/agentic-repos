//! Prometheus metrics exporter for SDLC monitoring.
//!
//! Exports QualityMetric values, DORA metrics, quality gate statuses,
//! and assessment scores as Prometheus gauges and counters so they can
//! be scraped by Prometheus and visualised in Grafana dashboards.
//!
//! ## Exported Metrics
//!
//! - `sdlc_metric_value` (gauge) — current value of each quality metric, labelled by name/category
//! - `sdlc_metric_target` (gauge) — target threshold for each quality metric
//! - `sdlc_metric_passing` (gauge) — 1 if passing, 0 if failing
//! - `sdlc_dora_deployment_frequency` (gauge) — DORA deployment frequency
//! - `sdlc_dora_lead_time_hours` (gauge) — DORA lead time for changes
//! - `sdlc_dora_mttr_hours` (gauge) — DORA mean time to restore
//! - `sdlc_dora_change_failure_rate` (gauge) — DORA change failure rate
//! - `sdlc_dora_performance_level` (gauge) — 3=Elite, 2=High, 1=Medium, 0=Low
//! - `sdlc_gate_status` (gauge) — 1=Passed, 0=Failed, -1=Pending, -2=Waived per gate
//! - `sdlc_assessment_score` (gauge) — overall assessment score
//! - `sdlc_assessment_category_score` (gauge) — per-category assessment score
//! - `sdlc_assessments_total` (counter) — total number of assessments run
//! - `sdlc_gates_evaluated_total` (counter) — total gate evaluations
//! - `sdlc_blocking_gates` (gauge) — number of currently blocking gates

use crate::sdlc::models::*;
use crate::sdlc::service::SdlcService;
use lazy_static::lazy_static;
use prometheus::{
    self,
    register_gauge, register_gauge_vec, register_int_counter_vec, register_int_gauge, Gauge, GaugeVec,
    IntCounterVec, IntGauge,
};

lazy_static! {
    // ── Quality Metrics ──

    pub static ref SDLC_METRIC_VALUE: GaugeVec = register_gauge_vec!(
        "sdlc_metric_value",
        "Current value of an SDLC quality metric",
        &["name", "category"]
    )
    .expect("Failed to create sdlc_metric_value");

    pub static ref SDLC_METRIC_TARGET: GaugeVec = register_gauge_vec!(
        "sdlc_metric_target",
        "Target threshold for an SDLC quality metric",
        &["name", "category"]
    )
    .expect("Failed to create sdlc_metric_target");

    pub static ref SDLC_METRIC_PASSING: GaugeVec = register_gauge_vec!(
        "sdlc_metric_passing",
        "Whether the metric is passing its target (1=pass, 0=fail)",
        &["name", "category"]
    )
    .expect("Failed to create sdlc_metric_passing");

    // ── DORA Metrics ──

    pub static ref SDLC_DORA_DEPLOYMENT_FREQUENCY: Gauge =
        register_gauge!("sdlc_dora_deployment_frequency_x100", "DORA deployment frequency * 100 (per week)")
        .expect("Failed to create sdlc_dora_deployment_frequency");

    pub static ref SDLC_DORA_DEPLOYMENT_FREQUENCY_F: GaugeVec = register_gauge_vec!(
        "sdlc_dora_deployment_frequency",
        "DORA deployment frequency (per week)",
        &["unit"]
    )
    .expect("Failed to create sdlc_dora_deployment_frequency_f");

    pub static ref SDLC_DORA_LEAD_TIME_HOURS: GaugeVec = register_gauge_vec!(
        "sdlc_dora_lead_time_hours",
        "DORA lead time for changes in hours",
        &["unit"]
    )
    .expect("Failed to create sdlc_dora_lead_time_hours");

    pub static ref SDLC_DORA_MTTR_HOURS: GaugeVec = register_gauge_vec!(
        "sdlc_dora_mttr_hours",
        "DORA mean time to restore in hours",
        &["unit"]
    )
    .expect("Failed to create sdlc_dora_mttr_hours");

    pub static ref SDLC_DORA_CHANGE_FAILURE_RATE: GaugeVec = register_gauge_vec!(
        "sdlc_dora_change_failure_rate",
        "DORA change failure rate as percentage",
        &["unit"]
    )
    .expect("Failed to create sdlc_dora_change_failure_rate");

    pub static ref SDLC_DORA_PERFORMANCE_LEVEL: Gauge =
        register_gauge!("sdlc_dora_performance_level", "DORA performance level: 3=Elite, 2=High, 1=Medium, 0=Low")
        .expect("Failed to create sdlc_dora_performance_level");

    // ── Quality Gates ──

    pub static ref SDLC_GATE_STATUS: GaugeVec = register_gauge_vec!(
        "sdlc_gate_status",
        "Quality gate status: 1=Passed, 0=Failed, -1=Pending, -2=Waived",
        &["gate_name", "phase", "enforcement"]
    )
    .expect("Failed to create sdlc_gate_status");

    pub static ref SDLC_BLOCKING_GATES: IntGauge =
        register_int_gauge!("sdlc_blocking_gates", "Number of currently blocking quality gates")
        .expect("Failed to create sdlc_blocking_gates");

    // ── Assessments ──

    pub static ref SDLC_ASSESSMENT_SCORE: GaugeVec = register_gauge_vec!(
        "sdlc_assessment_score",
        "Overall SDLC assessment score (0-100)",
        &["title", "grade"]
    )
    .expect("Failed to create sdlc_assessment_score");

    pub static ref SDLC_ASSESSMENT_CATEGORY_SCORE: GaugeVec = register_gauge_vec!(
        "sdlc_assessment_category_score",
        "SDLC assessment score per category (0-100)",
        &["title", "category"]
    )
    .expect("Failed to create sdlc_assessment_category_score");

    pub static ref SDLC_ASSESSMENTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "sdlc_assessments_total",
        "Total number of SDLC assessments completed",
        &["assessor"]
    )
    .expect("Failed to create sdlc_assessments_total");

    pub static ref SDLC_GATES_EVALUATED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "sdlc_gates_evaluated_total",
        "Total number of quality gate evaluations",
        &["gate_name", "result"]
    )
    .expect("Failed to create sdlc_gates_evaluated_total");
}

/// Export all current SDLC metrics to Prometheus.
///
/// Call this periodically (e.g. every 60 seconds) or on-demand before
/// a Prometheus scrape. It reads from the SDLC store and updates all
/// registered Prometheus gauges/counters.
pub fn export_sdlc_metrics(service: &SdlcService) -> Result<(), String> {
    // ── Export quality metrics ──
    let metrics = service.get_latest_metrics().map_err(|e| e.to_string())?;

    // Clear old metric label sets to avoid stale series
    SDLC_METRIC_VALUE.reset();
    SDLC_METRIC_TARGET.reset();
    SDLC_METRIC_PASSING.reset();

    for m in &metrics {
        let labels = &[m.name.as_str(), &m.category.to_string()];
        SDLC_METRIC_VALUE.with_label_values(labels).set(m.value);
        SDLC_METRIC_TARGET.with_label_values(labels).set(m.target);
        SDLC_METRIC_PASSING
            .with_label_values(labels)
            .set(if m.passing { 1.0 } else { 0.0 });
    }

    // ── Export DORA metrics ──
    let dashboard = service.build_dashboard().map_err(|e| e.to_string())?;
    let dora = &dashboard.dora_metrics;

    if let Some(freq) = dora.deployment_frequency {
        SDLC_DORA_DEPLOYMENT_FREQUENCY_F
            .with_label_values(&["per_week"])
            .set(freq);
    }
    if let Some(lead) = dora.lead_time_hours {
        SDLC_DORA_LEAD_TIME_HOURS
            .with_label_values(&["hours"])
            .set(lead);
    }
    if let Some(mttr) = dora.mttr_hours {
        SDLC_DORA_MTTR_HOURS
            .with_label_values(&["hours"])
            .set(mttr);
    }
    if let Some(cfr) = dora.change_failure_rate {
        SDLC_DORA_CHANGE_FAILURE_RATE
            .with_label_values(&["percent"])
            .set(cfr);
    }

    let level_val = match dora.performance_level {
        DoraPerformanceLevel::Elite => 3,
        DoraPerformanceLevel::High => 2,
        DoraPerformanceLevel::Medium => 1,
        DoraPerformanceLevel::Low => 0,
    };
    SDLC_DORA_PERFORMANCE_LEVEL.set(level_val as f64);

    // ── Export quality gates ──
    SDLC_GATE_STATUS.reset();

    let mut blocking_count = 0i64;
    for gate in &dashboard.gates {
        let status_val = match gate.status {
            GateStatus::Passed => 1.0,
            GateStatus::Failed => 0.0,
            GateStatus::Pending => -1.0,
            GateStatus::Waived => -2.0,
        };
        let enforcement = match gate.enforcement {
            GateEnforcement::Blocking => "blocking",
            GateEnforcement::Advisory => "advisory",
        };
        let labels = &[gate.name.as_str(), &gate.phase.to_string(), enforcement];
        SDLC_GATE_STATUS.with_label_values(labels).set(status_val);

        if gate.is_blocking() {
            blocking_count += 1;
        }
    }
    SDLC_BLOCKING_GATES.set(blocking_count);

    // ── Export assessments ──
    if let Some(assessment) = &dashboard.latest_assessment {
        let grade = assessment.grade().to_string();
        SDLC_ASSESSMENT_SCORE
            .with_label_values(&[assessment.title.as_str(), &grade])
            .set(assessment.overall_score);

        SDLC_ASSESSMENT_CATEGORY_SCORE.reset();
        for (category, score) in &assessment.scores {
            SDLC_ASSESSMENT_CATEGORY_SCORE
                .with_label_values(&[assessment.title.as_str(), &category.to_string()])
                .set(*score);
        }
    }

    Ok(())
}

/// Record that a gate was evaluated (call after each evaluation).
pub fn record_gate_evaluation(gate_name: &str, status: &GateStatus) {
    let result = match status {
        GateStatus::Passed => "passed",
        GateStatus::Failed => "failed",
        GateStatus::Pending => "pending",
        GateStatus::Waived => "waived",
    };
    SDLC_GATES_EVALUATED_TOTAL
        .with_label_values(&[gate_name, result])
        .inc();
}

/// Record that an assessment was completed.
pub fn record_assessment(assessor: &str) {
    SDLC_ASSESSMENTS_TOTAL
        .with_label_values(&[assessor])
        .inc();
}

/// Encode all registered Prometheus metrics (LFS + SDLC) as text.
/// Use this as the handler for the `/metrics` HTTP endpoint.
pub fn encode_all_metrics() -> Result<String, Box<dyn std::error::Error>> {
    crate::lfs::metrics::encode_metrics()
}

// Helper for SdlcPhase Display
impl std::fmt::Display for SdlcPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SdlcPhase::Planning => "planning",
            SdlcPhase::Design => "design",
            SdlcPhase::Implementation => "implementation",
            SdlcPhase::Testing => "testing",
            SdlcPhase::Deployment => "deployment",
            SdlcPhase::Operations => "operations",
            SdlcPhase::Maintenance => "maintenance",
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod tests {
    use crate::sdlc::service::InMemorySdlcStore;
    use std::sync::Arc;
    use super::*;

    fn make_service() -> SdlcService {
        SdlcService::new(Arc::new(InMemorySdlcStore::new()))
    }

    #[test]
    fn test_export_empty_service() {
        let svc = make_service();
        let result = export_sdlc_metrics(&svc);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_with_metrics() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "test_coverage",
            MetricCategory::TestCoverage,
            87.5,
            80.0,
            "%",
        ))
        .unwrap();
        svc.record_metric(QualityMetric::new(
            "build_success_rate",
            MetricCategory::BuildHealth,
            95.0,
            90.0,
            "%",
        ))
        .unwrap();

        export_sdlc_metrics(&svc).unwrap();

        // Verify the gauges were set
        let val = SDLC_METRIC_VALUE
            .with_label_values(&["test_coverage", "test_coverage"])
            .get();
        assert!((val - 87.5).abs() < 0.01);

        let target = SDLC_METRIC_TARGET
            .with_label_values(&["test_coverage", "test_coverage"])
            .get();
        assert!((target - 80.0).abs() < 0.01);

        let passing = SDLC_METRIC_PASSING
            .with_label_values(&["test_coverage", "test_coverage"])
            .get();
        assert_eq!(passing, 1.0);

        let failing = SDLC_METRIC_PASSING
            .with_label_values(&["build_success_rate", "build_health"])
            .get();
        assert_eq!(failing, 1.0); // 95 >= 90
    }

    #[test]
    fn test_export_with_dora_metrics() {
        let svc = make_service();
        let mut dora = DoraMetrics {
            deployment_frequency: Some(5.0),
            lead_time_hours: Some(12.0),
            mttr_hours: Some(2.0),
            change_failure_rate: Some(3.0),
            ..Default::default()
        };
        dora.classify();
        svc.record_dora_metrics(&dora).unwrap();

        export_sdlc_metrics(&svc).unwrap();

        let freq = SDLC_DORA_DEPLOYMENT_FREQUENCY_F
            .with_label_values(&["per_week"])
            .get();
        assert!((freq - 5.0).abs() < 0.01);

        let level = SDLC_DORA_PERFORMANCE_LEVEL.get();
        assert!(level >= 1.0); // At least Medium with these values
    }

    #[test]
    fn test_export_with_gates() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            85.0,
            80.0,
            "%",
        ))
        .unwrap();

        let gate = QualityGate::new("merge-gate", SdlcPhase::Implementation)
            .with_condition(GateCondition::new(
                "cov",
                ComparisonOperator::GreaterEqual,
                80.0,
                "Coverage >= 80%",
            ));
        svc.create_gate(gate).unwrap();
        svc.evaluate_all_gates().unwrap();

        export_sdlc_metrics(&svc).unwrap();

        let status = SDLC_GATE_STATUS
            .with_label_values(&["merge-gate", "implementation", "advisory"])
            .get();
        assert_eq!(status, 1.0); // Passed
    }

    #[test]
    fn test_export_blocking_gate() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            40.0,
            80.0,
            "%",
        ))
        .unwrap();

        let gate = QualityGate::new("release-gate", SdlcPhase::Deployment)
            .with_condition(GateCondition::new(
                "cov",
                ComparisonOperator::GreaterEqual,
                80.0,
                "cov >= 80%",
            ))
            .enforced();
        svc.create_gate(gate).unwrap();
        svc.evaluate_all_gates().unwrap();

        export_sdlc_metrics(&svc).unwrap();

        assert_eq!(SDLC_BLOCKING_GATES.get(), 1);
    }

    #[test]
    fn test_export_with_assessment() {
        let svc = make_service();
        let assessment = SdlcAssessment::new("Q1 2026", AssessmentPeriod::q(2026, 1), "bot")
            .with_score(MetricCategory::TestCoverage, 92.0)
            .with_score(MetricCategory::SecurityPosture, 78.0);
        svc.create_assessment(assessment).unwrap();

        export_sdlc_metrics(&svc).unwrap();

        let score = SDLC_ASSESSMENT_SCORE
            .with_label_values(&["Q1 2026", "B"])
            .get();
        assert!((score - 85.0).abs() < 0.1);

        let cat_score = SDLC_ASSESSMENT_CATEGORY_SCORE
            .with_label_values(&["Q1 2026", "test_coverage"])
            .get();
        assert!((cat_score - 92.0).abs() < 0.01);
    }

    #[test]
    fn test_record_gate_evaluation() {
        record_gate_evaluation("test-gate", &GateStatus::Passed);
        record_gate_evaluation("test-gate", &GateStatus::Failed);

        // Just verify it doesn't panic and the counter exists
        let encoded = crate::lfs::metrics::encode_metrics().unwrap();
        assert!(encoded.contains("sdlc_gates_evaluated_total"));
    }

    #[test]
    fn test_record_assessment_counter() {
        record_assessment("ci-bot");
        record_assessment("ci-bot");

        let encoded = crate::lfs::metrics::encode_metrics().unwrap();
        assert!(encoded.contains("sdlc_assessments_total"));
    }

    #[test]
    fn test_encode_all_metrics_includes_sdlc() {
        let svc = make_service();
        svc.record_metric(QualityMetric::new(
            "cov",
            MetricCategory::TestCoverage,
            90.0,
            80.0,
            "%",
        ))
        .unwrap();
        export_sdlc_metrics(&svc).unwrap();

        let encoded = encode_all_metrics().unwrap();
        assert!(encoded.contains("sdlc_metric_value"));
        assert!(encoded.contains("sdlc_dora_performance_level"));
    }
}

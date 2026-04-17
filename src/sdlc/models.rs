//! SDLC Monitoring data models — quality metrics, gates, assessments, and dashboard types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Quality Metrics ──

/// A single quality metric measurement at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetric {
    pub id: Uuid,
    pub name: String,
    pub category: MetricCategory,
    /// Current measured value.
    pub value: f64,
    /// Unit of measurement (percent, count, seconds, etc.).
    pub unit: String,
    /// Target threshold (passing value).
    pub target: f64,
    /// Whether this metric is currently passing its target.
    pub passing: bool,
    /// Timestamp of measurement.
    pub measured_at: DateTime<Utc>,
    /// Tags for filtering (repository, team, phase, etc.).
    pub tags: HashMap<String, String>,
}

impl QualityMetric {
    pub fn new(
        name: impl Into<String>,
        category: MetricCategory,
        value: f64,
        target: f64,
        unit: impl Into<String>,
    ) -> Self {
        let passing = value >= target;
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            category,
            value,
            unit: unit.into(),
            target,
            passing,
            measured_at: Utc::now(),
            tags: HashMap::new(),
        }
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

/// Category of SDLC quality metric.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MetricCategory {
    /// Code coverage percentage.
    TestCoverage,
    /// Build success rate.
    BuildHealth,
    /// Mean time to detect issues.
    MTTD,
    /// Mean time to resolve issues.
    MTTR,
    /// Deployment frequency.
    DeploymentFrequency,
    /// Change failure rate.
    ChangeFailureRate,
    /// Code review turnaround time.
    ReviewTurnaround,
    /// Security vulnerability count.
    SecurityPosture,
    /// Technical debt ratio.
    TechnicalDebt,
    /// Documentation coverage.
    DocumentationCoverage,
    /// Custom metric.
    Custom,
}

impl std::fmt::Display for MetricCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MetricCategory::TestCoverage => "test_coverage",
            MetricCategory::BuildHealth => "build_health",
            MetricCategory::MTTD => "mttd",
            MetricCategory::MTTR => "mttr",
            MetricCategory::DeploymentFrequency => "deployment_frequency",
            MetricCategory::ChangeFailureRate => "change_failure_rate",
            MetricCategory::ReviewTurnaround => "review_turnaround",
            MetricCategory::SecurityPosture => "security_posture",
            MetricCategory::TechnicalDebt => "technical_debt",
            MetricCategory::DocumentationCoverage => "documentation_coverage",
            MetricCategory::Custom => "custom",
        };
        write!(f, "{}", s)
    }
}

// ── Quality Gates ──

/// A quality gate — a set of conditions that must pass before progression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGate {
    pub id: Uuid,
    pub name: String,
    /// The SDLC phase this gate applies to.
    pub phase: SdlcPhase,
    /// Conditions that must all pass.
    pub conditions: Vec<GateCondition>,
    /// Whether the gate is currently passing.
    pub status: GateStatus,
    /// Enforcement mode.
    pub enforcement: GateEnforcement,
    pub created_at: DateTime<Utc>,
    pub last_evaluated_at: Option<DateTime<Utc>>,
}

impl QualityGate {
    pub fn new(name: impl Into<String>, phase: SdlcPhase) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            phase,
            conditions: Vec::new(),
            status: GateStatus::Pending,
            enforcement: GateEnforcement::Advisory,
            created_at: Utc::now(),
            last_evaluated_at: None,
        }
    }

    pub fn with_condition(mut self, condition: GateCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn enforced(mut self) -> Self {
        self.enforcement = GateEnforcement::Blocking;
        self
    }

    /// Evaluate the gate against current metric values.
    pub fn evaluate(&mut self, metrics: &[QualityMetric]) -> GateStatus {
        let mut all_pass = true;
        for condition in &mut self.conditions {
            let matching = metrics.iter().find(|m| m.name == condition.metric_name);
            condition.evaluate(matching);
            if !condition.passing {
                all_pass = false;
            }
        }
        self.status = if all_pass {
            GateStatus::Passed
        } else {
            GateStatus::Failed
        };
        self.last_evaluated_at = Some(Utc::now());
        self.status.clone()
    }

    /// Check if any blocking conditions failed.
    pub fn is_blocking(&self) -> bool {
        self.enforcement == GateEnforcement::Blocking && self.status == GateStatus::Failed
    }
}

/// A single condition within a quality gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCondition {
    /// Name of the metric this condition checks.
    pub metric_name: String,
    /// Comparison operator.
    pub operator: ComparisonOperator,
    /// Threshold value.
    pub threshold: f64,
    /// Whether the condition is currently passing.
    pub passing: bool,
    /// Human-readable description.
    pub description: String,
}

impl GateCondition {
    pub fn new(
        metric_name: impl Into<String>,
        operator: ComparisonOperator,
        threshold: f64,
        description: impl Into<String>,
    ) -> Self {
        Self {
            metric_name: metric_name.into(),
            operator,
            threshold,
            passing: false,
            description: description.into(),
        }
    }

    pub fn evaluate(&mut self, metric: Option<&QualityMetric>) {
        self.passing = match metric {
            Some(m) => match self.operator {
                ComparisonOperator::GreaterEqual => m.value >= self.threshold,
                ComparisonOperator::GreaterThan => m.value > self.threshold,
                ComparisonOperator::LessEqual => m.value <= self.threshold,
                ComparisonOperator::LessThan => m.value < self.threshold,
                ComparisonOperator::Equal => (m.value - self.threshold).abs() < f64::EPSILON,
            },
            None => false,
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    GreaterEqual,
    GreaterThan,
    LessEqual,
    LessThan,
    Equal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Pending,
    Passed,
    Failed,
    Waived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateEnforcement {
    /// Gate failure blocks progression.
    Blocking,
    /// Gate failure is advisory only.
    Advisory,
}

/// SDLC phases that quality gates can apply to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SdlcPhase {
    Planning,
    Design,
    Implementation,
    Testing,
    Deployment,
    Operations,
    Maintenance,
}

// ── Assessments ──

/// A quarterly or periodic SDLC assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdlcAssessment {
    pub id: Uuid,
    pub title: String,
    pub period: AssessmentPeriod,
    /// Scores per category (0-100).
    pub scores: HashMap<MetricCategory, f64>,
    /// Overall score.
    pub overall_score: f64,
    /// Findings and recommendations.
    pub findings: Vec<AssessmentFinding>,
    pub assessed_at: DateTime<Utc>,
    pub assessor: String,
}

impl SdlcAssessment {
    pub fn new(
        title: impl Into<String>,
        period: AssessmentPeriod,
        assessor: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            period,
            scores: HashMap::new(),
            overall_score: 0.0,
            findings: Vec::new(),
            assessed_at: Utc::now(),
            assessor: assessor.into(),
        }
    }

    pub fn with_score(mut self, category: MetricCategory, score: f64) -> Self {
        self.scores.insert(category, score.clamp(0.0, 100.0));
        self.recalculate_overall();
        self
    }

    pub fn with_finding(mut self, finding: AssessmentFinding) -> Self {
        self.findings.push(finding);
        self
    }

    fn recalculate_overall(&mut self) {
        if self.scores.is_empty() {
            self.overall_score = 0.0;
        } else {
            self.overall_score = self.scores.values().sum::<f64>() / self.scores.len() as f64;
        }
    }

    /// Letter grade based on overall score.
    pub fn grade(&self) -> char {
        match self.overall_score {
            s if s >= 90.0 => 'A',
            s if s >= 80.0 => 'B',
            s if s >= 70.0 => 'C',
            s if s >= 60.0 => 'D',
            _ => 'F',
        }
    }
}

/// Assessment period — quarter or custom range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentPeriod {
    pub year: i32,
    pub quarter: u32,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl AssessmentPeriod {
    pub fn q(year: i32, quarter: u32) -> Self {
        use chrono::TimeZone;
        let start_month = (quarter - 1) * 3 + 1;
        let start = chrono::Utc
            .with_ymd_and_hms(year, start_month, 1, 0, 0, 0)
            .single()
            .unwrap_or_default();
        let end_month = start_month + 2;
        // Approximate end
        let end = chrono::Utc
            .with_ymd_and_hms(year, end_month, 28, 23, 59, 59)
            .single()
            .unwrap_or_default();
        Self {
            year,
            quarter,
            start,
            end,
        }
    }
}

/// A finding from an SDLC assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentFinding {
    pub category: MetricCategory,
    pub severity: FindingSeverity,
    pub title: String,
    pub description: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

// ── Dashboard ──

/// Real-time SDLC dashboard data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdlcDashboard {
    /// Summary metrics by category.
    pub metrics: Vec<QualityMetric>,
    /// Quality gate statuses.
    pub gates: Vec<QualityGate>,
    /// Latest assessment (if any).
    pub latest_assessment: Option<SdlcAssessment>,
    /// DORA metrics summary.
    pub dora_metrics: DoraMetrics,
    /// Generated at timestamp.
    pub generated_at: DateTime<Utc>,
}

/// DORA (DevOps Research and Assessment) metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DoraMetrics {
    /// Deployment frequency (per week).
    pub deployment_frequency: Option<f64>,
    /// Lead time for changes (hours).
    pub lead_time_hours: Option<f64>,
    /// Mean time to restore (hours).
    pub mttr_hours: Option<f64>,
    /// Change failure rate (percentage).
    pub change_failure_rate: Option<f64>,
    /// Overall DORA performance level.
    pub performance_level: DoraPerformanceLevel,
}

impl DoraMetrics {
    /// Classify DORA performance based on the four key metrics.
    pub fn classify(&mut self) {
        let freq = self.deployment_frequency.unwrap_or(0.0);
        let lead = self.lead_time_hours.unwrap_or(f64::MAX);
        let mttr = self.mttr_hours.unwrap_or(f64::MAX);
        let cfr = self.change_failure_rate.unwrap_or(100.0);

        self.performance_level = if freq >= 7.0 && lead <= 24.0 && mttr <= 1.0 && cfr <= 5.0 {
            DoraPerformanceLevel::Elite
        } else if freq >= 1.0 && lead <= 168.0 && mttr <= 24.0 && cfr <= 10.0 {
            DoraPerformanceLevel::High
        } else if freq >= 0.25 && lead <= 720.0 && mttr <= 168.0 && cfr <= 15.0 {
            DoraPerformanceLevel::Medium
        } else {
            DoraPerformanceLevel::Low
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoraPerformanceLevel {
    Elite,
    High,
    #[default]
    Medium,
    Low,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_metric_passing() {
        let m = QualityMetric::new(
            "test_coverage",
            MetricCategory::TestCoverage,
            85.0,
            80.0,
            "%",
        );
        assert!(m.passing);
        assert_eq!(m.value, 85.0);
    }

    #[test]
    fn test_quality_metric_failing() {
        let m = QualityMetric::new(
            "test_coverage",
            MetricCategory::TestCoverage,
            65.0,
            80.0,
            "%",
        );
        assert!(!m.passing);
    }

    #[test]
    fn test_quality_gate_evaluate_pass() {
        let mut gate = QualityGate::new("merge gate", SdlcPhase::Implementation).with_condition(
            GateCondition::new(
                "test_coverage",
                ComparisonOperator::GreaterEqual,
                80.0,
                "Test coverage >= 80%",
            ),
        );

        let metrics = vec![QualityMetric::new(
            "test_coverage",
            MetricCategory::TestCoverage,
            85.0,
            80.0,
            "%",
        )];
        let status = gate.evaluate(&metrics);
        assert_eq!(status, GateStatus::Passed);
        assert!(!gate.is_blocking());
    }

    #[test]
    fn test_quality_gate_evaluate_fail() {
        let mut gate = QualityGate::new("release gate", SdlcPhase::Deployment)
            .with_condition(GateCondition::new(
                "test_coverage",
                ComparisonOperator::GreaterEqual,
                80.0,
                "Coverage >= 80%",
            ))
            .enforced();

        let metrics = vec![QualityMetric::new(
            "test_coverage",
            MetricCategory::TestCoverage,
            45.0,
            80.0,
            "%",
        )];
        let status = gate.evaluate(&metrics);
        assert_eq!(status, GateStatus::Failed);
        assert!(gate.is_blocking());
    }

    #[test]
    fn test_quality_gate_missing_metric_fails() {
        let mut gate =
            QualityGate::new("gate", SdlcPhase::Testing).with_condition(GateCondition::new(
                "nonexistent",
                ComparisonOperator::GreaterEqual,
                50.0,
                "Missing metric",
            ));
        gate.evaluate(&[]);
        assert_eq!(gate.status, GateStatus::Failed);
    }

    #[test]
    fn test_assessment_scores() {
        let assessment = SdlcAssessment::new("Q1 2026", AssessmentPeriod::q(2026, 1), "sdlc-bot")
            .with_score(MetricCategory::TestCoverage, 92.0)
            .with_score(MetricCategory::SecurityPosture, 78.0)
            .with_score(MetricCategory::BuildHealth, 95.0);

        assert!((assessment.overall_score - 88.333).abs() < 0.1);
        assert_eq!(assessment.grade(), 'B');
    }

    #[test]
    fn test_assessment_grade_boundaries() {
        let a = SdlcAssessment::new("test", AssessmentPeriod::q(2026, 1), "bot")
            .with_score(MetricCategory::TestCoverage, 95.0);
        assert_eq!(a.grade(), 'A');

        let b = SdlcAssessment::new("test", AssessmentPeriod::q(2026, 1), "bot")
            .with_score(MetricCategory::TestCoverage, 55.0);
        assert_eq!(b.grade(), 'F');
    }

    #[test]
    fn test_dora_metrics_elite() {
        let mut dora = DoraMetrics {
            deployment_frequency: Some(10.0),
            lead_time_hours: Some(4.0),
            mttr_hours: Some(0.5),
            change_failure_rate: Some(2.0),
            ..Default::default()
        };
        dora.classify();
        assert_eq!(dora.performance_level, DoraPerformanceLevel::Elite);
    }

    #[test]
    fn test_dora_metrics_low() {
        let mut dora = DoraMetrics {
            deployment_frequency: Some(0.05),
            lead_time_hours: Some(2000.0),
            mttr_hours: Some(500.0),
            change_failure_rate: Some(45.0),
            ..Default::default()
        };
        dora.classify();
        assert_eq!(dora.performance_level, DoraPerformanceLevel::Low);
    }

    #[test]
    fn test_dora_metrics_high() {
        let mut dora = DoraMetrics {
            deployment_frequency: Some(2.0),
            lead_time_hours: Some(72.0),
            mttr_hours: Some(12.0),
            change_failure_rate: Some(8.0),
            ..Default::default()
        };
        dora.classify();
        assert_eq!(dora.performance_level, DoraPerformanceLevel::High);
    }

    #[test]
    fn test_metric_category_display() {
        assert_eq!(MetricCategory::TestCoverage.to_string(), "test_coverage");
        assert_eq!(MetricCategory::MTTR.to_string(), "mttr");
    }

    #[test]
    fn test_comparison_operators() {
        let mut c = GateCondition::new("x", ComparisonOperator::LessThan, 10.0, "test");
        c.evaluate(Some(&QualityMetric::new(
            "x",
            MetricCategory::Custom,
            5.0,
            10.0,
            "ms",
        )));
        assert!(c.passing);

        let mut c = GateCondition::new("x", ComparisonOperator::GreaterThan, 10.0, "test");
        c.evaluate(Some(&QualityMetric::new(
            "x",
            MetricCategory::Custom,
            15.0,
            10.0,
            "ms",
        )));
        assert!(c.passing);
    }
}

# Continuous SDLC Improvement Plan

**Document owner:** Platform Engineering  
**Review cadence:** Quarterly  
**Last reviewed:** 2026-04-12

---

## 1. Purpose

This document defines the continuous improvement processes for agentic-repos's software development lifecycle. It leverages the SDLC monitoring framework (module `sdlc/`) and DORA metrics to drive measurable, quarterly improvements.

---

## 2. Quarterly Assessment Process

### Schedule

| Quarter | Assessment Window | Report Due |
|---|---|---|
| Q1 | Jan 1 – Mar 31 | Apr 15 |
| Q2 | Apr 1 – Jun 30 | Jul 15 |
| Q3 | Jul 1 – Sep 30 | Oct 15 |
| Q4 | Oct 1 – Dec 31 | Jan 15 |

### Process

1. **Automated metric collection** — the `SdlcService::build_dashboard()` endpoint aggregates current metrics, gate statuses, DORA classification, and latest assessment into a single `SdlcDashboard` struct.
2. **Manual assessment** — a named assessor creates an `SdlcAssessment` with per-category scores (0-100) and findings via `POST /sdlc/assessments`.
3. **Grade calculation** — `SdlcAssessment::grade()` returns A-F based on average score.
4. **Finding triage** — findings with severity High or Critical become improvement tasks for the next quarter.
5. **Executive report** — results published to `docs/` and communicated to stakeholders.

### Scoring Criteria (per category)

| Category | Measurement Method | Target (Level 4) |
|---|---|---|
| TestCoverage | `cargo test --lib` pass rate + line coverage | ≥85% |
| BuildHealth | CI success rate over quarter | ≥95% |
| MTTD | Mean time from defect introduction to detection | ≤4h |
| MTTR | Mean time from detection to resolution | ≤24h |
| DeploymentFrequency | Production deployments per week | ≥3 |
| ChangeFailureRate | % of deployments causing incidents | ≤5% |
| ReviewTurnaround | Median time from PR open to merge | ≤4h |
| SecurityPosture | Open critical/high vulnerabilities | 0 |
| TechnicalDebt | Clippy warnings + TODO count | ≤10 |
| DocumentationCoverage | Public API items with doc comments | ≥90% |

---

## 3. Quality Gates

### Standard Gates

Each SDLC phase has a default quality gate enforced by the monitoring framework:

| Phase | Gate | Blocking? | Conditions |
|---|---|---|---|
| Implementation | `merge-gate` | Yes | test_coverage ≥ 80%, clippy clean |
| Testing | `release-candidate-gate` | Yes | all tests pass, no security criticals |
| Deployment | `deploy-gate` | Yes | release-candidate-gate passed, change_failure_rate ≤ 10% |
| Operations | `health-gate` | Advisory | MTTR ≤ 24h, deployment_frequency ≥ 1/week |

### Gate Evaluation

Gates are evaluated via `POST /sdlc/gates/{id}/evaluate` which checks current metrics against gate conditions. The `SdlcService::any_blocking_gates_failed()` method returns whether deployment should be blocked.

---

## 4. DORA Metrics Monitoring

### Target: Elite Performance by Q4 2026

| Metric | Current (Q1 2026) | Q2 Target | Q3 Target | Q4 Target |
|---|---|---|---|---|
| Deployment frequency | N/A (new) | ≥1/week | ≥3/week | ≥7/week |
| Lead time for changes | N/A | ≤168h | ≤72h | ≤24h |
| Mean time to restore | N/A | ≤48h | ≤24h | ≤1h |
| Change failure rate | N/A | ≤15% | ≤10% | ≤5% |

### Collection

DORA metrics are recorded via `SdlcService::record_dora_metrics()` which stores them as `QualityMetric` entries. The dashboard auto-classifies performance level (Elite/High/Medium/Low).

---

## 5. Automated Compliance Checks

### GDPR (EU Primary)

| Check | Frequency | Automation |
|---|---|---|
| RoPA completeness | Quarterly | `EuComplianceService::has_processing_records()` |
| DPIA coverage for high-risk processing | Quarterly | Manual review of `DataProtectionImpactAssessment` records |
| DPO appointment | Annual | `EuComplianceService::has_dpo()` |
| Cross-border transfer safeguard validity | Quarterly | Review `ThirdCountryTransfer.safeguard` records |
| GDPR readiness score | Quarterly | `EuComplianceService::gdpr_readiness_check()` |

### HIPAA (US Secondary)

| Check | Frequency | Automation |
|---|---|---|
| BAA validity | Quarterly | `HipaaService::all_baas_valid()` |
| BAA expiry warnings | Monthly | `HipaaService::expiring_baas(30)` |
| Security training completion | Quarterly | Review in `HipaaService::run_compliance_check()` |

---

## 6. Improvement Backlog

Items are prioritised from quarterly assessment findings:

### Pre-Production (Before Q2 2026)

- [x] Clippy cleanup — all warnings resolved (8877116)
- [ ] Integration testing — end-to-end SSH clone/push verification
- [ ] Load testing — validate DORA Elite targets under production load

### Q2 2026

- [ ] SQLite/PostgreSQL stores — replace InMemoryStore implementations
- [ ] adp-web-ui — Svelte 5 frontend, rust-embed integration
- [ ] Prometheus metrics export — wire SDLC metrics to existing prometheus dependency
- [ ] Integration test suite — test full SSH git protocol lifecycle

### Q3 2026

- [ ] DORA compliance — ICT risk management framework
- [ ] NIS2 compliance — incident notification framework
- [ ] Multi-tenant isolation verification — security audit
- [ ] Performance regression testing — automated benchmarks

### Q4 2026

- [ ] Target DORA Elite classification
- [ ] Full EU/US compliance certification readiness
- [ ] Production deployment with monitoring

---

## 7. Monitoring & Alerting

The SDLC monitoring framework exposes a dashboard at `GET /sdlc/dashboard` with:
- Current metric values and targets
- Quality gate statuses (pass/fail)
- DORA performance level
- Latest assessment grade

Alert thresholds (configurable via `AlertThresholds`):
- Replication latency > 5000ms
- Pending replication objects > 10,000
- Replication staleness > 3600s
- Any failed replication targets
- Any unresolved conflicts

---

## 8. Review & Amendment

This plan is reviewed quarterly alongside the assessment. Amendments require:
1. Assessment finding or incident triggering the change
2. Approval from DPO (for compliance changes) or platform lead (for process changes)
3. Update to this document with effective date

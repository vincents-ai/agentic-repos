//! Metrics constants and labels for the integration crates.
//!
//! Defines the metric names, labels, and helper types used across all crates.
//! The actual metrics backend (Prometheus, OpenTelemetry) is configured at the
//! adp binary level — crates only emit via the `tracing` spans and these
//! constants define the contract.

/// Metric: number of active SSE connections.
pub const SSE_CONNECTIONS_ACTIVE: &str = "adp_sse_connections_active";

/// Metric: total SSE connection attempts.
pub const SSE_CONNECTION_ATTEMPTS_TOTAL: &str = "adp_sse_connection_attempts_total";

/// Metric: total SSE disconnections.
pub const SSE_DISCONNECTIONS_TOTAL: &str = "adp_sse_disconnections_total";

/// Metric: SSE events sent.
pub const SSE_EVENTS_SENT_TOTAL: &str = "adp_sse_events_sent_total";

/// Metric: TTL expiry events.
pub const SSE_TTL_EXPIRATIONS_TOTAL: &str = "adp_sse_ttl_expirations_total";

/// Metric: RBAC write checks.
pub const RBAC_WRITE_CHECKS_TOTAL: &str = "adp_rbac_write_checks_total";

/// Metric: RBAC write denials.
pub const RBAC_WRITE_DENIALS_TOTAL: &str = "adp_rbac_write_denials_total";

/// Metric: RBAC read checks.
pub const RBAC_READ_CHECKS_TOTAL: &str = "adp_rbac_read_checks_total";

/// Metric: schema validations.
pub const SCHEMA_VALIDATIONS_TOTAL: &str = "adp_schema_validations_total";

/// Metric: schema validation failures.
pub const SCHEMA_VALIDATION_FAILURES_TOTAL: &str = "adp_schema_validation_failures_total";

/// Metric: entity writes.
pub const ENTITY_WRITES_TOTAL: &str = "adp_entity_writes_total";

/// Metric: entity write validation failures.
pub const ENTITY_WRITE_FAILURES_TOTAL: &str = "adp_entity_write_failures_total";

/// Metric: registration attempts.
pub const REGISTRATION_ATTEMPTS_TOTAL: &str = "adp_registration_attempts_total";

/// Metric: registration renewals.
pub const REGISTRATION_RENEWALS_TOTAL: &str = "adp_registration_renewals_total";

/// Metric: key pruning cycles.
pub const KEY_PRUNE_CYCLES_TOTAL: &str = "adp_key_prune_cycles_total";

/// Metric: keys pruned.
pub const KEYS_PRUNED_TOTAL: &str = "adp_keys_pruned_total";

/// Service type label value.
pub fn service_type_label(st: crate::rbac::ServiceType) -> &'static str {
    match st {
        crate::rbac::ServiceType::AgenticLoop => "agentic_loop",
        crate::rbac::ServiceType::CiRunner => "ci_runner",
        crate::rbac::ServiceType::Integration => "integration",
        crate::rbac::ServiceType::User => "user",
    }
}

/// Access decision label value.
pub fn decision_label(allowed: bool) -> &'static str {
    if allowed { "allowed" } else { "denied" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::ServiceType;

    #[test]
    fn service_type_labels() {
        assert_eq!(service_type_label(ServiceType::AgenticLoop), "agentic_loop");
        assert_eq!(service_type_label(ServiceType::CiRunner), "ci_runner");
        assert_eq!(service_type_label(ServiceType::Integration), "integration");
        assert_eq!(service_type_label(ServiceType::User), "user");
    }

    #[test]
    fn decision_labels() {
        assert_eq!(decision_label(true), "allowed");
        assert_eq!(decision_label(false), "denied");
    }
}

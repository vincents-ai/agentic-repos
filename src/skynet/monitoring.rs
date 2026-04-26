//! Monitoring and metrics collection for Skynet v2 plugin

use parking_lot::RwLock;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

/// Metrics collector
pub struct MetricsCollector {
    request_counts: Arc<RwLock<HashMap<String, u64>>>,
    error_counts: Arc<RwLock<HashMap<String, u64>>>,
    success_counts: Arc<RwLock<HashMap<String, u64>>>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            request_counts: Arc::new(RwLock::new(HashMap::new())),
            error_counts: Arc::new(RwLock::new(HashMap::new())),
            success_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a request
    pub fn record_request(&self, method: &str) {
        let mut counts = self.request_counts.write();
        *counts.entry(method.to_string()).or_insert(0) += 1;
        debug!("Request recorded: method={}", method);
    }

    /// Record a success
    pub fn record_success(&self, method: &str) {
        let mut counts = self.success_counts.write();
        *counts.entry(method.to_string()).or_insert(0) += 1;
    }

    /// Record an error
    pub fn record_error(&self, method: &str, code: &str) {
        let mut counts = self.error_counts.write();
        let key = format!("{}:{}", method, code);
        *counts.entry(key).or_insert(0) += 1;
    }

    /// Get metrics
    pub fn get_metrics(&self) -> Value {
        let requests = self.request_counts.read().clone();
        let successes = self.success_counts.read().clone();
        let errors = self.error_counts.read().clone();

        json!({
            "requests": requests,
            "successes": successes,
            "errors": errors,
        })
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let m = MetricsCollector::new();
        let metrics = m.get_metrics();
        assert_eq!(metrics["requests"], json!({}));
        assert_eq!(metrics["successes"], json!({}));
        assert_eq!(metrics["errors"], json!({}));
    }

    #[test]
    fn record_request_increments() {
        let m = MetricsCollector::new();
        m.record_request("GET");
        m.record_request("GET");
        m.record_request("POST");
        let metrics = m.get_metrics();
        assert_eq!(metrics["requests"]["GET"], 2);
        assert_eq!(metrics["requests"]["POST"], 1);
    }

    #[test]
    fn record_success_increments() {
        let m = MetricsCollector::new();
        m.record_success("GET");
        m.record_success("GET");
        let metrics = m.get_metrics();
        assert_eq!(metrics["successes"]["GET"], 2);
    }

    #[test]
    fn record_error_tracks_method_and_code() {
        let m = MetricsCollector::new();
        m.record_error("GET", "404");
        m.record_error("GET", "500");
        m.record_error("POST", "422");
        let metrics = m.get_metrics();
        assert_eq!(metrics["errors"]["GET:404"], 1);
        assert_eq!(metrics["errors"]["GET:500"], 1);
        assert_eq!(metrics["errors"]["POST:422"], 1);
    }

    #[test]
    fn default_is_same_as_new() {
        let m = MetricsCollector::default();
        let metrics = m.get_metrics();
        assert_eq!(metrics["requests"], json!({}));
    }
}

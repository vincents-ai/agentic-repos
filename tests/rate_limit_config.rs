//! Tests for rate limit configuration types

use agentic_repos::skynet::rate_limit::config::RateLimitConfig;

#[test]
fn rate_limit_config_defaults() {
    let cfg = RateLimitConfig::default();
    assert!(cfg.enabled);
    assert_eq!(cfg.per_user.api_calls_per_minute, 1000);
    assert_eq!(cfg.per_user.concurrent_connections, 10);
    assert_eq!(cfg.per_user.data_transfer_mb_per_hour, 1000);
    assert_eq!(cfg.per_operation.clone_timeout_secs, 300);
    assert_eq!(cfg.per_operation.push_timeout_secs, 300);
    assert_eq!(cfg.global.max_concurrent_users, 1000);
    assert_eq!(cfg.global.max_data_transfer_mb_per_hour, 100000);
}

#[test]
fn rate_limit_config_serialization_roundtrip() {
    let cfg = RateLimitConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: RateLimitConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.enabled, cfg.enabled);
    assert_eq!(parsed.per_user.api_calls_per_minute, cfg.per_user.api_calls_per_minute);
    assert_eq!(parsed.global.max_concurrent_users, cfg.global.max_concurrent_users);
}

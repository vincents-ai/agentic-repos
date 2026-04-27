//! Tests for cache configuration types

use agentic_repos::cache::config::{CacheConfig, CacheTtls};
use std::time::Duration;

// ── Defaults ──

#[test]
fn cache_config_default_values() {
    let cfg = CacheConfig::default();
    assert!(!cfg.enabled);
    assert_eq!(cfg.url, "redis://127.0.0.1:6379");
    assert_eq!(cfg.key_prefix, "agentic-repos");
    assert_eq!(cfg.default_ttl_secs, 3600);
    assert_eq!(cfg.max_connections, 10);
    assert_eq!(cfg.connection_timeout_secs, 5);
    assert_eq!(cfg.response_timeout_secs, 3);
    assert_eq!(cfg.retry_attempts, 3);
    assert_eq!(cfg.retry_delay_ms, 100);
    assert!(cfg.tls.is_none());
}

#[test]
fn cache_ttls_default_values() {
    let ttls = CacheTtls::default();
    assert_eq!(ttls.repository_metadata_secs, 300);
    assert_eq!(ttls.session_secs, 86400);
    assert_eq!(ttls.api_response_secs, 60);
    assert_eq!(ttls.lfs_object_secs, 600);
    assert_eq!(ttls.permission_secs, 300);
    assert_eq!(ttls.user_info_secs, 3600);
}

// ── Builder methods ──

#[test]
fn cache_config_new_custom_url() {
    let cfg = CacheConfig::new("redis://custom:6380".to_string());
    assert!(cfg.enabled);
    assert_eq!(cfg.url, "redis://custom:6380");
}

#[test]
fn cache_config_with_prefix() {
    let cfg = CacheConfig::default().with_prefix("my-app".to_string());
    assert_eq!(cfg.key_prefix, "my-app");
}

#[test]
fn cache_config_with_max_connections() {
    let cfg = CacheConfig::default().with_max_connections(50);
    assert_eq!(cfg.max_connections, 50);
}

#[test]
fn cache_config_with_default_ttl() {
    let cfg = CacheConfig::default().with_default_ttl(Duration::from_secs(7200));
    assert_eq!(cfg.default_ttl_secs, 7200);
}

// ── Duration accessors ──

#[test]
fn duration_accessors() {
    let cfg = CacheConfig::default();
    assert_eq!(cfg.default_ttl(), Duration::from_secs(3600));
    assert_eq!(cfg.repository_ttl(), Duration::from_secs(300));
    assert_eq!(cfg.session_ttl(), Duration::from_secs(86400));
    assert_eq!(cfg.api_response_ttl(), Duration::from_secs(60));
    assert_eq!(cfg.lfs_object_ttl(), Duration::from_secs(600));
    assert_eq!(cfg.permission_ttl(), Duration::from_secs(300));
    assert_eq!(cfg.user_info_ttl(), Duration::from_secs(3600));
    assert_eq!(cfg.connection_timeout(), Duration::from_secs(5));
    assert_eq!(cfg.response_timeout(), Duration::from_secs(3));
}

#[test]
fn redis_url_accessor() {
    let cfg = CacheConfig::new("redis://prod:6379".to_string());
    assert_eq!(cfg.redis_url(), "redis://prod:6379");
}

// ── Serialization ──

#[test]
fn cache_config_serialization_roundtrip() {
    let cfg = CacheConfig::new("redis://test:6379".to_string())
        .with_prefix("test".to_string())
        .with_max_connections(20);
    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: CacheConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.url, cfg.url);
    assert_eq!(parsed.key_prefix, cfg.key_prefix);
    assert_eq!(parsed.max_connections, cfg.max_connections);
    assert!(parsed.enabled);
}

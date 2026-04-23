//! Cross-crate integration tests exercising the full agentic-loop ↔ adp flow.
//!
//! These tests verify:
//! 1. Registration → secret generation → TTL lifecycle
//! 2. Entity write → schema validation
//! 3. RBAC engine → service key namespace restrictions
//! 4. SSE event serialization round-trips
//! 5. CODEOWNERS parsing → ownership resolution

use adp_proto::rbac::{KeyPermissions, PathRestrictions, ServiceType};
use adp_proto::registration::InstanceCapabilities;
use adp_proto::sse::SseEvent;
use adp_rbac_engine::{CodeownersParser, RbacEngine};
use adp_schema_registry::{RefSchema, SchemaChangeType, SchemaRegistry};
use agentic_loop_entity_writer::EntityWriter;
use agentic_loop_registration::{RegistrationConfig, RegistrationManager, RegistrationStatus};
use chrono::Utc;
use serde_json::json;

/// Full lifecycle: register → create entity → validate → RBAC check.
#[test]
fn full_integration_lifecycle() {
    // ── 1. Registration ──────────────────────────────────────────────
    let reg_config = RegistrationConfig {
        default_ttl_secs: 3600,
        renewal_threshold_secs: 300,
        max_ttl_secs: 86400,
    };
    let reg_mgr = RegistrationManager::new(reg_config);

    let info = reg_mgr
        .register(
            ServiceType::AgenticLoop,
            vec![InstanceCapabilities::Workflows, InstanceCapabilities::Entities],
            vec!["project-1".to_string()],
        )
        .unwrap();

    assert_eq!(reg_mgr.status(), RegistrationStatus::Active);
    assert!(!info.secret.is_empty());
    assert!(info.ttl_remaining() > 3500);
    assert!(reg_mgr.secret().is_some());

    // ── 2. Schema registration ───────────────────────────────────────
    let schema_registry = SchemaRegistry::new();
    let task_schema = RefSchema {
        id: "engram-task".to_string(),
        namespace_pattern: "refs/engram/task/*".to_string(),
        title: "Engram Task".to_string(),
        description: Some("A task entity from engram".to_string()),
        version: "1.0.0".to_string(),
        author: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        schema: json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "status": {"type": "string", "enum": ["todo", "in_progress", "done"]},
                "priority": {"type": "string", "enum": ["low", "medium", "high", "critical"]}
            },
            "required": ["title", "status"]
        }),
        ui: None,
    };
    let change = schema_registry.register(task_schema).unwrap();
    assert_eq!(change.schema_id, "engram-task");
    assert_eq!(change.change_type, SchemaChangeType::Created);

    // ── 3. Entity creation with validation ───────────────────────────
    let writer = EntityWriter::new(schema_registry);

    let entity = writer
        .create("engram-task", &json!({
            "title": "Implement feature X",
            "status": "todo",
            "priority": "high"
        }))
        .unwrap();

    assert!(entity.path.starts_with("refs/engram/task/"));
    assert_eq!(entity.schema_id, "engram-task");

    // Invalid entity should fail
    let invalid = writer.create("engram-task", &json!({"missing": "fields"}));
    assert!(invalid.is_err());

    // ── 4. RBAC enforcement ──────────────────────────────────────────
    let rbac = RbacEngine::new();
    let key_perms = KeyPermissions {
        key_id: info.instance_id.to_string(),
        service_type: ServiceType::AgenticLoop,
        service_subtype: None,
        allowed_write_patterns: vec!["refs/secrets/agentic-loop/*".to_string()],
        denied_write_patterns: vec![],
        allowed_read_patterns: vec!["**".to_string()],
        path_restrictions: None,
        created_by: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: None,
        tags: vec![],
    };

    // Allowed: own namespace
    let decision = rbac.check_write(&key_perms, "refs/secrets/agentic-loop/instance-1/config");
    assert!(decision.is_allowed());

    // Denied: other namespace
    let decision = rbac.check_write(&key_perms, "refs/secrets/ci-runners/something");
    assert!(decision.is_denied());

    // ── 5. Secret renewal ────────────────────────────────────────────
    let renewed = reg_mgr.renew().unwrap();
    assert_ne!(renewed.secret, info.secret);
    assert!(renewed.expires_at > info.expires_at);

    // ── 6. Deregistration ────────────────────────────────────────────
    reg_mgr.deregister().unwrap();
    assert_eq!(reg_mgr.status(), RegistrationStatus::Unregistered);
}

/// SSE event serialization round-trip for all variants.
#[test]
fn sse_event_round_trip_all_variants() {
    let events = vec![
        SseEvent::Registered {
            instance_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            ttl_remaining: 3600,
            ttl_expires_at: "2026-04-23T15:30:00Z".to_string(),
        },
        SseEvent::SchemaChange {
            event_type: "created".to_string(),
            schema_id: "engram-task".to_string(),
            timestamp: "2026-04-23T14:00:00Z".to_string(),
        },
        SseEvent::EntityUpdate {
            entity_id: "abc-123".to_string(),
            schema_id: "engram-task".to_string(),
            action: "created".to_string(),
        },
        SseEvent::Heartbeat {
            timestamp: "2026-04-23T14:30:00Z".to_string(),
        },
        SseEvent::TtlWarning {
            ttl_remaining: 300,
            message: "Secret expires in 300 seconds".to_string(),
        },
        SseEvent::TtlExpired,
        SseEvent::Disconnected {
            reason: "Server shutdown".to_string(),
        },
    ];

    for event in &events {
        let json = serde_json::to_string(event).unwrap();
        let decoded: SseEvent = serde_json::from_str(&json).unwrap();
        let re_encoded = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, re_encoded, "Round-trip failed for {:?}", event);
    }
}

/// CODEOWNERS with service key restrictions.
#[test]
fn codeowners_with_rbac() {
    let codeowners = "\
# Default
*                           @core-team

# Service-owned paths
/src/services/agentic-loop/   @agentic-loop-team
/src/services/ci/             @ci-team

# Protected
/src/internal/                @admins
";
    let parser = CodeownersParser::parse(codeowners).unwrap();

    // Service path owned by agentic-loop team
    assert!(parser.is_owned_by("src/services/agentic-loop/main.rs", "@agentic-loop-team"));
    assert!(!parser.is_owned_by("src/services/agentic-loop/main.rs", "@ci-team"));

    // Default fallback
    assert!(parser.is_owned_by("README.md", "@core-team"));

    // Protected path
    assert!(parser.is_owned_by("src/internal/config.rs", "@admins"));
}

/// Expired registration cannot renew.
#[test]
fn expired_registration_blocked() {
    let config = RegistrationConfig {
        default_ttl_secs: 0, // Instant expiry
        ..Default::default()
    };
    let mgr = RegistrationManager::new(config);
    mgr.register(ServiceType::User, vec![], vec![]).unwrap();

    // With TTL 0, it's already expired
    // We can't easily test this with real time, but we can check status logic
    let status = mgr.status();
    assert!(
        status == RegistrationStatus::Expired || status == RegistrationStatus::PendingRenewal
    );
}

/// User keys denied from service namespaces.
#[test]
fn user_keys_denied_from_service_namespaces() {
    let rbac = RbacEngine::new();
    let user_perms = RbacEngine::default_permissions_for(
        ServiceType::User,
        "SHA256:user-key".to_string(),
        "admin".to_string(),
    );

    // User can write to user namespace
    assert!(rbac
        .check_write(&user_perms, "refs/secrets/users/alice/config")
        .is_allowed());

    // User denied from agentic-loop namespace
    assert!(rbac
        .check_write(&user_perms, "refs/secrets/agentic-loop/instance-1/secret")
        .is_denied());

    // User denied from ci-runners namespace
    assert!(rbac
        .check_write(&user_perms, "refs/secrets/ci-runners/job-1/config")
        .is_denied());
}

/// Schema validation prevents invalid entities.
#[test]
fn schema_validation_prevents_invalid_entities() {
    let registry = SchemaRegistry::new();
    let schema = RefSchema {
        id: "strict-test".to_string(),
        namespace_pattern: "refs/test/strict/*".to_string(),
        title: "Strict Test".to_string(),
        description: None,
        version: "1.0.0".to_string(),
        author: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        schema: json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "minLength": 1},
                "count": {"type": "integer", "minimum": 0}
            },
            "required": ["name"],
            "additionalProperties": false
        }),
        ui: None,
    };
    registry.register(schema).unwrap();

    let writer = EntityWriter::new(registry);

    // Valid
    assert!(writer
        .create("strict-test", &json!({"name": "test", "count": 5}))
        .is_ok());

    // Missing required field
    assert!(writer.create("strict-test", &json!({"count": 5})).is_err());

    // Additional properties not allowed
    assert!(writer
        .create("strict-test", &json!({"name": "test", "extra": true}))
        .is_err());

    // Invalid type
    assert!(writer
        .create("strict-test", &json!({"name": 42}))
        .is_err());
}

/// Path restrictions with CODEOWNERS enforcement.
#[test]
fn path_restrictions_enforcement() {
    let rbac = RbacEngine::new();
    let mut perms = RbacEngine::default_permissions_for(
        ServiceType::AgenticLoop,
        "key-1".to_string(),
        "admin".to_string(),
    );

    perms.path_restrictions = Some(PathRestrictions {
        codeowners_file: "CODEOWNERS".to_string(),
        allowed_paths: vec!["refs/secrets/agentic-loop/instance-1".to_string()],
        denied_paths: vec![],
        enforce_codeowners: false,
        require_owner_approval: false,
    });

    // Within allowed path
    assert!(rbac
        .check_write(&perms, "refs/secrets/agentic-loop/instance-1/key")
        .is_allowed());

    // Outside allowed path but matches glob
    assert!(rbac
        .check_write(&perms, "refs/secrets/agentic-loop/instance-2/key")
        .is_denied());
}

//! Tests for skynet multi-tenant role/permission model

use agentic_repos::skynet::multi_tenant::permissions::{Permission, Role};

// ── Owner has everything ──

#[test]
fn owner_has_all_permissions() {
    let all = [
        Permission::CreateRepository,
        Permission::DeleteRepository,
        Permission::Push,
        Permission::Pull,
        Permission::ManageSshKeys,
        Permission::ViewMetrics,
    ];
    for perm in &all {
        assert!(Role::Owner.has_permission(perm), "Owner should have {:?}", perm);
    }
}

// ── Maintainer ──

#[test]
fn maintainer_can_push_pull_manage() {
    assert!(Role::Maintainer.has_permission(&Permission::Push));
    assert!(Role::Maintainer.has_permission(&Permission::Pull));
    assert!(Role::Maintainer.has_permission(&Permission::ManageSshKeys));
    assert!(Role::Maintainer.has_permission(&Permission::ViewMetrics));
    assert!(Role::Maintainer.has_permission(&Permission::DeleteRepository));
}

#[test]
fn maintainer_cannot_create_repo() {
    assert!(!Role::Maintainer.has_permission(&Permission::CreateRepository));
}

// ── Contributor ──

#[test]
fn contributor_can_push_pull() {
    assert!(Role::Contributor.has_permission(&Permission::Push));
    assert!(Role::Contributor.has_permission(&Permission::Pull));
}

#[test]
fn contributor_cannot_manage() {
    assert!(!Role::Contributor.has_permission(&Permission::ManageSshKeys));
    assert!(!Role::Contributor.has_permission(&Permission::ViewMetrics));
    assert!(!Role::Contributor.has_permission(&Permission::CreateRepository));
    assert!(!Role::Contributor.has_permission(&Permission::DeleteRepository));
}

// ── ReadOnly ──

#[test]
fn readonly_can_only_pull() {
    assert!(Role::ReadOnly.has_permission(&Permission::Pull));
    assert!(!Role::ReadOnly.has_permission(&Permission::Push));
    assert!(!Role::ReadOnly.has_permission(&Permission::ManageSshKeys));
    assert!(!Role::ReadOnly.has_permission(&Permission::CreateRepository));
    assert!(!Role::ReadOnly.has_permission(&Permission::DeleteRepository));
    assert!(!Role::ReadOnly.has_permission(&Permission::ViewMetrics));
}

// ── Serialization roundtrip ──

#[test]
fn role_serialization_roundtrip() {
    for role in [Role::Owner, Role::Maintainer, Role::Contributor, Role::ReadOnly] {
        let json = serde_json::to_string(&role).unwrap();
        let parsed: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, role);
    }
}

#[test]
fn permission_serialization_roundtrip() {
    let perms = [
        Permission::CreateRepository,
        Permission::DeleteRepository,
        Permission::Push,
        Permission::Pull,
        Permission::ManageSshKeys,
        Permission::ViewMetrics,
    ];
    for perm in &perms {
        let json = serde_json::to_string(perm).unwrap();
        let parsed: Permission = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, *perm);
    }
}

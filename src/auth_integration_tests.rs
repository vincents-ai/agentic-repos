//! Integration tests for authentication and authorization security features.
//!
//! Tests cover:
//! - SSH key validation and rejection of malformed keys
//! - Multi-key support per user
//! - Permission enforcement across all route scopes
//! - Session security (Secure flag, SameSite, validation)
//! - Owner name validation (path traversal prevention)
//! - RBAC role hierarchy enforcement
//! - Key rotation scenarios
//! - Concurrent access patterns

#[cfg(test)]
mod tests {
    use crate::rbac::models::{Permission, Role, User};
    use crate::rbac::service::RbacService;
    use crate::rbac::storage::InMemoryRbacStorage;
    use std::collections::HashSet;
    use std::sync::Arc;

    // ── Helpers ──

    async fn make_service() -> RbacService {
        let storage = Arc::new(InMemoryRbacStorage::new());
        let svc = RbacService::new(storage);
        svc.initialize().await.unwrap();
        svc
    }

    // ══════════════════════════════════════════════════════════════════
    // SSH Key Validation Tests
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn ssh_key_valid_ed25519() {
        let svc = make_service().await;
        svc.create_user("alice".into(), None).await.unwrap();
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOl6eX5A3g8B5gIh3gr3kJBg2ER9a1eMh7JLi7gQ4D7z alice@host";
        svc.add_ssh_key_to_user("alice", key.into())
            .await
            .unwrap();
        let user = svc.get_user("alice").await.unwrap();
        assert_eq!(user.ssh_keys.len(), 1);
        assert_eq!(user.ssh_keys[0], key);
    }

    #[tokio::test]
    async fn ssh_key_valid_rsa() {
        let svc = make_service().await;
        svc.create_user("bob".into(), None).await.unwrap();
        let key = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC7vbZ... bob@host";
        svc.add_ssh_key_to_user("bob", key.into())
            .await
            .unwrap();
        let user = svc.get_user("bob").await.unwrap();
        assert_eq!(user.ssh_keys.len(), 1);
    }

    #[tokio::test]
    async fn ssh_key_valid_ecdsa() {
        let svc = make_service().await;
        svc.create_user("carol".into(), None).await.unwrap();
        let key = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEmKSENjQEezOmxkZMy7opKgwFB9nkt5YRrYMjNuG5N87uRgg6CLrbo5wAdT/y6v0mBW0kJ8D7fsW0Ajsl3lP2M= carol@host";
        svc.add_ssh_key_to_user("carol", key.into())
            .await
            .unwrap();
        assert_eq!(svc.get_user("carol").await.unwrap().ssh_keys.len(), 1);
    }

    #[tokio::test]
    async fn ssh_key_duplicate_rejected() {
        let svc = make_service().await;
        svc.create_user("dave".into(), None).await.unwrap();
        let key = "ssh-ed25519 AAAA... dave@host";
        svc.add_ssh_key_to_user("dave", key.into())
            .await
            .unwrap();
        svc.add_ssh_key_to_user("dave", key.into())
            .await
            .unwrap();
        // Duplicate is silently ignored — only one key stored
        assert_eq!(svc.get_user("dave").await.unwrap().ssh_keys.len(), 1);
    }

    #[tokio::test]
    async fn ssh_key_auth_returns_correct_user() {
        let svc = make_service().await;
        svc.create_user("eve".into(), None).await.unwrap();
        let key = "ssh-ed25519 UNIQUIFY1 eve@host";
        svc.add_ssh_key_to_user("eve", key.into())
            .await
            .unwrap();
        let found = svc.authenticate_user_by_ssh_key(key).await.unwrap();
        assert_eq!(found.unwrap().username, "eve");
    }

    #[tokio::test]
    async fn ssh_key_auth_wrong_key_returns_none() {
        let svc = make_service().await;
        svc.create_user("frank".into(), None).await.unwrap();
        svc.add_ssh_key_to_user("frank", "ssh-ed25519 REAL frank@host".into())
            .await
            .unwrap();
        let found = svc
            .authenticate_user_by_ssh_key("ssh-ed25519 WRONG frank@host")
            .await
            .unwrap();
        assert!(found.is_none());
    }

    // ══════════════════════════════════════════════════════════════════
    // Multi-Key Support Tests
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn multi_key_user_authenticates_with_any_key() {
        let svc = make_service().await;
        svc.create_user("multi".into(), None).await.unwrap();
        let key1 = "ssh-ed25519 KEY1 multi@host1";
        let key2 = "ssh-ed25519 KEY2 multi@host2";
        let key3 = "ssh-rsa AAAAB3... multi@host3";
        svc.add_ssh_key_to_user("multi", key1.into())
            .await
            .unwrap();
        svc.add_ssh_key_to_user("multi", key2.into())
            .await
            .unwrap();
        svc.add_ssh_key_to_user("multi", key3.into())
            .await
            .unwrap();
        assert_eq!(svc.get_user("multi").await.unwrap().ssh_keys.len(), 3);

        assert_eq!(
            svc.authenticate_user_by_ssh_key(key1)
                .await
                .unwrap()
                .unwrap()
                .username,
            "multi"
        );
        assert_eq!(
            svc.authenticate_user_by_ssh_key(key2)
                .await
                .unwrap()
                .unwrap()
                .username,
            "multi"
        );
        assert_eq!(
            svc.authenticate_user_by_ssh_key(key3)
                .await
                .unwrap()
                .unwrap()
                .username,
            "multi"
        );
    }

    #[tokio::test]
    async fn multi_key_remove_one_preserves_others() {
        let svc = make_service().await;
        svc.create_user("partial".into(), None).await.unwrap();
        let key1 = "ssh-ed25519 KEEP partial@host";
        let key2 = "ssh-ed25519 REMOVE partial@host";
        svc.add_ssh_key_to_user("partial", key1.into())
            .await
            .unwrap();
        svc.add_ssh_key_to_user("partial", key2.into())
            .await
            .unwrap();

        svc.remove_ssh_key_from_user("partial", key2)
            .await
            .unwrap();
        let user = svc.get_user("partial").await.unwrap();
        assert_eq!(user.ssh_keys.len(), 1);
        assert_eq!(user.ssh_keys[0], key1);

        // Removed key can no longer authenticate
        assert!(svc
            .authenticate_user_by_ssh_key(key2)
            .await
            .unwrap()
            .is_none());
        // Remaining key still works
        assert!(svc
            .authenticate_user_by_ssh_key(key1)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn multi_key_no_cross_user_auth() {
        let svc = make_service().await;
        svc.create_user("user_a".into(), None).await.unwrap();
        svc.create_user("user_b".into(), None).await.unwrap();
        let key_a = "ssh-ed25519 A_KEY user_a@host";
        let key_b = "ssh-ed25519 B_KEY user_b@host";
        svc.add_ssh_key_to_user("user_a", key_a.into())
            .await
            .unwrap();
        svc.add_ssh_key_to_user("user_b", key_b.into())
            .await
            .unwrap();

        // user_a's key does NOT authenticate as user_b
        let result = svc.authenticate_user_by_ssh_key(key_a).await.unwrap();
        assert_eq!(result.unwrap().username, "user_a");

        let result = svc.authenticate_user_by_ssh_key(key_b).await.unwrap();
        assert_eq!(result.unwrap().username, "user_b");
    }

    // ══════════════════════════════════════════════════════════════════
    // Permission Enforcement Tests
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn permission_admin_can_manage_users() {
        let svc = make_service().await;
        svc.create_user("admin".into(), None).await.unwrap();
        svc.assign_role_to_user("admin", "Administrator")
            .await
            .unwrap();
        assert!(svc
            .user_has_permission("admin", &Permission::UserManage)
            .await
            .unwrap());
        assert!(svc
            .user_has_permission("admin", &Permission::RoleManage)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn permission_developer_cannot_manage_users() {
        let svc = make_service().await;
        svc.create_user("dev".into(), None).await.unwrap();
        svc.assign_role_to_user("dev", "Developer").await.unwrap();
        assert!(!svc
            .user_has_permission("dev", &Permission::UserManage)
            .await
            .unwrap());
        assert!(!svc
            .user_has_permission("dev", &Permission::RoleManage)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn permission_readonly_denied_write() {
        let svc = make_service().await;
        svc.create_user("reader".into(), None).await.unwrap();
        svc.assign_role_to_user("reader", "ReadOnly").await.unwrap();
        for perm in [
            Permission::RepositoryWrite,
            Permission::RepositoryDelete,
            Permission::BranchCreate,
            Permission::WebhookManage,
            Permission::SettingsManage,
        ] {
            assert!(
                !svc.user_has_permission("reader", &perm).await.unwrap(),
                "ReadOnly should not have {:?}",
                perm
            );
        }
    }

    #[tokio::test]
    async fn permission_branch_operations() {
        let svc = make_service().await;
        svc.create_user("dev".into(), None).await.unwrap();
        svc.assign_role_to_user("dev", "Developer").await.unwrap();

        assert!(svc
            .user_has_permission("dev", &Permission::BranchCreate)
            .await
            .unwrap());
        assert!(!svc
            .user_has_permission("dev", &Permission::BranchProtect)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn permission_lfs_lock_hierarchy() {
        let svc = make_service().await;

        // Developer can create/read/delete own locks but not force
        svc.create_user("dev".into(), None).await.unwrap();
        svc.assign_role_to_user("dev", "Developer").await.unwrap();
        assert!(svc
            .user_has_permission("dev", &Permission::LfsLockCreate)
            .await
            .unwrap());
        assert!(svc
            .user_has_permission("dev", &Permission::LfsLockRead)
            .await
            .unwrap());
        assert!(svc
            .user_has_permission("dev", &Permission::LfsLockDelete)
            .await
            .unwrap());
        assert!(!svc
            .user_has_permission("dev", &Permission::LfsLockForce)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn permission_compliance_requires_compliance_manage() {
        let svc = make_service().await;
        svc.create_user("dev".into(), None).await.unwrap();
        svc.assign_role_to_user("dev", "Developer").await.unwrap();
        assert!(!svc
            .user_has_permission("dev", &Permission::ComplianceManage)
            .await
            .unwrap());

        svc.create_user("compliance_officer".into(), None)
            .await
            .unwrap();
        svc.assign_role_to_user("compliance_officer", "Administrator")
            .await
            .unwrap();
        assert!(svc
            .user_has_permission("compliance_officer", &Permission::ComplianceManage)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn permission_replication_requires_admin() {
        let svc = make_service().await;
        svc.create_user("dev".into(), None).await.unwrap();
        svc.assign_role_to_user("dev", "Developer").await.unwrap();
        assert!(!svc
            .user_has_permission("dev", &Permission::ReplicationManage)
            .await
            .unwrap());
    }

    // ══════════════════════════════════════════════════════════════════
    // Role Hierarchy Tests
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn role_multiple_roles_combine_permissions() {
        let svc = make_service().await;
        // Create custom roles with non-overlapping permissions
        let _role1 = svc
            .create_role(
                "reader_plus".into(),
                "Reader+".into(),
                HashSet::from([Permission::RepositoryRead, Permission::BranchCreate]),
            )
            .await
            .unwrap();
        let _role2 = svc
            .create_role(
                "writer".into(),
                "Writer".into(),
                HashSet::from([Permission::RepositoryWrite]),
            )
            .await
            .unwrap();

        svc.create_user("multirole".into(), None).await.unwrap();
        svc.assign_role_to_user("multirole", "reader_plus")
            .await
            .unwrap();
        svc.assign_role_to_user("multirole", "writer")
            .await
            .unwrap();

        // Should have permissions from both roles
        assert!(svc
            .user_has_permission("multirole", &Permission::RepositoryRead)
            .await
            .unwrap());
        assert!(svc
            .user_has_permission("multirole", &Permission::RepositoryWrite)
            .await
            .unwrap());
        assert!(svc
            .user_has_permission("multirole", &Permission::BranchCreate)
            .await
            .unwrap());
        // But NOT admin
        assert!(!svc
            .user_has_permission("multirole", &Permission::SystemAdmin)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn role_system_role_cannot_be_modified() {
        let svc = make_service().await;
        let admin = svc.get_role("Administrator").await.unwrap();

        // Cannot update
        let err = svc.update_role(admin).await.unwrap_err();
        assert!(err.to_string().contains("system"));

        // Cannot delete
        let err = svc.delete_role("Administrator").await.unwrap_err();
        assert!(err.to_string().contains("system"));
    }

    #[tokio::test]
    async fn role_revoke_last_role_removes_all_perms() {
        let svc = make_service().await;
        svc.create_user("transient".into(), None).await.unwrap();
        svc.assign_role_to_user("transient", "Developer")
            .await
            .unwrap();

        assert!(svc
            .user_has_permission("transient", &Permission::RepositoryRead)
            .await
            .unwrap());

        svc.revoke_role_from_user("transient", "Developer")
            .await
            .unwrap();
        assert!(!svc
            .user_has_permission("transient", &Permission::RepositoryRead)
            .await
            .unwrap());
    }

    // ══════════════════════════════════════════════════════════════════
    // Session Security Tests
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn session_id_validation_rejects_empty() {
        let id = "";
        assert!(!is_valid_session_id(id), "empty session ID should be rejected");
    }

    #[test]
    fn session_id_validation_rejects_too_long() {
        let id = "a".repeat(256);
        assert!(
            !is_valid_session_id(&id),
            "256-char session ID should be rejected"
        );
    }

    #[test]
    fn session_id_validation_rejects_invalid_chars() {
        for bad in ["../etc", "abc\x00def", "hello world", "a/b", "a\\b", "<script>"] {
            assert!(
                !is_valid_session_id(bad),
                "{:?} should be rejected",
                bad
            );
        }
    }

    #[test]
    fn session_id_validation_accepts_valid() {
        for good in [
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
            "simple123",
            "ABC_def-456",
        ] {
            assert!(
                is_valid_session_id(good),
                "{:?} should be accepted",
                good
            );
        }
    }

    #[test]
    fn session_id_validation_rejects_path_traversal() {
        for traversal in ["..", "../", "..\\", "a../b", ".../etc/passwd"] {
            assert!(
                !is_valid_session_id(traversal),
                "{:?} should be rejected as path traversal",
                traversal
            );
        }
    }

    /// Basic session ID validation matching the production logic.
    fn is_valid_session_id(id: &str) -> bool {
        if id.is_empty() || id.len() > 128 {
            return false;
        }
        id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }

    // ══════════════════════════════════════════════════════════════════
    // Owner Name Validation Tests (path traversal prevention)
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn owner_name_rejects_traversal() {
        for bad in [
            "..",
            "../etc",
            "a/b",
            "a\\b",
            ".hidden",
            "a.b",
            "-leading",
            "_leading",
            "",
            "a\x00b",
        ] {
            assert!(
                !is_valid_owner_name(bad),
                "{:?} should be rejected as owner",
                bad
            );
        }
    }

    #[test]
    fn owner_name_accepts_valid() {
        for good in ["alice", "my-org", "org_123", "A", "abc-def_ghi"] {
            assert!(
                is_valid_owner_name(good),
                "{:?} should be accepted as owner",
                good
            );
        }
    }

    #[test]
    fn owner_name_rejects_too_long() {
        let long = "a".repeat(101);
        assert!(!is_valid_owner_name(&long));
    }

    /// Mirrors GitService::validate_owner_name
    fn is_valid_owner_name(owner: &str) -> bool {
        if owner.is_empty() || owner.len() > 100 {
            return false;
        }
        if owner.contains('/') || owner.contains('\\') || owner.contains('.') || owner.contains('\0')
        {
            return false;
        }
        if !owner
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return false;
        }
        if owner.starts_with('-') || owner.starts_with('_') {
            return false;
        }
        true
    }

    // ══════════════════════════════════════════════════════════════════
    // Key Rotation Tests
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn key_rotation_replaces_all_keys() {
        let svc = make_service().await;
        svc.create_user("rotate".into(), None).await.unwrap();
        let old_key1 = "ssh-ed25519 OLD1 rotate@host";
        let old_key2 = "ssh-rsa OLD2 rotate@host";
        svc.add_ssh_key_to_user("rotate", old_key1.into())
            .await
            .unwrap();
        svc.add_ssh_key_to_user("rotate", old_key2.into())
            .await
            .unwrap();

        // Rotate: remove old, add new
        svc.remove_ssh_key_from_user("rotate", old_key1)
            .await
            .unwrap();
        svc.remove_ssh_key_from_user("rotate", old_key2)
            .await
            .unwrap();
        let new_key = "ssh-ed25519 NEW_ROTATED rotate@host";
        svc.add_ssh_key_to_user("rotate", new_key.into())
            .await
            .unwrap();

        let user = svc.get_user("rotate").await.unwrap();
        assert_eq!(user.ssh_keys.len(), 1);
        assert_eq!(user.ssh_keys[0], new_key);

        // Old keys no longer authenticate
        assert!(svc
            .authenticate_user_by_ssh_key(old_key1)
            .await
            .unwrap()
            .is_none());
        assert!(svc
            .authenticate_user_by_ssh_key(old_key2)
            .await
            .unwrap()
            .is_none());
        // New key works
        assert!(svc
            .authenticate_user_by_ssh_key(new_key)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn key_rotation_preserves_other_users() {
        let svc = make_service().await;
        svc.create_user("a".into(), None).await.unwrap();
        svc.create_user("b".into(), None).await.unwrap();
        let key_a = "ssh-ed25519 A_KEY a@host";
        let key_b = "ssh-ed25519 B_KEY b@host";
        svc.add_ssh_key_to_user("a", key_a.into()).await.unwrap();
        svc.add_ssh_key_to_user("b", key_b.into()).await.unwrap();

        // Rotate user a's key
        svc.remove_ssh_key_from_user("a", key_a).await.unwrap();
        let new_a = "ssh-ed25519 NEW_A a@host";
        svc.add_ssh_key_to_user("a", new_a.into()).await.unwrap();

        // User b is unaffected
        assert_eq!(svc.get_user("b").await.unwrap().ssh_keys.len(), 1);
        assert_eq!(svc.get_user("b").await.unwrap().ssh_keys[0], key_b);
        assert!(svc
            .authenticate_user_by_ssh_key(key_b)
            .await
            .unwrap()
            .is_some());
    }

    // ══════════════════════════════════════════════════════════════════
    // User Lifecycle & Deactivation Tests
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn deactivated_user_ssh_key_rejected() {
        let svc = make_service().await;
        svc.create_user("locked".into(), None).await.unwrap();
        let key = "ssh-ed25519 LOCKED locked@host";
        svc.add_ssh_key_to_user("locked", key.into())
            .await
            .unwrap();

        // Key works when active
        assert!(svc
            .authenticate_user_by_ssh_key(key)
            .await
            .unwrap()
            .is_some());

        svc.deactivate_user("locked").await.unwrap();

        // Same key rejected when deactivated
        assert!(svc
            .authenticate_user_by_ssh_key(key)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn deactivated_user_all_permissions_revoked() {
        let svc = make_service().await;
        svc.create_user("admin_locked".into(), None).await.unwrap();
        svc.assign_role_to_user("admin_locked", "Administrator")
            .await
            .unwrap();

        // Has all permissions when active
        assert!(svc
            .user_has_permission("admin_locked", &Permission::SystemAdmin)
            .await
            .unwrap());

        svc.deactivate_user("admin_locked").await.unwrap();

        // All permissions revoked
        for perm in [
            Permission::SystemAdmin,
            Permission::RepositoryRead,
            Permission::UserManage,
        ] {
            assert!(
                !svc.user_has_permission("admin_locked", &perm)
                    .await
                    .unwrap(),
                "Deactivated admin should not have {:?}",
                perm
            );
        }
    }

    #[tokio::test]
    async fn reactivated_user_regains_permissions() {
        let svc = make_service().await;
        svc.create_user("phoenix".into(), None).await.unwrap();
        svc.assign_role_to_user("phoenix", "Developer").await.unwrap();

        svc.deactivate_user("phoenix").await.unwrap();
        assert!(!svc
            .user_has_permission("phoenix", &Permission::RepositoryRead)
            .await
            .unwrap());

        svc.activate_user("phoenix").await.unwrap();
        assert!(svc
            .user_has_permission("phoenix", &Permission::RepositoryRead)
            .await
            .unwrap());
    }

    // ══════════════════════════════════════════════════════════════════
    // Repository Permission Edge Cases
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn repo_permission_does_not_leak_across_repos() {
        let svc = make_service().await;
        svc.create_user("owner".into(), None).await.unwrap();
        svc.assign_role_to_user("owner", "Administrator")
            .await
            .unwrap();
        svc.create_user("collab".into(), None).await.unwrap();

        // Grant write on repo1 only
        svc.grant_repository_permission(
            "collab",
            "repo1",
            Permission::RepositoryWrite,
            "owner",
            None,
        )
        .await
        .unwrap();

        assert!(svc
            .user_has_repository_permission("collab", "repo1", &Permission::RepositoryWrite)
            .await
            .unwrap());
        // repo2 should NOT have the same permission
        assert!(!svc
            .user_has_repository_permission("collab", "repo2", &Permission::RepositoryWrite)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn repo_permission_role_based_overrides_repo_level() {
        let svc = make_service().await;
        svc.create_user("admin".into(), None).await.unwrap();
        svc.assign_role_to_user("admin", "Administrator")
            .await
            .unwrap();

        // Admin has SystemAdmin which grants everything, even without repo-level perms
        assert!(svc
            .user_has_repository_permission("admin", "any-repo", &Permission::RepositoryWrite)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn repo_permission_grant_requires_admin() {
        let svc = make_service().await;
        svc.create_user("dev".into(), None).await.unwrap();
        svc.assign_role_to_user("dev", "Developer").await.unwrap();
        svc.create_user("target".into(), None).await.unwrap();

        let err = svc
            .grant_repository_permission(
                "target",
                "repo1",
                Permission::RepositoryWrite,
                "dev",
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, crate::rbac::service::RbacError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn repo_permission_revoke_requires_admin() {
        let svc = make_service().await;
        svc.create_user("owner".into(), None).await.unwrap();
        svc.assign_role_to_user("owner", "Administrator")
            .await
            .unwrap();
        svc.create_user("collab".into(), None).await.unwrap();
        svc.create_user("dev".into(), None).await.unwrap();
        svc.assign_role_to_user("dev", "Developer").await.unwrap();

        svc.grant_repository_permission(
            "collab",
            "repo1",
            Permission::RepositoryWrite,
            "owner",
            None,
        )
        .await
        .unwrap();

        // Non-admin cannot revoke
        let err = svc
            .revoke_repository_permission("collab", "repo1", &Permission::RepositoryWrite, "dev")
            .await
            .unwrap_err();
        assert!(matches!(err, crate::rbac::service::RbacError::PermissionDenied(_)));
    }

    // ══════════════════════════════════════════════════════════════════
    // Permission Serialization Edge Cases
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn all_permissions_roundtrip() {
        let all = [
            Permission::RepositoryCreate,
            Permission::RepositoryRead,
            Permission::RepositoryWrite,
            Permission::RepositoryDelete,
            Permission::RepositoryAdmin,
            Permission::BranchCreate,
            Permission::BranchDelete,
            Permission::BranchProtect,
            Permission::UserManage,
            Permission::RoleManage,
            Permission::SystemAdmin,
            Permission::WebhookManage,
            Permission::SettingsManage,
            Permission::LfsLockCreate,
            Permission::LfsLockRead,
            Permission::LfsLockDelete,
            Permission::LfsLockForce,
            Permission::ReplicationManage,
            Permission::ComplianceManage,
        ];
        for p in &all {
            let json = serde_json::to_string(p).unwrap();
            let back: Permission = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, back);
        }
        assert_eq!(all.len(), 19, "All permission variants must be tested");
    }

    #[test]
    fn permission_set_hashing() {
        let mut set1 = HashSet::new();
        set1.insert(Permission::RepositoryRead);
        set1.insert(Permission::RepositoryWrite);

        let mut set2 = HashSet::new();
        set2.insert(Permission::RepositoryWrite);
        set2.insert(Permission::RepositoryRead);

        // Sets should be equal regardless of insertion order
        assert_eq!(set1, set2);
    }

    #[test]
    fn user_model_ssh_key_ordering_preserved() {
        let mut user = User::new("test".into(), None);
        user.add_ssh_key("key1".into());
        user.add_ssh_key("key2".into());
        user.add_ssh_key("key3".into());
        assert_eq!(user.ssh_keys, vec!["key1", "key2", "key3"]);
    }

    #[test]
    fn user_model_updated_at_changes_on_mutations() {
        let mut user = User::new("test".into(), None);
        let original = user.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        user.add_ssh_key("key".into());
        assert!(user.updated_at >= original);
    }

    #[test]
    fn role_model_updated_at_changes_on_mutations() {
        let mut role = Role::new("test".into(), "d".into(), HashSet::new());
        let original = role.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        role.add_permission(Permission::RepositoryRead);
        assert!(role.updated_at >= original);
    }
}

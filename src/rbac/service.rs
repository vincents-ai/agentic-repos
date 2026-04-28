use crate::rbac::models::{Permission, RepositoryPermission, Role, User};
use crate::rbac::storage::RbacStorage;
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RbacError {
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Role not found: {0}")]
    RoleNotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("User already exists: {0}")]
    UserExists(String),
    #[error("Role already exists: {0}")]
    RoleExists(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

pub type RbacResult<T> = Result<T, RbacError>;

pub struct RbacService {
    storage: Arc<dyn RbacStorage>,
}

impl RbacService {
    pub fn new(storage: Arc<dyn RbacStorage>) -> Self {
        Self { storage }
    }

    pub async fn initialize(&self) -> RbacResult<()> {
        let default_roles = crate::rbac::models::create_default_roles();

        for role in default_roles {
            if !self
                .storage
                .role_exists(&role.name)
                .await
                .map_err(|e| RbacError::Storage(e.to_string()))?
            {
                self.storage
                    .create_role(role)
                    .await
                    .map_err(|e| RbacError::Storage(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn create_user(&self, username: String, email: Option<String>) -> RbacResult<User> {
        if self
            .storage
            .user_exists(&username)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?
        {
            return Err(RbacError::UserExists(username));
        }

        let user = User::new(username, email);
        self.storage
            .create_user(user.clone())
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;

        Ok(user)
    }

    pub async fn get_user(&self, username: &str) -> RbacResult<User> {
        self.storage
            .get_user(username)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?
            .ok_or_else(|| RbacError::UserNotFound(username.to_string()))
    }

    pub async fn get_user_by_id(&self, user_id: &Uuid) -> RbacResult<User> {
        self.storage
            .get_user_by_id(user_id)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?
            .ok_or_else(|| RbacError::UserNotFound(user_id.to_string()))
    }

    pub async fn assign_role_to_user(&self, username: &str, role_name: &str) -> RbacResult<()> {
        let mut user = self.get_user(username).await?;
        let role = self.get_role(role_name).await?;

        user.assign_role(role.id);
        self.storage
            .update_user(user)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;

        Ok(())
    }

    pub async fn revoke_role_from_user(&self, username: &str, role_name: &str) -> RbacResult<()> {
        let mut user = self.get_user(username).await?;
        let role = self.get_role(role_name).await?;

        user.revoke_role(&role.id);
        self.storage
            .update_user(user)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;

        Ok(())
    }

    pub async fn create_role(
        &self,
        name: String,
        description: String,
        permissions: HashSet<Permission>,
    ) -> RbacResult<Role> {
        if self
            .storage
            .role_exists(&name)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?
        {
            return Err(RbacError::RoleExists(name));
        }

        let role = Role::new(name, description, permissions);
        self.storage
            .create_role(role.clone())
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;

        Ok(role)
    }

    pub async fn get_role(&self, name: &str) -> RbacResult<Role> {
        self.storage
            .get_role(name)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?
            .ok_or_else(|| RbacError::RoleNotFound(name.to_string()))
    }

    pub async fn update_role(&self, role: Role) -> RbacResult<()> {
        if role.is_system_role {
            return Err(RbacError::InvalidOperation(
                "Cannot modify system roles".to_string(),
            ));
        }

        self.storage
            .update_role(role)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn delete_role(&self, name: &str) -> RbacResult<()> {
        let role = self.get_role(name).await?;

        if role.is_system_role {
            return Err(RbacError::InvalidOperation(
                "Cannot delete system roles".to_string(),
            ));
        }

        self.storage
            .delete_role(name)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn user_has_permission(
        &self,
        username: &str,
        permission: &Permission,
    ) -> RbacResult<bool> {
        let user = self.get_user(username).await?;

        if !user.is_active {
            return Ok(false);
        }

        for role_id in &user.roles {
            let role = self
                .storage
                .get_role_by_id(role_id)
                .await
                .map_err(|e| RbacError::Storage(e.to_string()))?;

            if let Some(role) = role {
                if role.has_permission(permission) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    pub async fn user_has_repository_permission(
        &self,
        username: &str,
        repository_id: &str,
        permission: &Permission,
    ) -> RbacResult<bool> {
        if self.user_has_permission(username, permission).await? {
            return Ok(true);
        }

        let user = self.get_user(username).await?;
        let repo_permissions = self
            .storage
            .get_repository_permissions(repository_id, &user.id)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;

        for repo_perm in repo_permissions {
            if repo_perm
                .expires_at
                .is_none_or(|expires| expires > chrono::Utc::now())
                && repo_perm.permissions.contains(permission)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub async fn grant_repository_permission(
        &self,
        username: &str,
        repository_id: &str,
        permission: Permission,
        granted_by_username: &str,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> RbacResult<()> {
        let user = self.get_user(username).await?;
        let granted_by_user = self.get_user(granted_by_username).await?;

        if !self
            .user_has_permission(granted_by_username, &Permission::RepositoryAdmin)
            .await?
        {
            return Err(RbacError::PermissionDenied(
                "Only repository admins can grant permissions".to_string(),
            ));
        }

        let mut permissions = HashSet::new();
        permissions.insert(permission);

        let repo_permission = RepositoryPermission {
            repository_id: repository_id.to_string(),
            user_id: user.id,
            permissions,
            granted_by: granted_by_user.id,
            granted_at: chrono::Utc::now(),
            expires_at,
        };

        self.storage
            .create_repository_permission(repo_permission)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;

        Ok(())
    }

    pub async fn revoke_repository_permission(
        &self,
        username: &str,
        repository_id: &str,
        permission: &Permission,
        revoked_by_username: &str,
    ) -> RbacResult<()> {
        let user = self.get_user(username).await?;

        if !self
            .user_has_permission(revoked_by_username, &Permission::RepositoryAdmin)
            .await?
        {
            return Err(RbacError::PermissionDenied(
                "Only repository admins can revoke permissions".to_string(),
            ));
        }

        self.storage
            .delete_repository_permission(repository_id, &user.id, permission)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;

        Ok(())
    }

    pub async fn list_users(&self) -> RbacResult<Vec<User>> {
        self.storage
            .list_users()
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))
    }

    pub async fn list_roles(&self) -> RbacResult<Vec<Role>> {
        self.storage
            .list_roles()
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))
    }

    pub async fn update_user(&self, user: User) -> RbacResult<()> {
        self.storage
            .update_user(user)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))
    }

    pub async fn authenticate_user_by_ssh_key(&self, ssh_key: &str) -> RbacResult<Option<User>> {
        let users = self.list_users().await?;

        for user in users {
            if user.is_active && user.ssh_keys.contains(&ssh_key.to_string()) {
                let mut updated_user = user.clone();
                updated_user.set_last_login();
                let _ = self.storage.update_user(updated_user).await;
                return Ok(Some(user));
            }
        }

        Ok(None)
    }

    pub async fn add_ssh_key_to_user(&self, username: &str, ssh_key: String) -> RbacResult<()> {
        let mut user = self.get_user(username).await?;
        user.add_ssh_key(ssh_key);
        self.storage
            .update_user(user)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn remove_ssh_key_from_user(&self, username: &str, ssh_key: &str) -> RbacResult<()> {
        let mut user = self.get_user(username).await?;
        user.remove_ssh_key(ssh_key);
        self.storage
            .update_user(user)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn deactivate_user(&self, username: &str) -> RbacResult<()> {
        let mut user = self.get_user(username).await?;
        user.deactivate();
        self.storage
            .update_user(user)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn activate_user(&self, username: &str) -> RbacResult<()> {
        let mut user = self.get_user(username).await?;
        user.activate();
        self.storage
            .update_user(user)
            .await
            .map_err(|e| RbacError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::storage::InMemoryRbacStorage;
    use std::collections::HashSet;

    async fn make_service() -> RbacService {
        let storage = Arc::new(InMemoryRbacStorage::new());
        let svc = RbacService::new(storage);
        svc.initialize().await.unwrap();
        svc
    }

    // ── User CRUD ──

    #[tokio::test]
    async fn create_user_basic() {
        let svc = make_service().await;
        let user = svc
            .create_user("alice".into(), Some("alice@test.com".into()))
            .await
            .unwrap();
        assert_eq!(user.username, "alice");
        assert!(user.is_active);
    }

    #[tokio::test]
    async fn create_user_duplicate_rejected() {
        let svc = make_service().await;
        svc.create_user("bob".into(), None).await.unwrap();
        let err = svc.create_user("bob".into(), None).await.unwrap_err();
        assert!(matches!(err, RbacError::UserExists(_)));
    }

    #[tokio::test]
    async fn get_user_not_found() {
        let svc = make_service().await;
        let err = svc.get_user("nobody").await.unwrap_err();
        assert!(matches!(err, RbacError::UserNotFound(_)));
    }

    #[tokio::test]
    async fn get_user_by_id() {
        let svc = make_service().await;
        let user = svc.create_user("charlie".into(), None).await.unwrap();
        let found = svc.get_user_by_id(&user.id).await.unwrap();
        assert_eq!(found.username, "charlie");
    }

    #[tokio::test]
    async fn get_user_by_id_not_found() {
        let svc = make_service().await;
        let err = svc.get_user_by_id(&Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, RbacError::UserNotFound(_)));
    }

    // ── Role CRUD ──

    #[tokio::test]
    async fn create_custom_role() {
        let svc = make_service().await;
        let role = svc
            .create_role(
                "custom".into(),
                "Custom role".into(),
                HashSet::from([Permission::RepositoryRead]),
            )
            .await
            .unwrap();
        assert_eq!(role.name, "custom");
        assert!(!role.is_system_role);
    }

    #[tokio::test]
    async fn create_role_duplicate_rejected() {
        let svc = make_service().await;
        svc.create_role("dup".into(), "d".into(), HashSet::new())
            .await
            .unwrap();
        let err = svc
            .create_role("dup".into(), "d".into(), HashSet::new())
            .await
            .unwrap_err();
        assert!(matches!(err, RbacError::RoleExists(_)));
    }

    #[tokio::test]
    async fn update_system_role_rejected() {
        let svc = make_service().await;
        let admin_role = svc.get_role("Administrator").await.unwrap();
        let err = svc.update_role(admin_role).await.unwrap_err();
        assert!(matches!(err, RbacError::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn delete_system_role_rejected() {
        let svc = make_service().await;
        let err = svc.delete_role("Administrator").await.unwrap_err();
        assert!(matches!(err, RbacError::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn delete_custom_role() {
        let svc = make_service().await;
        svc.create_role("temp".into(), "d".into(), HashSet::new())
            .await
            .unwrap();
        svc.delete_role("temp").await.unwrap();
        assert!(svc
            .get_role("temp")
            .await
            .unwrap_err()
            .to_string()
            .contains("not found"));
    }

    // ── Role assignment ──

    #[tokio::test]
    async fn assign_revoke_role() {
        let svc = make_service().await;
        svc.create_user("dave".into(), None).await.unwrap();

        svc.assign_role_to_user("dave", "Developer").await.unwrap();
        let user = svc.get_user("dave").await.unwrap();
        assert!(user.has_role(&svc.get_role("Developer").await.unwrap().id));

        svc.revoke_role_from_user("dave", "Developer")
            .await
            .unwrap();
        let user = svc.get_user("dave").await.unwrap();
        assert!(!user.has_role(&svc.get_role("Developer").await.unwrap().id));
    }

    #[tokio::test]
    async fn assign_nonexistent_role() {
        let svc = make_service().await;
        svc.create_user("eve".into(), None).await.unwrap();
        let err = svc
            .assign_role_to_user("eve", "NoSuchRole")
            .await
            .unwrap_err();
        assert!(matches!(err, RbacError::RoleNotFound(_)));
    }

    // ── Permission checking ──

    #[tokio::test]
    async fn admin_has_all_permissions() {
        let svc = make_service().await;
        svc.create_user("admin".into(), None).await.unwrap();
        svc.assign_role_to_user("admin", "Administrator")
            .await
            .unwrap();

        assert!(svc
            .user_has_permission("admin", &Permission::SystemAdmin)
            .await
            .unwrap());
        assert!(svc
            .user_has_permission("admin", &Permission::RepositoryRead)
            .await
            .unwrap());
        assert!(svc
            .user_has_permission("admin", &Permission::LfsLockForce)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn developer_limited_permissions() {
        let svc = make_service().await;
        svc.create_user("dev".into(), None).await.unwrap();
        svc.assign_role_to_user("dev", "Developer").await.unwrap();

        assert!(svc
            .user_has_permission("dev", &Permission::RepositoryRead)
            .await
            .unwrap());
        assert!(svc
            .user_has_permission("dev", &Permission::RepositoryWrite)
            .await
            .unwrap());
        assert!(!svc
            .user_has_permission("dev", &Permission::SystemAdmin)
            .await
            .unwrap());
        assert!(!svc
            .user_has_permission("dev", &Permission::UserManage)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn readonly_user_cannot_write() {
        let svc = make_service().await;
        svc.create_user("reader".into(), None).await.unwrap();
        svc.assign_role_to_user("reader", "ReadOnly").await.unwrap();

        assert!(svc
            .user_has_permission("reader", &Permission::RepositoryRead)
            .await
            .unwrap());
        assert!(!svc
            .user_has_permission("reader", &Permission::RepositoryWrite)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn inactive_user_has_no_permissions() {
        let svc = make_service().await;
        svc.create_user("suspended".into(), None).await.unwrap();
        svc.assign_role_to_user("suspended", "Administrator")
            .await
            .unwrap();
        svc.deactivate_user("suspended").await.unwrap();

        assert!(!svc
            .user_has_permission("suspended", &Permission::SystemAdmin)
            .await
            .unwrap());
        assert!(!svc
            .user_has_permission("suspended", &Permission::RepositoryRead)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn user_no_roles_has_no_permissions() {
        let svc = make_service().await;
        svc.create_user("newbie".into(), None).await.unwrap();
        assert!(!svc
            .user_has_permission("newbie", &Permission::RepositoryRead)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn permission_check_unknown_user() {
        let svc = make_service().await;
        let err = svc
            .user_has_permission("ghost", &Permission::RepositoryRead)
            .await
            .unwrap_err();
        assert!(matches!(err, RbacError::UserNotFound(_)));
    }

    // ── Repository permissions ──

    #[tokio::test]
    async fn repo_permission_grant_and_check() {
        let svc = make_service().await;
        svc.create_user("owner".into(), None).await.unwrap();
        svc.assign_role_to_user("owner", "Administrator")
            .await
            .unwrap();
        svc.create_user("collab".into(), None).await.unwrap();

        // owner grants collab write access to repo
        svc.grant_repository_permission(
            "collab",
            "repo1",
            Permission::RepositoryWrite,
            "owner",
            None,
        )
        .await
        .unwrap();

        // collab can now write (via repo-level override, no role-based permission)
        assert!(svc
            .user_has_repository_permission("collab", "repo1", &Permission::RepositoryWrite)
            .await
            .unwrap());
        // collab still can't read other repos via role
        assert!(!svc
            .user_has_permission("collab", &Permission::RepositoryRead)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn repo_permission_non_admin_cannot_grant() {
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
        assert!(matches!(err, RbacError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn repo_permission_expired_is_ignored() {
        let svc = make_service().await;
        svc.create_user("owner".into(), None).await.unwrap();
        svc.assign_role_to_user("owner", "Administrator")
            .await
            .unwrap();
        svc.create_user("temp".into(), None).await.unwrap();

        // Grant with past expiry
        svc.grant_repository_permission(
            "temp",
            "repo1",
            Permission::RepositoryWrite,
            "owner",
            Some(chrono::Utc::now() - chrono::Duration::hours(1)),
        )
        .await
        .unwrap();

        assert!(!svc
            .user_has_repository_permission("temp", "repo1", &Permission::RepositoryWrite)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn repo_permission_revoke() {
        let svc = make_service().await;
        svc.create_user("owner".into(), None).await.unwrap();
        svc.assign_role_to_user("owner", "Administrator")
            .await
            .unwrap();
        svc.create_user("collab".into(), None).await.unwrap();

        svc.grant_repository_permission(
            "collab",
            "repo1",
            Permission::RepositoryWrite,
            "owner",
            None,
        )
        .await
        .unwrap();

        svc.revoke_repository_permission("collab", "repo1", &Permission::RepositoryWrite, "owner")
            .await
            .unwrap();

        assert!(!svc
            .user_has_repository_permission("collab", "repo1", &Permission::RepositoryWrite)
            .await
            .unwrap());
    }

    // ── SSH key auth ──

    #[tokio::test]
    async fn ssh_key_auth_success() {
        let svc = make_service().await;
        svc.create_user("sshuser".into(), None).await.unwrap();
        let key = "ssh-ed25519 AAAA...";
        svc.add_ssh_key_to_user("sshuser", key.into())
            .await
            .unwrap();

        let found = svc.authenticate_user_by_ssh_key(key).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "sshuser");
    }

    #[tokio::test]
    async fn ssh_key_auth_unknown_key() {
        let svc = make_service().await;
        let found = svc
            .authenticate_user_by_ssh_key("ssh-ed25519 NOPE")
            .await
            .unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn ssh_key_auth_inactive_user_rejected() {
        let svc = make_service().await;
        svc.create_user("disabled".into(), None).await.unwrap();
        let key = "ssh-ed25519 DISABLED";
        svc.add_ssh_key_to_user("disabled", key.into())
            .await
            .unwrap();
        svc.deactivate_user("disabled").await.unwrap();

        let found = svc.authenticate_user_by_ssh_key(key).await.unwrap();
        assert!(found.is_none());
    }

    // ── User lifecycle ──

    #[tokio::test]
    async fn deactivate_activate_user() {
        let svc = make_service().await;
        svc.create_user("lifecycle".into(), None).await.unwrap();

        let user = svc.get_user("lifecycle").await.unwrap();
        assert!(user.is_active);

        svc.deactivate_user("lifecycle").await.unwrap();
        let user = svc.get_user("lifecycle").await.unwrap();
        assert!(!user.is_active);

        svc.activate_user("lifecycle").await.unwrap();
        let user = svc.get_user("lifecycle").await.unwrap();
        assert!(user.is_active);
    }

    #[tokio::test]
    async fn list_users() {
        let svc = make_service().await;
        svc.create_user("u1".into(), None).await.unwrap();
        svc.create_user("u2".into(), None).await.unwrap();
        let users = svc.list_users().await.unwrap();
        assert_eq!(users.len(), 2);
    }

    #[tokio::test]
    async fn list_roles_includes_defaults() {
        let svc = make_service().await;
        let roles = svc.list_roles().await.unwrap();
        assert!(roles.len() >= 3); // Administrator, Developer, ReadOnly
    }

    // ── Initialize idempotent ──

    #[tokio::test]
    async fn initialize_is_idempotent() {
        let svc = make_service().await;
        // Second initialize should not fail
        svc.initialize().await.unwrap();
        let roles = svc.list_roles().await.unwrap();
        assert_eq!(roles.len(), 3); // Still exactly 3 defaults, no duplicates
    }
}

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
                .map_or(true, |expires| expires > chrono::Utc::now())
            {
                if repo_perm.permissions.contains(permission) {
                    return Ok(true);
                }
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

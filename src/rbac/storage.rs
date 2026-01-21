use crate::rbac::models::{Permission, RepositoryPermission, Role, User};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Storage operation failed: {0}")]
    OperationFailed(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

#[async_trait]
pub trait RbacStorage: Send + Sync {
    async fn create_user(&self, user: User) -> Result<(), StorageError>;
    async fn get_user(&self, username: &str) -> Result<Option<User>, StorageError>;
    async fn get_user_by_id(&self, id: &Uuid) -> Result<Option<User>, StorageError>;
    async fn update_user(&self, user: User) -> Result<(), StorageError>;
    async fn delete_user(&self, username: &str) -> Result<(), StorageError>;
    async fn user_exists(&self, username: &str) -> Result<bool, StorageError>;
    async fn list_users(&self) -> Result<Vec<User>, StorageError>;

    async fn create_role(&self, role: Role) -> Result<(), StorageError>;
    async fn get_role(&self, name: &str) -> Result<Option<Role>, StorageError>;
    async fn get_role_by_id(&self, id: &Uuid) -> Result<Option<Role>, StorageError>;
    async fn update_role(&self, role: Role) -> Result<(), StorageError>;
    async fn delete_role(&self, name: &str) -> Result<(), StorageError>;
    async fn role_exists(&self, name: &str) -> Result<bool, StorageError>;
    async fn list_roles(&self) -> Result<Vec<Role>, StorageError>;

    async fn create_repository_permission(
        &self,
        permission: RepositoryPermission,
    ) -> Result<(), StorageError>;
    async fn get_repository_permissions(
        &self,
        repository_id: &str,
        user_id: &Uuid,
    ) -> Result<Vec<RepositoryPermission>, StorageError>;
    async fn delete_repository_permission(
        &self,
        repository_id: &str,
        user_id: &Uuid,
        permission: &Permission,
    ) -> Result<(), StorageError>;
}

pub struct InMemoryRbacStorage {
    users: RwLock<HashMap<String, User>>,
    users_by_id: RwLock<HashMap<Uuid, User>>,
    roles: RwLock<HashMap<String, Role>>,
    roles_by_id: RwLock<HashMap<Uuid, Role>>,
    repository_permissions: RwLock<HashMap<String, Vec<RepositoryPermission>>>,
}

impl InMemoryRbacStorage {
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
            users_by_id: RwLock::new(HashMap::new()),
            roles: RwLock::new(HashMap::new()),
            roles_by_id: RwLock::new(HashMap::new()),
            repository_permissions: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryRbacStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RbacStorage for InMemoryRbacStorage {
    async fn create_user(&self, user: User) -> Result<(), StorageError> {
        let mut users = self
            .users
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let mut users_by_id = self
            .users_by_id
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        users.insert(user.username.clone(), user.clone());
        users_by_id.insert(user.id, user);
        Ok(())
    }

    async fn get_user(&self, username: &str) -> Result<Option<User>, StorageError> {
        let users = self
            .users
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        Ok(users.get(username).cloned())
    }

    async fn get_user_by_id(&self, id: &Uuid) -> Result<Option<User>, StorageError> {
        let users_by_id = self
            .users_by_id
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        Ok(users_by_id.get(id).cloned())
    }

    async fn update_user(&self, user: User) -> Result<(), StorageError> {
        let mut users = self
            .users
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let mut users_by_id = self
            .users_by_id
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        users.insert(user.username.clone(), user.clone());
        users_by_id.insert(user.id, user);
        Ok(())
    }

    async fn delete_user(&self, username: &str) -> Result<(), StorageError> {
        let mut users = self
            .users
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let mut users_by_id = self
            .users_by_id
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        if let Some(user) = users.remove(username) {
            users_by_id.remove(&user.id);
        }
        Ok(())
    }

    async fn user_exists(&self, username: &str) -> Result<bool, StorageError> {
        let users = self
            .users
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        Ok(users.contains_key(username))
    }

    async fn list_users(&self) -> Result<Vec<User>, StorageError> {
        let users = self
            .users
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        Ok(users.values().cloned().collect())
    }

    async fn create_role(&self, role: Role) -> Result<(), StorageError> {
        let mut roles = self
            .roles
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let mut roles_by_id = self
            .roles_by_id
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        roles.insert(role.name.clone(), role.clone());
        roles_by_id.insert(role.id, role);
        Ok(())
    }

    async fn get_role(&self, name: &str) -> Result<Option<Role>, StorageError> {
        let roles = self
            .roles
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        Ok(roles.get(name).cloned())
    }

    async fn get_role_by_id(&self, id: &Uuid) -> Result<Option<Role>, StorageError> {
        let roles_by_id = self
            .roles_by_id
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        Ok(roles_by_id.get(id).cloned())
    }

    async fn update_role(&self, role: Role) -> Result<(), StorageError> {
        let mut roles = self
            .roles
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let mut roles_by_id = self
            .roles_by_id
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        roles.insert(role.name.clone(), role.clone());
        roles_by_id.insert(role.id, role);
        Ok(())
    }

    async fn delete_role(&self, name: &str) -> Result<(), StorageError> {
        let mut roles = self
            .roles
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let mut roles_by_id = self
            .roles_by_id
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        if let Some(role) = roles.remove(name) {
            roles_by_id.remove(&role.id);
        }
        Ok(())
    }

    async fn role_exists(&self, name: &str) -> Result<bool, StorageError> {
        let roles = self
            .roles
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        Ok(roles.contains_key(name))
    }

    async fn list_roles(&self) -> Result<Vec<Role>, StorageError> {
        let roles = self
            .roles
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        Ok(roles.values().cloned().collect())
    }

    async fn create_repository_permission(
        &self,
        permission: RepositoryPermission,
    ) -> Result<(), StorageError> {
        let mut repo_perms = self
            .repository_permissions
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let key = format!("{}:{}", permission.repository_id, permission.user_id);

        repo_perms
            .entry(key)
            .or_insert_with(Vec::new)
            .push(permission);
        Ok(())
    }

    async fn get_repository_permissions(
        &self,
        repository_id: &str,
        user_id: &Uuid,
    ) -> Result<Vec<RepositoryPermission>, StorageError> {
        let repo_perms = self
            .repository_permissions
            .read()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let key = format!("{}:{}", repository_id, user_id);

        Ok(repo_perms.get(&key).cloned().unwrap_or_default())
    }

    async fn delete_repository_permission(
        &self,
        repository_id: &str,
        user_id: &Uuid,
        permission: &Permission,
    ) -> Result<(), StorageError> {
        let mut repo_perms = self
            .repository_permissions
            .write()
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        let key = format!("{}:{}", repository_id, user_id);

        if let Some(perms) = repo_perms.get_mut(&key) {
            perms.retain(|rp| !rp.permissions.contains(permission));
            if perms.is_empty() {
                repo_perms.remove(&key);
            }
        }

        Ok(())
    }
}

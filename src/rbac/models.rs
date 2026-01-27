use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    RepositoryCreate,
    RepositoryRead,
    RepositoryWrite,
    RepositoryDelete,
    RepositoryAdmin,

    BranchCreate,
    BranchDelete,
    BranchProtect,

    UserManage,
    RoleManage,

    SystemAdmin,

    WebhookManage,
    SettingsManage,

    LfsLockCreate,
    LfsLockRead,
    LfsLockDelete,
    LfsLockForce,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub permissions: HashSet<Permission>,
    pub is_system_role: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub ssh_keys: Vec<String>,
    pub roles: HashSet<Uuid>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPermission {
    pub repository_id: String,
    pub user_id: Uuid,
    pub permissions: HashSet<Permission>,
    pub granted_by: Uuid,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Permission::RepositoryCreate => "RepositoryCreate",
            Permission::RepositoryRead => "RepositoryRead",
            Permission::RepositoryWrite => "RepositoryWrite",
            Permission::RepositoryDelete => "RepositoryDelete",
            Permission::RepositoryAdmin => "RepositoryAdmin",
            Permission::BranchCreate => "BranchCreate",
            Permission::BranchDelete => "BranchDelete",
            Permission::BranchProtect => "BranchProtect",
            Permission::UserManage => "UserManage",
            Permission::RoleManage => "RoleManage",
            Permission::SystemAdmin => "SystemAdmin",
            Permission::WebhookManage => "WebhookManage",
            Permission::SettingsManage => "SettingsManage",
            Permission::LfsLockCreate => "LfsLockCreate",
            Permission::LfsLockRead => "LfsLockRead",
            Permission::LfsLockDelete => "LfsLockDelete",
            Permission::LfsLockForce => "LfsLockForce",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for Permission {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "RepositoryCreate" => Ok(Permission::RepositoryCreate),
            "RepositoryRead" => Ok(Permission::RepositoryRead),
            "RepositoryWrite" => Ok(Permission::RepositoryWrite),
            "RepositoryDelete" => Ok(Permission::RepositoryDelete),
            "RepositoryAdmin" => Ok(Permission::RepositoryAdmin),
            "BranchCreate" => Ok(Permission::BranchCreate),
            "BranchDelete" => Ok(Permission::BranchDelete),
            "BranchProtect" => Ok(Permission::BranchProtect),
            "UserManage" => Ok(Permission::UserManage),
            "RoleManage" => Ok(Permission::RoleManage),
            "SystemAdmin" => Ok(Permission::SystemAdmin),
            "WebhookManage" => Ok(Permission::WebhookManage),
            "SettingsManage" => Ok(Permission::SettingsManage),
            "LfsLockCreate" => Ok(Permission::LfsLockCreate),
            "LfsLockRead" => Ok(Permission::LfsLockRead),
            "LfsLockDelete" => Ok(Permission::LfsLockDelete),
            "LfsLockForce" => Ok(Permission::LfsLockForce),
            other => Err(format!("Unknown permission: {}", other)),
        }
    }
}

impl Role {
    pub fn new(name: String, description: String, permissions: HashSet<Permission>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description,
            permissions,
            is_system_role: false,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn system_role(
        name: String,
        description: String,
        permissions: HashSet<Permission>,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description,
            permissions,
            is_system_role: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }

    pub fn add_permission(&mut self, permission: Permission) {
        self.permissions.insert(permission);
        self.updated_at = chrono::Utc::now();
    }

    pub fn remove_permission(&mut self, permission: &Permission) {
        self.permissions.remove(permission);
        self.updated_at = chrono::Utc::now();
    }
}

impl User {
    pub fn new(username: String, email: Option<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            username,
            email,
            display_name: None,
            ssh_keys: Vec::new(),
            roles: HashSet::new(),
            is_active: true,
            created_at: now,
            updated_at: now,
            last_login: None,
        }
    }

    pub fn assign_role(&mut self, role_id: Uuid) {
        self.roles.insert(role_id);
        self.updated_at = chrono::Utc::now();
    }

    pub fn revoke_role(&mut self, role_id: &Uuid) {
        self.roles.remove(role_id);
        self.updated_at = chrono::Utc::now();
    }

    pub fn has_role(&self, role_id: &Uuid) -> bool {
        self.roles.contains(role_id)
    }

    pub fn add_ssh_key(&mut self, ssh_key: String) {
        if !self.ssh_keys.contains(&ssh_key) {
            self.ssh_keys.push(ssh_key);
            self.updated_at = chrono::Utc::now();
        }
    }

    pub fn remove_ssh_key(&mut self, ssh_key: &str) {
        self.ssh_keys.retain(|key| key != ssh_key);
        self.updated_at = chrono::Utc::now();
    }

    pub fn set_last_login(&mut self) {
        self.last_login = Some(chrono::Utc::now());
        self.updated_at = chrono::Utc::now();
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = chrono::Utc::now();
    }

    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = chrono::Utc::now();
    }
}

pub fn create_default_roles() -> Vec<Role> {
    let mut admin_permissions = HashSet::new();
    admin_permissions.insert(Permission::RepositoryCreate);
    admin_permissions.insert(Permission::RepositoryRead);
    admin_permissions.insert(Permission::RepositoryWrite);
    admin_permissions.insert(Permission::RepositoryDelete);
    admin_permissions.insert(Permission::RepositoryAdmin);
    admin_permissions.insert(Permission::BranchCreate);
    admin_permissions.insert(Permission::BranchDelete);
    admin_permissions.insert(Permission::BranchProtect);
    admin_permissions.insert(Permission::UserManage);
    admin_permissions.insert(Permission::RoleManage);
    admin_permissions.insert(Permission::SystemAdmin);
    admin_permissions.insert(Permission::WebhookManage);
    admin_permissions.insert(Permission::SettingsManage);
    admin_permissions.insert(Permission::LfsLockCreate);
    admin_permissions.insert(Permission::LfsLockRead);
    admin_permissions.insert(Permission::LfsLockDelete);
    admin_permissions.insert(Permission::LfsLockForce);

    let mut developer_permissions = HashSet::new();
    developer_permissions.insert(Permission::RepositoryRead);
    developer_permissions.insert(Permission::RepositoryWrite);
    developer_permissions.insert(Permission::BranchCreate);
    developer_permissions.insert(Permission::LfsLockCreate);
    developer_permissions.insert(Permission::LfsLockRead);
    developer_permissions.insert(Permission::LfsLockDelete);

    let mut readonly_permissions = HashSet::new();
    readonly_permissions.insert(Permission::RepositoryRead);
    readonly_permissions.insert(Permission::LfsLockRead);

    vec![
        Role::system_role(
            "Administrator".to_string(),
            "Full system administration access".to_string(),
            admin_permissions,
        ),
        Role::system_role(
            "Developer".to_string(),
            "Standard developer access with read/write permissions".to_string(),
            developer_permissions,
        ),
        Role::system_role(
            "ReadOnly".to_string(),
            "Read-only access to repositories".to_string(),
            readonly_permissions,
        ),
    ]
}

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

    /// Manage replication targets, failover, and sync operations.
    ReplicationManage,

    /// Access compliance reports, retention policies, GDPR data.
    ComplianceManage,
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
            Permission::ReplicationManage => "ReplicationManage",
            Permission::ComplianceManage => "ComplianceManage",
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
            "ReplicationManage" => Ok(Permission::ReplicationManage),
            "ComplianceManage" => Ok(Permission::ComplianceManage),
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
    admin_permissions.insert(Permission::ReplicationManage);
    admin_permissions.insert(Permission::ComplianceManage);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::str::FromStr;

    // ── Permission Display / FromStr roundtrip ──

    #[test]
    fn permission_display_from_str_roundtrip() {
        let all = vec![
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
        ];
        for p in &all {
            let s = p.to_string();
            assert_eq!(Permission::from_str(&s).unwrap(), *p);
        }
    }

    #[test]
    fn permission_from_str_unknown() {
        assert!(Permission::from_str("NotARealPermission").is_err());
    }

    // ── Role ──

    #[test]
    fn role_new_is_not_system() {
        let role = Role::new(
            "custom".into(),
            "desc".into(),
            HashSet::from([Permission::RepositoryRead]),
        );
        assert!(!role.is_system_role);
        assert_eq!(role.name, "custom");
        assert!(role.has_permission(&Permission::RepositoryRead));
        assert!(!role.has_permission(&Permission::SystemAdmin));
    }

    #[test]
    fn role_system_role_is_system() {
        let role = Role::system_role(
            "admin".into(),
            "desc".into(),
            HashSet::from([Permission::SystemAdmin]),
        );
        assert!(role.is_system_role);
    }

    #[test]
    fn role_add_remove_permission() {
        let mut role = Role::new("r".into(), "d".into(), HashSet::new());
        assert!(!role.has_permission(&Permission::RepositoryWrite));

        role.add_permission(Permission::RepositoryWrite);
        assert!(role.has_permission(&Permission::RepositoryWrite));

        role.remove_permission(&Permission::RepositoryWrite);
        assert!(!role.has_permission(&Permission::RepositoryWrite));
    }

    #[test]
    fn role_id_unique() {
        let a = Role::new("a".into(), "d".into(), HashSet::new());
        let b = Role::new("b".into(), "d".into(), HashSet::new());
        assert_ne!(a.id, b.id);
    }

    // ── User ──

    #[test]
    fn user_new_defaults() {
        let user = User::new("alice".into(), Some("alice@example.com".into()));
        assert_eq!(user.username, "alice");
        assert_eq!(user.email.as_deref(), Some("alice@example.com"));
        assert!(user.is_active);
        assert!(user.ssh_keys.is_empty());
        assert!(user.roles.is_empty());
        assert!(user.last_login.is_none());
    }

    #[test]
    fn user_assign_revoke_role() {
        let mut user = User::new("bob".into(), None);
        let role_id = Uuid::new_v4();

        assert!(!user.has_role(&role_id));
        user.assign_role(role_id);
        assert!(user.has_role(&role_id));

        user.revoke_role(&role_id);
        assert!(!user.has_role(&role_id));
    }

    #[test]
    fn user_ssh_key_management() {
        let mut user = User::new("carol".into(), None);
        let key = "ssh-ed25519 AAAA...".to_string();

        user.add_ssh_key(key.clone());
        assert_eq!(user.ssh_keys.len(), 1);

        // Duplicate insert is a no-op
        user.add_ssh_key(key.clone());
        assert_eq!(user.ssh_keys.len(), 1);

        user.remove_ssh_key(&key);
        assert!(user.ssh_keys.is_empty());
    }

    #[test]
    fn user_activate_deactivate() {
        let mut user = User::new("dave".into(), None);
        assert!(user.is_active);

        user.deactivate();
        assert!(!user.is_active);

        user.activate();
        assert!(user.is_active);
    }

    #[test]
    fn user_set_last_login() {
        let mut user = User::new("eve".into(), None);
        assert!(user.last_login.is_none());
        user.set_last_login();
        assert!(user.last_login.is_some());
    }

    // ── Default roles ──

    #[test]
    fn default_roles_are_system_roles() {
        let roles = create_default_roles();
        assert_eq!(roles.len(), 3);

        let names: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Administrator"));
        assert!(names.contains(&"Developer"));
        assert!(names.contains(&"ReadOnly"));

        for role in &roles {
            assert!(role.is_system_role);
        }
    }

    #[test]
    fn admin_has_all_permissions() {
        let roles = create_default_roles();
        let admin = roles.iter().find(|r| r.name == "Administrator").unwrap();
        // Admin should have every variant
        assert!(admin.has_permission(&Permission::SystemAdmin));
        assert!(admin.has_permission(&Permission::RepositoryDelete));
        assert!(admin.has_permission(&Permission::LfsLockForce));
    }

    #[test]
    fn developer_has_no_admin() {
        let roles = create_default_roles();
        let dev = roles.iter().find(|r| r.name == "Developer").unwrap();
        assert!(!dev.has_permission(&Permission::SystemAdmin));
        assert!(!dev.has_permission(&Permission::UserManage));
        assert!(dev.has_permission(&Permission::RepositoryWrite));
    }

    #[test]
    fn readonly_minimal() {
        let roles = create_default_roles();
        let ro = roles.iter().find(|r| r.name == "ReadOnly").unwrap();
        assert!(ro.has_permission(&Permission::RepositoryRead));
        assert!(ro.has_permission(&Permission::LfsLockRead));
        assert!(!ro.has_permission(&Permission::RepositoryWrite));
        assert!(!ro.has_permission(&Permission::LfsLockCreate));
    }

    // ── Serialization ──

    #[test]
    fn permission_json_roundtrip() {
        let p = Permission::RepositoryWrite;
        let json = serde_json::to_string(&p).unwrap();
        let back: Permission = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn role_json_roundtrip() {
        let role = Role::new(
            "test".into(),
            "desc".into(),
            HashSet::from([Permission::RepositoryRead, Permission::RepositoryWrite]),
        );
        let json = serde_json::to_string(&role).unwrap();
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role.id, back.id);
        assert_eq!(role.name, back.name);
        assert_eq!(role.permissions, back.permissions);
    }

    #[test]
    fn user_json_roundtrip() {
        let mut user = User::new("alice".into(), Some("a@b.com".into()));
        user.assign_role(Uuid::new_v4());
        user.add_ssh_key("ssh-key".into());
        let json = serde_json::to_string(&user).unwrap();
        let back: User = serde_json::from_str(&json).unwrap();
        assert_eq!(user.id, back.id);
        assert_eq!(user.username, back.username);
        assert_eq!(user.roles, back.roles);
        assert_eq!(user.ssh_keys, back.ssh_keys);
    }
}

//! Multi-tenancy support for Skynet v2 Git Operations Plugin

pub mod namespace;
pub mod permissions;
pub mod quota;

pub use namespace::TenantNamespace;
pub use permissions::{Permission, Role};
pub use quota::TenantQuota;

/// Tenant context
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: String,
    pub user_id: String,
    pub roles: Vec<Role>,
    pub api_token: Option<String>,
}

impl TenantContext {
    /// Create a new tenant context
    pub fn new(tenant_id: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            user_id: user_id.into(),
            roles: vec![Role::ReadOnly],
            api_token: None,
        }
    }

    /// Check if user has permission
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.roles.iter().any(|role| role.has_permission(permission))
    }
}

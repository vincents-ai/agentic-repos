//! Permission and role management for multi-tenancy

use serde::{Deserialize, Serialize};

/// User roles
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    /// Full access
    Owner,
    /// Repository management and branch protection
    Maintainer,
    /// Can push and pull
    Contributor,
    /// Can only pull
    ReadOnly,
}

/// Permissions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    /// Create repositories
    CreateRepository,
    /// Delete repositories
    DeleteRepository,
    /// Push to repository
    Push,
    /// Pull from repository
    Pull,
    /// Manage SSH keys
    ManageSshKeys,
    /// View logs and metrics
    ViewMetrics,
}

impl Role {
    /// Check if role has permission
    pub fn has_permission(&self, permission: &Permission) -> bool {
        match self {
            Role::Owner => true,
            Role::Maintainer => matches!(
                permission,
                Permission::Push
                    | Permission::Pull
                    | Permission::ManageSshKeys
                    | Permission::ViewMetrics
                    | Permission::DeleteRepository
            ),
            Role::Contributor => {
                matches!(permission, Permission::Push | Permission::Pull)
            }
            Role::ReadOnly => matches!(permission, Permission::Pull),
        }
    }
}

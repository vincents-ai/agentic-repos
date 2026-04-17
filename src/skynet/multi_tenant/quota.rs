//! Tenant quota management

use serde::{Deserialize, Serialize};

/// Tenant quota
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    pub tenant_id: String,
    pub max_storage_bytes: u64,
    pub max_repositories: u64,
    pub max_users: u64,
}

impl TenantQuota {
    /// Create a new tenant quota
    pub fn new(
        tenant_id: String,
        max_storage_bytes: u64,
        max_repositories: u64,
        max_users: u64,
    ) -> Self {
        Self {
            tenant_id,
            max_storage_bytes,
            max_repositories,
            max_users,
        }
    }
}

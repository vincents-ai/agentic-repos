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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_fields() {
        let q = TenantQuota::new("t1".into(), 1000, 10, 5);
        assert_eq!(q.tenant_id, "t1");
        assert_eq!(q.max_storage_bytes, 1000);
        assert_eq!(q.max_repositories, 10);
        assert_eq!(q.max_users, 5);
    }

    #[test]
    fn serialization_roundtrip() {
        let q = TenantQuota::new("t1".into(), 9999, 50, 20);
        let json = serde_json::to_string(&q).unwrap();
        let parsed: TenantQuota = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tenant_id, q.tenant_id);
        assert_eq!(parsed.max_storage_bytes, q.max_storage_bytes);
    }
}

//! Tenant namespace isolation

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Tenant namespace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantNamespace {
    pub tenant_id: String,
    pub base_path: PathBuf,
}

impl TenantNamespace {
    /// Create a new tenant namespace
    pub fn new(tenant_id: String, base_path: PathBuf) -> Self {
        Self {
            tenant_id,
            base_path,
        }
    }

    /// Get repository path
    pub fn repo_path(&self, repo_name: &str) -> PathBuf {
        self.base_path
            .join(&self.tenant_id)
            .join("repos")
            .join(format!("{}.git", repo_name))
    }
}

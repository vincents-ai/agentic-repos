//! Database models for Skynet v2 Git Operations Plugin
//!
//! This module provides SQLx models for:
//! - Repository metadata
//! - SSH keys
//! - User management
//! - Audit logging

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Repository metadata stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryModel {
    pub id: Uuid,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub is_private: bool,
    pub default_branch: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Repository creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRepositoryRequest {
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub is_private: bool,
    pub default_branch: Option<String>,
}

/// SSH key stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKeyModel {
    pub id: Uuid,
    pub tenant_id: String,
    pub user_id: Uuid,
    pub name: String,
    pub public_key: String,
    pub key_type: String,
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// SSH key creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSshKeyRequest {
    pub tenant_id: String,
    pub user_id: Uuid,
    pub name: String,
    pub public_key: String,
    pub key_type: String,
    pub fingerprint: String,
    pub expires_at: Option<DateTime<Utc>>,
}

/// User account model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModel {
    pub id: Uuid,
    pub tenant_id: String,
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// User creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub tenant_id: String,
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
}

/// Audit log entry for git operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogModel {
    pub id: Uuid,
    pub tenant_id: String,
    pub user_id: Option<Uuid>,
    pub operation: String,
    pub resource_type: String,
    pub resource_id: String,
    pub action: String,
    pub status: String,
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Audit log creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAuditLogRequest {
    pub tenant_id: String,
    pub user_id: Option<Uuid>,
    pub operation: String,
    pub resource_type: String,
    pub resource_id: String,
    pub action: String,
    pub status: String,
    pub details: Option<String>,
    pub ip_address: Option<String>,
}

/// Database migration scripts
pub mod migrations {
    pub const CREATE_REPOSITORIES_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS repositories (
            id UUID PRIMARY KEY,
            tenant_id VARCHAR(255) NOT NULL,
            name VARCHAR(255) NOT NULL,
            description TEXT,
            owner_id UUID NOT NULL,
            is_private BOOLEAN NOT NULL DEFAULT false,
            default_branch VARCHAR(255) NOT NULL DEFAULT 'main',
            created_at TIMESTAMP WITH TIME ZONE NOT NULL,
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
            deleted_at TIMESTAMP WITH TIME ZONE,
            UNIQUE(tenant_id, name),
            INDEX idx_tenant_id (tenant_id),
            INDEX idx_owner_id (owner_id),
            INDEX idx_deleted_at (deleted_at)
        );
    "#;

    pub const CREATE_SSH_KEYS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS ssh_keys (
            id UUID PRIMARY KEY,
            tenant_id VARCHAR(255) NOT NULL,
            user_id UUID NOT NULL,
            name VARCHAR(255) NOT NULL,
            public_key TEXT NOT NULL,
            key_type VARCHAR(50) NOT NULL,
            fingerprint VARCHAR(255) NOT NULL UNIQUE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL,
            expires_at TIMESTAMP WITH TIME ZONE,
            last_used_at TIMESTAMP WITH TIME ZONE,
            is_active BOOLEAN NOT NULL DEFAULT true,
            deleted_at TIMESTAMP WITH TIME ZONE,
            INDEX idx_tenant_id (tenant_id),
            INDEX idx_user_id (user_id),
            INDEX idx_is_active (is_active),
            INDEX idx_deleted_at (deleted_at)
        );
    "#;

    pub const CREATE_USERS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY,
            tenant_id VARCHAR(255) NOT NULL,
            username VARCHAR(255) NOT NULL,
            email VARCHAR(255) NOT NULL,
            password_hash TEXT,
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL,
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
            deleted_at TIMESTAMP WITH TIME ZONE,
            UNIQUE(tenant_id, username),
            UNIQUE(tenant_id, email),
            INDEX idx_tenant_id (tenant_id),
            INDEX idx_is_active (is_active),
            INDEX idx_deleted_at (deleted_at)
        );
    "#;

    pub const CREATE_AUDIT_LOGS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS audit_logs (
            id UUID PRIMARY KEY,
            tenant_id VARCHAR(255) NOT NULL,
            user_id UUID,
            operation VARCHAR(255) NOT NULL,
            resource_type VARCHAR(255) NOT NULL,
            resource_id VARCHAR(255) NOT NULL,
            action VARCHAR(255) NOT NULL,
            status VARCHAR(50) NOT NULL,
            details TEXT,
            ip_address VARCHAR(45),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL,
            INDEX idx_tenant_id (tenant_id),
            INDEX idx_user_id (user_id),
            INDEX idx_operation (operation),
            INDEX idx_resource_type (resource_type),
            INDEX idx_created_at (created_at)
        );
    "#;

    pub const CREATE_REPOSITORY_QUOTAS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS repository_quotas (
            id UUID PRIMARY KEY,
            tenant_id VARCHAR(255) NOT NULL UNIQUE,
            storage_limit_gb BIGINT NOT NULL,
            storage_used_gb BIGINT NOT NULL DEFAULT 0,
            max_repositories INT NOT NULL,
            current_repositories INT NOT NULL DEFAULT 0,
            max_users INT NOT NULL,
            current_users INT NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL,
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
            INDEX idx_tenant_id (tenant_id)
        );
    "#;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_serialization() {
        let repo = RepositoryModel {
            id: Uuid::new_v4(),
            tenant_id: "tenant-1".to_string(),
            name: "test-repo".to_string(),
            description: Some("Test repository".to_string()),
            owner_id: Uuid::new_v4(),
            is_private: false,
            default_branch: "main".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        };

        let json = serde_json::to_string(&repo).unwrap();
        let deserialized: RepositoryModel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, repo.name);
    }

    #[test]
    fn test_ssh_key_model_serialization() {
        let key = SshKeyModel {
            id: Uuid::new_v4(),
            tenant_id: "tenant-1".to_string(),
            user_id: Uuid::new_v4(),
            name: "my-key".to_string(),
            public_key: "ssh-rsa AAAAB3NzaC1yc2E...".to_string(),
            key_type: "ssh-rsa".to_string(),
            fingerprint: "SHA256:abc123".to_string(),
            created_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
            is_active: true,
            deleted_at: None,
        };

        let json = serde_json::to_string(&key).unwrap();
        let deserialized: SshKeyModel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.key_type, key.key_type);
    }
}

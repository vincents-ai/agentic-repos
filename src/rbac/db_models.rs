use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DbUser {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_active: bool,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DbRole {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Represents a single permission row in repository_permissions table
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DbRepositoryPermission {
    pub id: Uuid,
    pub repository_id: String,
    pub user_id: Uuid,
    pub permission_id: String,
    pub granted_at: DateTime<Utc>,
    pub granted_by: Uuid,
}

// Conversions to domain models
impl From<DbUser> for crate::rbac::models::User {
    fn from(d: DbUser) -> Self {
        crate::rbac::models::User {
            id: d.id,
            username: d.username,
            email: d.email,
            display_name: None,
            ssh_keys: Vec::new(),
            roles: HashSet::new(),
            is_active: d.is_active,
            created_at: d.created_at,
            updated_at: d.updated_at,
            last_login: None,
        }
    }
}

impl From<DbRole> for crate::rbac::models::Role {
    fn from(d: DbRole) -> Self {
        crate::rbac::models::Role {
            id: d.id,
            name: d.name,
            description: d.description,
            permissions: HashSet::new(),
            is_system_role: false,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

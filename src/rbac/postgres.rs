use async_trait::async_trait;
use crate::rbac::models::{Permission, RepositoryPermission, Role, User};
use crate::rbac::storage::{RbacStorage, StorageError};
use crate::rbac::db_models::{DbUser, DbRole, DbRepositoryPermission};
use sqlx::PgPool;
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

pub struct PostgresRbacStorage {
    pool: PgPool,
}

impl PostgresRbacStorage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RbacStorage for PostgresRbacStorage {
    async fn create_user(&self, user: User) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, created_at, updated_at, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (username) DO NOTHING
            "#,
        )
        .bind(user.id)
        .bind(&user.username)
        .bind(&user.email)
        // domain User doesn't carry password_hash; store NULL
        .bind(None::<String>)
        .bind(&user.created_at)
        .bind(&user.updated_at)
        .bind(user.is_active)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }

    async fn get_user(&self, username: &str) -> Result<Option<User>, StorageError> {
        let db = sqlx::query_as::<_, DbUser>(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(db.map(|d| d.into()))
    }

    async fn get_user_by_id(&self, id: &Uuid) -> Result<Option<User>, StorageError> {
        let db = sqlx::query_as::<_, DbUser>(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(db.map(|d| d.into()))
    }

    async fn update_user(&self, user: User) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            UPDATE users
            SET email = $2, password_hash = $3, updated_at = $4, is_active = $5
            WHERE username = $1
            "#,
        )
        .bind(&user.username)
        .bind(&user.email)
        // domain User doesn't carry password_hash; do not change it
        .bind(None::<String>)
        .bind(&user.updated_at)
        .bind(user.is_active)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }

    async fn delete_user(&self, username: &str) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            DELETE FROM users
            WHERE username = $1
            "#,
        )
        .bind(username)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }

    async fn user_exists(&self, username: &str) -> Result<bool, StorageError> {
        let result = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM users WHERE username = $1
            )
            "#,
        )
        .bind(username)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(result)
    }

    async fn list_users(&self) -> Result<Vec<User>, StorageError> {
        let db_users = sqlx::query_as::<_, DbUser>(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(db_users.into_iter().map(|d| d.into()).collect())
    }

    async fn create_role(&self, role: Role) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO roles (id, name, description, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (name) DO NOTHING
            "#,
        )
        .bind(role.id)
        .bind(&role.name)
        .bind(&role.description)
        .bind(&role.created_at)
        .bind(&role.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }

    async fn get_role(&self, name: &str) -> Result<Option<Role>, StorageError> {
        let db = sqlx::query_as::<_, DbRole>(
            r#"
            SELECT id, name, description, created_at, updated_at
            FROM roles
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(db.map(|d| d.into()))
    }

    async fn get_role_by_id(&self, id: &Uuid) -> Result<Option<Role>, StorageError> {
        let db = sqlx::query_as::<_, DbRole>(
            r#"
            SELECT id, name, description, created_at, updated_at
            FROM roles
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(db.map(|d| d.into()))
    }

    async fn update_role(&self, role: Role) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            UPDATE roles
            SET description = $2, updated_at = $3
            WHERE name = $1
            "#,
        )
        .bind(&role.name)
        .bind(&role.description)
        .bind(&role.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }

    async fn delete_role(&self, name: &str) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            DELETE FROM roles
            WHERE name = $1
            "#,
        )
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }

    async fn role_exists(&self, name: &str) -> Result<bool, StorageError> {
        let result = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM roles WHERE name = $1
            )
            "#,
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(result)
    }

    async fn list_roles(&self) -> Result<Vec<Role>, StorageError> {
        let db_roles = sqlx::query_as::<_, DbRole>(
            r#"
            SELECT id, name, description, created_at, updated_at
            FROM roles
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(db_roles.into_iter().map(|d| d.into()).collect())
    }

    async fn create_repository_permission(
        &self,
        permission: RepositoryPermission,
    ) -> Result<(), StorageError> {
        let key = format!("{}:{}", permission.repository_id, permission.user_id);
        
        for perm in &permission.permissions {
            sqlx::query(
                r#"
                INSERT INTO repository_permissions (id, repository_id, user_id, permission_id, granted_at, granted_by)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (repository_id, user_id, permission_id) DO NOTHING
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(&permission.repository_id)
            .bind(&permission.user_id)
            .bind((*perm).to_string())
            .bind(chrono::Utc::now())
            .bind(permission.granted_by.clone())
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
        }

        Ok(())
    }

    async fn get_repository_permissions(
        &self,
        repository_id: &str,
        user_id: &Uuid,
    ) -> Result<Vec<RepositoryPermission>, StorageError> {
        let rows = sqlx::query_as::<_, DbRepositoryPermission>(
            r#"
            SELECT id, repository_id, user_id, permission_id, granted_at, granted_by
            FROM repository_permissions
            WHERE repository_id = $1 AND user_id = $2
            "#,
        )
        .bind(repository_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        // Group rows by (repository_id, user_id) into RepositoryPermission structs
        let mut map: HashMap<(String, Uuid), RepositoryPermission> = HashMap::new();
        for r in rows {
            let key = (r.repository_id.clone(), r.user_id);
            let perm = match Permission::from_str(&r.permission_id) {
                Ok(p) => p,
                Err(_) => continue, // skip unknown permissions
            };

            map.entry(key.clone()).and_modify(|rp| {
                rp.permissions.insert(perm.clone());
            }).or_insert_with(|| RepositoryPermission {
                repository_id: key.0.clone(),
                user_id: key.1,
                permissions: {
                    let mut s = std::collections::HashSet::new();
                    s.insert(perm.clone());
                    s
                },
                granted_by: r.granted_by,
                granted_at: r.granted_at,
                expires_at: None,
            });
        }

        Ok(map.into_iter().map(|(_, v)| v).collect())
    }

    async fn delete_repository_permission(
        &self,
        repository_id: &str,
        user_id: &Uuid,
        permission: &Permission,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            DELETE FROM repository_permissions
            WHERE repository_id = $1 AND user_id = $2 AND permission_id = $3
            "#,
        )
        .bind(repository_id)
        .bind(user_id)
        .bind(permission.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }
}

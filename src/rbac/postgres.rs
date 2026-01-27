use async_trait::async_trait;
use crate::rbac::models::{Permission, RepositoryPermission, Role, User};
use crate::rbac::storage::{RbacStorage, StorageError};
use sqlx::PgPool;
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
        .bind(&user.password_hash)
        .bind(&user.created_at)
        .bind(&user.updated_at)
        .bind(user.is_active)
        .execute(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

        Ok(())
    }

    async fn get_user(&self, username: &str) -> Result<Option<User>, StorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

        Ok(result)
    }

    async fn get_user_by_id(&self, id: &Uuid) -> Result<Option<User>, StorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

        Ok(result)
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
        .bind(&user.password_hash)
        .bind(&user.updated_at)
        .bind(user.is_active)
        .execute(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

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
        .map_err(StorageError::OperationFailed)?;

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
        .map_err(StorageError::OperationFailed)?;

        Ok(result)
    }

    async fn list_users(&self) -> Result<Vec<User>, StorageError> {
        let users = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

        Ok(users)
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
        .map_err(StorageError::OperationFailed)?;

        Ok(())
    }

    async fn get_role(&self, name: &str) -> Result<Option<Role>, StorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, name, description, created_at, updated_at
            FROM roles
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

        Ok(result)
    }

    async fn get_role_by_id(&self, id: &Uuid) -> Result<Option<Role>, StorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, name, description, created_at, updated_at
            FROM roles
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

        Ok(result)
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
        .map_err(StorageError::OperationFailed)?;

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
        .map_err(StorageError::OperationFailed)?;

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
        .map_err(StorageError::OperationFailed)?;

        Ok(result)
    }

    async fn list_roles(&self) -> Result<Vec<Role>, StorageError> {
        let roles = sqlx::query_as(
            r#"
            SELECT id, name, description, created_at, updated_at
            FROM roles
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

        Ok(roles)
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
            .bind(perm)
            .bind(chrono::Utc::now())
            .bind(permission.granted_by.as_ref())
            .execute(&self.pool)
            .await
            .map_err(StorageError::OperationFailed)?;
        }

        Ok(())
    }

    async fn get_repository_permissions(
        &self,
        repository_id: &str,
        user_id: &Uuid,
    ) -> Result<Vec<RepositoryPermission>, StorageError> {
        let permissions = sqlx::query_as(
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
        .map_err(StorageError::OperationFailed)?;

        Ok(permissions)
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
        .bind(permission)
        .execute(&self.pool)
        .await
        .map_err(StorageError::OperationFailed)?;

        Ok(())
    }
}

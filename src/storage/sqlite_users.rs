use super::user_trait::{CreateUser, UpdateUser, User, UserStorage, UserStorageError};
use crate::lfs::audit::{AuditEvent as LfsAuditEvent, AuditEventType as LfsAuditEventType};
use crate::rbac::models::{Permission, RepositoryPermission, Role, User as RbacUser};
use crate::rbac::storage::{RbacStorage, StorageError as RbacStorageError};
use sqlx::sqlite::SqlitePool;
use uuid::Uuid;

pub struct SqliteUserStorage {
    pool: SqlitePool,
}

impl SqliteUserStorage {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl UserStorage for SqliteUserStorage {
    async fn create(&self, user: CreateUser) -> Result<User, UserStorageError> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let row = sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, created_at, updated_at, is_active)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            RETURNING id, username, email, password_hash, created_at, updated_at, is_active
            "#,
        )
        .bind(id.to_string())
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(1)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = &e {
                if db_err.is_unique_violation() {
                    if db_err.constraint().unwrap_or_default().contains("users_username") {
                        return UserStorageError::AlreadyExists(format!("Username '{}' already exists", user.username));
                    }
                    if db_err.constraint().unwrap_or_default().contains("users_email") {
                        return UserStorageError::AlreadyExists(format!("Email '{}' already exists", user.email));
                    }
                }
            }
            UserStorageError::Database(e.to_string())
        })?;

        Ok(User {
            id: Uuid::parse_str(&row.get("id")).unwrap(),
            username: row.get("username"),
            email: row.get("email"),
            password_hash: row.get("password_hash"),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get("created_at")).unwrap(),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get("updated_at")).unwrap(),
            is_active: row.get("is_active") == 1,
        })
    }

    async fn get_by_username(&self, username: &str) -> Result<Option<User>, UserStorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE username = ?1 AND is_active = 1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(UserStorageError::Database)?;

        Ok(result)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<User>, UserStorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE id = ?1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(UserStorageError::Database)?;

        Ok(result)
    }

    async fn get_by_email(&self, email: &str) -> Result<Option<User>, UserStorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE email = ?1 AND is_active = 1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(UserStorageError::Database)?;

        Ok(result)
    }

    async fn update(&self, username: &str, update: UpdateUser) -> Result<User, UserStorageError> {
        let now = chrono::Utc::now();

        let row = sqlx::query(
            r#"
            UPDATE users
            SET email = COALESCE(?2, email),
                password_hash = COALESCE(?3, password_hash),
                is_active = COALESCE(?4, is_active),
                updated_at = ?5
            WHERE username = ?1
            RETURNING id, username, email, password_hash, created_at, updated_at, is_active
            "#,
        )
        .bind(username)
        .bind(&update.email)
        .bind(&update.password_hash)
        .bind(update.is_active)
        .bind(now.to_rfc3339())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::RowNotFound = e {
                return UserStorageError::NotFound(username.to_string());
            }
            UserStorageError::Database(e.to_string())
        })?;

        Ok(User {
            id: Uuid::parse_str(&row.get("id")).unwrap(),
            username: row.get("username"),
            email: row.get("email"),
            password_hash: row.get("password_hash"),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get("created_at")).unwrap(),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get("updated_at")).unwrap(),
            is_active: row.get("is_active") == 1,
        })
    }

    async fn delete(&self, username: &str) -> Result<(), UserStorageError> {
        let result = sqlx::query(
            r#"
            DELETE FROM users
            WHERE username = ?1
            "#,
        )
        .bind(username)
        .execute(&self.pool)
        .await
        .map_err(UserStorageError::Database)?;

        if result.rows_affected() == 0 {
            return Err(UserStorageError::NotFound(username.to_string()));
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<User>, UserStorageError> {
        let users = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(UserStorageError::Database)?;

        Ok(users)
    }

    async fn exists(&self, username: &str) -> Result<bool, UserStorageError> {
        let result = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM users WHERE username = ?1
            )
            "#,
        )
        .bind(username)
        .fetch_one(&self.pool)
        .await
        .map_err(UserStorageError::Database)?;

        Ok(result)
    }

    async fn count(&self) -> Result<i64, UserStorageError> {
        let count = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM users
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(UserStorageError::Database)?;

        Ok(count)
    }
}

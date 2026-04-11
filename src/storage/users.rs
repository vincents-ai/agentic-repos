use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct CreateUser {
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateUser {
    pub username: String,
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Error)]
pub enum UserStorageError {
    #[error("User not found: {0}")]
    NotFound(String),

    #[error("User already exists: {0}")]
    AlreadyExists(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Validation error: {0}")]
    Validation(String),
}

pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user: CreateUser) -> Result<User, UserStorageError> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let row = sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, created_at, updated_at, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, username, email, password_hash, created_at, updated_at, is_active
            "#,
        )
        .bind(id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(now)
        .bind(now)
        .bind(true)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.is_unique_violation() {
                    if db_err.constraint().unwrap_or_default().contains("users_username") {
                        return UserStorageError::AlreadyExists(format!("Username '{}' already exists", user.username));
                    }
                    if db_err.constraint().unwrap_or_default().contains("users_email") {
                        return UserStorageError::AlreadyExists(format!("Email '{}' already exists", user.email));
                    }
                }
            }
            UserStorageError::Database(e)
        })?;

        Ok(User {
            id: row.get("id"),
            username: row.get("username"),
            email: row.get("email"),
            password_hash: row.get("password_hash"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            is_active: row.get("is_active"),
        })
    }

    pub async fn get_by_username(&self, username: &str) -> Result<Option<User>, UserStorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE username = $1 AND is_active = true
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserStorageError::Database(e))?;

        Ok(result)
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<User>, UserStorageError> {
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
        .map_err(|e| UserStorageError::Database(e))?;

        Ok(result)
    }

    pub async fn get_by_email(&self, email: &str) -> Result<Option<User>, UserStorageError> {
        let result = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            WHERE email = $1 AND is_active = true
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserStorageError::Database(e))?;

        Ok(result)
    }

    pub async fn update(
        &self,
        username: &str,
        update: UpdateUser,
    ) -> Result<User, UserStorageError> {
        let now = Utc::now();

        let row = sqlx::query(
            r#"
            UPDATE users
            SET email = COALESCE($2, email),
                password_hash = COALESCE($3, password_hash),
                is_active = COALESCE($4, is_active),
                updated_at = $5
            WHERE username = $1
            RETURNING id, username, email, password_hash, created_at, updated_at, is_active
            "#,
        )
        .bind(username)
        .bind(&update.email)
        .bind(&update.password_hash)
        .bind(update.is_active)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::RowNotFound = e {
                return UserStorageError::NotFound(username.to_string());
            }
            UserStorageError::Database(e)
        })?;

        Ok(User {
            id: row.get("id"),
            username: row.get("username"),
            email: row.get("email"),
            password_hash: row.get("password_hash"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            is_active: row.get("is_active"),
        })
    }

    pub async fn delete(&self, username: &str) -> Result<(), UserStorageError> {
        let result = sqlx::query(
            r#"
            DELETE FROM users
            WHERE username = $1
            "#,
        )
        .bind(username)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(UserStorageError::NotFound(username.to_string()));
        }

        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<User>, UserStorageError> {
        let users = sqlx::query_as(
            r#"
            SELECT id, username, email, password_hash, created_at, updated_at, is_active
            FROM users
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| UserStorageError::Database(e))?;

        Ok(users)
    }

    pub async fn exists(&self, username: &str) -> Result<bool, UserStorageError> {
        let result = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM users WHERE username = $1
            )
            "#,
        )
        .bind(username)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn count(&self) -> Result<i64, UserStorageError> {
        let count = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM users
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }
}

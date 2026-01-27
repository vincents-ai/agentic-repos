use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use thiserror::Error;

#[derive(Debug, Clone)]
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
    Database(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
}

#[async_trait]
pub trait UserStorage: Send + Sync {
    async fn create(&self, user: CreateUser) -> Result<User, UserStorageError>;
    async fn get_by_username(&self, username: &str) -> Result<Option<User>, UserStorageError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<User>, UserStorageError>;
    async fn get_by_email(&self, email: &str) -> Result<Option<User>, UserStorageError>;
    async fn update(&self, username: &str, update: UpdateUser) -> Result<User, UserStorageError>;
    async fn delete(&self, username: &str) -> Result<(), UserStorageError>;
    async fn list(&self) -> Result<Vec<User>, UserStorageError>;
    async fn exists(&self, username: &str) -> Result<bool, UserStorageError>;
    async fn count(&self) -> Result<i64, UserStorageError>;
}

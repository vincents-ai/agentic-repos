use actix_web::HttpResponse;
use serde::{Deserialize, Serialize};

impl actix_web::ResponseError for ApiError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(
            actix_web::http::StatusCode::from_u16(self.code)
                .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
        )
        .json(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub owner: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub size: u64,
    pub clone_url_ssh: String,
    pub clone_url_http: Option<String>,
    pub default_branch: String,
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRepositoryRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_private: Option<bool>,
}

impl CreateRepositoryRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Repository name must be 1-100 characters".to_string());
        }
        if let Some(ref desc) = self.description {
            if desc.len() > 1000 {
                return Err("Description must be under 1000 characters".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRepositoryRequest {
    pub description: Option<String>,
    pub is_private: Option<bool>,
}

impl UpdateRepositoryRequest {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref desc) = self.description {
            if desc.len() > 1000 {
                return Err("Description must be under 1000 characters".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryList {
    pub repositories: Vec<Repository>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub email: String,
    pub created_at: String,
    pub updated_at: String,
    pub repository_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub ssh_public_key: Option<String>,
}

impl CreateUserRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.username.is_empty() || self.username.len() > 100 {
            return Err("Username must be 1-100 characters".to_string());
        }
        if self.email.is_empty() || self.email.len() > 254 {
            return Err("Email must be 1-254 characters".to_string());
        }
        if !self.email.contains('@') {
            return Err("Invalid email format".to_string());
        }
        if let Some(ref key) = self.ssh_public_key {
            if key.len() > 100000 {
                return Err("SSH key too large".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    pub ssh_public_key: Option<String>,
}

impl UpdateUserRequest {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref email) = self.email {
            if email.is_empty() || email.len() > 254 {
                return Err("Email must be 1-254 characters".to_string());
            }
            if !email.contains('@') {
                return Err("Invalid email format".to_string());
            }
        }
        if let Some(ref key) = self.ssh_public_key {
            if key.len() > 100000 {
                return Err("SSH key too large".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub author: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchList {
    pub branches: Vec<Branch>,
    pub total: usize,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.error, self.message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
    pub code: u16,
}

impl ApiError {
    pub fn new(error: &str, message: &str, code: u16) -> Self {
        Self {
            error: error.to_string(),
            message: message.to_string(),
            code,
        }
    }

    pub fn bad_request(message: &str) -> Self {
        Self::new("bad_request", message, 400)
    }

    pub fn not_found(message: &str) -> Self {
        Self::new("not_found", message, 404)
    }

    pub fn conflict(message: &str) -> Self {
        Self::new("conflict", message, 409)
    }

    pub fn internal_error(message: &str) -> Self {
        // Log full error server-side for debugging
        tracing::error!("Internal error: {}", message);
        // Return generic message to client — don't leak internals
        Self::new("internal_server_error", "An internal error occurred", 500)
    }
}

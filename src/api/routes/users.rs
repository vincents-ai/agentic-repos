use crate::api::models::{ApiError, CreateUserRequest, UpdateUserRequest, User as ApiUser};
use crate::git::GitService;
use crate::storage::users::{CreateUser, UpdateUser as StorageUpdateUser, UserRepository, User};
use actix_web::{web, HttpResponse, Result as ActixResult};
use std::sync::Arc;
use bcrypt::{hash, verify, DEFAULT_COST};
use uuid::Uuid;

pub fn configure_user_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(list_users))
        .route("", web::post().to(create_user))
        .route("/{username}", web::get().to(get_user))
        .route("/{username}", web::put().to(update_user))
        .route("/{username}", web::delete().to(delete_user))
        .route(
            "/{username}/repositories",
            web::get().to(list_user_repositories),
        );
}

pub async fn list_users(
    pool: web::Data<sqlx::PgPool>,
) -> ActixResult<HttpResponse> {
    let repo = UserRepository::new(pool.get_ref().clone());
    
    match repo.list().await {
        Ok(users) => {
            let api_users: Vec<ApiUser> = users
                .into_iter()
                .map(|u| ApiUser {
                    username: u.username.clone(),
                    email: u.email.clone(),
                    created_at: u.created_at.to_rfc3339(),
                    updated_at: u.updated_at.to_rfc3339(),
                    repository_count: 0, // TODO: Implement repository counting
                })
                .collect();
            
            Ok(HttpResponse::Ok().json(api_users))
        }
        Err(e) => {
            let error = ApiError::internal_error(&format!("Failed to list users: {}", e));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn create_user(
    pool: web::Data<sqlx::PgPool>,
    payload: web::Json<CreateUserRequest>,
) -> ActixResult<HttpResponse> {
    let request = payload.into_inner();

    if !is_valid_username(&request.username) {
        let error = ApiError::bad_request("Invalid username format");
        return Ok(HttpResponse::BadRequest().json(error));
    }

    if !is_valid_email(&request.email) {
        let error = ApiError::bad_request("Invalid email format");
        return Ok(HttpResponse::BadRequest().json(error));
    }

    let repo = UserRepository::new(pool.get_ref().clone());
    
    let create_user = CreateUser {
        username: request.username,
        email: request.email,
        password_hash: None, // SSH key auth users don't need password hash
    };

    match repo.create(create_user).await {
        Ok(user) => {
            let api_user = ApiUser {
                username: user.username.clone(),
                email: user.email.clone(),
                created_at: user.created_at.to_rfc3339(),
                updated_at: user.updated_at.to_rfc3339(),
                repository_count: 0, // TODO: Count repositories
            };
            Ok(HttpResponse::Created().json(api_user))
        }
        Err(e) => {
            let error = ApiError::bad_request(&format!("Failed to create user: {}", e));
            Ok(HttpResponse::BadRequest().json(error))
        }
    }
}

pub async fn get_user(
    pool: web::Data<sqlx::PgPool>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();

    let repo = UserRepository::new(pool.get_ref().clone());
    
    match repo.get_by_username(&username).await {
        Ok(Some(user)) => {
            let api_user = ApiUser {
                username: user.username.clone(),
                email: user.email.clone(),
                created_at: user.created_at.to_rfc3339(),
                updated_at: user.updated_at.to_rfc3339(),
                repository_count: 0, // TODO: Count repositories
            };
            Ok(HttpResponse::Ok().json(api_user))
        }
        Ok(None) => {
            let error = ApiError::not_found(&format!("User '{}' not found", username));
            Ok(HttpResponse::NotFound().json(error))
        }
        Err(e) => {
            let error = ApiError::internal_error(&format!("Failed to delete user: {}", e));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn update_user(
    pool: web::Data<sqlx::PgPool>,
    path: web::Path<String>,
    payload: web::Json<UpdateUserRequest>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();
    let request = payload.into_inner();

    if let Some(ref email) = request.email {
        if !is_valid_email(email) {
            let error = ApiError::bad_request("Invalid email format");
            return Ok(HttpResponse::BadRequest().json(error));
        }
    }

    let repo = UserRepository::new(pool.get_ref().clone());
    
    let update_user = StorageUpdateUser {
        username: username.clone(),
        email: request.email,
        password_hash: None, // Password updates handled separately
        is_active: None,
    };

    match repo.update(&username, update_user).await {
        Ok(user) => {
            let api_user = ApiUser {
                username: user.username.clone(),
                email: user.email.clone(),
                created_at: user.created_at.to_rfc3339(),
                updated_at: user.updated_at.to_rfc3339(),
                repository_count: 0, // TODO: Count repositories
            };
            Ok(HttpResponse::Ok().json(api_user))
        }
        Err(e) => {
            let error = ApiError::internal_error(&format!("Failed to update user: {}", e));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn delete_user(
    pool: web::Data<sqlx::PgPool>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();
    let repo = UserRepository::new(pool.get_ref().clone());
    
    match repo.delete(&username).await {
        Ok(_) => {
            Ok(HttpResponse::NoContent().finish())
        }
        Err(_e) => {
            let error = ApiError::not_found(&format!("User '{}' not found", username));
            Ok(HttpResponse::NotFound().json(error))
        }
    }
}

pub async fn list_user_repositories(
    pool: web::Data<sqlx::PgPool>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();
    
    let repo = UserRepository::new(pool.get_ref().clone());
    
    match repo.get_by_username(&username).await {
        Ok(Some(_user)) => {
            // TODO: Implement repository listing when repository storage is added
            let repositories: Vec<crate::api::models::Repository> = vec![];
            Ok(HttpResponse::Ok().json(repositories))
        }
        Ok(None) => {
            let error = ApiError::not_found(&format!("User '{}' not found", username));
            Ok(HttpResponse::NotFound().json(error))
        }
        Err(e) => {
            let error = ApiError::internal_error(&format!("Failed to list user repositories: {}", e));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

fn is_valid_username(username: &str) -> bool {
    !username.is_empty()
        && username.len() <= 50
        && username
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        && !username.starts_with('-')
        && !username.ends_with('-')
}

fn is_valid_email(email: &str) -> bool {
    email.contains('@') && email.len() <= 254 && email.len() >= 5
}

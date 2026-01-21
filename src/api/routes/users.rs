use crate::api::models::{ApiError, CreateUserRequest, UpdateUserRequest, User};
use crate::git::GitService;
use actix_web::{web, HttpResponse, Result as ActixResult};
use std::sync::Arc;

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

pub async fn list_users(_git_service: web::Data<Arc<GitService>>) -> ActixResult<HttpResponse> {
    let users = vec![User {
        username: "admin".to_string(),
        email: "admin@example.com".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        repository_count: 1,
    }];

    Ok(HttpResponse::Ok().json(users))
}

pub async fn create_user(
    _git_service: web::Data<Arc<GitService>>,
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

    let user = User {
        username: request.username,
        email: request.email,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        repository_count: 0,
    };

    Ok(HttpResponse::Created().json(user))
}

pub async fn get_user(
    _git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();

    let user = User {
        username,
        email: "user@example.com".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        repository_count: 2,
    };

    Ok(HttpResponse::Ok().json(user))
}

pub async fn update_user(
    _git_service: web::Data<Arc<GitService>>,
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

    let user = User {
        username,
        email: request
            .email
            .unwrap_or_else(|| "user@example.com".to_string()),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        repository_count: 2,
    };

    Ok(HttpResponse::Ok().json(user))
}

pub async fn delete_user(
    _git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();
    println!("Deleting user: {}", username);

    Ok(HttpResponse::NoContent().finish())
}

pub async fn list_user_repositories(
    _git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();

    let repositories = vec![crate::api::models::Repository {
        name: "user-repo".to_string(),
        owner: username,
        description: Some("User's repository".to_string()),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        size: 1024,
        clone_url_ssh: "ssh://git@localhost:2222/user/user-repo.git".to_string(),
        clone_url_http: Some("http://localhost:8080/user/user-repo.git".to_string()),
        default_branch: "main".to_string(),
        is_private: false,
    }];

    Ok(HttpResponse::Ok().json(repositories))
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

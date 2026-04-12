use crate::api::models::{ApiError, CreateUserRequest, UpdateUserRequest, User as ApiUser};
use crate::git::GitService;
use crate::storage::users::{CreateUser, UpdateUser as StorageUpdateUser, UserRepository};
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

/// Build an ApiUser with live repository count from the git service.
async fn build_api_user(
    user: &crate::storage::users::User,
    git_service: Option<&Arc<GitService>>,
) -> ApiUser {
    let repository_count = match git_service {
        Some(gs) => gs.count_repositories(&user.username).await.unwrap_or(0),
        None => 0,
    };
    ApiUser {
        username: user.username.clone(),
        email: user.email.clone(),
        created_at: user.created_at.to_rfc3339(),
        updated_at: user.updated_at.to_rfc3339(),
        repository_count,
    }
}

pub async fn list_users(
    pool: web::Data<sqlx::PgPool>,
    git_service: web::Data<Arc<GitService>>,
) -> ActixResult<HttpResponse> {
    let repo = UserRepository::new(pool.get_ref().clone());

    match repo.list().await {
        Ok(users) => {
            let mut api_users = Vec::with_capacity(users.len());
            for u in &users {
                api_users.push(build_api_user(u, Some(git_service.get_ref())).await);
            }
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
            let api_user = build_api_user(&user, None).await;
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
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();

    let repo = UserRepository::new(pool.get_ref().clone());

    match repo.get_by_username(&username).await {
        Ok(Some(user)) => {
            let api_user = build_api_user(&user, Some(git_service.get_ref())).await;
            Ok(HttpResponse::Ok().json(api_user))
        }
        Ok(None) => {
            let error = ApiError::not_found(&format!("User '{}' not found", username));
            Ok(HttpResponse::NotFound().json(error))
        }
        Err(e) => {
            let error = ApiError::internal_error(&format!("Failed to get user: {}", e));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn update_user(
    pool: web::Data<sqlx::PgPool>,
    git_service: web::Data<Arc<GitService>>,
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
            let api_user = build_api_user(&user, Some(git_service.get_ref())).await;
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
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(_e) => {
            let error = ApiError::not_found(&format!("User '{}' not found", username));
            Ok(HttpResponse::NotFound().json(error))
        }
    }
}

pub async fn list_user_repositories(
    pool: web::Data<sqlx::PgPool>,
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();

    let repo = UserRepository::new(pool.get_ref().clone());

    match repo.get_by_username(&username).await {
        Ok(Some(_user)) => {
            match git_service.list_repositories(Some(&username)).await {
                Ok(repo_infos) => {
                    let repositories: Vec<crate::api::models::Repository> = repo_infos
                        .into_iter()
                        .map(|ri| crate::api::models::Repository {
                            name: ri.name.clone(),
                            owner: ri.owner.clone(),
                            description: ri.description,
                            created_at: ri.created_at,
                            updated_at: ri.updated_at,
                            size: ri.size,
                            default_branch: ri.default_branch,
                            is_private: ri.is_private,
                            clone_url_ssh: ri.clone_url_ssh,
                            clone_url_http: ri.clone_url_http,
                        })
                        .collect();
                    let total = repositories.len();
                    Ok(HttpResponse::Ok().json(serde_json::json!({
                        "repositories": repositories,
                        "total": total,
                    })))
                }
                Err(e) => {
                    let error = ApiError::internal_error(&format!(
                        "Failed to list repositories: {}",
                        e
                    ));
                    Ok(HttpResponse::InternalServerError().json(error))
                }
            }
        }
        Ok(None) => {
            let error = ApiError::not_found(&format!("User '{}' not found", username));
            Ok(HttpResponse::NotFound().json(error))
        }
        Err(e) => {
            let error =
                ApiError::internal_error(&format!("Failed to list user repositories: {}", e));
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

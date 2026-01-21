use crate::api::models::{
    ApiError, CreateRepositoryRequest, Repository, RepositoryList, UpdateRepositoryRequest,
};
use crate::git::{GitService, GitServiceError};
use actix_web::{web, HttpResponse, Result as ActixResult};
use std::sync::Arc;

pub fn configure_repository_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(list_repositories))
        .route("/{owner}", web::post().to(create_repository))
        .route("/{owner}/{repo}", web::get().to(get_repository))
        .route("/{owner}/{repo}", web::put().to(update_repository))
        .route("/{owner}/{repo}", web::delete().to(delete_repository))
        .route("/{owner}/{repo}/branches", web::get().to(list_branches));
}

pub async fn list_repositories(
    git_service: web::Data<Arc<GitService>>,
    query: web::Query<ListRepositoriesQuery>,
) -> ActixResult<HttpResponse> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10).min(100);

    let owner = query.owner.as_deref();

    match git_service.list_repositories(owner).await {
        Ok(repo_infos) => {
            let repositories: Vec<Repository> = repo_infos
                .into_iter()
                .map(|info| Repository {
                    name: info.name,
                    owner: info.owner,
                    description: info.description,
                    created_at: info.created_at,
                    updated_at: info.updated_at,
                    size: info.size,
                    clone_url_ssh: info.clone_url_ssh,
                    clone_url_http: info.clone_url_http,
                    default_branch: info.default_branch,
                    is_private: info.is_private,
                })
                .collect();

            let total = repositories.len();
            let response = RepositoryList {
                repositories,
                total,
                page,
                per_page,
            };

            Ok(HttpResponse::Ok().json(response))
        }
        Err(err) => {
            let error = ApiError::internal_error(&format!("Failed to list repositories: {}", err));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn create_repository(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<String>,
    payload: web::Json<CreateRepositoryRequest>,
) -> ActixResult<HttpResponse> {
    let owner = path.into_inner();
    let request = payload.into_inner();

    if !is_valid_repository_name(&request.name) {
        let error = ApiError::bad_request("Invalid repository name");
        return Ok(HttpResponse::BadRequest().json(error));
    }

    match git_service
        .create_repository(
            &owner,
            &request.name,
            request.description,
            request.is_private.unwrap_or(false),
        )
        .await
    {
        Ok(repo_info) => {
            let repository = Repository {
                name: repo_info.name,
                owner: repo_info.owner,
                description: repo_info.description,
                created_at: repo_info.created_at,
                updated_at: repo_info.updated_at,
                size: repo_info.size,
                clone_url_ssh: repo_info.clone_url_ssh,
                clone_url_http: repo_info.clone_url_http,
                default_branch: repo_info.default_branch,
                is_private: repo_info.is_private,
            };

            Ok(HttpResponse::Created().json(repository))
        }
        Err(GitServiceError::RepositoryExists(_)) => {
            let error = ApiError::conflict("Repository already exists");
            Ok(HttpResponse::Conflict().json(error))
        }
        Err(GitServiceError::InvalidName(msg)) => {
            let error = ApiError::bad_request(&msg);
            Ok(HttpResponse::BadRequest().json(error))
        }
        Err(err) => {
            let error = ApiError::internal_error(&format!("Failed to create repository: {}", err));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn get_repository(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();

    match git_service.get_repository(&owner, &repo).await {
        Ok(repo_info) => {
            let repository = Repository {
                name: repo_info.name,
                owner: repo_info.owner,
                description: repo_info.description,
                created_at: repo_info.created_at,
                updated_at: repo_info.updated_at,
                size: repo_info.size,
                clone_url_ssh: repo_info.clone_url_ssh,
                clone_url_http: repo_info.clone_url_http,
                default_branch: repo_info.default_branch,
                is_private: repo_info.is_private,
            };

            Ok(HttpResponse::Ok().json(repository))
        }
        Err(GitServiceError::RepositoryNotFound(_)) => {
            let error = ApiError::not_found("Repository not found");
            Ok(HttpResponse::NotFound().json(error))
        }
        Err(err) => {
            let error = ApiError::internal_error(&format!("Failed to get repository: {}", err));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn update_repository(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
    _payload: web::Json<UpdateRepositoryRequest>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();

    match git_service.get_repository(&owner, &repo).await {
        Ok(repo_info) => {
            let repository = Repository {
                name: repo_info.name,
                owner: repo_info.owner,
                description: repo_info.description,
                created_at: repo_info.created_at,
                updated_at: chrono::Utc::now().to_rfc3339(),
                size: repo_info.size,
                clone_url_ssh: repo_info.clone_url_ssh,
                clone_url_http: repo_info.clone_url_http,
                default_branch: repo_info.default_branch,
                is_private: repo_info.is_private,
            };

            Ok(HttpResponse::Ok().json(repository))
        }
        Err(GitServiceError::RepositoryNotFound(_)) => {
            let error = ApiError::not_found("Repository not found");
            Ok(HttpResponse::NotFound().json(error))
        }
        Err(err) => {
            let error = ApiError::internal_error(&format!("Failed to update repository: {}", err));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn delete_repository(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();

    match git_service.delete_repository(&owner, &repo).await {
        Ok(()) => Ok(HttpResponse::NoContent().finish()),
        Err(GitServiceError::RepositoryNotFound(_)) => {
            let error = ApiError::not_found("Repository not found");
            Ok(HttpResponse::NotFound().json(error))
        }
        Err(err) => {
            let error = ApiError::internal_error(&format!("Failed to delete repository: {}", err));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

pub async fn list_branches(
    git_service: web::Data<Arc<GitService>>,
    path: web::Path<(String, String)>,
) -> ActixResult<HttpResponse> {
    let (owner, repo) = path.into_inner();

    match git_service.list_branches(&owner, &repo).await {
        Ok(branch_infos) => {
            let branches: Vec<crate::api::models::Branch> = branch_infos
                .into_iter()
                .map(|info| crate::api::models::Branch {
                    name: info.name,
                    commit_sha: info.commit_sha,
                    commit_message: info.commit_message,
                    author: info.author,
                    created_at: info.created_at,
                })
                .collect();

            let total = branches.len();
            let response = crate::api::models::BranchList { branches, total };

            Ok(HttpResponse::Ok().json(response))
        }
        Err(GitServiceError::RepositoryNotFound(_)) => {
            let error = ApiError::not_found("Repository not found");
            Ok(HttpResponse::NotFound().json(error))
        }
        Err(err) => {
            let error = ApiError::internal_error(&format!("Failed to list branches: {}", err));
            Ok(HttpResponse::InternalServerError().json(error))
        }
    }
}

#[derive(serde::Deserialize)]
pub struct ListRepositoriesQuery {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub owner: Option<String>,
}

fn is_valid_repository_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

use crate::api::models::{
    ApiError, CreateRepositoryRequest, Repository, RepositoryList, UpdateRepositoryRequest,
};
use crate::auth::middleware::get_current_session;
use crate::git::{GitService, GitServiceError};
use actix_web::{web, HttpRequest, HttpResponse, Result as ActixResult};
use std::sync::Arc;

/// Require authentication for mutation endpoints.
/// Returns 401 if no valid session is present.
fn require_auth(req: &HttpRequest) -> ActixResult<()> {
    if get_current_session(req).is_some() {
        return Ok(());
    }
    // Log and reject — no valid session means unauthenticated
    tracing::warn!("Unauthenticated mutation attempt on {}", req.path());
    Err(actix_web::error::ErrorUnauthorized(
        serde_json::json!({"error": "Authentication required"}).to_string(),
    ))
}

pub fn configure_repository_routes(cfg: &mut web::ServiceConfig) {
    let read_perm = crate::rbac::middleware::require_permission(
        crate::rbac::models::Permission::RepositoryRead,
    );
    let write_perm = crate::rbac::middleware::require_permission(
        crate::rbac::models::Permission::RepositoryCreate,
    );
    cfg.route("", web::get().to(list_repositories).wrap(read_perm.clone()))
        .route("/{owner}", web::post().to(create_repository).wrap(write_perm.clone()))
        .route("/{owner}/{repo}", web::get().to(get_repository).wrap(read_perm.clone()))
        .route("/{owner}/{repo}", web::put().to(update_repository).wrap(write_perm.clone()))
        .route("/{owner}/{repo}", web::delete().to(delete_repository).wrap(write_perm.clone()))
        .route("/{owner}/{repo}/branches", web::get().to(list_branches).wrap(read_perm));
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
    req: HttpRequest,
    path: web::Path<String>,
    payload: web::Json<CreateRepositoryRequest>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    let owner = path.into_inner();
    let request = payload.into_inner();

    // Validate input bounds
    if let Err(msg) = request.validate() {
        return Ok(HttpResponse::BadRequest().json(ApiError::bad_request(&msg)));
    }

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
    req: HttpRequest,
    path: web::Path<(String, String)>,
    payload: web::Json<UpdateRepositoryRequest>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    let (owner, repo) = path.into_inner();
    let request = payload.into_inner();

    // Validate input bounds
    if let Err(msg) = request.validate() {
        return Ok(HttpResponse::BadRequest().json(ApiError::bad_request(&msg)));
    }

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
    req: HttpRequest,
    path: web::Path<(String, String)>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_repo_name_alphanumeric() {
        assert!(is_valid_repository_name("my-repo"));
        assert!(is_valid_repository_name("repo_123"));
        assert!(is_valid_repository_name("MyRepo"));
        assert!(is_valid_repository_name("a"));
    }

    #[test]
    fn test_valid_repo_name_with_dashes_and_underscores() {
        assert!(is_valid_repository_name("my-awesome-repo"));
        assert!(is_valid_repository_name("repo_v2_final"));
        assert!(is_valid_repository_name("a-b_c"));
    }

    #[test]
    fn test_invalid_repo_name_empty() {
        assert!(!is_valid_repository_name(""));
    }

    #[test]
    fn test_invalid_repo_name_too_long() {
        let long_name = "a".repeat(101);
        assert!(!is_valid_repository_name(&long_name));
        // Exactly 100 chars is fine
        let max_name = "a".repeat(100);
        assert!(is_valid_repository_name(&max_name));
    }

    #[test]
    fn test_invalid_repo_name_starts_with_dash() {
        assert!(!is_valid_repository_name("-repo"));
    }

    #[test]
    fn test_invalid_repo_name_ends_with_dash() {
        assert!(!is_valid_repository_name("repo-"));
    }

    #[test]
    fn test_invalid_repo_name_special_chars() {
        assert!(!is_valid_repository_name("repo.name"));
        assert!(!is_valid_repository_name("repo name"));
        assert!(!is_valid_repository_name("repo/name"));
        assert!(!is_valid_repository_name("repo@main"));
        assert!(!is_valid_repository_name("repo!"));
    }

    #[test]
    fn test_list_repositories_query_defaults() {
        // Verify the query struct can be constructed with defaults
        let q = ListRepositoriesQuery {
            page: None,
            per_page: None,
            owner: None,
        };
        assert!(q.page.is_none());
        assert!(q.per_page.is_none());
        assert!(q.owner.is_none());

        let q = ListRepositoriesQuery {
            page: Some(2),
            per_page: Some(5),
            owner: Some("acme".to_string()),
        };
        assert_eq!(q.page, Some(2));
        assert_eq!(q.per_page, Some(5));
        assert_eq!(q.owner.as_deref(), Some("acme"));
    }
}

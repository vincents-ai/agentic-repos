use crate::rbac::middleware::{require_permission, AuthenticatedUser};
use crate::rbac::models::{Permission, Role, User};
use crate::rbac::service::RbacService;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: Option<String>,
    pub roles: Option<Vec<String>>,
    pub ssh_keys: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: String,
    pub permissions: Vec<Permission>,
}

#[derive(Deserialize)]
pub struct AssignRoleRequest {
    pub username: String,
    pub role_name: String,
}

#[derive(Deserialize)]
pub struct GrantRepositoryPermissionRequest {
    pub username: String,
    pub repository_id: String,
    pub permission: Permission,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub roles: Vec<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
pub struct RoleResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<Permission>,
    pub is_system_role: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            roles: user.roles.iter().map(|r| r.to_string()).collect(),
            is_active: user.is_active,
            created_at: user.created_at,
            last_login: user.last_login,
        }
    }
}

impl From<Role> for RoleResponse {
    fn from(role: Role) -> Self {
        Self {
            id: role.id.to_string(),
            name: role.name,
            description: role.description,
            permissions: role.permissions.into_iter().collect(),
            is_system_role: role.is_system_role,
            created_at: role.created_at,
        }
    }
}

pub async fn create_user(
    rbac_service: web::Data<Arc<RbacService>>,
    req: web::Json<CreateUserRequest>,
) -> Result<HttpResponse> {
    match rbac_service
        .create_user(req.username.clone(), req.email.clone())
        .await
    {
        Ok(user) => {
            if let Some(roles) = &req.roles {
                for role_name in roles {
                    if let Err(e) = rbac_service
                        .assign_role_to_user(&user.username, role_name)
                        .await
                    {
                        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                            "error": "Failed to assign role",
                            "message": e.to_string()
                        })));
                    }
                }
            }

            if let Some(ssh_keys) = &req.ssh_keys {
                for ssh_key in ssh_keys {
                    if let Err(e) = rbac_service
                        .add_ssh_key_to_user(&user.username, ssh_key.clone())
                        .await
                    {
                        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                            "error": "Failed to add SSH key",
                            "message": e.to_string()
                        })));
                    }
                }
            }

            let updated_user = rbac_service.get_user(&user.username).await.unwrap_or(user);
            Ok(HttpResponse::Created().json(UserResponse::from(updated_user)))
        }
        Err(e) => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Failed to create user",
            "message": e.to_string()
        }))),
    }
}

pub async fn list_users(rbac_service: web::Data<Arc<RbacService>>) -> Result<HttpResponse> {
    match rbac_service.list_users().await {
        Ok(users) => {
            let user_responses: Vec<UserResponse> =
                users.into_iter().map(UserResponse::from).collect();
            Ok(HttpResponse::Ok().json(user_responses))
        }
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to list users",
            "message": e.to_string()
        }))),
    }
}

pub async fn get_user(
    rbac_service: web::Data<Arc<RbacService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let username = path.into_inner();

    match rbac_service.get_user(&username).await {
        Ok(user) => Ok(HttpResponse::Ok().json(UserResponse::from(user))),
        Err(e) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "User not found",
            "message": e.to_string()
        }))),
    }
}

pub async fn deactivate_user(
    rbac_service: web::Data<Arc<RbacService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let username = path.into_inner();

    match rbac_service.deactivate_user(&username).await {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "User deactivated successfully"
        }))),
        Err(e) => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Failed to deactivate user",
            "message": e.to_string()
        }))),
    }
}

pub async fn activate_user(
    rbac_service: web::Data<Arc<RbacService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let username = path.into_inner();

    match rbac_service.activate_user(&username).await {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "User activated successfully"
        }))),
        Err(e) => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Failed to activate user",
            "message": e.to_string()
        }))),
    }
}

pub async fn assign_role(
    rbac_service: web::Data<Arc<RbacService>>,
    req: web::Json<AssignRoleRequest>,
) -> Result<HttpResponse> {
    match rbac_service
        .assign_role_to_user(&req.username, &req.role_name)
        .await
    {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Role assigned successfully"
        }))),
        Err(e) => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Failed to assign role",
            "message": e.to_string()
        }))),
    }
}

pub async fn revoke_role(
    rbac_service: web::Data<Arc<RbacService>>,
    req: web::Json<AssignRoleRequest>,
) -> Result<HttpResponse> {
    match rbac_service
        .revoke_role_from_user(&req.username, &req.role_name)
        .await
    {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Role revoked successfully"
        }))),
        Err(e) => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Failed to revoke role",
            "message": e.to_string()
        }))),
    }
}

pub async fn create_role(
    rbac_service: web::Data<Arc<RbacService>>,
    req: web::Json<CreateRoleRequest>,
) -> Result<HttpResponse> {
    let permissions: HashSet<Permission> = req.permissions.iter().cloned().collect();

    match rbac_service
        .create_role(req.name.clone(), req.description.clone(), permissions)
        .await
    {
        Ok(role) => Ok(HttpResponse::Created().json(RoleResponse::from(role))),
        Err(e) => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Failed to create role",
            "message": e.to_string()
        }))),
    }
}

pub async fn list_roles(rbac_service: web::Data<Arc<RbacService>>) -> Result<HttpResponse> {
    match rbac_service.list_roles().await {
        Ok(roles) => {
            let role_responses: Vec<RoleResponse> =
                roles.into_iter().map(RoleResponse::from).collect();
            Ok(HttpResponse::Ok().json(role_responses))
        }
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to list roles",
            "message": e.to_string()
        }))),
    }
}

pub async fn get_role(
    rbac_service: web::Data<Arc<RbacService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let role_name = path.into_inner();

    match rbac_service.get_role(&role_name).await {
        Ok(role) => Ok(HttpResponse::Ok().json(RoleResponse::from(role))),
        Err(e) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Role not found",
            "message": e.to_string()
        }))),
    }
}

pub async fn grant_repository_permission(
    rbac_service: web::Data<Arc<RbacService>>,
    req: web::Json<GrantRepositoryPermissionRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let authenticated_username = {
        let extensions = http_req.extensions();
        let authenticated_user = extensions
            .get::<AuthenticatedUser>()
            .expect("User should be authenticated");
        authenticated_user.user.username.clone()
    };

    match rbac_service
        .grant_repository_permission(
            &req.username,
            &req.repository_id,
            req.permission.clone(),
            &authenticated_username,
            req.expires_at,
        )
        .await
    {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Repository permission granted successfully"
        }))),
        Err(e) => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Failed to grant repository permission",
            "message": e.to_string()
        }))),
    }
}

pub async fn get_current_user(http_req: HttpRequest) -> Result<HttpResponse> {
    if let Some(authenticated_user) = http_req.extensions().get::<AuthenticatedUser>() {
        Ok(HttpResponse::Ok().json(UserResponse::from(authenticated_user.user.clone())))
    } else {
        Ok(HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Not authenticated"
        })))
    }
}

pub fn configure_rbac_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/rbac")
            .service(
                web::scope("/users")
                    .route(
                        "",
                        web::post()
                            .to(create_user)
                            .wrap(require_permission(Permission::UserManage)),
                    )
                    .route(
                        "",
                        web::get()
                            .to(list_users)
                            .wrap(require_permission(Permission::UserManage)),
                    )
                    .route("/me", web::get().to(get_current_user))
                    .route(
                        "/{username}",
                        web::get()
                            .to(get_user)
                            .wrap(require_permission(Permission::UserManage)),
                    )
                    .route(
                        "/{username}/deactivate",
                        web::post()
                            .to(deactivate_user)
                            .wrap(require_permission(Permission::UserManage)),
                    )
                    .route(
                        "/{username}/activate",
                        web::post()
                            .to(activate_user)
                            .wrap(require_permission(Permission::UserManage)),
                    ),
            )
            .service(
                web::scope("/roles")
                    .route(
                        "",
                        web::post()
                            .to(create_role)
                            .wrap(require_permission(Permission::RoleManage)),
                    )
                    .route(
                        "",
                        web::get()
                            .to(list_roles)
                            .wrap(require_permission(Permission::RoleManage)),
                    )
                    .route(
                        "/{role_name}",
                        web::get()
                            .to(get_role)
                            .wrap(require_permission(Permission::RoleManage)),
                    )
                    .route(
                        "/assign",
                        web::post()
                            .to(assign_role)
                            .wrap(require_permission(Permission::UserManage)),
                    )
                    .route(
                        "/revoke",
                        web::post()
                            .to(revoke_role)
                            .wrap(require_permission(Permission::UserManage)),
                    ),
            )
            .service(
                web::scope("/permissions").route(
                    "/repository/grant",
                    web::post()
                        .to(grant_repository_permission)
                        .wrap(require_permission(Permission::RepositoryAdmin)),
                ),
            ),
    );
}

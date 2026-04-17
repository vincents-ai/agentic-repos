//! Integration tests for RBAC handlers using actix-web test framework
//! with InMemoryRbacStorage — no mocks, real service logic.

use actix_web::{test, web, App};
use agentic_repos::rbac::handlers::configure_rbac_routes;
use agentic_repos::rbac::service::RbacService;
use agentic_repos::rbac::storage::InMemoryRbacStorage;
use std::sync::Arc;

fn make_service() -> Arc<RbacService> {
    Arc::new(RbacService::new(Arc::new(InMemoryRbacStorage::new())))
}

macro_rules! test_app {
    ($service:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($service.clone()))
                .configure(configure_rbac_routes),
        )
        .await
    };
}

#[actix_web::test]
async fn test_list_users_requires_auth() {
    let service = make_service();
    let app = test_app!(service);
    let req = test::TestRequest::get()
        .uri("/rbac/users")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    assert!(status == 401 || status == 403, "got {}", status);
}

#[actix_web::test]
async fn test_create_user_requires_auth() {
    let service = make_service();
    let app = test_app!(service);
    let req = test::TestRequest::post()
        .uri("/rbac/users")
        .set_json(serde_json::json!({"username": "alice"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    assert!(status == 401 || status == 403, "got {}", status);
}

#[actix_web::test]
async fn test_get_me_unauthenticated() {
    let service = make_service();
    let app = test_app!(service);
    let req = test::TestRequest::get()
        .uri("/rbac/users/me")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_list_roles_requires_auth() {
    let service = make_service();
    let app = test_app!(service);
    let req = test::TestRequest::get()
        .uri("/rbac/roles")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    assert!(status == 401 || status == 403, "got {}", status);
}

#[actix_web::test]
async fn test_create_role_requires_auth() {
    let service = make_service();
    let app = test_app!(service);
    let req = test::TestRequest::post()
        .uri("/rbac/roles")
        .set_json(serde_json::json!({
            "name": "custom-role",
            "description": "A custom role",
            "permissions": ["Pull"]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    assert!(status == 401 || status == 403, "got {}", status);
}

#[actix_web::test]
async fn test_grant_repo_permission_requires_auth() {
    let service = make_service();
    let app = test_app!(service);
    let req = test::TestRequest::post()
        .uri("/rbac/permissions/repository/grant")
        .set_json(serde_json::json!({
            "username": "alice",
            "repository_id": "org/repo",
            "permission": "Pull"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    assert!(status == 401 || status == 403, "got {}", status);
}

#[actix_web::test]
async fn test_all_rbac_routes_registered() {
    let service = make_service();
    let app = test_app!(service);

    let get_routes = [
        "/rbac/users",
        "/rbac/users/me",
        "/rbac/users/testuser",
        "/rbac/roles",
        "/rbac/roles/admin",
    ];

    let post_routes = [
        "/rbac/users",
        "/rbac/users/testuser/deactivate",
        "/rbac/users/testuser/activate",
        "/rbac/roles",
        "/rbac/roles/assign",
        "/rbac/roles/revoke",
        "/rbac/permissions/repository/grant",
    ];

    for path in &get_routes {
        let req = test::TestRequest::get().uri(path).to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status().as_u16(), 404, "GET {} should exist", path);
    }

    for path in &post_routes {
        let req = test::TestRequest::post()
            .uri(path)
            .set_json(serde_json::json!({}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status().as_u16(), 404, "POST {} should exist", path);
    }
}

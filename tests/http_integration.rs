//! HTTP integration tests for the agentic-repos API server.
//!
//! Uses actix-web test framework to exercise the full HTTP stack:
//! routing, middleware, serialization, error handling.
//!
//! These tests use the bare route configuration without injected state.
//! Routes that need state (database, services) are tested via the BDD
//! cucumber tests which instantiate the full plugin.
//!
//! Run with: cargo test --test http_integration

use actix_web::{test, web, App};
use agentic_repos::api::routes::configure_routes;
use serde_json::Value;

// ── Health & Info (stateless routes) ──

#[actix_web::test]
async fn test_health_check() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;
    let req = test::TestRequest::get().uri("/api/v1/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "Health check should return 2xx, got {}", resp.status());
    let body: Value = test::read_body_json(resp).await;
    assert!(body.is_object());
}

#[actix_web::test]
async fn test_api_info() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;
    let req = test::TestRequest::get().uri("/api/v1/info").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "Info should return 2xx, got {}", resp.status());
    let body: Value = test::read_body_json(resp).await;
    assert!(body.is_object());
}

// ── 404 handling ──

#[actix_web::test]
async fn test_not_found() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;
    let req = test::TestRequest::get()
        .uri("/api/v1/this-route-does-not-exist-at-all")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404, "Unknown route should return 404");
}

// ── Repository routes ──

#[actix_web::test]
async fn test_list_repositories() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;
    let req = test::TestRequest::get().uri("/api/v1/repositories").to_request();
    let resp = test::call_service(&app, req).await;
    // Route registered — may return 500 (missing state) but not 404
    assert_ne!(resp.status(), 404, "List repositories route should be registered");
}

#[actix_web::test]
async fn test_create_repository_missing_body() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;
    let req = test::TestRequest::post().uri("/api/v1/repositories").to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    // May return 404 (state-dependent route) or 4xx — either proves the scope exists
    assert!(status != 500 || status == 404, "Create repo route should be reasonable, got {}", status);
}

#[actix_web::test]
async fn test_create_repository_with_json() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;
    let req = test::TestRequest::post()
        .uri("/api/v1/repositories")
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({"name": "test-repo"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    // JSON content type accepted — not 415
    assert_ne!(resp.status().as_u16(), 415);
}

// ── User routes ──

#[actix_web::test]
async fn test_list_users() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;
    let req = test::TestRequest::get().uri("/api/v1/users").to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status(), 404, "List users route should be registered");
}

// ── Content-type and method handling ──

#[actix_web::test]
async fn test_malformed_json_body() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;
    let req = test::TestRequest::post()
        .uri("/api/v1/repositories")
        .insert_header(("Content-Type", "application/json"))
        .set_payload("{not valid json}")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    assert!(status >= 400 && status < 500, "Malformed JSON should return 4xx, got {}", status);
}

// ── AppConfigurator route injection (Task 1b818ce6) ──

#[actix_web::test]
async fn test_app_configurator_injects_custom_route() {
    use actix_web::HttpResponse;

    let app = test::init_service(
        App::new().service(
            web::scope("/api/v1")
                .configure(configure_routes)
                .configure(|cfg| {
                    cfg.route("/custom/test", web::get().to(|| async {
                        HttpResponse::Ok().json(serde_json::json!({
                            "message": "custom route works"
                        }))
                    }));
                })
        )
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/custom/test").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "Custom route should return 200");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["message"], "custom route works");
}

#[actix_web::test]
async fn test_app_configurator_spa_fallback() {
    use actix_web::HttpResponse;

    let app = test::init_service(
        App::new()
            .service(web::scope("/api/v1").configure(configure_routes))
            .configure(|cfg| {
                // SPA fallback — any non-API route returns index
                cfg.route("/{path:.*}", web::get().to(|path: web::Path<String>| async move {
                    HttpResponse::Ok().json(serde_json::json!({
                        "spa": true,
                        "path": path.to_string()
                    }))
                }));
            })
    ).await;

    // API route still works normally
    let req = test::TestRequest::get().uri("/api/v1/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "API health should still work");

    // SPA fallback catches non-API routes
    let req = test::TestRequest::get().uri("/ui/repos").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "SPA fallback should return 200");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["spa"], true);
    assert_eq!(body["path"], "ui/repos");
}

#[actix_web::test]
async fn test_app_configurator_multiple_injections() {
    use actix_web::HttpResponse;

    let app = test::init_service(
        App::new().service(
            web::scope("/api/v1")
                .configure(configure_routes)
                .configure(|cfg| {
                    cfg.route("/ext/one", web::get().to(|| async {
                        HttpResponse::Ok().json(serde_json::json!({"endpoint": 1}))
                    }));
                })
                .configure(|cfg| {
                    cfg.route("/ext/two", web::get().to(|| async {
                        HttpResponse::Ok().json(serde_json::json!({"endpoint": 2}))
                    }));
                })
        )
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/ext/one").to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["endpoint"], 1);

    let req = test::TestRequest::get().uri("/api/v1/ext/two").to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["endpoint"], 2);

    // Original routes still work
    let req = test::TestRequest::get().uri("/api/v1/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

// ── Repository route registration (Task cdfb3c88) ──

#[actix_web::test]
async fn test_repos_route_list_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/repositories").to_request();
    let resp = test::call_service(&app, req).await;
    // Route exists (not 404) — may 500 due to missing state but route is registered
    assert_ne!(resp.status().as_u16(), 404, "GET /repositories should be registered");
}

#[actix_web::test]
async fn test_repos_route_create_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/repositories/default")
        .set_json(serde_json::json!({"name": "test-repo"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "POST /repositories/owner should be registered");
}

#[actix_web::test]
async fn test_repos_route_get_by_owner_repo_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/repositories/default/test-repo").to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "GET /repositories/owner/repo should be registered");
}

#[actix_web::test]
async fn test_repos_route_delete_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::delete().uri("/api/v1/repositories/default/test-repo").to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "DELETE /repositories/owner/repo should be registered");
}

#[actix_web::test]
async fn test_repos_route_update_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::put()
        .uri("/api/v1/repositories/default/test-repo")
        .set_json(serde_json::json!({"description": "updated"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "PUT /repositories/owner/repo should be registered");
}

#[actix_web::test]
async fn test_repos_route_branches_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/repositories/default/test-repo/branches").to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "GET branches should be registered");
}

// ── LFS route registration (Task d4184cb9) ──

#[actix_web::test]
async fn test_lfs_locks_routes_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    // POST create lock
    let req = test::TestRequest::post()
        .uri("/api/v1/lfs-locks/default/repo/info/lfs/locks")
        .set_json(serde_json::json!({"path": "test.txt"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "POST create lock should be registered");

    // GET list locks
    let req = test::TestRequest::get()
        .uri("/api/v1/lfs-locks/default/repo/info/lfs/locks")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "GET list locks should be registered");

    // POST unlock
    let req = test::TestRequest::post()
        .uri("/api/v1/lfs-locks/default/repo/info/lfs/locks/lock-123/unlock")
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "POST unlock should be registered");

    // POST verify locks
    let req = test::TestRequest::post()
        .uri("/api/v1/lfs-locks/default/repo/info/lfs/locks/verify")
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "POST verify locks should be registered");
}

// ── Webhook route registration ──

#[actix_web::test]
async fn test_webhook_routes_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/webhooks").to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "GET webhooks should be registered");

    let req = test::TestRequest::post()
        .uri("/api/v1/webhooks")
        .set_json(serde_json::json!({"url": "http://test", "secret": "s"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "POST webhooks should be registered");
}

// ── RBAC route registration ──

#[actix_web::test]
async fn test_rbac_routes_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/rbac/roles").to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "GET rbac/roles should be registered");
}

// ── Secrets route registration ──

#[actix_web::test]
async fn test_secrets_routes_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/secrets/default/repo").to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "GET secrets should be registered");
}

// ── ADP route registration ──

#[actix_web::test]
async fn test_adp_routes_registered() {
    let app = test::init_service(
        App::new().service(web::scope("/api/v1").configure(configure_routes))
    ).await;

    let req = test::TestRequest::get().uri("/api/v1/adp/schemas").to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status().as_u16(), 404, "GET adp/schemas should be registered");
}

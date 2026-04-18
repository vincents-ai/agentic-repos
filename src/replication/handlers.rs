//! Replication REST API handlers.

use crate::auth::middleware::get_current_session;
use crate::features::FeatureFlags;
use crate::replication::models::*;
use crate::replication::monitor::ReplicationMonitor;
use crate::replication::service::ReplicationService;
use actix_web::{web, HttpRequest, HttpResponse, Result as ActixResult};
use std::sync::Arc;
use uuid::Uuid;

/// Require authentication for all replication endpoints.
fn require_auth(req: &HttpRequest) -> ActixResult<()> {
    if get_current_session(req).is_some() {
        return Ok(());
    }
    tracing::warn!(
        "Unauthenticated access to replication endpoint: {}",
        req.path()
    );
    Err(actix_web::error::ErrorUnauthorized(
        serde_json::json!({"error": "Authentication required"}).to_string(),
    ))
}

/// Sanitize replication errors for HTTP responses.
/// Logs the full error server-side but returns a generic message to the client
/// to prevent information leakage (internal paths, version details, etc.).
fn sanitize_error(e: &crate::replication::service::ReplicationError) -> String {
    use crate::replication::service::ReplicationError;
    tracing::warn!("Replication error: {}", e);
    match e {
        ReplicationError::TargetNotFound(_) => "Target not found".to_string(),
        ReplicationError::TargetExists(_) => "Target already exists".to_string(),
        ReplicationError::ConflictDetected { .. } => "Replication conflict detected".to_string(),
        ReplicationError::ReplicationFailed(_)
        | ReplicationError::Storage(_)
        | ReplicationError::LockPoisoned => "Internal replication error".to_string(),
    }
}

pub fn configure_replication_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/replication")
            .route("/targets", web::get().to(list_targets))
            .route("/targets", web::post().to(add_target))
            .route("/targets/{id}", web::get().to(get_target))
            .route("/targets/{id}", web::delete().to(remove_target))
            .route("/targets/{id}/enable", web::post().to(enable_target))
            .route("/targets/{id}/disable", web::post().to(disable_target))
            .route("/health", web::get().to(list_health))
            .route("/health/{id}", web::get().to(get_health))
            .route("/dashboard", web::get().to(get_dashboard))
            .route("/conflicts", web::get().to(list_conflicts))
            .route("/conflicts/{id}/resolve", web::post().to(resolve_conflict))
            .route("/alerts", web::get().to(check_alerts)),
    );
}

async fn list_targets(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    match service.list_targets() {
        Ok(targets) => Ok(HttpResponse::Ok().json(targets)),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": sanitize_error(&e)}))),
    }
}

async fn add_target(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
    body: web::Json<CreateTargetRequest>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    let target = ReplicationTarget::from(body.into_inner());
    match service.register_target(target) {
        Ok(()) => Ok(HttpResponse::Created().json(serde_json::json!({"status": "created"}))),
        Err(e) => {
            Ok(HttpResponse::Conflict().json(serde_json::json!({"error": sanitize_error(&e)})))
        }
    }
}

async fn get_target(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
    path: web::Path<Uuid>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    let id = path.into_inner();
    match service.get_target(id) {
        Ok(Some(t)) => Ok(HttpResponse::Ok().json(t)),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({"error": "not found"}))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": sanitize_error(&e)}))),
    }
}

async fn remove_target(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
    path: web::Path<Uuid>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    let id = path.into_inner();
    match service.remove_target(id) {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({"status": "removed"}))),
        Err(e) => {
            Ok(HttpResponse::NotFound().json(serde_json::json!({"error": sanitize_error(&e)})))
        }
    }
}

async fn enable_target(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
    path: web::Path<Uuid>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    let id = path.into_inner();
    match service.enable_target(id) {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({"status": "enabled"}))),
        Err(e) => {
            Ok(HttpResponse::NotFound().json(serde_json::json!({"error": sanitize_error(&e)})))
        }
    }
}

async fn disable_target(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
    path: web::Path<Uuid>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    let id = path.into_inner();
    match service.disable_target(id) {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({"status": "disabled"}))),
        Err(e) => {
            Ok(HttpResponse::NotFound().json(serde_json::json!({"error": sanitize_error(&e)})))
        }
    }
}

async fn list_health(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    match service.get_all_health() {
        Ok(health) => Ok(HttpResponse::Ok().json(health)),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": sanitize_error(&e)}))),
    }
}

async fn get_health(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
    path: web::Path<Uuid>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    let id = path.into_inner();
    match service.get_target_health(id) {
        Ok(Some(h)) => Ok(HttpResponse::Ok().json(h)),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({"error": "not found"}))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": sanitize_error(&e)}))),
    }
}

async fn get_dashboard(
    monitor: web::Data<Arc<ReplicationMonitor>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    match monitor.dashboard() {
        Ok(dash) => Ok(HttpResponse::Ok().json(dash)),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": sanitize_error(&e)}))),
    }
}

async fn list_conflicts(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    match service.list_conflicts() {
        Ok(conflicts) => Ok(HttpResponse::Ok().json(conflicts)),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": sanitize_error(&e)}))),
    }
}

#[derive(serde::Deserialize)]
struct ResolveRequest {
    resolution: ConflictResolution,
}

async fn resolve_conflict(
    service: web::Data<Arc<ReplicationService>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
    path: web::Path<Uuid>,
    body: web::Json<ResolveRequest>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    let id = path.into_inner();
    match service.resolve_conflict(id, body.resolution) {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({"status": "resolved"}))),
        Err(e) => {
            Ok(HttpResponse::NotFound().json(serde_json::json!({"error": sanitize_error(&e)})))
        }
    }
}

async fn check_alerts(
    monitor: web::Data<Arc<ReplicationMonitor>>,
    req: HttpRequest,
    flags: web::Data<FeatureFlags>,
) -> ActixResult<HttpResponse> {
    require_auth(&req)?;
    if !flags.multi_region {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "multi_region feature is not enabled"})));
    }
    match monitor.check_alerts() {
        Ok(alerts) => Ok(HttpResponse::Ok().json(alerts)),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": sanitize_error(&e)}))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication::monitor::{AlertThresholds, ReplicationMonitor};
    use crate::replication::InMemoryReplicationStore;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_list_targets_empty() {
        let svc = Arc::new(ReplicationService::new(InMemoryReplicationStore::shared()));
        let monitor = Arc::new(ReplicationMonitor::new(
            svc.clone(),
            AlertThresholds::default(),
        ));
        let flags = FeatureFlags {
            multi_region: true,
            ..FeatureFlags::default()
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .app_data(web::Data::new(monitor))
                .app_data(web::Data::new(flags))
                .configure(configure_replication_routes),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/replication/targets")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Vec<ReplicationTarget> = test::read_body_json(resp).await;
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn test_add_and_get_target() {
        let svc = Arc::new(ReplicationService::new(InMemoryReplicationStore::shared()));
        let monitor = Arc::new(ReplicationMonitor::new(
            svc.clone(),
            AlertThresholds::default(),
        ));
        let flags = FeatureFlags {
            multi_region: true,
            ..FeatureFlags::default()
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .app_data(web::Data::new(monitor))
                .app_data(web::Data::new(flags))
                .configure(configure_replication_routes),
        )
        .await;
        let create_req = CreateTargetRequest {
            name: "test-mirror".to_string(),
            region: "eu-west-1".to_string(),
            bucket: "my-bucket".to_string(),
            role_arn: None,
        };
        let req = test::TestRequest::post()
            .uri("/replication/targets")
            .set_json(&create_req)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        // Get the created target's ID from response
        let created: ReplicationTarget = test::read_body_json(resp).await;
        let req = test::TestRequest::get()
            .uri(&format!("/replication/targets/{}", created.id))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_feature_disabled() {
        let svc = Arc::new(ReplicationService::new(InMemoryReplicationStore::shared()));
        let monitor = Arc::new(ReplicationMonitor::new(
            svc.clone(),
            AlertThresholds::default(),
        ));
        let flags = FeatureFlags {
            multi_region: false,
            ..FeatureFlags::default()
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .app_data(web::Data::new(monitor))
                .app_data(web::Data::new(flags))
                .configure(configure_replication_routes),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/replication/targets")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn test_remove_target() {
        let svc = Arc::new(ReplicationService::new(InMemoryReplicationStore::shared()));
        let monitor = Arc::new(ReplicationMonitor::new(
            svc.clone(),
            AlertThresholds::default(),
        ));
        let flags = FeatureFlags {
            multi_region: true,
            ..FeatureFlags::default()
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .app_data(web::Data::new(monitor))
                .app_data(web::Data::new(flags))
                .configure(configure_replication_routes),
        )
        .await;
        let create_req = CreateTargetRequest {
            name: "removeme".to_string(),
            region: "us-west-2".to_string(),
            bucket: "bucket".to_string(),
            role_arn: None,
        };
        let req = test::TestRequest::post()
            .uri("/replication/targets")
            .set_json(&create_req)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let created: ReplicationTarget = test::read_body_json(resp).await;
        let req = test::TestRequest::delete()
            .uri(&format!("/replication/targets/{}", created.id))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_dashboard_empty() {
        let svc = Arc::new(ReplicationService::new(InMemoryReplicationStore::shared()));
        let monitor = Arc::new(ReplicationMonitor::new(
            svc.clone(),
            AlertThresholds::default(),
        ));
        let flags = FeatureFlags {
            multi_region: true,
            ..FeatureFlags::default()
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .app_data(web::Data::new(monitor))
                .app_data(web::Data::new(flags))
                .configure(configure_replication_routes),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/replication/dashboard")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: crate::replication::monitor::ReplicationDashboard =
            test::read_body_json(resp).await;
        assert_eq!(body.total_targets, 0);
    }
}

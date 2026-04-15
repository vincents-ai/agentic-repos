//! SDLC REST API handlers -- metrics, gates, assessments, dashboard.

use crate::sdlc::models::*;
use crate::sdlc::service::SdlcService;
use actix_web::{web, HttpResponse, Result as ActixResult};
use std::sync::Arc;

pub fn configure_sdlc_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/metrics", web::get().to(get_metrics))
        .route("/metrics", web::post().to(record_metric))
        .route("/gates", web::get().to(list_gates))
        .route("/gates", web::post().to(create_gate))
        .route("/gates/{id}/evaluate", web::post().to(evaluate_gate))
        .route("/gates/evaluate-all", web::post().to(evaluate_all_gates))
        .route("/assessments", web::get().to(list_assessments))
        .route("/assessments", web::post().to(create_assessment))
        .route("/dashboard", web::get().to(get_dashboard));
}

async fn get_metrics(service: web::Data<Arc<SdlcService>>) -> ActixResult<HttpResponse> {
    match service.get_latest_metrics() {
        Ok(metrics) => Ok(HttpResponse::Ok().json(metrics)),
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn record_metric(
    service: web::Data<Arc<SdlcService>>,
    body: web::Json<QualityMetric>,
) -> ActixResult<HttpResponse> {
    match service.record_metric(body.into_inner()) {
        Ok(()) => Ok(HttpResponse::Created().json(serde_json::json!({"status": "recorded"}))),
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn list_gates(service: web::Data<Arc<SdlcService>>) -> ActixResult<HttpResponse> {
    match service.list_gates() {
        Ok(gates) => Ok(HttpResponse::Ok().json(gates)),
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn create_gate(
    service: web::Data<Arc<SdlcService>>,
    body: web::Json<QualityGate>,
) -> ActixResult<HttpResponse> {
    match service.create_gate(body.into_inner()) {
        Ok(()) => Ok(HttpResponse::Created().json(serde_json::json!({"status": "created"}))),
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn evaluate_gate(
    service: web::Data<Arc<SdlcService>>,
    path: web::Path<uuid::Uuid>,
) -> ActixResult<HttpResponse> {
    let id = path.into_inner();
    match service.evaluate_gate(id) {
        Ok(status) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "gate_id": id.to_string(),
            "status": format!("{:?}", status),
        }))),
        Err(e) => Ok(HttpResponse::NotFound().json(serde_json::json!({"error": e.to_string()}))),
    }
}

async fn evaluate_all_gates(service: web::Data<Arc<SdlcService>>) -> ActixResult<HttpResponse> {
    match service.evaluate_all_gates() {
        Ok(results) => {
            let mapped: Vec<serde_json::Value> = results
                .into_iter()
                .map(|(id, status)| {
                    serde_json::json!({
                        "gate_id": id.to_string(),
                        "status": format!("{:?}", status),
                    })
                })
                .collect();
            Ok(HttpResponse::Ok().json(mapped))
        }
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn list_assessments(service: web::Data<Arc<SdlcService>>) -> ActixResult<HttpResponse> {
    match service.list_assessments() {
        Ok(assessments) => Ok(HttpResponse::Ok().json(assessments)),
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn create_assessment(
    service: web::Data<Arc<SdlcService>>,
    body: web::Json<SdlcAssessment>,
) -> ActixResult<HttpResponse> {
    match service.create_assessment(body.into_inner()) {
        Ok(()) => Ok(HttpResponse::Created().json(serde_json::json!({"status": "created"}))),
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn get_dashboard(service: web::Data<Arc<SdlcService>>) -> ActixResult<HttpResponse> {
    match service.build_dashboard() {
        Ok(dashboard) => Ok(HttpResponse::Ok().json(dashboard)),
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdlc::service::InMemorySdlcStore;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_get_metrics_empty() {
        let svc = Arc::new(SdlcService::new(InMemorySdlcStore::shared()));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .configure(configure_sdlc_routes),
        )
        .await;
        let req = test::TestRequest::get().uri("/sdlc/metrics").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_record_metric() {
        let svc = Arc::new(SdlcService::new(InMemorySdlcStore::shared()));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .configure(configure_sdlc_routes),
        )
        .await;
        let metric = QualityMetric::new("test_cov", MetricCategory::TestCoverage, 90.0, 80.0, "%");
        let req = test::TestRequest::post()
            .uri("/sdlc/metrics")
            .set_json(&metric)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn test_create_and_list_gates() {
        let svc = Arc::new(SdlcService::new(InMemorySdlcStore::shared()));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .configure(configure_sdlc_routes),
        )
        .await;
        let gate = QualityGate::new("merge-gate", SdlcPhase::Implementation).with_condition(
            GateCondition::new("cov", ComparisonOperator::GreaterEqual, 80.0, "cov"),
        );
        let req = test::TestRequest::post()
            .uri("/sdlc/gates")
            .set_json(&gate)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);

        let req = test::TestRequest::get().uri("/sdlc/gates").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_dashboard_empty() {
        let svc = Arc::new(SdlcService::new(InMemorySdlcStore::shared()));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .configure(configure_sdlc_routes),
        )
        .await;
        let req = test::TestRequest::get().uri("/sdlc/dashboard").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_create_assessment() {
        let svc = Arc::new(SdlcService::new(InMemorySdlcStore::shared()));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .configure(configure_sdlc_routes),
        )
        .await;
        let assessment = SdlcAssessment::new("Q1", AssessmentPeriod::q(2026, 1), "bot")
            .with_score(MetricCategory::TestCoverage, 92.0);
        let req = test::TestRequest::post()
            .uri("/sdlc/assessments")
            .set_json(&assessment)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }
}

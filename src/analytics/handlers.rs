//! Analytics REST API handlers.

use crate::analytics::aggregator::{aggregate_repo_stats, aggregate_tenant_stats, build_trend};
use crate::analytics::collector::AnalyticsCollector;
use crate::analytics::models::TrendQuery;
use crate::features::FeatureFlags;
use actix_web::{web, HttpResponse, Result as ActixResult};
use std::sync::Arc;

pub fn configure_analytics_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/repos/{repo_id}/stats", web::get().to(get_repo_stats))
        .route(
            "/tenants/{tenant_id}/stats",
            web::get().to(get_tenant_stats),
        )
        .route("/trends", web::post().to(get_trends));
}

async fn get_repo_stats(
    collector: web::Data<Arc<AnalyticsCollector>>,
    feature_flags: web::Data<FeatureFlags>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    if !feature_flags.analytics {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "analytics feature is not enabled"})));
    }

    let repo_id = path.into_inner();
    match collector.store().events_for_repo(&repo_id) {
        Ok(events) => {
            let all_stats = aggregate_repo_stats(&events);
            let stats = all_stats.get(&repo_id).cloned().unwrap_or_default();
            Ok(HttpResponse::Ok().json(stats))
        }
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn get_tenant_stats(
    collector: web::Data<Arc<AnalyticsCollector>>,
    feature_flags: web::Data<FeatureFlags>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    if !feature_flags.analytics {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "analytics feature is not enabled"})));
    }

    let tenant_id = path.into_inner();
    match collector.store().events_for_tenant(&tenant_id) {
        Ok(events) => {
            let stats = aggregate_tenant_stats(&events, 10);
            Ok(HttpResponse::Ok().json(stats))
        }
        Err(e) => {
            Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

async fn get_trends(
    collector: web::Data<Arc<AnalyticsCollector>>,
    feature_flags: web::Data<FeatureFlags>,
    query: web::Json<TrendQuery>,
) -> ActixResult<HttpResponse> {
    if !feature_flags.analytics {
        return Ok(HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "analytics feature is not enabled"})));
    }

    let all_events = match collector.store().all_events() {
        Ok(e) => e,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})))
        }
    };

    let points = build_trend(&all_events, &query);
    Ok(HttpResponse::Ok().json(points))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::collector::{AnalyticsCollector, InMemoryAnalyticsStore};
    use actix_web::{test, App};

    fn make_collector() -> Arc<AnalyticsCollector> {
        Arc::new(AnalyticsCollector::new(InMemoryAnalyticsStore::shared(
            1000,
        )))
    }

    #[actix_web::test]
    async fn test_repo_stats_feature_disabled() {
        let collector = make_collector();
        let flags = FeatureFlags {
            analytics: false,
            ..FeatureFlags::default()
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(collector))
                .app_data(web::Data::new(flags))
                .service(web::scope("/analytics").configure(configure_analytics_routes)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/analytics/repos/org%2Frepo/stats")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn test_repo_stats_returns_json() {
        let collector = make_collector();
        collector.record_clone("t1", "org/repo", Some("alice"));
        collector.record_push("t1", "org/repo", Some("bob"), Some("main"), 2);

        let flags = FeatureFlags {
            analytics: true,
            ..FeatureFlags::default()
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(collector))
                .app_data(web::Data::new(flags))
                .service(web::scope("/analytics").configure(configure_analytics_routes)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/analytics/repos/org%2Frepo/stats")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let body: crate::analytics::models::RepoStats = test::read_body_json(resp).await;
        assert_eq!(body.clone_count, 1);
        assert_eq!(body.push_count, 1);
    }

    #[actix_web::test]
    async fn test_tenant_stats_returns_json() {
        let collector = make_collector();
        collector.record_clone("alpha", "alpha/repo", None);
        collector.record_push("alpha", "alpha/repo", Some("alice"), Some("main"), 1);

        let flags = FeatureFlags {
            analytics: true,
            ..FeatureFlags::default()
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(collector))
                .app_data(web::Data::new(flags))
                .service(web::scope("/analytics").configure(configure_analytics_routes)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/analytics/tenants/alpha/stats")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let body: crate::analytics::models::TenantStats = test::read_body_json(resp).await;
        assert_eq!(body.total_clones, 1);
        assert_eq!(body.active_contributors, 1);
    }
}

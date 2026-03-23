//! Cluster API routes for horizontal scaling

use crate::api::models::ApiError;
use crate::scaling::{
    ClusterManager, HealthChecker, LeaderElection, ScalingConfig,
};
use actix_web::{web, HttpResponse, Result as ActixResult};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ClusterState {
    pub cluster_manager: Arc<RwLock<ClusterManager>>,
    pub health_checker: Arc<HealthChecker>,
    pub leader_election: Option<Arc<LeaderElection>>,
    pub config: ScalingConfig,
}

pub fn configure_cluster_routes(cfg: &mut web::ServiceConfig, state: web::Data<ClusterState>) {
    cfg.app_data(state)
        .route("/cluster/status", web::get().to(get_cluster_status))
        .route("/cluster/instances", web::get().to(get_instances))
        .route("/cluster/leader", web::get().to(get_leader))
        .route("/cluster/health/live", web::get().to(liveness_probe))
        .route("/cluster/health/ready", web::get().to(readiness_probe));
}

async fn get_cluster_status(
    state: web::Data<ClusterState>,
) -> ActixResult<HttpResponse> {
    let manager = state.cluster_manager.read().await;
    let status = manager.get_cluster_status().await;

    Ok(HttpResponse::Ok().json(status))
}

async fn get_instances(
    state: web::Data<ClusterState>,
) -> ActixResult<HttpResponse> {
    let manager = state.cluster_manager.read().await;
    let instances = manager.get_instances().await;

    Ok(HttpResponse::Ok().json(instances))
}

async fn get_leader(
    state: web::Data<ClusterState>,
) -> ActixResult<HttpResponse> {
    if let Some(ref election) = state.leader_election {
        if let Some(leader) = election.get_current_leader().await {
            return Ok(HttpResponse::Ok().json(leader));
        }
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "leader_id": null,
        "message": "No leader elected or leader election not enabled"
    })))
}

async fn liveness_probe(
    state: web::Data<ClusterState>,
) -> ActixResult<HttpResponse> {
    let status = state.health_checker.check_liveness().await;

    match status.state {
        crate::scaling::health::HealthState::Unhealthy => {
            Ok(HttpResponse::ServiceUnavailable().json(status))
        }
        _ => Ok(HttpResponse::Ok().json(status)),
    }
}

async fn readiness_probe(
    state: web::Data<ClusterState>,
) -> ActixResult<HttpResponse> {
    let status = state.health_checker.check_readiness().await;

    match status.state {
        crate::scaling::health::HealthState::Unhealthy
        | crate::scaling::health::HealthState::Starting
        | crate::scaling::health::HealthState::Draining => {
            Ok(HttpResponse::ServiceUnavailable().json(status))
        }
        _ => Ok(HttpResponse::Ok().json(status)),
    }
}

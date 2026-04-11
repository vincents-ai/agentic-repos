//! Cluster API routes for horizontal scaling

use crate::scaling::{
    ClusterManager, DistributedLock, HealthChecker, LeaderElection, LockOptions, ScalingConfig,
    SessionAffinity,
};
use actix_web::{web, HttpResponse, Result as ActixResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ClusterState {
    pub cluster_manager: Arc<RwLock<ClusterManager>>,
    pub health_checker: Arc<HealthChecker>,
    pub leader_election: Option<Arc<LeaderElection>>,
    pub config: ScalingConfig,
    pub session_affinity: Option<Arc<SessionAffinity>>,
    pub distributed_lock: Option<Arc<DistributedLock>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub session_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub instance_id: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ValidateSessionResponse {
    pub valid: bool,
    pub instance_id: String,
    pub expires_at: i64,
}

#[derive(Debug, Serialize)]
pub struct DeleteSessionResponse {
    pub deleted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcquireLockRequest {
    pub instance_id: String,
}

#[derive(Debug, Serialize)]
pub struct AcquireLockResponse {
    pub acquired: bool,
    pub lock_id: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct GetLockResponse {
    pub locked: bool,
    pub holder: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReleaseLockRequest {
    pub lock_id: String,
}

#[derive(Debug, Serialize)]
pub struct ReleaseLockResponse {
    pub released: bool,
}

pub fn configure_cluster_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/cluster/status", web::get().to(get_cluster_status))
        .route("/cluster/instances", web::get().to(get_instances))
        .route("/cluster/leader", web::get().to(get_leader))
        .route("/cluster/health/live", web::get().to(liveness_probe))
        .route("/cluster/health/ready", web::get().to(readiness_probe))
        .route("/sessions", web::post().to(create_session))
        .route("/sessions/{id}", web::get().to(validate_session))
        .route("/sessions/{id}", web::delete().to(delete_session))
        .route("/locks/{resource}", web::post().to(acquire_lock))
        .route("/locks/{resource}", web::get().to(get_lock))
        .route("/locks/{resource}", web::delete().to(release_lock));
}

async fn get_cluster_status(state: web::Data<ClusterState>) -> ActixResult<HttpResponse> {
    let manager = state.cluster_manager.read().await;
    let status = manager.get_cluster_status().await;

    Ok(HttpResponse::Ok().json(status))
}

async fn get_instances(state: web::Data<ClusterState>) -> ActixResult<HttpResponse> {
    let manager = state.cluster_manager.read().await;
    let instances = manager.get_instances().await;

    Ok(HttpResponse::Ok().json(instances))
}

async fn get_leader(state: web::Data<ClusterState>) -> ActixResult<HttpResponse> {
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

async fn liveness_probe(state: web::Data<ClusterState>) -> ActixResult<HttpResponse> {
    let status = state.health_checker.check_liveness().await;

    match status.state {
        crate::scaling::health::HealthState::Unhealthy => {
            Ok(HttpResponse::ServiceUnavailable().json(status))
        }
        _ => Ok(HttpResponse::Ok().json(status)),
    }
}

async fn readiness_probe(state: web::Data<ClusterState>) -> ActixResult<HttpResponse> {
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

pub async fn create_session(
    state: web::Data<ClusterState>,
    payload: web::Json<CreateSessionRequest>,
) -> ActixResult<HttpResponse> {
    if let Some(ref affinity) = state.session_affinity {
        let session = affinity.create_session(payload.session_id.clone()).await;
        let response = CreateSessionResponse {
            session_id: session.session_id,
            instance_id: session.instance_id,
            created_at: session.created_at.timestamp(),
        };
        Ok(HttpResponse::Ok().json(response))
    } else {
        Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": "Session affinity not enabled"
        })))
    }
}

pub async fn validate_session(
    state: web::Data<ClusterState>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let session_id = path.into_inner();

    if let Some(ref affinity) = state.session_affinity {
        if let Some(session) = affinity.validate_session(&session_id).await {
            let response = ValidateSessionResponse {
                valid: true,
                instance_id: session.instance_id,
                expires_at: session.expires_at.timestamp(),
            };
            Ok(HttpResponse::Ok().json(response))
        } else {
            let response = ValidateSessionResponse {
                valid: false,
                instance_id: String::new(),
                expires_at: 0,
            };
            Ok(HttpResponse::Ok().json(response))
        }
    } else {
        Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": "Session affinity not enabled"
        })))
    }
}

pub async fn delete_session(
    state: web::Data<ClusterState>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let session_id = path.into_inner();

    if let Some(ref affinity) = state.session_affinity {
        affinity.remove_session(&session_id).await;
        Ok(HttpResponse::Ok().json(DeleteSessionResponse { deleted: true }))
    } else {
        Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": "Session affinity not enabled"
        })))
    }
}

pub async fn acquire_lock(
    state: web::Data<ClusterState>,
    path: web::Path<String>,
    query: web::Query<AcquireLockQuery>,
    _payload: web::Json<AcquireLockRequest>,
) -> ActixResult<HttpResponse> {
    let resource = path.into_inner();
    let ttl_seconds = query.ttl_seconds.unwrap_or(30);

    if let Some(ref lock) = state.distributed_lock {
        let options =
            LockOptions::new(&resource).with_ttl(std::time::Duration::from_secs(ttl_seconds));

        match lock.acquire(options).await {
            Ok(handle) => {
                let response = AcquireLockResponse {
                    acquired: true,
                    lock_id: Some(handle.value),
                    expires_at: Some(handle.expires_at.timestamp()),
                };
                Ok(HttpResponse::Ok().json(response))
            }
            Err(_) => {
                let response = AcquireLockResponse {
                    acquired: false,
                    lock_id: None,
                    expires_at: None,
                };
                Ok(HttpResponse::Conflict().json(response))
            }
        }
    } else {
        Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": "Distributed locking not enabled"
        })))
    }
}

#[derive(Debug, Deserialize)]
pub struct AcquireLockQuery {
    pub ttl_seconds: Option<u64>,
}

pub async fn get_lock(
    state: web::Data<ClusterState>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    let _resource = path.into_inner();

    if let Some(ref _lock) = state.distributed_lock {
        let response = GetLockResponse {
            locked: false,
            holder: None,
            expires_at: None,
        };
        Ok(HttpResponse::Ok().json(response))
    } else {
        Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": "Distributed locking not enabled"
        })))
    }
}

pub async fn release_lock(
    state: web::Data<ClusterState>,
    path: web::Path<String>,
    payload: web::Json<ReleaseLockRequest>,
) -> ActixResult<HttpResponse> {
    let resource = path.into_inner();

    if let Some(ref lock) = state.distributed_lock {
        use crate::scaling::lock::LockHandle;
        let handle = LockHandle {
            key: resource,
            value: payload.lock_id.clone(),
            acquired_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(30),
        };

        match lock.release(handle).await {
            Ok(()) => Ok(HttpResponse::Ok().json(ReleaseLockResponse { released: true })),
            Err(_) => Ok(HttpResponse::Ok().json(ReleaseLockResponse { released: false })),
        }
    } else {
        Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": "Distributed locking not enabled"
        })))
    }
}

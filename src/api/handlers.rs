use crate::api::models::ApiError;
use actix_web::{HttpResponse, Result as ActixResult};

pub async fn health_check() -> ActixResult<HttpResponse> {
    let response = serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    });

    Ok(HttpResponse::Ok().json(response))
}

pub async fn api_info() -> ActixResult<HttpResponse> {
    let response = serde_json::json!({
        "name": "agentic-repos API",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Multi-tenant Git server repository management API",
        "endpoints": {
            "repositories": "/api/v1/repositories",
            "users": "/api/v1/users",
            "health": "/health",
            "info": "/info"
        }
    });

    Ok(HttpResponse::Ok().json(response))
}

pub async fn not_found() -> ActixResult<HttpResponse> {
    let error = ApiError::not_found("Endpoint not found");
    Ok(HttpResponse::NotFound().json(error))
}

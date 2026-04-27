//! # adp-sse-registry
//!
//! Server-side SSE connection registry for the agentic-loop ↔ adp integration.
//!
//! Manages SSE connections from agentic-loop instances, broadcasts events,
//! monitors TTL expiry, and provides connection lifecycle management.

mod error;
mod registry;
mod stream;

pub use error::SseRegistryError;
pub use registry::{ConnectionId, ConnectionState, SseRegistry};
pub use stream::SseStream;

use actix_web::{web, HttpRequest, HttpResponse, Resource};

/// Configuration for the SSE registry.
#[derive(Debug, Clone)]
pub struct SseRegistryConfig {
    /// How often to check for expired secrets (seconds).
    pub ttl_check_interval_secs: u64,
    /// Minimum TTL required to establish a connection (seconds).
    pub min_ttl_for_connection_secs: i64,
    /// Allowed clock skew between client and server (seconds).
    pub allowed_clock_skew_secs: i64,
    /// Channel buffer size per connection.
    pub channel_buffer_size: usize,
    /// Heartbeat interval (seconds).
    pub heartbeat_interval_secs: u64,
    /// TTL warning threshold (seconds). Send warning when TTL drops below this.
    pub ttl_warning_threshold_secs: i64,
}

impl Default for SseRegistryConfig {
    fn default() -> Self {
        Self {
            ttl_check_interval_secs: 60,
            min_ttl_for_connection_secs: 300,
            allowed_clock_skew_secs: 30,
            channel_buffer_size: 256,
            heartbeat_interval_secs: 60,
            ttl_warning_threshold_secs: 300,
        }
    }
}

/// Creates the SSE connect endpoint as an actix-web resource.
///
/// Mount at: `GET /api/agentic-loop/connect`
///
/// Headers expected:
/// - `X-Instance-Id`: Instance UUID
/// - `X-Instance-Secret`: Connection secret
/// - `X-Repo-Identifier`: Repository/project identifier
/// - `X-Instance-TTL-Remaining`: Client's TTL remaining in seconds
pub fn sse_connect_resource(
    _registry: web::Data<SseRegistry>,
) -> Resource {
    web::resource("/connect")
        .route(web::get().to(sse_connect_handler))
}

/// Actix-web handler for SSE connections.
pub async fn sse_connect_handler(
    req: HttpRequest,
    registry: web::Data<SseRegistry>,
) -> HttpResponse {
    // Extract headers
    let headers = req.headers();

    let instance_id = match headers.get("X-Instance-Id").and_then(|v| v.to_str().ok()) {
        Some(id) => id.to_string(),
        None => return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "MissingHeader",
            "message": "X-Instance-Id header is required"
        })),
    };

    let secret = match headers.get("X-Instance-Secret").and_then(|v| v.to_str().ok()) {
        Some(s) => s.to_string(),
        None => return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "MissingHeader",
            "message": "X-Instance-Secret header is required"
        })),
    };

    let repo_identifier = headers
        .get("X-Repo-Identifier")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("default")
        .to_string();

    let ttl_remaining: i64 = headers
        .get("X-Instance-TTL-Remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    // Register connection
    match registry
        .connect(instance_id, secret, repo_identifier, ttl_remaining)
        .await
    {
        Ok(stream) => HttpResponse::Ok()
            .content_type("text/event-stream")
            .insert_header(("Cache-Control", "no-cache"))
            .insert_header(("Connection", "keep-alive"))
            .streaming(stream),
        Err(e) => {
            tracing::warn!(error = %e, "SSE connection rejected");
            match e {
                SseRegistryError::SecretTooCloseToExpiry { ttl_remaining, min_required } => {
                    HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "SecretTooCloseToExpiry",
                        "message": format!("Secret expires in {} seconds. Minimum {} seconds required.", ttl_remaining, min_required),
                        "ttl_remaining": ttl_remaining,
                        "min_required_ttl": min_required,
                    }))
                }
                SseRegistryError::TtlMismatch { client_ttl, server_ttl, allowed_skew } => {
                    HttpResponse::BadRequest().json(serde_json::json!({
                        "error": "TtlMismatch",
                        "message": "Client and server TTL calculations differ beyond allowed skew",
                        "client_ttl": client_ttl,
                        "server_ttl": server_ttl,
                        "allowed_skew": allowed_skew,
                    }))
                }
                SseRegistryError::AuthenticationFailed(reason) => {
                    HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "AuthenticationFailed",
                        "message": reason,
                    }))
                }
                _ => HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": "InternalServerError",
                    "message": e.to_string(),
                })),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let config = SseRegistryConfig::default();
        assert_eq!(config.ttl_check_interval_secs, 60);
        assert_eq!(config.min_ttl_for_connection_secs, 300);
        assert_eq!(config.allowed_clock_skew_secs, 30);
        assert_eq!(config.channel_buffer_size, 256);
        assert_eq!(config.heartbeat_interval_secs, 60);
        assert_eq!(config.ttl_warning_threshold_secs, 300);
    }
}

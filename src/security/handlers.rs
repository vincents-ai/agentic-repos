use crate::api::models::ApiError;
use crate::auth::middleware::get_current_session;
use crate::features::FeatureFlags;
use crate::security::scanner::{ScanRequest, SecretScanner};
use actix_web::{web, HttpRequest, HttpResponse, Result as ActixResult};
use std::sync::Arc;

/// Simple in-process rate limiter for scan endpoint.
/// Limits to MAX_SCANS_PER_MINUTE per process (not per-user, since auth
/// already gates access and per-user state would require Redis).
mod scan_rate_limit {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    const MAX_SCANS_PER_MINUTE: u64 = 30;

    static COUNT: AtomicU64 = AtomicU64::new(0);
    static WINDOW_START: std::sync::OnceLock<std::sync::Mutex<Instant>> =
        std::sync::OnceLock::new();

    pub fn check() -> bool {
        let start = WINDOW_START.get_or_init(|| std::sync::Mutex::new(Instant::now()));
        let mut guard = start.lock().unwrap();
        if guard.elapsed().as_secs() >= 60 {
            *guard = Instant::now();
            COUNT.store(1, Ordering::Relaxed);
            return true;
        }
        drop(guard);
        let current = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        current <= MAX_SCANS_PER_MINUTE
    }
}

/// Maximum content size for scan requests (10 KB).
/// Secret scanning doesn't need large payloads — individual files only.
const MAX_SCAN_CONTENT_BYTES: usize = 10 * 1024;

pub fn configure_security_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/scan", web::post().to(scan_content))
        .route("/results/{scan_id}", web::get().to(get_scan_result));
}

/// Require authentication for security endpoints.
fn require_auth(req: &HttpRequest) -> ActixResult<()> {
    if get_current_session(req).is_some() {
        return Ok(());
    }
    tracing::warn!(
        "Unauthenticated access to security endpoint: {}",
        req.path()
    );
    Err(actix_web::error::ErrorUnauthorized(
        serde_json::json!({"error": "Authentication required"}).to_string(),
    ))
}

pub async fn scan_content(
    scanner: web::Data<Arc<SecretScanner>>,
    feature_flags: web::Data<FeatureFlags>,
    http_req: HttpRequest,
    payload: web::Json<ScanRequest>,
) -> ActixResult<HttpResponse> {
    require_auth(&http_req)?;

    // Rate limit scan requests
    if !scan_rate_limit::check() {
        tracing::warn!("Scan rate limit exceeded");
        return Ok(HttpResponse::TooManyRequests().json(serde_json::json!({
            "error": "Rate limit exceeded — maximum 30 scans per minute"
        })));
    }

    if !feature_flags.security_scanning {
        return Ok(HttpResponse::Forbidden().json(ApiError::new(
            "feature_disabled",
            "Security scanning feature is not enabled",
            403,
        )));
    }

    // Validate content size
    if payload.content.len() > MAX_SCAN_CONTENT_BYTES {
        return Ok(HttpResponse::PayloadTooLarge().json(serde_json::json!({
            "error": format!("Content exceeds maximum size of {} bytes", MAX_SCAN_CONTENT_BYTES)
        })));
    }

    // Audit log: who initiated the scan
    let user = crate::auth::middleware::get_current_session(&http_req)
        .and_then(|s| s.user.username)
        .unwrap_or_else(|| "unknown".to_string());
    let file_path = payload.file_path.clone().unwrap_or_default();
    tracing::info!(
        user = %user,
        file_path = %file_path,
        content_len = payload.content.len(),
        "Security scan initiated"
    );

    let scan_req = payload.into_inner();
    let report = scanner.scan(scan_req).await;

    // Audit log: scan results summary
    if !report.matches.is_empty() {
        tracing::warn!(
            user = %user,
            file_path = %file_path,
            findings_count = report.matches.len(),
            severities = ?report.matches.iter().map(|f| &f.severity).collect::<Vec<_>>(),
            "Security scan found secrets"
        );
    } else {
        tracing::info!(
            user = %user,
            file_path = %file_path,
            "Security scan clean — no findings"
        );
    }

    Ok(HttpResponse::Ok().json(report))
}

pub async fn get_scan_result(
    scanner: web::Data<Arc<SecretScanner>>,
    feature_flags: web::Data<FeatureFlags>,
    http_req: HttpRequest,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
    require_auth(&http_req)?;

    if !feature_flags.security_scanning {
        let error = ApiError::new(
            "feature_disabled",
            "Security scanning feature is not enabled",
            403,
        );
        return Ok(HttpResponse::Forbidden().json(error));
    }

    let scan_id = path.into_inner();

    match scanner.get_result(&scan_id).await {
        Some(result) => Ok(HttpResponse::Ok().json(result)),
        None => {
            let error = ApiError::not_found("Scan result not found");
            Ok(HttpResponse::NotFound().json(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_scan_endpoint_feature_disabled() {
        let scanner = Arc::new(SecretScanner::new());
        let mut flags = FeatureFlags::default();
        flags.security_scanning = false;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(scanner))
                .app_data(web::Data::new(flags))
                .service(web::scope("/security").configure(configure_security_routes)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/security/scan")
            .set_json(&ScanRequest {
                content: "test".to_string(),
                file_path: None,
            })
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }
}

use crate::api::models::ApiError;
use crate::features::FeatureFlags;
use crate::security::scanner::{ScanRequest, SecretScanner};
use actix_web::{web, HttpResponse, Result as ActixResult};
use std::sync::Arc;

pub fn configure_security_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/scan", web::post().to(scan_content))
        .route("/results/{scan_id}", web::get().to(get_scan_result));
}

pub async fn scan_content(
    scanner: web::Data<Arc<SecretScanner>>,
    feature_flags: web::Data<FeatureFlags>,
    payload: web::Json<ScanRequest>,
) -> ActixResult<HttpResponse> {
    if !feature_flags.security_scanning {
        let error = ApiError::new(
            "feature_disabled",
            "Security scanning feature is not enabled",
            403,
        );
        return Ok(HttpResponse::Forbidden().json(error));
    }

    let result = scanner.scan(payload.into_inner()).await;

    Ok(HttpResponse::Ok().json(result))
}

pub async fn get_scan_result(
    scanner: web::Data<Arc<SecretScanner>>,
    feature_flags: web::Data<FeatureFlags>,
    path: web::Path<String>,
) -> ActixResult<HttpResponse> {
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

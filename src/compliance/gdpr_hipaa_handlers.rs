//! GDPR EU Data Protection and HIPAA compliance HTTP handlers.
//!
//! Routes are mounted under `/api/v1/compliance/gdpr/` and
//! `/api/v1/compliance/hipaa/` alongside the existing DORA/NIS2 routes.

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use std::sync::Arc;

use crate::compliance::eu_data_protection::{
    DataProtectionImpactAssessment, DataProtectionOfficer, EuComplianceService,
    ProcessingActivity, SupervisoryAuthority,
};
use crate::compliance::hipaa::{BusinessAssociateAgreement, HipaaService};

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

pub struct GdprHipaaState {
    pub gdpr: Arc<EuComplianceService>,
    pub hipaa: Arc<HipaaService>,
}

// ---------------------------------------------------------------------------
// GDPR routes
// ---------------------------------------------------------------------------

pub fn configure_gdpr_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/gdpr")
            .route("/activities", web::get().to(gdpr_list_activities))
            .route("/activities", web::post().to(gdpr_add_activity))
            .route("/dpia", web::get().to(gdpr_list_dpias))
            .route("/dpia", web::post().to(gdpr_add_dpia))
            .route("/dpia/{id}/approve", web::post().to(gdpr_approve_dpia))
            .route("/dpo", web::get().to(gdpr_get_dpo))
            .route("/dpo", web::post().to(gdpr_appoint_dpo))
            .route("/authorities", web::get().to(gdpr_list_authorities))
            .route("/authorities", web::post().to(gdpr_add_authority))
            .route("/readiness", web::get().to(gdpr_readiness)),
    );
}

async fn gdpr_list_activities(state: web::Data<Arc<GdprHipaaState>>) -> HttpResponse {
    match state.gdpr.list_activities() {
        Ok(activities) => HttpResponse::Ok().json(activities),
        Err(e) => {
            tracing::warn!("GDPR list activities error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list processing activities"
            }))
        }
    }
}

#[derive(Deserialize)]
struct AddActivityRequest {
    name: String,
    purpose: String,
    legal_basis: String,
    controller: String,
}

async fn gdpr_add_activity(
    state: web::Data<Arc<GdprHipaaState>>,
    body: web::Json<AddActivityRequest>,
) -> HttpResponse {
    use crate::compliance::eu_data_protection::LawfulBasis;
    let basis = match body.legal_basis.to_lowercase().as_str() {
        "consent" => LawfulBasis::Consent,
        "contract" => LawfulBasis::Contract,
        "legal_obligation" => LawfulBasis::LegalObligation,
        "vital_interests" => LawfulBasis::VitalInterests,
        "public_task" => LawfulBasis::PublicTask,
        "legitimate_interests" => LawfulBasis::LegitimateInterests,
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid legal_basis. Must be one of: consent, contract, legal_obligation, vital_interests, public_task, legitimate_interests"
            }))
        }
    };
    let activity = ProcessingActivity::new(&body.name, &body.purpose, basis, &body.controller);
    match state.gdpr.add_activity(activity) {
        Ok(()) => HttpResponse::Created().json(serde_json::json!({"status": "created"})),
        Err(e) => {
            tracing::warn!("GDPR add activity error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to add processing activity"
            }))
        }
    }
}

async fn gdpr_list_dpias(state: web::Data<Arc<GdprHipaaState>>) -> HttpResponse {
    match state.gdpr.list_dpias() {
        Ok(dpias) => HttpResponse::Ok().json(dpias),
        Err(e) => {
            tracing::warn!("GDPR list DPIAs error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list DPIAs"
            }))
        }
    }
}

#[derive(Deserialize)]
struct AddDpiaRequest {
    activity_name: String,
    author: String,
}

async fn gdpr_add_dpia(
    state: web::Data<Arc<GdprHipaaState>>,
    body: web::Json<AddDpiaRequest>,
) -> HttpResponse {
    let dpia = DataProtectionImpactAssessment::new(&body.activity_name, &body.author);
    match state.gdpr.add_dpia(dpia) {
        Ok(()) => HttpResponse::Created().json(serde_json::json!({"status": "created"})),
        Err(e) => {
            tracing::warn!("GDPR add DPIA error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to add DPIA"
            }))
        }
    }
}

async fn gdpr_approve_dpia(
    state: web::Data<Arc<GdprHipaaState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id_str = path.into_inner();
    let id = match uuid::Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid UUID"}))
        }
    };
    match state.gdpr.approve_dpia(id) {
        Ok(dpia) => HttpResponse::Ok().json(dpia),
        Err(e) => {
            tracing::warn!("GDPR approve DPIA error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to approve DPIA"
            }))
        }
    }
}

async fn gdpr_get_dpo(state: web::Data<Arc<GdprHipaaState>>) -> HttpResponse {
    match state.gdpr.get_dpo() {
        Ok(dpo) => HttpResponse::Ok().json(dpo),
        Err(e) => {
            tracing::warn!("GDPR get DPO error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to get DPO"
            }))
        }
    }
}

#[derive(Deserialize)]
struct AppointDpoRequest {
    name: String,
    email: String,
    organisation: String,
}

async fn gdpr_appoint_dpo(
    state: web::Data<Arc<GdprHipaaState>>,
    body: web::Json<AppointDpoRequest>,
) -> HttpResponse {
    let dpo = DataProtectionOfficer::new(&body.name, &body.email, &body.organisation);
    match state.gdpr.appoint_dpo(dpo) {
        Ok(()) => HttpResponse::Created().json(serde_json::json!({"status": "appointed"})),
        Err(e) => {
            tracing::warn!("GDPR appoint DPO error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to appoint DPO"
            }))
        }
    }
}

async fn gdpr_list_authorities(state: web::Data<Arc<GdprHipaaState>>) -> HttpResponse {
    match state.gdpr.list_authorities() {
        Ok(authorities) => HttpResponse::Ok().json(authorities),
        Err(e) => {
            tracing::warn!("GDPR list authorities error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list authorities"
            }))
        }
    }
}

#[derive(Deserialize)]
struct AddAuthorityRequest {
    name: String,
    country: String,
    email: String,
}

async fn gdpr_add_authority(
    state: web::Data<Arc<GdprHipaaState>>,
    body: web::Json<AddAuthorityRequest>,
) -> HttpResponse {
    let authority = SupervisoryAuthority::new(&body.name, &body.country, &body.email);
    match state.gdpr.add_authority(authority) {
        Ok(()) => HttpResponse::Created().json(serde_json::json!({"status": "created"})),
        Err(e) => {
            tracing::warn!("GDPR add authority error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to add authority"
            }))
        }
    }
}

async fn gdpr_readiness(state: web::Data<Arc<GdprHipaaState>>) -> HttpResponse {
    let report = state.gdpr.gdpr_readiness_check();
    HttpResponse::Ok().json(report)
}

// ---------------------------------------------------------------------------
// HIPAA routes
// ---------------------------------------------------------------------------

pub fn configure_hipaa_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/hipaa")
            .route("/baa", web::get().to(hipaa_list_baas))
            .route("/baa", web::post().to(hipaa_create_baa))
            .route("/baa/{id}", web::get().to(hipaa_get_baa))
            .route("/baa/{id}/execute", web::post().to(hipaa_execute_baa))
            .route("/baa/{id}/terminate", web::post().to(hipaa_terminate_baa))
            .route("/baa/expiring", web::get().to(hipaa_expiring_baas))
            .route("/baa/valid", web::get().to(hipaa_all_baas_valid))
            .route("/check", web::get().to(hipaa_compliance_check)),
    );
}

async fn hipaa_list_baas(state: web::Data<Arc<GdprHipaaState>>) -> HttpResponse {
    match state.hipaa.list_baas() {
        Ok(baas) => HttpResponse::Ok().json(baas),
        Err(e) => {
            tracing::warn!("HIPAA list BAAs error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list BAAs"
            }))
        }
    }
}

#[derive(Deserialize)]
struct CreateBaaRequest {
    associate_name: String,
    associate_type: String,
    services_description: String,
}

async fn hipaa_create_baa(
    state: web::Data<Arc<GdprHipaaState>>,
    body: web::Json<CreateBaaRequest>,
) -> HttpResponse {
    use crate::compliance::hipaa::AssociateType;
    let atype = match body.associate_type.to_lowercase().as_str() {
        "cloud_provider" => AssociateType::CloudProvider,
        "saas" => AssociateType::SaasVendor,
        "consultant" => AssociateType::Consultant,
        "healthcare_provider" => AssociateType::HealthcareProvider,
        "health_plan" => AssociateType::HealthPlan,
        "third_party_admin" => AssociateType::ThirdPartyAdmin,
        "other" => AssociateType::Other,
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid associate_type"
            }))
        }
    };
    let baa =
        BusinessAssociateAgreement::new(&body.associate_name, atype, &body.services_description);
    match state.hipaa.create_baa(baa) {
        Ok(()) => HttpResponse::Created().json(serde_json::json!({"status": "created"})),
        Err(e) => {
            tracing::warn!("HIPAA create BAA error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create BAA"
            }))
        }
    }
}

async fn hipaa_get_baa(
    state: web::Data<Arc<GdprHipaaState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id_str = path.into_inner();
    let id = match uuid::Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid UUID"}))
        }
    };
    match state.hipaa.get_baa(id) {
        Ok(Some(baa)) => HttpResponse::Ok().json(baa),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": "BAA not found"})),
        Err(e) => {
            tracing::warn!("HIPAA get BAA error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to get BAA"
            }))
        }
    }
}

async fn hipaa_execute_baa(
    state: web::Data<Arc<GdprHipaaState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id_str = path.into_inner();
    let id = match uuid::Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid UUID"}))
        }
    };
    match state.hipaa.execute_baa(id) {
        Ok(baa) => HttpResponse::Ok().json(baa),
        Err(e) => {
            tracing::warn!("HIPAA execute BAA error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to execute BAA"
            }))
        }
    }
}

async fn hipaa_terminate_baa(
    state: web::Data<Arc<GdprHipaaState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id_str = path.into_inner();
    let id = match uuid::Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid UUID"}))
        }
    };
    match state.hipaa.terminate_baa(id) {
        Ok(baa) => HttpResponse::Ok().json(baa),
        Err(e) => {
            tracing::warn!("HIPAA terminate BAA error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to terminate BAA"
            }))
        }
    }
}

#[derive(Deserialize)]
struct ExpiringQuery {
    days: Option<i64>,
}

async fn hipaa_expiring_baas(
    state: web::Data<Arc<GdprHipaaState>>,
    query: web::Query<ExpiringQuery>,
) -> HttpResponse {
    let days = query.days.unwrap_or(30);
    match state.hipaa.expiring_baas(days) {
        Ok(baas) => HttpResponse::Ok().json(baas),
        Err(e) => {
            tracing::warn!("HIPAA expiring BAAs error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list expiring BAAs"
            }))
        }
    }
}

async fn hipaa_all_baas_valid(state: web::Data<Arc<GdprHipaaState>>) -> HttpResponse {
    match state.hipaa.all_baas_valid() {
        Ok(valid) => HttpResponse::Ok().json(serde_json::json!({"all_valid": valid})),
        Err(e) => {
            tracing::warn!("HIPAA all BAAs valid check error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to check BAA validity"
            }))
        }
    }
}

async fn hipaa_compliance_check(state: web::Data<Arc<GdprHipaaState>>) -> HttpResponse {
    let check = state.hipaa.run_compliance_check();
    HttpResponse::Ok().json(check)
}

// ---------------------------------------------------------------------------
// Combined configurator
// ---------------------------------------------------------------------------

pub fn configure_gdpr_hipaa_routes(
    cfg: &mut web::ServiceConfig,
    state: web::Data<Arc<GdprHipaaState>>,
) {
    let perm = crate::rbac::middleware::require_permission(
        crate::rbac::models::Permission::ComplianceManage,
    );
    cfg.app_data(state)
        .service(
            web::scope("/gdpr")
                .wrap(perm.clone())
                .route("/activities", web::get().to(gdpr_list_activities))
                .route("/activities", web::post().to(gdpr_add_activity))
                .route("/dpia", web::get().to(gdpr_list_dpias))
                .route("/dpia", web::post().to(gdpr_add_dpia))
                .route("/dpia/{id}/approve", web::post().to(gdpr_approve_dpia))
                .route("/dpo", web::get().to(gdpr_get_dpo))
                .route("/dpo", web::post().to(gdpr_appoint_dpo))
                .route("/authorities", web::get().to(gdpr_list_authorities))
                .route("/authorities", web::post().to(gdpr_add_authority))
                .route("/readiness", web::get().to(gdpr_readiness)),
        )
        .service(
            web::scope("/hipaa")
                .wrap(perm)
                .route("/baa", web::get().to(hipaa_list_baas))
                .route("/baa", web::post().to(hipaa_create_baa))
                .route("/baa/{id}", web::get().to(hipaa_get_baa))
                .route("/baa/{id}/execute", web::post().to(hipaa_execute_baa))
                .route("/baa/{id}/terminate", web::post().to(hipaa_terminate_baa))
                .route("/baa/expiring", web::get().to(hipaa_expiring_baas))
                .route("/baa/valid", web::get().to(hipaa_all_baas_valid))
                .route("/check", web::get().to(hipaa_compliance_check)),
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use crate::compliance::eu_data_protection::InMemoryEuComplianceStore;
    use crate::compliance::hipaa::InMemoryBaaStore;

    fn make_state() -> Arc<GdprHipaaState> {
        Arc::new(GdprHipaaState {
            gdpr: Arc::new(EuComplianceService::new(InMemoryEuComplianceStore::shared())),
            hipaa: Arc::new(HipaaService::new(InMemoryBaaStore::shared())),
        })
    }

    #[actix_web::test]
    async fn test_gdpr_readiness() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_gdpr_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::get().uri("/gdpr/readiness").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_hipaa_compliance_check() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_hipaa_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::get().uri("/hipaa/check").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_gdpr_add_and_list_activities() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_gdpr_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/gdpr/activities")
            .set_json(serde_json::json!({
                "name": "Email marketing",
                "purpose": "Newsletter delivery",
                "legal_basis": "consent",
                "controller": "Acme Corp"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);

        let req = test::TestRequest::get().uri("/gdpr/activities").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }
}

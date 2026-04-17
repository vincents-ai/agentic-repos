//! DORA and NIS2 compliance REST API handlers.
//!
//! HTTP endpoints for EU regulatory compliance:
//! - `/compliance/dora/*` — ICT risk management, incident reporting, resilience testing,
//!   third-party providers, information sharing
//! - `/compliance/nis2/*` — entity management, security measures, incident notification

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::dora::{
    DoraService, IctIncident, IctRiskManagementFramework, IncidentClassification,
    IncidentSeverity, InformationSharingArrangement,
    SharingType, ThirdPartyProvider, ThirdPartyServiceType,
    ProviderCriticality,
};
use super::nis2::{
    EntityType, Nis2EntityProfile, Nis2Incident, Nis2IncidentCategory, Nis2IncidentSeverity,
    Nis2Sector, Nis2Service, SecurityMeasures,
};

// ── Shared Error ──

#[derive(Debug, Serialize)]
struct ComplianceApiError {
    error: String,
    message: String,
}

fn api_err(code: &str, msg: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(ComplianceApiError {
        error: code.to_string(),
        message: msg.to_string(),
    })
}

fn internal_err(msg: &str) -> HttpResponse {
    HttpResponse::InternalServerError().json(ComplianceApiError {
        error: "internal_error".to_string(),
        message: msg.to_string(),
    })
}

fn not_found_err(msg: &str) -> HttpResponse {
    HttpResponse::NotFound().json(ComplianceApiError {
        error: "not_found".to_string(),
        message: msg.to_string(),
    })
}

// ── State ──

pub struct DoraNis2State {
    pub dora: Arc<DoraService>,
    pub nis2: Arc<Nis2Service>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// DORA Endpoints
// ═══════════════════════════════════════════════════════════════════════════════

pub fn configure_dora_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/dora")
            .route("/framework", web::get().to(dora_get_framework))
            .route("/framework", web::post().to(dora_save_framework))
            .route("/incidents", web::get().to(dora_list_incidents))
            .route("/incidents", web::post().to(dora_report_incident))
            .route("/incidents/{id}/notify", web::post().to(dora_notify_authority))
            .route("/incidents/overdue", web::get().to(dora_overdue_reports))
            .route("/testing/{year}", web::get().to(dora_get_testing))
            .route("/testing/{year}", web::post().to(dora_save_testing))
            .route("/providers", web::get().to(dora_list_providers))
            .route("/providers", web::post().to(dora_register_provider))
            .route("/providers/missing-exit", web::get().to(dora_missing_exit))
            .route("/providers/needing-audit", web::get().to(dora_needing_audit))
            .route("/sharing", web::get().to(dora_list_sharing))
            .route("/sharing", web::post().to(dora_save_sharing))
            .route("/status", web::get().to(dora_status)),
    );
}

// ── Framework ──

async fn dora_get_framework(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.dora.get_framework() {
        Ok(Some(fw)) => HttpResponse::Ok().json(fw),
        Ok(None) => not_found_err("No DORA framework configured"),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct SaveFrameworkRequest {
    approved_by: String,
}

async fn dora_save_framework(
    state: web::Data<Arc<DoraNis2State>>,
    body: web::Json<SaveFrameworkRequest>,
) -> HttpResponse {
    let fw = IctRiskManagementFramework::new(&body.approved_by);
    match state.dora.save_framework(&fw) {
        Ok(()) => HttpResponse::Created().json(&fw),
        Err(e) => internal_err(&e),
    }
}

// ── Incidents ──

async fn dora_list_incidents(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.dora.list_incidents() {
        Ok(incidents) => HttpResponse::Ok().json(serde_json::json!({
            "incidents": incidents,
            "total": incidents.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct ReportDoraIncidentRequest {
    title: String,
    description: String,
    severity: String,
    classification: String,
}

async fn dora_report_incident(
    state: web::Data<Arc<DoraNis2State>>,
    body: web::Json<ReportDoraIncidentRequest>,
) -> HttpResponse {
    let severity = match body.severity.to_lowercase().as_str() {
        "minor" => IncidentSeverity::Minor,
        "major" => IncidentSeverity::Major,
        "critical" => IncidentSeverity::Critical,
        _ => return api_err("invalid_severity", "Must be: minor, major, critical"),
    };
    let classification = match body.classification.to_lowercase().as_str() {
        "cyber_attack" => IncidentClassification::CyberAttack,
        "system_failure" => IncidentClassification::SystemFailure,
        "data_breach" => IncidentClassification::DataBreach,
        "availability" => IncidentClassification::AvailabilityIncident,
        "third_party" => IncidentClassification::ThirdPartyIncident,
        "change_failure" => IncidentClassification::ChangeFailure,
        "physical" => IncidentClassification::PhysicalIncident,
        _ => return api_err("invalid_classification", "Unknown incident classification"),
    };

    let incident = IctIncident::new(&body.title, &body.description, severity, classification);
    match state.dora.report_incident(&incident) {
        Ok(()) => HttpResponse::Created().json(&incident),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct NotifyAuthorityRequest {
    authority_reference: String,
}

async fn dora_notify_authority(
    state: web::Data<Arc<DoraNis2State>>,
    path: web::Path<uuid::Uuid>,
    body: web::Json<NotifyAuthorityRequest>,
) -> HttpResponse {
    let id = path.into_inner();
    match state.dora.notify_authority(id, &body.authority_reference) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "reported"})),
        Err(e) => not_found_err(&e),
    }
}

async fn dora_overdue_reports(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.dora.get_overdue_reports() {
        Ok(incidents) => HttpResponse::Ok().json(serde_json::json!({
            "overdue": incidents,
            "total": incidents.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

// ── Testing ──

async fn dora_get_testing(
    state: web::Data<Arc<DoraNis2State>>,
    path: web::Path<i32>,
) -> HttpResponse {
    let year = path.into_inner();
    match state.dora.get_testing_programme(year) {
        Ok(Some(p)) => HttpResponse::Ok().json(p),
        Ok(None) => not_found_err(&format!("No testing programme for {}", year)),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct SaveTestingRequest {
    approved_by: String,
}

async fn dora_save_testing(
    state: web::Data<Arc<DoraNis2State>>,
    path: web::Path<i32>,
    body: web::Json<SaveTestingRequest>,
) -> HttpResponse {
    let year = path.into_inner();
    let programme =
        super::dora::ResilienceTestingProgramme::new(year, &body.approved_by);
    match state.dora.save_testing_programme(&programme) {
        Ok(()) => HttpResponse::Created().json(&programme),
        Err(e) => internal_err(&e),
    }
}

// ── Third-party ──

async fn dora_list_providers(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.dora.list_providers() {
        Ok(providers) => HttpResponse::Ok().json(serde_json::json!({
            "providers": providers,
            "total": providers.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct RegisterProviderRequest {
    name: String,
    service_type: String,
    criticality: String,
    contract_ref: String,
}

async fn dora_register_provider(
    state: web::Data<Arc<DoraNis2State>>,
    body: web::Json<RegisterProviderRequest>,
) -> HttpResponse {
    let service_type = match body.service_type.to_lowercase().as_str() {
        "cloud" => ThirdPartyServiceType::CloudInfrastructure,
        "managed" => ThirdPartyServiceType::ManagedServices,
        "data_processing" => ThirdPartyServiceType::DataProcessing,
        "identity" => ThirdPartyServiceType::IdentityAndAccess,
        "communication" => ThirdPartyServiceType::CommunicationServices,
        "security" => ThirdPartyServiceType::SecurityServices,
        "saas" => ThirdPartyServiceType::SoftwareAsAService,
        _ => ThirdPartyServiceType::Other,
    };
    let criticality = match body.criticality.to_lowercase().as_str() {
        "low" => ProviderCriticality::Low,
        "medium" => ProviderCriticality::Medium,
        "high" => ProviderCriticality::High,
        "critical" => ProviderCriticality::Critical,
        _ => return api_err("invalid_criticality", "Must be: low, medium, high, critical"),
    };

    let provider = ThirdPartyProvider::new(
        &body.name,
        service_type,
        criticality,
        &body.contract_ref,
    );
    match state.dora.register_provider(&provider) {
        Ok(()) => HttpResponse::Created().json(&provider),
        Err(e) => internal_err(&e),
    }
}

async fn dora_missing_exit(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.dora.get_providers_missing_exit_strategy() {
        Ok(providers) => HttpResponse::Ok().json(serde_json::json!({
            "providers": providers,
            "total": providers.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

async fn dora_needing_audit(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.dora.get_critical_providers_needing_audit() {
        Ok(providers) => HttpResponse::Ok().json(serde_json::json!({
            "providers": providers,
            "total": providers.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

// ── Information Sharing ──

async fn dora_list_sharing(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.dora.list_sharing_arrangements() {
        Ok(arrangements) => HttpResponse::Ok().json(serde_json::json!({
            "arrangements": arrangements,
            "total": arrangements.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct SaveSharingRequest {
    partner_name: String,
    arrangement_type: String,
    scope: String,
}

async fn dora_save_sharing(
    state: web::Data<Arc<DoraNis2State>>,
    body: web::Json<SaveSharingRequest>,
) -> HttpResponse {
    let arr_type = match body.arrangement_type.to_lowercase().as_str() {
        "isac" => SharingType::Isac,
        "bilateral" => SharingType::Bilateral,
        "multilateral" => SharingType::Multilateral,
        "government" => SharingType::Government,
        "eu" => SharingType::EuInstitution,
        _ => return api_err("invalid_type", "Must be: isac, bilateral, multilateral, government, eu"),
    };

    let arrangement =
        InformationSharingArrangement::new(&body.partner_name, arr_type, &body.scope);
    match state.dora.save_sharing_arrangement(&arrangement) {
        Ok(()) => HttpResponse::Created().json(&arrangement),
        Err(e) => internal_err(&e),
    }
}

// ── Status ──

async fn dora_status(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    let status = state.dora.compliance_status();
    HttpResponse::Ok().json(status)
}

// ═══════════════════════════════════════════════════════════════════════════════
// NIS2 Endpoints
// ═══════════════════════════════════════════════════════════════════════════════

pub fn configure_nis2_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/nis2")
            .route("/entities", web::get().to(nis2_list_entities))
            .route("/entities", web::post().to(nis2_register_entity))
            .route("/entities/{id}", web::get().to(nis2_get_entity))
            .route("/security-measures/{entity_id}", web::get().to(nis2_get_measures))
            .route("/security-measures", web::post().to(nis2_save_measures))
            .route("/security-measures/overdue", web::get().to(nis2_overdue_assessments))
            .route("/incidents", web::get().to(nis2_list_incidents))
            .route("/incidents", web::post().to(nis2_report_incident))
            .route("/incidents/{id}/notify/initial", web::post().to(nis2_initial_notify))
            .route("/incidents/{id}/notify/detailed", web::post().to(nis2_detailed_notify))
            .route("/incidents/{id}/notify/final", web::post().to(nis2_final_report))
            .route("/incidents/overdue", web::get().to(nis2_overdue_notifications))
            .route("/status", web::get().to(nis2_status)),
    );
}

// ── Entities ──

async fn nis2_list_entities(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.nis2.list_entities() {
        Ok(entities) => HttpResponse::Ok().json(serde_json::json!({
            "entities": entities,
            "total": entities.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct RegisterEntityRequest {
    name: String,
    entity_type: String,
    sector: String,
}

async fn nis2_register_entity(
    state: web::Data<Arc<DoraNis2State>>,
    body: web::Json<RegisterEntityRequest>,
) -> HttpResponse {
    let entity_type = match body.entity_type.to_lowercase().as_str() {
        "essential" => EntityType::Essential,
        "important" => EntityType::Important,
        _ => return api_err("invalid_entity_type", "Must be: essential, important"),
    };
    let sector = match body.sector.to_lowercase().as_str() {
        "energy" => Nis2Sector::Energy,
        "transport" => Nis2Sector::Transport,
        "banking" => Nis2Sector::Banking,
        "financial" => Nis2Sector::FinancialMarketInfrastructure,
        "health" => Nis2Sector::Health,
        "water" => Nis2Sector::DrinkingWater,
        "wastewater" => Nis2Sector::Wastewater,
        "digital" => Nis2Sector::DigitalInfrastructure,
        "public_admin" => Nis2Sector::PublicAdministration,
        "space" => Nis2Sector::Space,
        "postal" => Nis2Sector::PostalServices,
        "waste" => Nis2Sector::WasteManagement,
        "chemicals" => Nis2Sector::Chemicals,
        "food" => Nis2Sector::FoodProduction,
        "manufacturing" => Nis2Sector::Manufacturing,
        "digital_providers" => Nis2Sector::DigitalProviders,
        "research" => Nis2Sector::Research,
        _ => return api_err("invalid_sector", "Unknown NIS2 sector"),
    };

    let entity = Nis2EntityProfile::new(&body.name, entity_type, sector);
    match state.nis2.register_entity(&entity) {
        Ok(()) => HttpResponse::Created().json(&entity),
        Err(e) => internal_err(&e),
    }
}

async fn nis2_get_entity(
    state: web::Data<Arc<DoraNis2State>>,
    path: web::Path<uuid::Uuid>,
) -> HttpResponse {
    let id = path.into_inner();
    match state.nis2.get_entity(id) {
        Ok(Some(e)) => HttpResponse::Ok().json(e),
        Ok(None) => not_found_err(&format!("Entity {} not found", id)),
        Err(e) => internal_err(&e),
    }
}

// ── Security Measures ──

async fn nis2_get_measures(
    state: web::Data<Arc<DoraNis2State>>,
    path: web::Path<uuid::Uuid>,
) -> HttpResponse {
    let entity_id = path.into_inner();
    match state.nis2.get_security_measures(entity_id) {
        Ok(Some(m)) => HttpResponse::Ok().json(m),
        Ok(None) => not_found_err(&format!("No security measures for entity {}", entity_id)),
        Err(e) => internal_err(&e),
    }
}

async fn nis2_save_measures(
    state: web::Data<Arc<DoraNis2State>>,
    body: web::Json<SaveMeasuresRequest>,
) -> HttpResponse {
    let measures = SecurityMeasures::new(body.entity_id);
    match state.nis2.save_security_measures(&measures) {
        Ok(()) => HttpResponse::Created().json(&measures),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct SaveMeasuresRequest {
    entity_id: uuid::Uuid,
}

async fn nis2_overdue_assessments(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.nis2.get_entities_with_overdue_assessments() {
        Ok(entities) => HttpResponse::Ok().json(serde_json::json!({
            "entities": entities,
            "total": entities.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

// ── Incidents ──

async fn nis2_list_incidents(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.nis2.list_incidents() {
        Ok(incidents) => HttpResponse::Ok().json(serde_json::json!({
            "incidents": incidents,
            "total": incidents.len(),
        })),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct ReportNis2IncidentRequest {
    entity_id: uuid::Uuid,
    title: String,
    description: String,
    severity: String,
    category: String,
}

async fn nis2_report_incident(
    state: web::Data<Arc<DoraNis2State>>,
    body: web::Json<ReportNis2IncidentRequest>,
) -> HttpResponse {
    let severity = match body.severity.to_lowercase().as_str() {
        "low" => Nis2IncidentSeverity::Low,
        "medium" => Nis2IncidentSeverity::Medium,
        "significant" => Nis2IncidentSeverity::Significant,
        "major" => Nis2IncidentSeverity::Major,
        _ => return api_err("invalid_severity", "Must be: low, medium, significant, major"),
    };
    let category = match body.category.to_lowercase().as_str() {
        "compromise" => Nis2IncidentCategory::SystemCompromise,
        "data_breach" => Nis2IncidentCategory::DataBreach,
        "dos" => Nis2IncidentCategory::DenialOfService,
        "malware" => Nis2IncidentCategory::Malware,
        "phishing" => Nis2IncidentCategory::Phishing,
        "ransomware" => Nis2IncidentCategory::Ransomware,
        "insider" => Nis2IncidentCategory::InsiderThreat,
        "supply_chain" => Nis2IncidentCategory::SupplyChainAttack,
        "misconfiguration" => Nis2IncidentCategory::Misconfiguration,
        "physical" => Nis2IncidentCategory::PhysicalIncident,
        _ => Nis2IncidentCategory::Other,
    };

    let incident = Nis2Incident::new(
        body.entity_id,
        &body.title,
        &body.description,
        severity,
        category,
    );
    match state.nis2.report_incident(&incident) {
        Ok(()) => HttpResponse::Created().json(&incident),
        Err(e) => internal_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct NotificationRequest {
    csirt_reference: String,
    summary: String,
}

async fn nis2_initial_notify(
    state: web::Data<Arc<DoraNis2State>>,
    path: web::Path<uuid::Uuid>,
    body: web::Json<NotificationRequest>,
) -> HttpResponse {
    let id = path.into_inner();
    match state
        .nis2
        .submit_initial_notification(id, &body.csirt_reference, &body.summary)
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "initial_notified"})),
        Err(e) => not_found_err(&e),
    }
}

async fn nis2_detailed_notify(
    state: web::Data<Arc<DoraNis2State>>,
    path: web::Path<uuid::Uuid>,
    body: web::Json<NotificationRequest>,
) -> HttpResponse {
    let id = path.into_inner();
    match state
        .nis2
        .submit_detailed_notification(id, &body.csirt_reference, &body.summary)
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "detailed_notified"})),
        Err(e) => not_found_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct FinalReportRequest {
    csirt_reference: String,
    summary: String,
    root_cause: String,
}

async fn nis2_final_report(
    state: web::Data<Arc<DoraNis2State>>,
    path: web::Path<uuid::Uuid>,
    body: web::Json<FinalReportRequest>,
) -> HttpResponse {
    let id = path.into_inner();
    match state
        .nis2
        .submit_final_report(id, &body.csirt_reference, &body.summary, &body.root_cause)
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "closed"})),
        Err(e) => not_found_err(&e),
    }
}

async fn nis2_overdue_notifications(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    match state.nis2.get_overdue_notifications() {
        Ok(report) => HttpResponse::Ok().json(report),
        Err(e) => internal_err(&e),
    }
}

async fn nis2_status(state: web::Data<Arc<DoraNis2State>>) -> HttpResponse {
    let status = state.nis2.compliance_status();
    HttpResponse::Ok().json(status)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Combined Route Configuration
// ═══════════════════════════════════════════════════════════════════════════════

pub fn configure_dora_nis2_routes(cfg: &mut web::ServiceConfig, state: web::Data<Arc<DoraNis2State>>) {
    cfg.app_data(state);
    configure_dora_routes(cfg);
    configure_nis2_routes(cfg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::dora::{DoraService, InMemoryDoraStore};
    use crate::compliance::nis2::{InMemoryNis2Store, Nis2Service};
    use actix_web::{test, App};
    use serde_json::json;

    fn make_state() -> Arc<DoraNis2State> {
        Arc::new(DoraNis2State {
            dora: Arc::new(DoraService::new(Arc::new(InMemoryDoraStore::new()))),
            nis2: Arc::new(Nis2Service::new(Arc::new(InMemoryNis2Store::new()))),
        })
    }

    // ── DORA tests ──

    #[actix_web::test]
    async fn test_dora_save_and_get_framework() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_dora_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/dora/framework")
            .set_json(json!({"approved_by": "CTO"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);

        let req = test::TestRequest::get()
            .uri("/dora/framework")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_dora_report_incident() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_dora_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/dora/incidents")
            .set_json(json!({
                "title": "Git outage",
                "description": "SSH unavailable",
                "severity": "major",
                "classification": "availability"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn test_dora_list_incidents_empty() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_dora_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/dora/incidents")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_dora_register_provider() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_dora_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/dora/providers")
            .set_json(json!({
                "name": "AWS",
                "service_type": "cloud",
                "criticality": "critical",
                "contract_ref": "AWS-2026-001"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn test_dora_status() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_dora_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/dora/status")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_dora_save_sharing() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_dora_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/dora/sharing")
            .set_json(json!({
                "partner_name": "FS-ISAC",
                "arrangement_type": "isac",
                "scope": "Threat intelligence"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    // ── NIS2 tests ──

    #[actix_web::test]
    async fn test_nis2_register_entity() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_nis2_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/nis2/entities")
            .set_json(json!({
                "name": "GitCorp",
                "entity_type": "essential",
                "sector": "digital"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn test_nis2_list_entities_empty() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_nis2_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/nis2/entities")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_nis2_report_incident() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_nis2_routes(cfg)),
        )
        .await;

        // Register entity first
        let entity = Nis2EntityProfile::new("Test", EntityType::Essential, Nis2Sector::DigitalInfrastructure);
        let eid = entity.id;
        state.nis2.register_entity(&entity).unwrap();

        let req = test::TestRequest::post()
            .uri("/nis2/incidents")
            .set_json(json!({
                "entity_id": eid.to_string(),
                "title": "Data breach",
                "description": "Customer data exposed",
                "severity": "significant",
                "category": "data_breach"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn test_nis2_status() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_nis2_routes(cfg)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/nis2/status")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_nis2_save_measures() {
        let state = web::Data::new(make_state());
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(|cfg| configure_nis2_routes(cfg)),
        )
        .await;

        let entity = Nis2EntityProfile::new("Test", EntityType::Essential, Nis2Sector::Banking);
        let eid = entity.id;
        state.nis2.register_entity(&entity).unwrap();

        let req = test::TestRequest::post()
            .uri("/nis2/security-measures")
            .set_json(json!({"entity_id": eid.to_string()}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }
}


#[cfg(test)]
mod request_tests {
    use super::*;

    #[test]
    fn test_report_dora_incident_request() {
        let json = serde_json::json!({
            "title": "DB outage",
            "description": "Database unreachable",
            "severity": "high",
            "classification": "availability"
        });
        let req: ReportDoraIncidentRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.title, "DB outage");
        assert_eq!(req.severity, "high");
    }

    #[test]
    fn test_register_provider_request() {
        let json = serde_json::json!({
            "name": "CloudHost",
            "service_type": "hosting",
            "criticality": "high",
            "contract_ref": "CTR-001"
        });
        let req: RegisterProviderRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "CloudHost");
    }

    #[test]
    fn test_register_entity_request() {
        let json = serde_json::json!({
            "name": "Acme Corp",
            "entity_type": "essential",
            "sector": "energy"
        });
        let req: RegisterEntityRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "Acme Corp");
    }

    #[test]
    fn test_report_nis2_incident_request() {
        let json = serde_json::json!({
            "entity_id": "550e8400-e29b-41d4-a716-446655440000",
            "title": "Ransomware",
            "description": "Detected",
            "severity": "critical",
            "category": "malware"
        });
        let req: ReportNis2IncidentRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.title, "Ransomware");
    }

    #[test]
    fn test_save_framework_request() {
        let json = serde_json::json!({"approved_by": "CISO"});
        let req: SaveFrameworkRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.approved_by, "CISO");
    }

    #[test]
    fn test_notify_authority_request() {
        let json = serde_json::json!({"authority_reference": "BAFin-2026-001"});
        let req: NotifyAuthorityRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.authority_reference, "BAFin-2026-001");
    }
}

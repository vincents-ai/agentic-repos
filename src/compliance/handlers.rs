use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::audit::{ComplianceAuditStorage, InMemoryComplianceAuditStorage};
use super::privacy::PrivacyControls;
use super::reports::{
    ComplianceReportGenerator, ComplianceReportStorage, InMemoryReportStorage, ReportType,
};
use super::retention::{DataRetentionPolicy, DataRetentionService};
use super::ComplianceConfig;

#[derive(Debug, Deserialize)]
pub struct GenerateReportRequest {
    pub report_type: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExportDataRequest {
    pub user_id: String,
    pub anonymize: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteDataRequest {
    pub user_id: String,
    pub anonymize_instead: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePolicyRequest {
    pub name: String,
    pub description: String,
    pub data_type: String,
    pub retention_period_days: u32,
    pub action: String,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct EnforceRetentionRequest {
    pub policy_id: String,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ReportListResponse {
    pub reports: Vec<ReportSummary>,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct ReportSummary {
    pub id: String,
    pub report_type: String,
    pub tenant_id: String,
    pub generated_at: String,
    pub status: String,
    pub compliance_score: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

pub struct ComplianceState<S: ComplianceAuditStorage> {
    pub audit_storage: Arc<S>,
    pub report_storage: Arc<dyn ComplianceReportStorage>,
    pub report_generator: ComplianceReportGenerator<S>,
    pub privacy_controls: PrivacyControls<S>,
    pub retention_service: DataRetentionService<S>,
    pub config: ComplianceConfig,
}

impl ComplianceState<InMemoryComplianceAuditStorage> {
    pub fn new(config: ComplianceConfig) -> Self {
        let audit_storage = Arc::new(InMemoryComplianceAuditStorage::new(100000));
        let report_storage = Arc::new(InMemoryReportStorage::new(1000));
        let report_generator = ComplianceReportGenerator::new(audit_storage.clone());
        let privacy_controls = PrivacyControls::new(audit_storage.clone());
        let retention_service = DataRetentionService::new(audit_storage.clone());

        Self {
            audit_storage,
            report_storage,
            report_generator,
            privacy_controls,
            retention_service,
            config,
        }
    }
}

pub fn configure_compliance_routes<S: ComplianceAuditStorage + 'static>(
    cfg: &mut web::ServiceConfig,
    state: web::Data<ComplianceState<S>>,
) {
    cfg.app_data(state)
        .route("/reports", web::get().to(list_reports::<S>))
        .route("/reports", web::post().to(generate_report::<S>))
        .route("/reports/{id}", web::get().to(get_report::<S>))
        .route(
            "/reports/{id}/download",
            web::get().to(download_report::<S>),
        )
        .route("/gdpr/data-export", web::get().to(export_user_data::<S>))
        .route(
            "/gdpr/data-export",
            web::post().to(export_user_data_post::<S>),
        )
        .route("/gdpr/data", web::delete().to(delete_user_data::<S>))
        .route(
            "/retention/policies",
            web::get().to(list_retention_policies::<S>),
        )
        .route(
            "/retention/policies",
            web::post().to(create_retention_policy::<S>),
        )
        .route(
            "/retention/policies/{id}",
            web::get().to(get_retention_policy::<S>),
        )
        .route(
            "/retention/policies/{id}",
            web::delete().to(delete_retention_policy::<S>),
        )
        .route("/retention/enforce", web::post().to(enforce_retention::<S>))
        .route(
            "/retention/enforce-all",
            web::post().to(enforce_all_retention::<S>),
        )
        .route(
            "/retention/summary",
            web::get().to(get_retention_summary::<S>),
        );
}

async fn list_reports<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    query: web::Query<ListReportsQuery>,
) -> HttpResponse {
    let report_type = query
        .report_type
        .as_ref()
        .and_then(|s| s.parse::<ReportType>().ok());

    match state
        .report_storage
        .list(
            query.tenant_id.as_deref(),
            report_type,
            query.limit.unwrap_or(50),
            query.offset.unwrap_or(0),
        )
        .await
    {
        Ok(reports) => {
            let summaries: Vec<ReportSummary> = reports
                .iter()
                .map(|r| ReportSummary {
                    id: r.id.clone(),
                    report_type: r.report_type.to_string(),
                    tenant_id: r.tenant_id.clone(),
                    generated_at: r.generated_at.to_rfc3339(),
                    status: format!("{:?}", r.status).to_lowercase(),
                    compliance_score: r.compliance_score,
                })
                .collect();

            HttpResponse::Ok().json(ReportListResponse {
                total: summaries.len() as u64,
                reports: summaries,
            })
        }
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "storage_error".to_string(),
            message: e.to_string(),
        }),
    }
}

#[derive(Debug, Deserialize)]
struct ListReportsQuery {
    tenant_id: Option<String>,
    report_type: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn generate_report<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    body: web::Json<GenerateReportRequest>,
    tenant: web::ReqData<String>,
) -> HttpResponse {
    if !state.config.reports_enabled {
        return HttpResponse::Forbidden().json(ApiError {
            error: "feature_disabled".to_string(),
            message: "Compliance reports are not enabled".to_string(),
        });
    }

    let report_type = match body.report_type.parse::<ReportType>() {
        Ok(rt) => rt,
        Err(e) => {
            return HttpResponse::BadRequest().json(ApiError {
                error: "invalid_report_type".to_string(),
                message: e.to_string(),
            })
        }
    };

    let period_end = body
        .period_end
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let period_start = body
        .period_start
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| period_end - chrono::Duration::days(30));

    match state
        .report_generator
        .generate_report(report_type, &tenant, period_start, period_end, None)
        .await
    {
        Ok(report) => {
            let report_clone = report.clone();
            if let Err(e) = state.report_storage.store(report).await {
                tracing::warn!("Failed to store report: {}", e);
            }
            HttpResponse::Created().json(report_clone)
        }
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "generation_failed".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn get_report<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    path: web::Path<String>,
) -> HttpResponse {
    let report_id = path.into_inner();

    match state.report_storage.get(&report_id).await {
        Ok(Some(report)) => HttpResponse::Ok().json(report),
        Ok(None) => HttpResponse::NotFound().json(ApiError {
            error: "not_found".to_string(),
            message: format!("Report {} not found", report_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "storage_error".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn download_report<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    path: web::Path<String>,
    query: web::Query<DownloadQuery>,
) -> HttpResponse {
    let report_id = path.into_inner();
    let format = query.format.as_deref().unwrap_or("json");

    match state.report_storage.get(&report_id).await {
        Ok(Some(report)) => match format {
            "csv" => match ComplianceReportGenerator::<S>::format_report_as_csv(&report) {
                Ok(csv) => HttpResponse::Ok()
                    .content_type("text/csv")
                    .insert_header((
                        "Content-Disposition",
                        format!(
                            "attachment; filename=\"compliance-report-{}.csv\"",
                            report_id
                        ),
                    ))
                    .body(csv),
                Err(e) => HttpResponse::InternalServerError().json(ApiError {
                    error: "conversion_failed".to_string(),
                    message: e.to_string(),
                }),
            },
            _ => HttpResponse::Ok()
                .content_type("application/json")
                .insert_header((
                    "Content-Disposition",
                    format!(
                        "attachment; filename=\"compliance-report-{}.json\"",
                        report_id
                    ),
                ))
                .json(&report),
        },
        Ok(None) => HttpResponse::NotFound().json(ApiError {
            error: "not_found".to_string(),
            message: format!("Report {} not found", report_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "storage_error".to_string(),
            message: e.to_string(),
        }),
    }
}

#[derive(Debug, Deserialize)]
struct DownloadQuery {
    format: Option<String>,
}

async fn export_user_data<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    query: web::Query<ExportDataQuery>,
    tenant: web::ReqData<String>,
) -> HttpResponse {
    if !state.config.gdpr_tools_enabled {
        return HttpResponse::Forbidden().json(ApiError {
            error: "feature_disabled".to_string(),
            message: "GDPR tools are not enabled".to_string(),
        });
    }

    match state
        .privacy_controls
        .export_user_data(&query.user_id, &tenant, query.anonymize.unwrap_or(true))
        .await
    {
        Ok(export) => HttpResponse::Ok().json(export),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "export_failed".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn export_user_data_post<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    body: web::Json<ExportDataRequest>,
    tenant: web::ReqData<String>,
) -> HttpResponse {
    if !state.config.gdpr_tools_enabled {
        return HttpResponse::Forbidden().json(ApiError {
            error: "feature_disabled".to_string(),
            message: "GDPR tools are not enabled".to_string(),
        });
    }

    match state
        .privacy_controls
        .export_user_data(&body.user_id, &tenant, body.anonymize.unwrap_or(true))
        .await
    {
        Ok(export) => HttpResponse::Ok().json(export),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "export_failed".to_string(),
            message: e.to_string(),
        }),
    }
}

#[derive(Debug, Deserialize)]
struct ExportDataQuery {
    user_id: String,
    anonymize: Option<bool>,
}

async fn delete_user_data<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    body: web::Json<DeleteDataRequest>,
    tenant: web::ReqData<String>,
) -> HttpResponse {
    if !state.config.gdpr_tools_enabled {
        return HttpResponse::Forbidden().json(ApiError {
            error: "feature_disabled".to_string(),
            message: "GDPR tools are not enabled".to_string(),
        });
    }

    match state
        .privacy_controls
        .delete_user_data(
            &body.user_id,
            &tenant,
            body.anonymize_instead
                .unwrap_or(state.config.anonymize_on_deletion),
        )
        .await
    {
        Ok(request) => HttpResponse::Accepted().json(request),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "deletion_failed".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn list_retention_policies<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
) -> HttpResponse {
    match state.retention_service.list_policies().await {
        Ok(policies) => HttpResponse::Ok().json(policies),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "storage_error".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn create_retention_policy<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    body: web::Json<CreatePolicyRequest>,
) -> HttpResponse {
    let action =
        match body.action.to_lowercase().as_str() {
            "delete" => super::retention::RetentionAction::Delete,
            "anonymize" => super::retention::RetentionAction::Anonymize,
            "archive" => super::retention::RetentionAction::Archive,
            "notify" => super::retention::RetentionAction::Notify,
            _ => return HttpResponse::BadRequest().json(ApiError {
                error: "invalid_action".to_string(),
                message:
                    "Invalid retention action. Must be one of: delete, anonymize, archive, notify"
                        .to_string(),
            }),
        };

    let policy = DataRetentionPolicy {
        policy_id: uuid::Uuid::new_v4().to_string(),
        name: body.name.clone(),
        description: body.description.clone(),
        data_type: body.data_type.clone(),
        retention_period_days: body.retention_period_days,
        action,
        enabled: body.enabled.unwrap_or(true),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        exceptions: Vec::new(),
    };

    match state.retention_service.add_policy(policy).await {
        Ok(policy_id) => {
            if let Some(policy) = state
                .retention_service
                .get_policy(&policy_id)
                .await
                .ok()
                .flatten()
            {
                HttpResponse::Created().json(policy)
            } else {
                HttpResponse::Created().json(serde_json::json!({ "policy_id": policy_id }))
            }
        }
        Err(e) => HttpResponse::BadRequest().json(ApiError {
            error: "creation_failed".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn get_retention_policy<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    path: web::Path<String>,
) -> HttpResponse {
    let policy_id = path.into_inner();

    match state.retention_service.get_policy(&policy_id).await {
        Ok(Some(policy)) => HttpResponse::Ok().json(policy),
        Ok(None) => HttpResponse::NotFound().json(ApiError {
            error: "not_found".to_string(),
            message: format!("Policy {} not found", policy_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "storage_error".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn delete_retention_policy<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    path: web::Path<String>,
) -> HttpResponse {
    let policy_id = path.into_inner();

    match state.retention_service.delete_policy(&policy_id).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => HttpResponse::NotFound().json(ApiError {
            error: "not_found".to_string(),
            message: format!("Policy {} not found", policy_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "deletion_failed".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn enforce_retention<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    body: web::Json<EnforceRetentionRequest>,
    tenant: web::ReqData<String>,
) -> HttpResponse {
    match state
        .retention_service
        .enforce_retention(&tenant, &body.policy_id, body.dry_run.unwrap_or(false))
        .await
    {
        Ok(execution) => HttpResponse::Ok().json(execution),
        Err(e) => HttpResponse::BadRequest().json(ApiError {
            error: "enforcement_failed".to_string(),
            message: e.to_string(),
        }),
    }
}

async fn enforce_all_retention<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    query: web::Query<EnforceAllQuery>,
    tenant: web::ReqData<String>,
) -> HttpResponse {
    match state
        .retention_service
        .enforce_all_policies(&tenant, query.dry_run.unwrap_or(false))
        .await
    {
        Ok(executions) => HttpResponse::Ok().json(executions),
        Err(e) => HttpResponse::BadRequest().json(ApiError {
            error: "enforcement_failed".to_string(),
            message: e.to_string(),
        }),
    }
}

#[derive(Debug, Deserialize)]
struct EnforceAllQuery {
    dry_run: Option<bool>,
}

async fn get_retention_summary<S: ComplianceAuditStorage>(
    state: web::Data<ComplianceState<S>>,
    tenant: web::ReqData<String>,
) -> HttpResponse {
    match state.retention_service.get_retention_summary(&tenant).await {
        Ok(summary) => HttpResponse::Ok().json(summary),
        Err(e) => HttpResponse::InternalServerError().json(ApiError {
            error: "storage_error".to_string(),
            message: e.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{test, App};

    use super::*;

    #[actix_web::test]
    async fn test_list_reports_empty() {
        let config = ComplianceConfig::default();
        let state = web::Data::new(ComplianceState::new(config));

        let app = test::init_service(App::new().app_data(state.clone()).service(
            web::scope("/api/v1/compliance").configure(|cfg| {
                configure_compliance_routes(cfg, state);
            }),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/api/v1/compliance/reports")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}

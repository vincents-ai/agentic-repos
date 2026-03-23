use crate::rbac::middleware::require_permission;
use crate::rbac::models::Permission;
use crate::webhooks::models::{Webhook, WebhookEvent};
use crate::webhooks::service::{WebhookError, WebhookService};
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateWebhookRequest {
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<WebhookEvent>,
    pub tenant_id: String,
}

#[derive(Deserialize)]
pub struct UpdateWebhookRequest {
    pub url: Option<String>,
    pub secret: Option<String>,
    pub events: Option<Vec<WebhookEvent>>,
    pub active: Option<bool>,
}

#[derive(Serialize)]
pub struct WebhookResponse {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub tenant_id: String,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct WebhookDeliveryResponse {
    pub id: String,
    pub webhook_id: String,
    pub event: String,
    pub status: String,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
    pub duration_ms: u64,
    pub delivered_at: chrono::DateTime<chrono::Utc>,
}

impl From<Webhook> for WebhookResponse {
    fn from(webhook: Webhook) -> Self {
        Self {
            id: webhook.id.to_string(),
            url: webhook.url,
            events: webhook.events.iter().map(|e| e.to_string()).collect(),
            tenant_id: webhook.tenant_id,
            active: webhook.active,
            created_at: webhook.created_at,
            updated_at: webhook.updated_at,
        }
    }
}

impl From<crate::webhooks::models::WebhookDelivery> for WebhookDeliveryResponse {
    fn from(delivery: crate::webhooks::models::WebhookDelivery) -> Self {
        Self {
            id: delivery.id.to_string(),
            webhook_id: delivery.webhook_id.to_string(),
            event: delivery.event.to_string(),
            status: match delivery.status {
                crate::webhooks::models::DeliveryStatus::Success => "success",
                crate::webhooks::models::DeliveryStatus::Failed => "failed",
                crate::webhooks::models::DeliveryStatus::Timeout => "timeout",
            }
            .to_string(),
            status_code: delivery.status_code,
            error_message: delivery.error_message,
            duration_ms: delivery.duration_ms,
            delivered_at: delivery.delivered_at,
        }
    }
}

fn error_response(error: WebhookError) -> HttpResponse {
    match error {
        WebhookError::NotFound(_) => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Webhook not found",
            "message": error.to_string()
        })),
        WebhookError::InvalidUrl(_) => HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid URL",
            "message": error.to_string()
        })),
        WebhookError::Storage(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Storage error",
            "message": error.to_string()
        })),
        _ => HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Request failed",
            "message": error.to_string()
        })),
    }
}

pub async fn create_webhook(
    webhook_service: web::Data<Arc<WebhookService>>,
    req: web::Json<CreateWebhookRequest>,
) -> Result<HttpResponse> {
    let events: HashSet<WebhookEvent> = req.events.iter().cloned().collect();

    match webhook_service
        .register_webhook(
            req.url.clone(),
            req.secret.clone(),
            events,
            req.tenant_id.clone(),
        )
        .await
    {
        Ok(webhook) => Ok(HttpResponse::Created().json(WebhookResponse::from(webhook))),
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn list_webhooks(
    webhook_service: web::Data<Arc<WebhookService>>,
    query: web::Query<ListWebhooksQuery>,
) -> Result<HttpResponse> {
    let webhooks = if let Some(tenant_id) = &query.tenant_id {
        webhook_service.list_webhooks_by_tenant(tenant_id).await
    } else {
        webhook_service.list_webhooks().await
    };

    match webhooks {
        Ok(hooks) => {
            let responses: Vec<WebhookResponse> =
                hooks.into_iter().map(WebhookResponse::from).collect();
            Ok(HttpResponse::Ok().json(responses))
        }
        Err(e) => Ok(error_response(e)),
    }
}

#[derive(Deserialize)]
pub struct ListWebhooksQuery {
    pub tenant_id: Option<String>,
}

pub async fn get_webhook(
    webhook_service: web::Data<Arc<WebhookService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid webhook ID"
            })));
        }
    };

    match webhook_service.get_webhook(&id).await {
        Ok(webhook) => Ok(HttpResponse::Ok().json(WebhookResponse::from(webhook))),
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn update_webhook(
    webhook_service: web::Data<Arc<WebhookService>>,
    path: web::Path<String>,
    req: web::Json<UpdateWebhookRequest>,
) -> Result<HttpResponse> {
    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid webhook ID"
            })));
        }
    };

    let mut webhook = match webhook_service.get_webhook(&id).await {
        Ok(w) => w,
        Err(e) => return Ok(error_response(e)),
    };

    if let Some(url) = &req.url {
        webhook.url = url.clone();
    }
    if let Some(events) = &req.events {
        webhook.events = events.iter().cloned().collect();
    }
    if let Some(active) = req.active {
        if active {
            webhook.activate();
        } else {
            webhook.deactivate();
        }
    }

    match webhook_service.update_webhook(webhook).await {
        Ok(updated) => Ok(HttpResponse::Ok().json(WebhookResponse::from(updated))),
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn delete_webhook(
    webhook_service: web::Data<Arc<WebhookService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid webhook ID"
            })));
        }
    };

    match webhook_service.delete_webhook(&id).await {
        Ok(()) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Webhook deleted successfully"
        }))),
        Err(e) => Ok(error_response(e)),
    }
}

pub async fn get_webhook_deliveries(
    webhook_service: web::Data<Arc<WebhookService>>,
    path: web::Path<String>,
    query: web::Query<DeliveryQuery>,
) -> Result<HttpResponse> {
    let id_str = path.into_inner();
    let id = match Uuid::parse_str(&id_str) {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid webhook ID"
            })));
        }
    };

    let limit = query.limit.unwrap_or(50).min(100);

    match webhook_service.get_deliveries(&id, limit).await {
        Ok(deliveries) => {
            let responses: Vec<WebhookDeliveryResponse> = deliveries
                .into_iter()
                .map(WebhookDeliveryResponse::from)
                .collect();
            Ok(HttpResponse::Ok().json(responses))
        }
        Err(e) => Ok(error_response(e)),
    }
}

#[derive(Deserialize)]
pub struct DeliveryQuery {
    pub limit: Option<usize>,
}

pub fn configure_webhook_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/webhooks")
            .route(
                "",
                web::post()
                    .to(create_webhook)
                    .wrap(require_permission(Permission::WebhookManage)),
            )
            .route(
                "",
                web::get()
                    .to(list_webhooks)
                    .wrap(require_permission(Permission::WebhookManage)),
            )
            .route(
                "/{id}",
                web::get()
                    .to(get_webhook)
                    .wrap(require_permission(Permission::WebhookManage)),
            )
            .route(
                "/{id}",
                web::put()
                    .to(update_webhook)
                    .wrap(require_permission(Permission::WebhookManage)),
            )
            .route(
                "/{id}",
                web::delete()
                    .to(delete_webhook)
                    .wrap(require_permission(Permission::WebhookManage)),
            )
            .route(
                "/{id}/deliveries",
                web::get()
                    .to(get_webhook_deliveries)
                    .wrap(require_permission(Permission::WebhookManage)),
            ),
    );
}

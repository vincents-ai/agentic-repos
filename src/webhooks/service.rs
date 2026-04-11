use crate::webhooks::models::{Webhook, WebhookDelivery, WebhookEvent, WebhookPayload};
use crate::webhooks::storage::{WebhookStorage, WebhookStorageError};
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("Webhook not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    Storage(#[from] WebhookStorageError),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Delivery failed: {0}")]
    DeliveryFailed(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
}

pub type WebhookResult<T> = Result<T, WebhookError>;

#[allow(dead_code)]
pub struct WebhookService {
    storage: Arc<dyn WebhookStorage>,
    client: Client,
    timeout_seconds: u64,
}

impl WebhookService {
    pub fn new(storage: Arc<dyn WebhookStorage>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            storage,
            client,
            timeout_seconds: 10,
        }
    }

    pub fn with_timeout(storage: Arc<dyn WebhookStorage>, timeout_seconds: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            storage,
            client,
            timeout_seconds,
        }
    }

    pub async fn register_webhook(
        &self,
        url: String,
        secret: Option<String>,
        secret_keys: Vec<String>,
        events: HashSet<WebhookEvent>,
        tenant_id: String,
    ) -> WebhookResult<Webhook> {
        self.validate_url(&url)?;

        let mut webhook = Webhook::new(url, secret, events, tenant_id);
        webhook.secret_keys = secret_keys;
        self.storage.create(webhook.clone()).await?;

        info!(
            "Registered webhook {} for tenant {}",
            webhook.id, webhook.tenant_id
        );
        Ok(webhook)
    }

    pub async fn get_webhook(&self, id: &Uuid) -> WebhookResult<Webhook> {
        self.storage
            .get(id)
            .await?
            .ok_or_else(|| WebhookError::NotFound(id.to_string()))
    }

    pub async fn list_webhooks(&self) -> WebhookResult<Vec<Webhook>> {
        self.storage.list().await.map_err(WebhookError::from)
    }

    pub async fn list_webhooks_by_tenant(&self, tenant_id: &str) -> WebhookResult<Vec<Webhook>> {
        self.storage
            .get_by_tenant(tenant_id)
            .await
            .map_err(WebhookError::from)
    }

    pub async fn delete_webhook(&self, id: &Uuid) -> WebhookResult<()> {
        if !self.storage.exists(id).await? {
            return Err(WebhookError::NotFound(id.to_string()));
        }

        self.storage.delete(id).await?;
        info!("Deleted webhook {}", id);
        Ok(())
    }

    pub async fn update_webhook(&self, webhook: Webhook) -> WebhookResult<Webhook> {
        self.validate_url(&webhook.url)?;

        let mut updated = webhook.clone();
        updated.updated_at = chrono::Utc::now();

        self.storage.update(updated.clone()).await?;
        info!("Updated webhook {}", updated.id);
        Ok(updated)
    }

    pub async fn activate_webhook(&self, id: &Uuid) -> WebhookResult<Webhook> {
        let mut webhook = self.get_webhook(id).await?;
        webhook.activate();
        self.storage.update(webhook.clone()).await?;
        info!("Activated webhook {}", id);
        Ok(webhook)
    }

    pub async fn deactivate_webhook(&self, id: &Uuid) -> WebhookResult<Webhook> {
        let mut webhook = self.get_webhook(id).await?;
        webhook.deactivate();
        self.storage.update(webhook.clone()).await?;
        info!("Deactivated webhook {}", id);
        Ok(webhook)
    }

    pub async fn dispatch_event(&self, payload: WebhookPayload) -> Vec<WebhookDelivery> {
        let webhooks = match self.storage.get_by_tenant(&payload.tenant_id).await {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to get webhooks for dispatch: {}", e);
                return Vec::new();
            }
        };

        let matching_webhooks: Vec<_> = webhooks
            .iter()
            .filter(|w| w.subscribes_to(&payload.event))
            .collect();

        debug!(
            "Dispatching {:?} event to {} webhooks for tenant {}",
            payload.event,
            matching_webhooks.len(),
            payload.tenant_id
        );

        let mut deliveries = Vec::new();
        for webhook in matching_webhooks {
            match self.deliver_webhook(webhook, &payload).await {
                Ok(delivery) => deliveries.push(delivery),
                Err(e) => {
                    warn!("Failed to deliver webhook {}: {}", webhook.id, e);
                }
            }
        }

        deliveries
    }

    pub async fn deliver_webhook(
        &self,
        webhook: &Webhook,
        payload: &WebhookPayload,
    ) -> WebhookResult<WebhookDelivery> {
        let body = payload
            .to_json()
            .map_err(|e| WebhookError::Serialization(e.to_string()))?;

        let signature = self.compute_signature(&body, webhook.secret.as_deref())?;

        let start = std::time::Instant::now();

        let mut request = self
            .client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Event", payload.event.to_string())
            .header("X-Webhook-Delivery", Uuid::new_v4().to_string())
            .header("X-Webhook-Signature-256", format!("sha256={}", signature))
            .body(body.clone());

        if let Some(secret) = &webhook.secret {
            request = request.header("X-Webhook-Secret", secret);
        }

        let result = request.send().await;

        let duration_ms = start.elapsed().as_millis() as u64;

        let delivery = match result {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let response_body = response.text().await.ok();

                if status_code >= 200 && status_code < 300 {
                    info!(
                        "Webhook {} delivered successfully ({}ms)",
                        webhook.id, duration_ms
                    );
                    WebhookDelivery::success(
                        webhook.id,
                        payload.event.clone(),
                        status_code,
                        response_body,
                        duration_ms,
                    )
                } else {
                    warn!(
                        "Webhook {} returned status {} ({}ms)",
                        webhook.id, status_code, duration_ms
                    );
                    WebhookDelivery::failed(
                        webhook.id,
                        payload.event.clone(),
                        format!("HTTP {}", status_code),
                        duration_ms,
                    )
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    warn!("Webhook {} timed out ({}ms)", webhook.id, duration_ms);
                    WebhookDelivery::timeout(webhook.id, payload.event.clone(), duration_ms)
                } else {
                    error!("Webhook {} delivery failed: {}", webhook.id, e);
                    WebhookDelivery::failed(
                        webhook.id,
                        payload.event.clone(),
                        e.to_string(),
                        duration_ms,
                    )
                }
            }
        };

        let _ = self.storage.record_delivery(delivery.clone()).await;

        if delivery.status != crate::webhooks::models::DeliveryStatus::Success {
            return Err(WebhookError::DeliveryFailed(
                delivery
                    .error_message
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        Ok(delivery)
    }

    pub fn compute_signature(&self, body: &str, secret: Option<&str>) -> WebhookResult<String> {
        let secret = secret
            .ok_or_else(|| WebhookError::InvalidSignature("No secret configured".to_string()))?;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| WebhookError::InvalidSignature(e.to_string()))?;

        mac.update(body.as_bytes());
        let result = mac.finalize();
        Ok(hex::encode(result.into_bytes()))
    }

    pub fn verify_signature(
        &self,
        body: &str,
        signature: &str,
        secret: &str,
    ) -> WebhookResult<bool> {
        let expected = self.compute_signature(body, Some(secret))?;

        if signature.starts_with("sha256=") {
            let provided = &signature[7..];
            Ok(hex::decode(provided)
                .map(|decoded| {
                    let expected_bytes = hex::decode(&expected).unwrap_or_default();
                    decoded == expected_bytes
                })
                .unwrap_or(false))
        } else {
            Ok(false)
        }
    }

    fn validate_url(&self, url: &str) -> WebhookResult<()> {
        let parsed = url::Url::parse(url).map_err(|e| WebhookError::InvalidUrl(e.to_string()))?;

        if parsed.scheme() != "https" && parsed.scheme() != "http" {
            return Err(WebhookError::InvalidUrl(
                "URL must use http or https scheme".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn get_deliveries(
        &self,
        webhook_id: &Uuid,
        limit: usize,
    ) -> WebhookResult<Vec<WebhookDelivery>> {
        self.storage
            .get_deliveries(webhook_id, limit)
            .await
            .map_err(WebhookError::from)
    }
}

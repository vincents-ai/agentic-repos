use crate::webhooks::models::{Webhook, WebhookDelivery, WebhookEvent, WebhookPayload};
use crate::webhooks::storage::{WebhookStorage, WebhookStorageError};
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use url::Url;
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
            .redirect(reqwest::redirect::Policy::none()) // No redirect following — SSRF mitigation
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
            .redirect(reqwest::redirect::Policy::none()) // No redirect following — SSRF mitigation
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
        entity_schemas: Vec<String>,
        events: HashSet<WebhookEvent>,
        tenant_id: String,
    ) -> WebhookResult<Webhook> {
        self.validate_url(&url)?;

        let mut webhook = Webhook::new(url, secret, events, tenant_id);
        webhook.secret_keys = secret_keys;
        webhook.entity_schemas = entity_schemas;
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

        let request = self
            .client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Event", payload.event.to_string())
            .header("X-Webhook-Delivery", Uuid::new_v4().to_string())
            .header("X-Webhook-Signature-256", format!("sha256={}", signature))
            .body(body.clone());

        let _ = &webhook.secret; // secret is only used for HMAC computation, never sent in headers

        let result = request.send().await;

        let duration_ms = start.elapsed().as_millis() as u64;

        let delivery = match result {
            Ok(response) => {
                let status_code = response.status().as_u16();
                // Bound response body to 1KB — prevent memory exhaustion from large webhook responses
                let response_body = response
                    .text()
                    .await
                    .ok()
                    .map(|s| {
                        if s.len() > 1024 {
                            format!("{}... [truncated, {} bytes total]", &s[..1024], s.len())
                        } else {
                            s
                        }
                    });

                if (200..300).contains(&status_code) {
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

        if let Some(provided) = signature.strip_prefix("sha256=") {
            // Constant-time comparison to prevent timing attacks
            let provided_bytes = hex::decode(provided).unwrap_or_default();
            let expected_bytes = hex::decode(&expected).unwrap_or_default();
            if provided_bytes.len() != expected_bytes.len() {
                return Ok(false);
            }
            let mut diff: u8 = 0;
            for (a, b) in provided_bytes.iter().zip(expected_bytes.iter()) {
                diff |= a ^ b;
            }
            Ok(diff == 0)
        } else {
            Ok(false)
        }
    }

    fn validate_url(&self, url: &str) -> WebhookResult<()> {
        let parsed = Url::parse(url).map_err(|e| WebhookError::InvalidUrl(e.to_string()))?;

        // Only HTTPS allowed — prevents plaintext interception of webhook payloads
        if parsed.scheme() != "https" {
            return Err(WebhookError::InvalidUrl(
                "URL must use https scheme".to_string(),
            ));
        }

        // Must have a host
        let host = parsed
            .host_str()
            .ok_or_else(|| WebhookError::InvalidUrl("URL must have a host".to_string()))?;

        // Reject URLs with userinfo (user:pass@host)
        if parsed.username() != "" || parsed.password().is_some() {
            return Err(WebhookError::InvalidUrl(
                "URL must not contain credentials".to_string(),
            ));
        }

        // SSRF protection: resolve and reject private/internal IP ranges
        Self::check_ssrf(host)?;

        Ok(())
    }

    /// Check that a hostname does not resolve to a private, loopback,
    /// link-local, or otherwise internal IP address. This prevents SSRF
    /// attacks where webhooks are used to probe internal network services.
    fn check_ssrf(host: &str) -> WebhookResult<()> {
        // First check if the host is already a literal IP address
        if let Ok(ip) = host.parse::<IpAddr>() {
            if Self::is_private_ip(&ip) {
                return Err(WebhookError::InvalidUrl(format!(
                    "webhook URL resolves to private/internal IP address {}",
                    ip
                )));
            }
            return Ok(());
        }

        // For hostnames, perform a DNS resolution and check all results
        // Note: we check at registration time only. A TOCTOU race is possible
        // if DNS is changed after validation (DNS rebinding). The reqwest client
        // does not follow redirects (see Client::builder below), which mitigates
        // the redirect-based SSRF vector.
        #[cfg(not(test))]
        {
            use std::net::ToSocketAddrs;
            let addr = format!("{}:443", host);
            match addr.to_socket_addrs() {
                Ok(addrs) => {
                    for socket_addr in addrs {
                        if Self::is_private_ip(&socket_addr.ip()) {
                            return Err(WebhookError::InvalidUrl(format!(
                                "webhook URL host '{}' resolves to private/internal IP {}",
                                host, socket_addr.ip()
                            )));
                        }
                    }
                }
                Err(e) => {
                    return Err(WebhookError::InvalidUrl(format!(
                        "failed to resolve webhook URL host '{}': {}",
                        host, e
                    )));
                }
            }
        }

        Ok(())
    }

    /// Check if an IP address is private, loopback, link-local,
    /// or in other reserved ranges that should never be webhook targets.
    fn is_private_ip(ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(v4) => {
                // Loopback: 127.0.0.0/8
                if v4.is_loopback() {
                    return true;
                }
                // Private: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                if v4.is_private() {
                    return true;
                }
                // Link-local: 169.254.0.0/16
                if v4.is_link_local() {
                    return true;
                }
                // IETF protocol assignments: 192.0.0.0/24
                if v4.octets() == [192, 0, 0, 0]
                    || (v4.octets()[0] == 192
                        && v4.octets()[1] == 0
                        && v4.octets()[2] == 0)
                {
                    return true;
                }
                // Broadcast
                if v4.is_broadcast() {
                    return true;
                }
                // Unspecified: 0.0.0.0
                if v4.is_unspecified() {
                    return true;
                }
                // Documentation: 198.51.100.0/24, 203.0.113.0/24
                let octets = v4.octets();
                if octets[0] == 198 && octets[1] == 51 && octets[2] == 100 {
                    return true;
                }
                if octets[0] == 203 && octets[1] == 0 && octets[2] == 113 {
                    return true;
                }
                // Carrier-grade NAT: 100.64.0.0/10
                if octets[0] == 100 && (octets[1] & 0xC0) == 0x40 {
                    return true;
                }
                false
            }
            IpAddr::V6(v6) => {
                // Loopback, link-local, unique-local, unspecified
                if v6.is_loopback() || v6.is_unspecified() {
                    return true;
                }
                let segments = v6.segments();
                // Link-local: fe80::/10
                if (segments[0] & 0xFFC0) == 0xFE80 {
                    return true;
                }
                // Unique local: fc00::/7
                if (segments[0] & 0xFE00) == 0xFC00 {
                    return true;
                }
                // IPv4-mapped: ::ffff:0:0/96
                if segments[0..5] == [0, 0, 0, 0, 0]
                    && segments[5] == 0xFFFF
                {
                    // Check the embedded IPv4 address
                    let v4 = Ipv4Addr::new(
                        (segments[6] >> 8) as u8,
                        (segments[6] & 0xFF) as u8,
                        (segments[7] >> 8) as u8,
                        (segments[7] & 0xFF) as u8,
                    );
                    return Self::is_private_ip(&IpAddr::V4(v4));
                }
                false
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhooks::models::WebhookEvent;
    use crate::webhooks::storage::InMemoryWebhookStorage;
    use std::collections::HashSet;

    fn make_service() -> WebhookService {
        let storage = Arc::new(InMemoryWebhookStorage::new());
        WebhookService::new(storage)
    }

    #[tokio::test]
    async fn test_register_webhook_success() {
        let svc = make_service();
        let wh = svc
            .register_webhook(
                "https://example.com/hook".to_string(),
                Some("mysecret".to_string()),
                vec![],
                vec![],
                HashSet::from([WebhookEvent::Push]),
                "tenant-1".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(wh.tenant_id, "tenant-1");
        assert!(wh.active);
    }

    #[tokio::test]
    async fn test_register_webhook_invalid_url() {
        let svc = make_service();
        let result = svc
            .register_webhook(
                "not-a-url".to_string(),
                None,
                vec![],
                vec![],
                HashSet::from([WebhookEvent::Push]),
                "tenant-1".to_string(),
            )
            .await;
        assert!(matches!(result, Err(WebhookError::InvalidUrl(_))));
    }

    #[tokio::test]
    async fn test_get_webhook() {
        let svc = make_service();
        let wh = svc
            .register_webhook(
                "https://example.com/hook".to_string(),
                None,
                vec![],
                vec![],
                HashSet::from([WebhookEvent::Push]),
                "t1".to_string(),
            )
            .await
            .unwrap();
        let found = svc.get_webhook(&wh.id).await.unwrap();
        assert_eq!(found.id, wh.id);
    }

    #[tokio::test]
    async fn test_activate_deactivate() {
        let svc = make_service();
        let wh = svc
            .register_webhook(
                "https://example.com/hook".to_string(),
                None,
                vec![],
                vec![],
                HashSet::from([WebhookEvent::Push]),
                "t1".to_string(),
            )
            .await
            .unwrap();

        svc.deactivate_webhook(&wh.id).await.unwrap();
        let deactivated = svc.get_webhook(&wh.id).await.unwrap();
        assert!(!deactivated.active);

        svc.activate_webhook(&wh.id).await.unwrap();
        let reactivated = svc.get_webhook(&wh.id).await.unwrap();
        assert!(reactivated.active);
    }

    #[tokio::test]
    async fn test_delete_webhook() {
        let svc = make_service();
        let wh = svc
            .register_webhook(
                "https://example.com/hook".to_string(),
                None,
                vec![],
                vec![],
                HashSet::from([WebhookEvent::Push]),
                "t1".to_string(),
            )
            .await
            .unwrap();
        svc.delete_webhook(&wh.id).await.unwrap();
        let result = svc.get_webhook(&wh.id).await;
        assert!(matches!(result, Err(WebhookError::NotFound(_))));
    }

    #[test]
    fn test_compute_signature() {
        let svc = make_service();
        let sig = svc
            .compute_signature("hello world", Some("my-secret"))
            .unwrap();
        // HMAC-SHA256 of "hello world" with key "my-secret" is deterministic
        assert!(!sig.is_empty());
        // Same input → same output
        let sig2 = svc
            .compute_signature("hello world", Some("my-secret"))
            .unwrap();
        assert_eq!(sig, sig2);
    }

    #[test]
    fn test_compute_signature_requires_secret() {
        let svc = make_service();
        let result = svc.compute_signature("hello", None);
        assert!(matches!(result, Err(WebhookError::InvalidSignature(_))));
    }

    #[test]
    fn test_verify_signature_valid() {
        let svc = make_service();
        let body = r#"{"event":"push"}"#;
        let secret = "webhook-secret";
        let raw_sig = svc.compute_signature(body, Some(secret)).unwrap();
        let sig_header = format!("sha256={}", raw_sig);
        assert!(svc.verify_signature(body, &sig_header, secret).unwrap());
    }

    #[test]
    fn test_verify_signature_wrong_secret() {
        let svc = make_service();
        let body = r#"{"event":"push"}"#;
        let raw_sig = svc.compute_signature(body, Some("correct-secret")).unwrap();
        let sig_header = format!("sha256={}", raw_sig);
        assert!(!svc
            .verify_signature(body, &sig_header, "wrong-secret")
            .unwrap());
    }

    #[test]
    fn test_verify_signature_missing_prefix() {
        let svc = make_service();
        // Without "sha256=" prefix the verifier should return false
        let body = "test";
        let raw = svc.compute_signature(body, Some("s")).unwrap();
        assert!(!svc.verify_signature(body, &raw, "s").unwrap());
    }

    #[tokio::test]
    async fn test_validate_url_rejects_non_https() {
        let svc = make_service();
        // ftp: rejected
        let r1 = svc
            .register_webhook(
                "ftp://example.com/hook".to_string(),
                None,
                vec![],
                vec![],
                HashSet::from([WebhookEvent::Push]),
                "t".to_string(),
            )
            .await;
        assert!(matches!(r1, Err(WebhookError::InvalidUrl(_))));
        // file: rejected
        let r2 = svc
            .register_webhook(
                "file:///etc/passwd".to_string(),
                None,
                vec![],
                vec![],
                HashSet::from([WebhookEvent::Push]),
                "t".to_string(),
            )
            .await;
        assert!(matches!(r2, Err(WebhookError::InvalidUrl(_))));
        // http: also rejected (must be https)
        let r3 = svc
            .register_webhook(
                "http://example.com/hook".to_string(),
                None,
                vec![],
                vec![],
                HashSet::from([WebhookEvent::Push]),
                "t".to_string(),
            )
            .await;
        assert!(matches!(r3, Err(WebhookError::InvalidUrl(_))));
    }

    #[test]
    fn test_ssrf_rejects_private_ips() {
        // Loopback
        assert!(WebhookService::is_private_ip(&"127.0.0.1".parse().unwrap()));
        // Private ranges
        assert!(WebhookService::is_private_ip(&"10.0.0.1".parse().unwrap()));
        assert!(WebhookService::is_private_ip(&"172.16.0.1".parse().unwrap()));
        assert!(WebhookService::is_private_ip(&"192.168.1.1".parse().unwrap()));
        // Link-local
        assert!(WebhookService::is_private_ip(&"169.254.1.1".parse().unwrap()));
        // Unspecified
        assert!(WebhookService::is_private_ip(&"0.0.0.0".parse().unwrap()));
        // Broadcast
        assert!(WebhookService::is_private_ip(
            &"255.255.255.255".parse().unwrap()
        ));
        // Public IPs should be allowed
        assert!(!WebhookService::is_private_ip(&"1.1.1.1".parse().unwrap()));
        assert!(!WebhookService::is_private_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!WebhookService::is_private_ip(&"93.184.216.34".parse().unwrap()));
        // IPv6 loopback
        assert!(WebhookService::is_private_ip(&"::1".parse().unwrap()));
        // IPv6 unique local
        assert!(WebhookService::is_private_ip(&"fc00::1".parse().unwrap()));
        // IPv6 link-local
        assert!(WebhookService::is_private_ip(&"fe80::1".parse().unwrap()));
        // IPv4-mapped IPv6 that maps to private IP
        assert!(WebhookService::is_private_ip(
            &"::ffff:192.168.1.1".parse().unwrap()
        ));
        // Public IPv6
        assert!(!WebhookService::is_private_ip(
            &"2606:4700:4700::1111".parse().unwrap()
        ));
    }

    #[test]
    fn test_ssrf_check_literal_ip() {
        // Literal private IP rejected
        let r = WebhookService::check_ssrf("127.0.0.1");
        assert!(matches!(r, Err(WebhookError::InvalidUrl(_))));
        let r = WebhookService::check_ssrf("10.0.0.1");
        assert!(matches!(r, Err(WebhookError::InvalidUrl(_))));
        // Public IP allowed
        let r = WebhookService::check_ssrf("1.1.1.1");
        assert!(r.is_ok());
    }
}

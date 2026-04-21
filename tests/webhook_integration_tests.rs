//! Integration tests for webhook delivery and event flow.
//!
//! These tests verify:
//! - Webhook delivery payload structure and headers
//! - HMAC-SHA256 signature verification
//! - Multiple event types
//! - Failure handling for unreachable targets
//! - Webhook without secret

#[cfg(test)]
mod webhook_integration_tests {
    use agentic_repos::webhooks::models::{DeliveryStatus, Webhook, WebhookEvent, WebhookPayload};
    use agentic_repos::webhooks::service::WebhookService;
    use agentic_repos::webhooks::storage::InMemoryWebhookStorage;
    use actix_web::{web, App, HttpServer, HttpResponse};
    use serde_json::Value;
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};

    /// Captured webhook request: body + headers.
    #[derive(Debug, Clone)]
    struct CapturedRequest {
        body: String,
        headers: HashMap<String, String>,
    }

    /// Shared state for the test HTTP receiver.
    struct ReceiverState {
        captured: Mutex<Option<CapturedRequest>>,
    }

    /// Handler that captures the incoming webhook POST.
    async fn webhook_receiver(
        state: web::Data<Arc<ReceiverState>>,
        body: web::Bytes,
        req: actix_web::HttpRequest,
    ) -> HttpResponse {
        let headers: HashMap<String, String> = req
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str().ok().map(|s| (k.to_string().to_lowercase(), s.to_string()))
            })
            .collect();
        let body_str = String::from_utf8_lossy(&body).to_string();

        let mut guard = state.captured.lock().unwrap();
        *guard = Some(CapturedRequest { body: body_str, headers });

        HttpResponse::Ok().finish()
    }

    /// Start a local HTTP server on a random port to receive webhooks.
    /// Returns (port, captured_state).
    fn start_receiver() -> (u16, Arc<ReceiverState>) {
        let state = Arc::new(ReceiverState {
            captured: Mutex::new(None),
        });
        let state_clone = state.clone();

        let (port_tx, port_rx) = std::sync::mpsc::channel::<u16>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let state_data = web::Data::new(state_clone);
                let server = HttpServer::new(move || {
                    let state_data = state_data.clone();
                    App::new()
                        .app_data(state_data)
                        .route("/webhook", web::post().to(webhook_receiver))
                })
                .bind("127.0.0.1:0")
                .unwrap();

                let port = server.addrs()[0].port();
                port_tx.send(port).unwrap();

                // Run until the thread is killed (tests are short-lived)
                let _ = server.run().await;
            });
        });

        let port = port_rx.recv().unwrap();
        // Give the server time to start
        std::thread::sleep(std::time::Duration::from_millis(100));
        (port, state)
    }

    fn make_service() -> WebhookService {
        let storage = Arc::new(InMemoryWebhookStorage::new());
        WebhookService::new(storage)
    }

    /// Create a Webhook pointing at a local HTTP receiver (bypasses HTTPS validation).
    fn make_http_webhook(url: &str, secret: Option<&str>, events: HashSet<WebhookEvent>) -> Webhook {
        Webhook::new(
            url.to_string(),
            secret.map(|s| s.to_string()),
            events,
            "tenant-1".to_string(),
        )
    }

    // ── Test 1: Full payload and header verification ───────────────────

    #[tokio::test]
    async fn test_webhook_delivery_payload_and_headers() {
        let (port, captured) = start_receiver();
        let svc = make_service();
        let secret = "test-secret-key";

        let webhook = make_http_webhook(
            &format!("http://127.0.0.1:{}/webhook", port),
            Some(secret),
            HashSet::from([WebhookEvent::Push]),
        );

        let payload = WebhookPayload::new(
            WebhookEvent::Push,
            "tenant-1".to_string(),
            Some("repo-1".to_string()),
            serde_json::json!({
                "ref": "refs/heads/main",
                "commits": [{"id": "abc123", "message": "test commit"}]
            }),
        );

        let delivery = svc.deliver_webhook(&webhook, &payload).await.unwrap();
        assert_eq!(delivery.status_code, Some(200));
        assert_eq!(delivery.status, DeliveryStatus::Success);

        let req = captured.captured.lock().unwrap().take().unwrap();

        // Verify body JSON fields
        let body: Value = serde_json::from_str(&req.body).expect("Body should be valid JSON");
        assert_eq!(body["event"], "Push");
        assert_eq!(body["tenant_id"], "tenant-1");
        assert_eq!(body["repository_id"], "repo-1");
        assert_eq!(body["data"]["ref"], "refs/heads/main");
        assert_eq!(body["data"]["commits"][0]["id"], "abc123");

        // Verify required headers
        assert_eq!(req.headers.get("x-webhook-event").unwrap(), "push");

        // Header value is PascalCase as per WebhookEvent Display impl

        let delivery_id = req.headers.get("x-webhook-delivery").unwrap();
        assert!(uuid::Uuid::parse_str(delivery_id).is_ok(), "Delivery ID should be UUID");

        // Verify HMAC-SHA256 signature
        let sig_header = req.headers.get("x-webhook-signature-256").unwrap();
        assert!(sig_header.starts_with("sha256="));

        let verified = svc.verify_signature(&req.body, sig_header, secret).unwrap();
        assert!(verified, "Signature should verify against known secret");

        // Verify content type
        assert!(req.headers.get("content-type").unwrap().contains("application/json"));
    }

    // ── Test 2: Wrong secret fails verification ────────────────────────

    #[tokio::test]
    async fn test_webhook_delivery_wrong_secret_fails_verification() {
        let (port, captured) = start_receiver();
        let svc = make_service();
        let secret = "correct-secret";

        let webhook = make_http_webhook(
            &format!("http://127.0.0.1:{}/webhook", port),
            Some(secret),
            HashSet::from([WebhookEvent::Push]),
        );

        let payload = WebhookPayload::new(
            WebhookEvent::Push,
            "tenant-1".to_string(),
            None,
            serde_json::json!({"ref": "refs/heads/main"}),
        );

        svc.deliver_webhook(&webhook, &payload).await.unwrap();

        let req = captured.captured.lock().unwrap().take().unwrap();
        let sig_header = req.headers.get("x-webhook-signature-256").unwrap();

        let verified = svc.verify_signature(&req.body, sig_header, "wrong-secret").unwrap();
        assert!(!verified, "Wrong secret should fail");

        let verified = svc.verify_signature(&req.body, sig_header, secret).unwrap();
        assert!(verified, "Correct secret should pass");
    }

    // ── Test 3: Unreachable URL returns failure ────────────────────────

    #[tokio::test]
    async fn test_webhook_delivery_unreachable_url() {
        let svc = make_service();

        // Create webhook directly to bypass HTTPS validation
        let webhook = Webhook::new(
            "http://127.0.0.1:1/webhook".to_string(),
            Some("any-secret".to_string()),
            HashSet::from([WebhookEvent::Push]),
            "tenant-1".to_string(),
        );

        let payload = WebhookPayload::new(
            WebhookEvent::Push,
            "tenant-1".to_string(),
            None,
            serde_json::json!({"ref": "refs/heads/main"}),
        );

        let result = svc.deliver_webhook(&webhook, &payload).await;
        match result {
            Ok(delivery) => {
                assert!(matches!(delivery.status, DeliveryStatus::Failed));
            }
            Err(_) => {
                // Connection refused is also acceptable — the URL is unreachable
            }
        }
    }

    // ── Test 4: Different event types ──────────────────────────────────

    #[tokio::test]
    async fn test_webhook_delivery_different_event_types() {
        let (port, captured) = start_receiver();
        let svc = make_service();

        let webhook = make_http_webhook(
            &format!("http://127.0.0.1:{}/webhook", port),
            Some("secret"),
            HashSet::from([WebhookEvent::Push, WebhookEvent::TagCreate]),
        );

        // Push event
        let push_payload = WebhookPayload::new(
            WebhookEvent::Push,
            "tenant-1".to_string(),
            Some("repo-1".to_string()),
            serde_json::json!({"ref": "refs/heads/main"}),
        );
        svc.deliver_webhook(&webhook, &push_payload).await.unwrap();
        let req = captured.captured.lock().unwrap().take().unwrap();
        let body: Value = serde_json::from_str(&req.body).unwrap();
        assert_eq!(body["event"], "Push");

        // TagCreate event
        let tag_payload = WebhookPayload::new(
            WebhookEvent::TagCreate,
            "tenant-1".to_string(),
            Some("repo-1".to_string()),
            serde_json::json!({"tag": "v1.0.0", "target": "abc123"}),
        );
        svc.deliver_webhook(&webhook, &tag_payload).await.unwrap();
        let req = captured.captured.lock().unwrap().take().unwrap();
        let body: Value = serde_json::from_str(&req.body).unwrap();
        assert_eq!(body["event"], "TagCreate");
        assert_eq!(body["data"]["tag"], "v1.0.0");
    }

    // ── Test 5: Webhook without secret — delivery fails (known limitation) ──

    #[tokio::test]
    async fn test_webhook_delivery_no_secret_returns_error() {
        let svc = make_service();

        let webhook = Webhook::new(
            "http://127.0.0.1:1/webhook".to_string(), // unreachable, doesn't matter
            None,
            HashSet::from([WebhookEvent::Push]),
            "tenant-1".to_string(),
        );

        let payload = WebhookPayload::new(
            WebhookEvent::Push,
            "tenant-1".to_string(),
            None,
            serde_json::json!({"ref": "refs/heads/main"}),
        );

        // Without a secret, compute_signature returns InvalidSignature("No secret configured")
        // which propagates as an error from deliver_webhook.
        // This is a known limitation — all webhooks require a secret.
        let result = svc.deliver_webhook(&webhook, &payload).await;
        assert!(result.is_err(), "Delivery without secret should return error");
    }
}

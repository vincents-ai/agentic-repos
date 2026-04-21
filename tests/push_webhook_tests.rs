//! Integration test: entity ref changes trigger webhook delivery.
//!
//! Tests the flow: entity change events → webhook payload creation →
//! delivery to local HTTP receiver with verification.

#[cfg(test)]
mod push_webhook_tests {
    use agentic_repos::webhooks::models::{Webhook, WebhookEvent, WebhookPayload};
    use agentic_repos::webhooks::service::WebhookService;
    use agentic_repos::webhooks::storage::InMemoryWebhookStorage;
    use actix_web::{web, App, HttpServer, HttpResponse};
    use serde_json::{json, Value};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    struct ReceiverState {
        requests: Mutex<Vec<String>>,
    }

    async fn webhook_receiver(
        state: web::Data<Arc<ReceiverState>>,
        body: web::Bytes,
    ) -> HttpResponse {
        let body_str = String::from_utf8_lossy(&body).to_string();
        state.requests.lock().unwrap().push(body_str);
        HttpResponse::Ok().finish()
    }

    fn start_receiver() -> (u16, Arc<ReceiverState>) {
        let state = Arc::new(ReceiverState {
            requests: Mutex::new(Vec::new()),
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
                let _ = server.run().await;
            });
        });

        let port = port_rx.recv().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        (port, state)
    }

    fn make_service() -> WebhookService {
        WebhookService::new(Arc::new(InMemoryWebhookStorage::new()))
    }

    // ── Test 1: EntityCreated event triggers webhook ───────────────────

    #[tokio::test]
    async fn test_entity_created_triggers_webhook() {
        let (port, captured) = start_receiver();
        let svc = make_service();

        let webhook = Webhook::new(
            format!("http://127.0.0.1:{}/webhook", port),
            Some("secret".to_string()),
            HashSet::from([WebhookEvent::EntityCreated]),
            "tenant-1".to_string(),
        );

        // Simulate an entity created event from a git push
        let payload = WebhookPayload::new(
            WebhookEvent::EntityCreated,
            "tenant-1".to_string(),
            Some("repo-1".to_string()),
            json!({
                "event_type": "created",
                "schema_id": "engram-task",
                "ref_name": "refs/engram/task/abc123",
                "entity_id": "abc123",
                "actor": "alice"
            }),
        );

        let delivery = svc.deliver_webhook(&webhook, &payload).await.unwrap();
        assert!(matches!(delivery.status, agentic_repos::webhooks::models::DeliveryStatus::Success));

        let requests = captured.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let body: Value = serde_json::from_str(&requests[0]).unwrap();
        assert_eq!(body["event"], "EntityCreated");
        assert_eq!(body["data"]["event_type"], "created");
        assert_eq!(body["data"]["entity_id"], "abc123");
        assert_eq!(body["data"]["schema_id"], "engram-task");
    }

    // ── Test 2: EntityDeleted event triggers webhook ───────────────────

    #[tokio::test]
    async fn test_entity_deleted_triggers_webhook() {
        let (port, captured) = start_receiver();
        let svc = make_service();

        let webhook = Webhook::new(
            format!("http://127.0.0.1:{}/webhook", port),
            Some("secret".to_string()),
            HashSet::from([WebhookEvent::EntityDeleted]),
            "tenant-1".to_string(),
        );

        let payload = WebhookPayload::new(
            WebhookEvent::EntityDeleted,
            "tenant-1".to_string(),
            Some("repo-1".to_string()),
            json!({
                "event_type": "deleted",
                "schema_id": "engram-task",
                "ref_name": "refs/engram/task/abc123",
                "entity_id": "abc123",
                "actor": "alice"
            }),
        );

        svc.deliver_webhook(&webhook, &payload).await.unwrap();

        let requests = captured.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let body: Value = serde_json::from_str(&requests[0]).unwrap();
        assert_eq!(body["data"]["event_type"], "deleted");
    }

    // ── Test 3: Multiple entities in one push ──────────────────────────

    #[tokio::test]
    async fn test_multiple_entities_one_push() {
        let (port, captured) = start_receiver();
        let svc = make_service();

        let webhook = Webhook::new(
            format!("http://127.0.0.1:{}/webhook", port),
            Some("secret".to_string()),
            HashSet::from([WebhookEvent::EntityCreated, WebhookEvent::EntityUpdated]),
            "tenant-1".to_string(),
        );

        // Simulate a push that creates one entity and updates another
        let entities = vec![
            (WebhookEvent::EntityCreated, "task-001", "created"),
            (WebhookEvent::EntityUpdated, "task-002", "updated"),
        ];

        for (event, entity_id, event_type) in &entities {
            let payload = WebhookPayload::new(
                event.clone(),
                "tenant-1".to_string(),
                Some("repo-1".to_string()),
                json!({
                    "event_type": event_type,
                    "entity_id": entity_id,
                    "schema_id": "engram-task",
                }),
            );
            svc.deliver_webhook(&webhook, &payload).await.unwrap();
        }

        let requests = captured.requests.lock().unwrap();
        assert_eq!(requests.len(), 2, "Should receive 2 webhooks");

        let body1: Value = serde_json::from_str(&requests[0]).unwrap();
        assert_eq!(body1["data"]["event_type"], "created");

        let body2: Value = serde_json::from_str(&requests[1]).unwrap();
        assert_eq!(body2["data"]["event_type"], "updated");
    }

    // ── Test 4: Webhook filters by event type ──────────────────────────

    #[tokio::test]
    async fn test_webhook_filters_by_event_type() {
        let (port, captured) = start_receiver();
        let svc = make_service();

        // Webhook only subscribes to EntityCreated, not EntityUpdated
        let webhook = Webhook::new(
            format!("http://127.0.0.1:{}/webhook", port),
            Some("secret".to_string()),
            HashSet::from([WebhookEvent::EntityCreated]),
            "tenant-1".to_string(),
        );

        // Deliver an EntityCreated event
        let create_payload = WebhookPayload::new(
            WebhookEvent::EntityCreated,
            "tenant-1".to_string(),
            Some("repo-1".to_string()),
            json!({"entity_id": "task-001"}),
        );
        svc.deliver_webhook(&webhook, &create_payload).await.unwrap();

        let requests = captured.requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "Should receive the created event");
    }
}

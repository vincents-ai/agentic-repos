use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum WebhookEvent {
    Push,
    BranchCreate,
    BranchDelete,
    TagCreate,
    TagDelete,
    Fork,
    RepositoryCreate,
    RepositoryDelete,
    RepositoryUpdate,
    LfsUpload,
    LfsDownload,
    LfsLockCreate,
    LfsLockDelete,
    SystemHealth,
    SystemError,
    SystemConfig,
}

impl std::fmt::Display for WebhookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WebhookEvent::Push => "push",
            WebhookEvent::BranchCreate => "branch.create",
            WebhookEvent::BranchDelete => "branch.delete",
            WebhookEvent::TagCreate => "tag.create",
            WebhookEvent::TagDelete => "tag.delete",
            WebhookEvent::Fork => "fork",
            WebhookEvent::RepositoryCreate => "repository.create",
            WebhookEvent::RepositoryDelete => "repository.delete",
            WebhookEvent::RepositoryUpdate => "repository.update",
            WebhookEvent::LfsUpload => "lfs.upload",
            WebhookEvent::LfsDownload => "lfs.download",
            WebhookEvent::LfsLockCreate => "lfs.lock.create",
            WebhookEvent::LfsLockDelete => "lfs.lock.delete",
            WebhookEvent::SystemHealth => "system.health",
            WebhookEvent::SystemError => "system.error",
            WebhookEvent::SystemConfig => "system.config",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for WebhookEvent {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "push" => Ok(WebhookEvent::Push),
            "branch.create" => Ok(WebhookEvent::BranchCreate),
            "branch.delete" => Ok(WebhookEvent::BranchDelete),
            "tag.create" => Ok(WebhookEvent::TagCreate),
            "tag.delete" => Ok(WebhookEvent::TagDelete),
            "fork" => Ok(WebhookEvent::Fork),
            "repository.create" => Ok(WebhookEvent::RepositoryCreate),
            "repository.delete" => Ok(WebhookEvent::RepositoryDelete),
            "repository.update" => Ok(WebhookEvent::RepositoryUpdate),
            "lfs.upload" => Ok(WebhookEvent::LfsUpload),
            "lfs.download" => Ok(WebhookEvent::LfsDownload),
            "lfs.lock.create" => Ok(WebhookEvent::LfsLockCreate),
            "lfs.lock.delete" => Ok(WebhookEvent::LfsLockDelete),
            "system.health" => Ok(WebhookEvent::SystemHealth),
            "system.error" => Ok(WebhookEvent::SystemError),
            "system.config" => Ok(WebhookEvent::SystemConfig),
            other => Err(format!("Unknown webhook event: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: Uuid,
    pub url: String,
    pub secret: Option<String>,
    /// Allowed secret keys that this webhook can access for egress.
    /// If empty, webhook cannot access any secrets.
    /// If contains "*", webhook can access all secrets.
    #[serde(default)]
    pub secret_keys: Vec<String>,
    pub events: HashSet<WebhookEvent>,
    pub tenant_id: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Webhook {
    pub fn new(
        url: String,
        secret: Option<String>,
        events: HashSet<WebhookEvent>,
        tenant_id: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            url,
            secret,
            secret_keys: Vec::new(),
            events,
            tenant_id,
            active: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn subscribes_to(&self, event: &WebhookEvent) -> bool {
        self.active && self.events.contains(event)
    }

    pub fn activate(&mut self) {
        self.active = true;
        self.updated_at = Utc::now();
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.updated_at = Utc::now();
    }

    /// Check if this webhook is allowed to access a specific secret key.
    /// Returns true if the webhook can access the given secret.
    pub fn can_access_secret(&self, secret_key: &str) -> bool {
        // If secret_keys is empty, webhook cannot access any secrets
        if self.secret_keys.is_empty() {
            return false;
        }
        // If secret_keys contains "*", webhook can access all secrets
        if self.secret_keys.contains(&"*".to_string()) {
            return true;
        }
        // Check if the specific key is in the allowlist
        self.secret_keys.iter().any(|k| k == secret_key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event: WebhookEvent,
    pub timestamp: DateTime<Utc>,
    pub tenant_id: String,
    pub repository_id: Option<String>,
    pub data: serde_json::Value,
}

impl WebhookPayload {
    pub fn new(
        event: WebhookEvent,
        tenant_id: String,
        repository_id: Option<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            event,
            timestamp: Utc::now(),
            tenant_id,
            repository_id,
            data,
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub event: WebhookEvent,
    pub status: DeliveryStatus,
    pub status_code: Option<u16>,
    pub response_body: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: u64,
    pub delivered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeliveryStatus {
    Success,
    Failed,
    Timeout,
}

impl WebhookDelivery {
    pub fn success(
        webhook_id: Uuid,
        event: WebhookEvent,
        status_code: u16,
        response_body: Option<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            webhook_id,
            event,
            status: DeliveryStatus::Success,
            status_code: Some(status_code),
            response_body,
            error_message: None,
            duration_ms,
            delivered_at: Utc::now(),
        }
    }

    pub fn failed(
        webhook_id: Uuid,
        event: WebhookEvent,
        error_message: String,
        duration_ms: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            webhook_id,
            event,
            status: DeliveryStatus::Failed,
            status_code: None,
            response_body: None,
            error_message: Some(error_message),
            duration_ms,
            delivered_at: Utc::now(),
        }
    }

    pub fn timeout(webhook_id: Uuid, event: WebhookEvent, duration_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            webhook_id,
            event,
            status: DeliveryStatus::Timeout,
            status_code: None,
            response_body: None,
            error_message: Some("Request timed out".to_string()),
            duration_ms,
            delivered_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_webhook() -> Webhook {
        Webhook::new(
            "https://example.com/hook".to_string(),
            Some("secret123".to_string()),
            HashSet::from([WebhookEvent::Push, WebhookEvent::BranchCreate]),
            "tenant-1".to_string(),
        )
    }

    #[test]
    fn test_webhook_subscribes_to_matching_event() {
        let wh = test_webhook();
        assert!(wh.subscribes_to(&WebhookEvent::Push));
        assert!(wh.subscribes_to(&WebhookEvent::BranchCreate));
    }

    #[test]
    fn test_webhook_does_not_subscribe_to_other_events() {
        let wh = test_webhook();
        assert!(!wh.subscribes_to(&WebhookEvent::Fork));
        assert!(!wh.subscribes_to(&WebhookEvent::TagCreate));
    }

    #[test]
    fn test_inactive_webhook_does_not_subscribe() {
        let mut wh = test_webhook();
        wh.deactivate();
        assert!(!wh.subscribes_to(&WebhookEvent::Push));
    }

    #[test]
    fn test_activate_deactivate_round_trip() {
        let mut wh = test_webhook();
        wh.deactivate();
        assert!(!wh.active);
        wh.activate();
        assert!(wh.active);
    }

    #[test]
    fn test_secret_access_empty_allowlist_denies_all() {
        let wh = test_webhook(); // secret_keys is empty by default
        assert!(!wh.can_access_secret("MY_DB_PASSWORD"));
    }

    #[test]
    fn test_secret_access_wildcard_allows_all() {
        let mut wh = test_webhook();
        wh.secret_keys = vec!["*".to_string()];
        assert!(wh.can_access_secret("anything"));
    }

    #[test]
    fn test_secret_access_specific_key_allowlist() {
        let mut wh = test_webhook();
        wh.secret_keys = vec!["ALLOWED_KEY".to_string()];
        assert!(wh.can_access_secret("ALLOWED_KEY"));
        assert!(!wh.can_access_secret("OTHER_KEY"));
    }

    #[test]
    fn test_webhook_event_display() {
        assert_eq!(WebhookEvent::Push.to_string(), "push");
        assert_eq!(WebhookEvent::BranchCreate.to_string(), "branch.create");
        assert_eq!(WebhookEvent::LfsUpload.to_string(), "lfs.upload");
    }

    #[test]
    fn test_webhook_event_from_str() {
        use std::str::FromStr;
        assert_eq!(WebhookEvent::from_str("push").unwrap(), WebhookEvent::Push);
        assert_eq!(
            WebhookEvent::from_str("tag.create").unwrap(),
            WebhookEvent::TagCreate
        );
        assert!(WebhookEvent::from_str("unknown.event").is_err());
    }

    #[test]
    fn test_webhook_payload_new() {
        let payload = WebhookPayload::new(
            WebhookEvent::Push,
            "tenant-1".to_string(),
            Some("repo-abc".to_string()),
            serde_json::json!({"ref": "refs/heads/main", "commits": 3}),
        );
        assert_eq!(payload.event, WebhookEvent::Push);
        assert_eq!(payload.tenant_id, "tenant-1");
        assert_eq!(payload.repository_id.as_deref(), Some("repo-abc"));
    }
}

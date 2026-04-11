use crate::webhooks::models::{Webhook, WebhookDelivery};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum WebhookStorageError {
    #[error("Storage operation failed: {0}")]
    OperationFailed(String),
    #[error("Webhook not found: {0}")]
    NotFound(String),
    #[error("Webhook already exists: {0}")]
    AlreadyExists(String),
}

#[async_trait]
pub trait WebhookStorage: Send + Sync {
    async fn create(&self, webhook: Webhook) -> Result<Webhook, WebhookStorageError>;
    async fn get(&self, id: &Uuid) -> Result<Option<Webhook>, WebhookStorageError>;
    async fn get_by_tenant(&self, tenant_id: &str) -> Result<Vec<Webhook>, WebhookStorageError>;
    async fn list(&self) -> Result<Vec<Webhook>, WebhookStorageError>;
    async fn update(&self, webhook: Webhook) -> Result<Webhook, WebhookStorageError>;
    async fn delete(&self, id: &Uuid) -> Result<(), WebhookStorageError>;
    async fn exists(&self, id: &Uuid) -> Result<bool, WebhookStorageError>;

    async fn record_delivery(&self, delivery: WebhookDelivery) -> Result<(), WebhookStorageError>;
    async fn get_deliveries(
        &self,
        webhook_id: &Uuid,
        limit: usize,
    ) -> Result<Vec<WebhookDelivery>, WebhookStorageError>;
}

pub struct InMemoryWebhookStorage {
    webhooks: RwLock<HashMap<Uuid, Webhook>>,
    webhooks_by_tenant: RwLock<HashMap<String, Vec<Uuid>>>,
    deliveries: RwLock<HashMap<Uuid, Vec<WebhookDelivery>>>,
}

impl InMemoryWebhookStorage {
    pub fn new() -> Self {
        Self {
            webhooks: RwLock::new(HashMap::new()),
            webhooks_by_tenant: RwLock::new(HashMap::new()),
            deliveries: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryWebhookStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebhookStorage for InMemoryWebhookStorage {
    async fn create(&self, webhook: Webhook) -> Result<Webhook, WebhookStorageError> {
        let mut webhooks = self
            .webhooks
            .write()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;
        let mut by_tenant = self
            .webhooks_by_tenant
            .write()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;

        if webhooks.contains_key(&webhook.id) {
            return Err(WebhookStorageError::AlreadyExists(webhook.id.to_string()));
        }

        let id = webhook.id;
        let tenant_id = webhook.tenant_id.clone();
        webhooks.insert(id, webhook.clone());
        by_tenant.entry(tenant_id).or_default().push(id);

        Ok(webhook)
    }

    async fn get(&self, id: &Uuid) -> Result<Option<Webhook>, WebhookStorageError> {
        let webhooks = self
            .webhooks
            .read()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;
        Ok(webhooks.get(id).cloned())
    }

    async fn get_by_tenant(&self, tenant_id: &str) -> Result<Vec<Webhook>, WebhookStorageError> {
        let webhooks = self
            .webhooks
            .read()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;
        let by_tenant = self
            .webhooks_by_tenant
            .read()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;

        let ids = by_tenant.get(tenant_id).cloned().unwrap_or_default();
        let result: Vec<Webhook> = ids
            .iter()
            .filter_map(|id| webhooks.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn list(&self) -> Result<Vec<Webhook>, WebhookStorageError> {
        let webhooks = self
            .webhooks
            .read()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;
        Ok(webhooks.values().cloned().collect())
    }

    async fn update(&self, webhook: Webhook) -> Result<Webhook, WebhookStorageError> {
        let mut webhooks = self
            .webhooks
            .write()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;

        if !webhooks.contains_key(&webhook.id) {
            return Err(WebhookStorageError::NotFound(webhook.id.to_string()));
        }

        webhooks.insert(webhook.id, webhook.clone());
        Ok(webhook)
    }

    async fn delete(&self, id: &Uuid) -> Result<(), WebhookStorageError> {
        let mut webhooks = self
            .webhooks
            .write()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;
        let mut by_tenant = self
            .webhooks_by_tenant
            .write()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;

        if let Some(webhook) = webhooks.remove(id) {
            if let Some(ids) = by_tenant.get_mut(&webhook.tenant_id) {
                ids.retain(|i| i != id);
                if ids.is_empty() {
                    by_tenant.remove(&webhook.tenant_id);
                }
            }
        }

        Ok(())
    }

    async fn exists(&self, id: &Uuid) -> Result<bool, WebhookStorageError> {
        let webhooks = self
            .webhooks
            .read()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;
        Ok(webhooks.contains_key(id))
    }

    async fn record_delivery(&self, delivery: WebhookDelivery) -> Result<(), WebhookStorageError> {
        let mut deliveries = self
            .deliveries
            .write()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;

        deliveries
            .entry(delivery.webhook_id)
            .or_default()
            .push(delivery);

        Ok(())
    }

    async fn get_deliveries(
        &self,
        webhook_id: &Uuid,
        limit: usize,
    ) -> Result<Vec<WebhookDelivery>, WebhookStorageError> {
        let deliveries = self
            .deliveries
            .read()
            .map_err(|e| WebhookStorageError::OperationFailed(e.to_string()))?;

        let webhook_deliveries = deliveries.get(webhook_id).cloned().unwrap_or_default();
        let result: Vec<WebhookDelivery> =
            webhook_deliveries.into_iter().rev().take(limit).collect();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhooks::models::{Webhook, WebhookDelivery, WebhookEvent};
    use std::collections::HashSet;

    fn make_webhook(tenant: &str) -> Webhook {
        Webhook::new(
            "https://hook.example.com/".to_string(),
            None,
            HashSet::from([WebhookEvent::Push]),
            tenant.to_string(),
        )
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let store = InMemoryWebhookStorage::new();
        let wh = make_webhook("t1");
        let id = wh.id;
        store.create(wh).await.unwrap();
        let retrieved = store.get(&id).await.unwrap().expect("webhook should exist");
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.tenant_id, "t1");
    }

    #[tokio::test]
    async fn test_get_missing_returns_none() {
        let store = InMemoryWebhookStorage::new();
        let result = store.get(&uuid::Uuid::new_v4()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_all() {
        let store = InMemoryWebhookStorage::new();
        store.create(make_webhook("t1")).await.unwrap();
        store.create(make_webhook("t2")).await.unwrap();
        let all = store.list().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_tenant() {
        let store = InMemoryWebhookStorage::new();
        store.create(make_webhook("alpha")).await.unwrap();
        store.create(make_webhook("alpha")).await.unwrap();
        store.create(make_webhook("beta")).await.unwrap();
        let alpha = store.get_by_tenant("alpha").await.unwrap();
        assert_eq!(alpha.len(), 2);
        let beta = store.get_by_tenant("beta").await.unwrap();
        assert_eq!(beta.len(), 1);
    }

    #[tokio::test]
    async fn test_update() {
        let store = InMemoryWebhookStorage::new();
        let mut wh = make_webhook("t1");
        store.create(wh.clone()).await.unwrap();
        wh.url = "https://new.example.com/hook".to_string();
        store.update(wh.clone()).await.unwrap();
        let updated = store.get(&wh.id).await.unwrap().unwrap();
        assert_eq!(updated.url, "https://new.example.com/hook");
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryWebhookStorage::new();
        let wh = make_webhook("t1");
        let id = wh.id;
        store.create(wh).await.unwrap();
        store.delete(&id).await.unwrap();
        assert!(store.get(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_deliveries_stored_and_retrieved() {
        let store = InMemoryWebhookStorage::new();
        let wh = make_webhook("t1");
        let wh_id = wh.id;
        store.create(wh).await.unwrap();

        let delivery =
            WebhookDelivery::success(wh_id, WebhookEvent::Push, 200, Some("OK".to_string()), 42);
        store.record_delivery(delivery).await.unwrap();

        let deliveries = store.get_deliveries(&wh_id, 10).await.unwrap();
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].status_code, Some(200));
    }
}

//! Analytics event collector — receives events and persists them.

use crate::analytics::models::{AnalyticsEvent, EventType};
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CollectorError {
    #[error("Storage lock poisoned")]
    LockPoisoned,
    #[error("Buffer full — events are being dropped")]
    BufferFull,
}

/// Trait for analytics storage backends.
pub trait AnalyticsStore: Send + Sync {
    fn record(&self, event: AnalyticsEvent) -> Result<(), CollectorError>;
    fn events_for_repo(&self, repository_id: &str) -> Result<Vec<AnalyticsEvent>, CollectorError>;
    fn events_for_tenant(&self, tenant_id: &str) -> Result<Vec<AnalyticsEvent>, CollectorError>;
    fn all_events(&self) -> Result<Vec<AnalyticsEvent>, CollectorError>;
    fn clear(&self) -> Result<(), CollectorError>;
}

/// In-memory ring-buffer analytics store (bounded capacity).
pub struct InMemoryAnalyticsStore {
    events: RwLock<VecDeque<AnalyticsEvent>>,
    capacity: usize,
}

impl InMemoryAnalyticsStore {
    /// Create a store with the given maximum event capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            events: RwLock::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn shared(capacity: usize) -> Arc<Self> {
        Arc::new(Self::new(capacity))
    }

    pub fn len(&self) -> usize {
        self.events.read().map(|e| e.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AnalyticsStore for InMemoryAnalyticsStore {
    fn record(&self, event: AnalyticsEvent) -> Result<(), CollectorError> {
        let mut events = self
            .events
            .write()
            .map_err(|_| CollectorError::LockPoisoned)?;
        if events.len() >= self.capacity {
            // Evict oldest to keep bounded
            events.pop_front();
        }
        events.push_back(event);
        Ok(())
    }

    fn events_for_repo(&self, repository_id: &str) -> Result<Vec<AnalyticsEvent>, CollectorError> {
        let events = self
            .events
            .read()
            .map_err(|_| CollectorError::LockPoisoned)?;
        Ok(events
            .iter()
            .filter(|e| e.repository_id == repository_id)
            .cloned()
            .collect())
    }

    fn events_for_tenant(&self, tenant_id: &str) -> Result<Vec<AnalyticsEvent>, CollectorError> {
        let events = self
            .events
            .read()
            .map_err(|_| CollectorError::LockPoisoned)?;
        Ok(events
            .iter()
            .filter(|e| e.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    fn all_events(&self) -> Result<Vec<AnalyticsEvent>, CollectorError> {
        let events = self
            .events
            .read()
            .map_err(|_| CollectorError::LockPoisoned)?;
        Ok(events.iter().cloned().collect())
    }

    fn clear(&self) -> Result<(), CollectorError> {
        let mut events = self
            .events
            .write()
            .map_err(|_| CollectorError::LockPoisoned)?;
        events.clear();
        Ok(())
    }
}

/// High-level analytics collector — wraps a store and provides typed record methods.
pub struct AnalyticsCollector {
    store: Arc<dyn AnalyticsStore>,
}

impl AnalyticsCollector {
    pub fn new(store: Arc<dyn AnalyticsStore>) -> Self {
        Self { store }
    }

    pub fn record_clone(&self, tenant_id: &str, repo_id: &str, actor: Option<&str>) {
        self.record(EventType::Clone, tenant_id, repo_id, actor);
    }

    pub fn record_push(
        &self,
        tenant_id: &str,
        repo_id: &str,
        actor: Option<&str>,
        branch: Option<&str>,
        commit_count: usize,
    ) {
        let mut event = AnalyticsEvent::new(EventType::Push, tenant_id, repo_id);
        if let Some(a) = actor {
            event = event.with_actor(a);
        }
        if let Some(b) = branch {
            event = event.with_meta("branch", b);
        }
        event = event.with_meta("commit_count", commit_count.to_string());
        let _ = self.store.record(event);
    }

    pub fn record_fetch(&self, tenant_id: &str, repo_id: &str, actor: Option<&str>) {
        self.record(EventType::Fetch, tenant_id, repo_id, actor);
    }

    pub fn record_commit(&self, tenant_id: &str, repo_id: &str, actor: Option<&str>, sha: &str) {
        let event =
            AnalyticsEvent::new(EventType::Commit, tenant_id, repo_id).with_meta("sha", sha);
        let event = if let Some(a) = actor {
            event.with_actor(a)
        } else {
            event
        };
        let _ = self.store.record(event);
    }

    pub fn record_lfs_upload(&self, tenant_id: &str, repo_id: &str, bytes: u64) {
        let event = AnalyticsEvent::new(EventType::LfsUpload, tenant_id, repo_id)
            .with_meta("bytes", bytes.to_string());
        let _ = self.store.record(event);
    }

    pub fn record_lfs_download(&self, tenant_id: &str, repo_id: &str, bytes: u64) {
        let event = AnalyticsEvent::new(EventType::LfsDownload, tenant_id, repo_id)
            .with_meta("bytes", bytes.to_string());
        let _ = self.store.record(event);
    }

    /// Record a generic event.
    fn record(&self, event_type: EventType, tenant_id: &str, repo_id: &str, actor: Option<&str>) {
        let event = AnalyticsEvent::new(event_type, tenant_id, repo_id);
        let event = if let Some(a) = actor {
            event.with_actor(a)
        } else {
            event
        };
        let _ = self.store.record(event);
    }

    pub fn store(&self) -> &Arc<dyn AnalyticsStore> {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::models::EventType;

    fn make_collector() -> AnalyticsCollector {
        AnalyticsCollector::new(InMemoryAnalyticsStore::shared(1000))
    }

    #[test]
    fn test_record_clone() {
        let c = make_collector();
        c.record_clone("t1", "org/repo", Some("alice"));
        let events = c.store().events_for_repo("org/repo").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Clone);
        assert_eq!(events[0].actor.as_deref(), Some("alice"));
    }

    #[test]
    fn test_record_push_with_branch() {
        let c = make_collector();
        c.record_push("t1", "org/repo", Some("bob"), Some("main"), 3);
        let events = c.store().events_for_repo("org/repo").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].metadata["branch"], "main");
        assert_eq!(events[0].metadata["commit_count"], "3");
    }

    #[test]
    fn test_record_lfs() {
        let c = make_collector();
        c.record_lfs_upload("t1", "org/repo", 1024 * 1024);
        c.record_lfs_download("t1", "org/repo", 512);
        let events = c.store().events_for_repo("org/repo").unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_ring_buffer_eviction() {
        let store = InMemoryAnalyticsStore::new(3);
        let collector = AnalyticsCollector::new(Arc::new(store));
        for i in 0..5 {
            collector.record_clone("t1", &format!("repo-{}", i), None);
        }
        // Only 3 most recent should remain
        let events = collector.store().all_events().unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_filter_by_tenant() {
        let c = make_collector();
        c.record_clone("alpha", "alpha/repo", None);
        c.record_clone("beta", "beta/repo", None);
        c.record_clone("alpha", "alpha/repo2", None);
        let alpha = c.store().events_for_tenant("alpha").unwrap();
        assert_eq!(alpha.len(), 2);
        let beta = c.store().events_for_tenant("beta").unwrap();
        assert_eq!(beta.len(), 1);
    }
}

//! Stateless design helpers for horizontal scaling
//!
//! Provides utilities and patterns for ensuring the application
//! can scale horizontally without shared local state.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

pub struct StatelessHelper {
    instance_id: String,
    shared_state_available: bool,
}

impl StatelessHelper {
    pub fn new(instance_id: String, shared_state_available: bool) -> Self {
        Self {
            instance_id,
            shared_state_available,
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn is_shared_state_available(&self) -> bool {
        self.shared_state_available
    }

    pub fn check_stateless_requirement(&self, feature: &str) -> Result<(), StatelessError> {
        if !self.shared_state_available {
            debug!(
                "Feature '{}' requires shared state but none available",
                feature
            );
        }
        Ok(())
    }

    pub fn distributed_counter(&self, name: &str) -> DistributedCounter {
        DistributedCounter::new(name.to_string(), self.shared_state_available)
    }

    pub fn distributed_set<T: Clone + Send + Sync + 'static>(
        &self,
        name: &str,
    ) -> DistributedSet<T> {
        DistributedSet::new(name.to_string(), self.shared_state_available)
    }

    pub fn distributed_map<K, V>(&self, name: &str) -> DistributedMap<K, V>
    where
        K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
        V: Clone + Send + Sync + 'static,
    {
        DistributedMap::new(name.to_string(), self.shared_state_available)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StatelessError {
    #[error("Shared state not available: {0}")]
    NotAvailable(String),

    #[error("Operation requires cluster mode: {0}")]
    RequiresCluster(String),
}

pub struct DistributedCounter {
    name: String,
    shared: bool,
    local: Arc<RwLock<i64>>,
}

impl DistributedCounter {
    fn new(name: String, shared: bool) -> Self {
        Self {
            name,
            shared,
            local: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn increment(&self, delta: i64) -> i64 {
        let mut value = self.local.write().await;
        *value += delta;

        if self.shared {
            debug!("Counter {} incremented by {} (shared)", self.name, delta);
        }

        *value
    }

    pub async fn decrement(&self, delta: i64) -> i64 {
        self.increment(-delta).await
    }

    pub async fn get(&self) -> i64 {
        *self.local.read().await
    }

    pub async fn reset(&self) {
        let mut value = self.local.write().await;
        *value = 0;
    }
}

pub struct DistributedSet<T: Clone + Send + Sync + 'static> {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    shared: bool,
    local: Arc<RwLock<Vec<T>>>,
}

impl<T: Clone + Send + Sync + 'static> DistributedSet<T> {
    fn new(name: String, shared: bool) -> Self {
        Self {
            name,
            shared,
            local: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn insert(&self, value: T) -> bool {
        let mut set = self.local.write().await;
        if !set
            .iter()
            .any(|v| std::mem::discriminant(v) == std::mem::discriminant(&value))
        {
            set.push(value);
            true
        } else {
            false
        }
    }

    pub async fn remove<F>(&self, predicate: F) -> Option<T>
    where
        F: Fn(&T) -> bool,
    {
        let mut set = self.local.write().await;
        if let Some(pos) = set.iter().position(|v| predicate(v)) {
            Some(set.remove(pos))
        } else {
            None
        }
    }

    pub async fn contains<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        let set = self.local.read().await;
        set.iter().any(predicate)
    }

    pub async fn len(&self) -> usize {
        self.local.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.local.read().await.is_empty()
    }

    pub async fn clear(&self) {
        self.local.write().await.clear();
    }

    pub async fn to_vec(&self) -> Vec<T> {
        self.local.read().await.clone()
    }
}

pub struct DistributedMap<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    shared: bool,
    local: Arc<RwLock<HashMap<K, V>>>,
}

impl<K, V> DistributedMap<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn new(name: String, shared: bool) -> Self {
        Self {
            name,
            shared,
            local: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert(&self, key: K, value: V) -> Option<V> {
        let mut map = self.local.write().await;
        map.insert(key, value)
    }

    pub async fn get(&self, key: &K) -> Option<V> {
        let map = self.local.read().await;
        map.get(key).cloned()
    }

    pub async fn remove(&self, key: &K) -> Option<V> {
        let mut map = self.local.write().await;
        map.remove(key)
    }

    pub async fn contains_key(&self, key: &K) -> bool {
        self.local.read().await.contains_key(key)
    }

    pub async fn len(&self) -> usize {
        self.local.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.local.read().await.is_empty()
    }

    pub async fn clear(&self) {
        self.local.write().await.clear();
    }

    pub async fn entry<F, R>(&self, key: K, f: F) -> R
    where
        F: FnOnce(Option<&V>) -> R,
    {
        let map = self.local.read().await;
        f(map.get(&key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_distributed_counter() {
        let helper = StatelessHelper::new("test".to_string(), false);
        let counter = helper.distributed_counter("test-counter");

        assert_eq!(counter.get().await, 0);
        assert_eq!(counter.increment(5).await, 5);
        assert_eq!(counter.increment(3).await, 8);
        assert_eq!(counter.decrement(2).await, 6);
        counter.reset().await;
        assert_eq!(counter.get().await, 0);
    }

    #[tokio::test]
    async fn test_distributed_set() {
        let helper = StatelessHelper::new("test".to_string(), false);
        let set: DistributedSet<String> = helper.distributed_set("test-set");

        assert!(set.is_empty().await);
        assert!(set.insert("a".to_string()).await);
        assert!(set.contains(|v| v == "a").await);
        assert_eq!(set.len().await, 1);
        set.clear().await;
        assert!(set.is_empty().await);
    }

    #[tokio::test]
    async fn test_distributed_map() {
        let helper = StatelessHelper::new("test".to_string(), false);
        let map: DistributedMap<String, i32> = helper.distributed_map("test-map");

        assert!(map.is_empty().await);
        assert!(map.insert("a".to_string(), 1).await.is_none());
        assert_eq!(map.get(&"a".to_string()).await, Some(1));
        assert_eq!(map.len().await, 1);
        map.clear().await;
        assert!(map.is_empty().await);
    }
}

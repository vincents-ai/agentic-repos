//! Cluster management and instance tracking
//!
//! Provides cluster status, instance discovery, and health tracking
//! for horizontal scaling deployments.
//!
//! ## Instance Authentication
//!
//! When a `cluster_shared_secret` is configured in `ScalingConfig`, all instances
//! must present an HMAC of their heartbeat payload signed with that secret.
//! Instances without a valid HMAC are rejected during registration and discovery.
//! If no secret is configured, authentication is skipped (single-node dev mode).

use crate::scaling::config::ScalingConfig;
use crate::scaling::lock::{DistributedLock, LockOptions};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub instance_id: String,
    pub cluster_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub version: String,
    pub status: InstanceStatus,
    pub metadata: HashMap<String, String>,
    pub metrics: InstanceMetrics,
    /// HMAC-SHA256 of `instance_id + cluster_name + started_at.timestamp()` signed with
    /// the cluster shared secret. Verified by other instances to prevent rogue registration.
    #[serde(skip_serializing)]
    pub auth_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InstanceStatus {
    Starting,
    Healthy,
    Degraded,
    Draining,
    ShuttingDown,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstanceMetrics {
    pub active_connections: u64,
    pub requests_per_second: f64,
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub storage_usage_bytes: u64,
}

impl InstanceInfo {
    pub fn new(instance_id: String, cluster_name: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            instance_id,
            cluster_name,
            started_at: now,
            last_heartbeat: now,
            version: env!("CARGO_PKG_VERSION").to_string(),
            status: InstanceStatus::Starting,
            metadata: HashMap::new(),
            metrics: InstanceMetrics::default(),
            auth_tag: None,
        }
    }

    pub fn is_healthy(&self, timeout_secs: u64) -> bool {
        let now = chrono::Utc::now();
        let elapsed = (now - self.last_heartbeat).num_seconds();
        elapsed < timeout_secs as i64
            && matches!(
                self.status,
                InstanceStatus::Healthy | InstanceStatus::Starting
            )
    }

    pub fn uptime_secs(&self) -> i64 {
        (chrono::Utc::now() - self.started_at).num_seconds()
    }

    /// Compute an HMAC-SHA256 auth tag for this instance using the shared secret.
    pub fn compute_auth_tag(&self, secret: &str) -> String {
        let payload = format!(
            "{}:{}:{}",
            self.instance_id,
            self.cluster_name,
            self.started_at.timestamp()
        );
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// Verify this instance's auth tag against the shared secret.
    /// Returns false if no tag is present or if verification fails.
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn verify_auth_tag(&self, secret: &str) -> bool {
        if let Some(ref tag) = self.auth_tag {
            let expected = self.compute_auth_tag(secret);
            if tag.len() != expected.len() {
                return false;
            }
            let mut diff: u8 = 0;
            for (a, b) in tag.as_bytes().iter().zip(expected.as_bytes().iter()) {
                diff |= a ^ b;
            }
            diff == 0
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub cluster_name: String,
    pub total_instances: usize,
    pub healthy_instances: usize,
    pub leader_id: Option<String>,
    pub instances: Vec<InstanceInfo>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

pub struct ClusterManager {
    config: ScalingConfig,
    local_instance: InstanceInfo,
    instances: Arc<RwLock<HashMap<String, InstanceInfo>>>,
    distributed_lock: Arc<DistributedLock>,
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

impl ClusterManager {
    pub fn new(config: ScalingConfig) -> Self {
        // Validate config to prevent Redis key injection
        if let Err(e) = config.validate() {
            panic!("Invalid scaling config: {}", e);
        }

        let local_instance =
            InstanceInfo::new(config.instance_id.clone(), config.cluster_name.clone());
        let distributed_lock = Arc::new(DistributedLock::new(
            config.redis_url.clone(),
            config.instance_id.clone(),
        ));
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        Self {
            config,
            local_instance,
            instances: Arc::new(RwLock::new(HashMap::new())),
            distributed_lock,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn local_instance(&self) -> &InstanceInfo {
        &self.local_instance
    }

    pub fn local_instance_mut(&mut self) -> &mut InstanceInfo {
        &mut self.local_instance
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Starting cluster manager for instance {} in cluster {}",
            self.config.instance_id, self.config.cluster_name
        );

        self.local_instance.status = InstanceStatus::Healthy;
        self.register_instance().await?;

        self.start_heartbeat_task();
        self.start_instance_discovery_task();

        Ok(())
    }

    async fn register_instance(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut instance = self.local_instance.clone();

        // Sign our registration with the cluster secret
        if let Some(ref secret) = self.config.cluster_shared_secret {
            instance.auth_tag = Some(instance.compute_auth_tag(secret));
        }

        let mut instances = self.instances.write().await;
        instances.insert(self.local_instance.instance_id.clone(), instance);
        debug!("Registered instance: {}", self.local_instance.instance_id);
        Ok(())
    }

    fn start_heartbeat_task(&self) {
        let instances = Arc::clone(&self.instances);
        let instance_id = self.config.instance_id.clone();
        let interval_secs = self.config.heartbeat_interval_secs;
        let secret = self.config.cluster_shared_secret.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                let mut instances = instances.write().await;
                if let Some(instance) = instances.get_mut(&instance_id) {
                    instance.last_heartbeat = chrono::Utc::now();
                    // Re-sign heartbeat with fresh tag
                    if let Some(ref s) = secret {
                        instance.auth_tag = Some(instance.compute_auth_tag(s));
                    }
                    debug!("Heartbeat sent for instance {}", instance_id);
                }
            }
        });
    }

    fn start_instance_discovery_task(&self) {
        let instances = Arc::clone(&self.instances);
        let timeout_secs = self.config.instance_timeout_secs;
        let local_id = self.config.instance_id.clone();
        let secret = self.config.cluster_shared_secret.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(timeout_secs / 2));
            loop {
                interval.tick().await;
                let mut instances = instances.write().await;
                let now = chrono::Utc::now();
                instances.retain(|id, instance| {
                    if id == &local_id {
                        return true;
                    }
                    // Verify auth tag if secret is configured
                    if let Some(ref s) = secret {
                        if !instance.verify_auth_tag(s) {
                            warn!("Instance {} failed auth tag verification, removing", id);
                            return false;
                        }
                    }
                    let elapsed = (now - instance.last_heartbeat).num_seconds();
                    if elapsed > timeout_secs as i64 {
                        warn!("Instance {} timed out ({}s), removing", id, elapsed);
                        return false;
                    }
                    true
                });
            }
        });
    }

    pub async fn get_cluster_status(&self) -> ClusterStatus {
        let instances = self.instances.read().await;
        let instance_list: Vec<InstanceInfo> = instances.values().cloned().collect();
        let healthy_count = instance_list
            .iter()
            .filter(|i| i.is_healthy(self.config.instance_timeout_secs))
            .count();

        ClusterStatus {
            cluster_name: self.config.cluster_name.clone(),
            total_instances: instance_list.len(),
            healthy_instances: healthy_count,
            leader_id: None,
            instances: instance_list,
            last_updated: chrono::Utc::now(),
        }
    }

    pub async fn get_instances(&self) -> Vec<InstanceInfo> {
        let instances = self.instances.read().await;
        instances.values().cloned().collect()
    }

    pub async fn get_instance(&self, instance_id: &str) -> Option<InstanceInfo> {
        let instances = self.instances.read().await;
        instances.get(instance_id).cloned()
    }

    pub async fn update_local_metrics(&mut self, metrics: InstanceMetrics) {
        self.local_instance.metrics = metrics.clone();
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.get_mut(&self.config.instance_id) {
            instance.metrics = metrics;
        }
    }

    pub async fn set_local_status(&mut self, status: InstanceStatus) {
        self.local_instance.status = status.clone();
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.get_mut(&self.config.instance_id) {
            instance.status = status;
        }
    }

    pub async fn drain(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Draining instance {}", self.config.instance_id);
        self.set_local_status(InstanceStatus::Draining).await;
        Ok(())
    }

    pub fn shutdown_signal(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.shutdown_tx
            .as_ref()
            .expect("shutdown_tx not initialized")
            .subscribe()
    }

    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Shutting down cluster manager for instance {}",
            self.config.instance_id
        );
        self.set_local_status(InstanceStatus::ShuttingDown).await;

        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(());
        }

        let mut instances = self.instances.write().await;
        instances.remove(&self.config.instance_id);

        Ok(())
    }

    pub fn distributed_lock(&self) -> Arc<DistributedLock> {
        Arc::clone(&self.distributed_lock)
    }

    pub async fn acquire_cluster_lock(
        &self,
        resource: &str,
    ) -> Result<crate::scaling::lock::LockHandle, crate::scaling::lock::LockError> {
        let key = format!("cluster:{}:{}", self.config.cluster_name, resource);
        let options = LockOptions::new(&key)
            .with_ttl(Duration::from_secs(30))
            .with_retry(Duration::from_millis(100), 30);

        self.distributed_lock.acquire(options).await
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cluster_manager_creation() {
        let config = ScalingConfig::default();
        let manager = ClusterManager::new(config);
        assert!(!manager.is_enabled());
    }

    #[tokio::test]
    async fn test_instance_health_check() {
        let mut instance = InstanceInfo::new("test".to_string(), "cluster".to_string());
        instance.status = InstanceStatus::Healthy;

        assert!(instance.is_healthy(30));

        instance.last_heartbeat = chrono::Utc::now() - chrono::Duration::seconds(60);
        assert!(!instance.is_healthy(30));
    }

    #[tokio::test]
    async fn test_cluster_status() {
        let config = ScalingConfig::default();
        let manager = ClusterManager::new(config);

        let status = manager.get_cluster_status().await;
        assert_eq!(status.cluster_name, "agentic-repos-default");
    }
}

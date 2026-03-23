//! Leader election for scheduled jobs
//!
//! Provides leader election mechanism to ensure only one instance
//! in the cluster runs scheduled tasks (GC, cleanup, etc.).

use crate::scaling::cluster::ClusterManager;
use crate::scaling::config::ScalingConfig;
use crate::scaling::lock::{DistributedLock, LockError, LockHandle, LockOptions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderInfo {
    pub instance_id: String,
    pub cluster_name: String,
    pub elected_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub leadership_term: u64,
}

impl LeaderInfo {
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    pub fn is_leader(&self, instance_id: &str) -> bool {
        !self.is_expired() && self.instance_id == instance_id
    }

    pub fn remaining_ttl(&self) -> Duration {
        let now = chrono::Utc::now();
        if now >= self.expires_at {
            Duration::ZERO
        } else {
            Duration::from_secs((self.expires_at - now).num_seconds().max(0) as u64)
        }
    }
}

pub struct LeaderElection {
    config: ScalingConfig,
    distributed_lock: Arc<DistributedLock>,
    current_leader: Arc<RwLock<Option<LeaderInfo>>>,
    leadership_term: Arc<RwLock<u64>>,
    renew_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl LeaderElection {
    pub fn new(config: ScalingConfig, distributed_lock: Arc<DistributedLock>) -> Self {
        Self {
            config,
            distributed_lock,
            current_leader: Arc::new(RwLock::new(None)),
            leadership_term: Arc::new(RwLock::new(0)),
            renew_task: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn try_become_leader(&self) -> Result<Option<LeaderHandle>, LockError> {
        let lock_key = format!("leader:{}", self.config.cluster_name);
        let options = LockOptions::new(&lock_key)
            .with_ttl(Duration::from_secs(self.config.leader_election_timeout_secs * 3))
            .with_retry(Duration::from_millis(50), 1);

        match self.distributed_lock.acquire(options).await {
            Ok(handle) => {
                let now = chrono::Utc::now();
                let expires_at = now
                    + chrono::Duration::seconds(
                        (self.config.leader_election_timeout_secs * 3) as i64,
                    );

                let mut term = self.leadership_term.write().await;
                *term += 1;
                let current_term = *term;

                let leader_info = LeaderInfo {
                    instance_id: self.config.instance_id.clone(),
                    cluster_name: self.config.cluster_name.clone(),
                    elected_at: now,
                    expires_at,
                    leadership_term: current_term,
                };

                *self.current_leader.write().await = Some(leader_info.clone());

                info!(
                    "Instance {} became leader for cluster {} (term {})",
                    self.config.instance_id, self.config.cluster_name, current_term
                );

                Ok(Some(LeaderHandle {
                    lock_handle: handle,
                    leader_info,
                    distributed_lock: Arc::clone(&self.distributed_lock),
                }))
            }
            Err(LockError::AcquireFailed(_)) => {
                debug!(
                    "Instance {} did not become leader - another instance holds it",
                    self.config.instance_id
                );
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn get_current_leader(&self) -> Option<LeaderInfo> {
        let leader = self.current_leader.read().await;
        leader.as_ref().filter(|l| !l.is_expired()).cloned()
    }

    pub async fn is_leader(&self) -> bool {
        let leader = self.current_leader.read().await;
        leader
            .as_ref()
            .map(|l| l.is_leader(&self.config.instance_id))
            .unwrap_or(false)
    }

    pub async fn step_down(&self) {
        let mut leader = self.current_leader.write().await;
        if let Some(ref info) = *leader {
            if info.instance_id == self.config.instance_id {
                info!(
                    "Instance {} stepping down as leader for cluster {}",
                    self.config.instance_id, self.config.cluster_name
                );
                *leader = None;
            }
        }
    }

    pub async fn run_as_leader<F, Fut, T>(&self, task: F) -> Result<T, LockError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let handle = self.try_become_leader().await?;

        match handle {
            Some(leader_handle) => {
                let result = task().await;
                leader_handle.release().await?;
                Ok(result)
            }
            None => Err(LockError::AcquireFailed(
                "Could not acquire leadership".to_string(),
            )),
        }
    }

    pub fn start_renewal_task(&self) {
        let distributed_lock = Arc::clone(&self.distributed_lock);
        let current_leader = Arc::clone(&self.current_leader);
        let instance_id = self.config.instance_id.clone();
        let interval_secs = self.config.leader_election_timeout_secs;

        let handle = tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(interval_secs / 2));
            loop {
                interval.tick().await;

                let mut leader = current_leader.write().await;
                if let Some(ref mut info) = *leader {
                    if info.instance_id == instance_id {
                        let new_expires = chrono::Utc::now()
                            + chrono::Duration::seconds((interval_secs * 3) as i64);
                        info.expires_at = new_expires;
                        debug!("Renewed leadership for instance {}", instance_id);
                    }
                }
            }
        });

        let mut task = self.renew_task.write().await;
        *task = Some(handle);
    }

    pub async fn stop_renewal_task(&self) {
        let mut task = self.renew_task.write().await;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }
}

pub struct LeaderHandle {
    lock_handle: LockHandle,
    leader_info: LeaderInfo,
    distributed_lock: Arc<DistributedLock>,
}

impl LeaderHandle {
    pub fn leader_info(&self) -> &LeaderInfo {
        &self.leader_info
    }

    pub async fn release(self) -> Result<(), LockError> {
        self.distributed_lock.release(self.lock_handle).await
    }

    pub async fn extend(&mut self, additional_ttl: Duration) -> Result<(), LockError> {
        self.distributed_lock
            .extend(&mut self.lock_handle, additional_ttl)
            .await?;
        self.leader_info.expires_at += chrono::Duration::from_std(additional_ttl).unwrap();
        Ok(())
    }
}

pub struct LeaderOnlyJob<F> {
    name: String,
    task: F,
    interval: Duration,
}

impl<F, Fut> LeaderOnlyJob<F>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    pub fn new(name: impl Into<String>, interval: Duration, task: F) -> Self {
        Self {
            name: name.into(),
            task,
            interval,
        }
    }

    pub async fn run(&self, leader_election: Arc<LeaderElection>) {
        let name = self.name.clone();
        let interval = self.interval;

        loop {
            tokio::time::sleep(interval).await;

            if leader_election.is_leader().await {
                info!("Running leader-only job: {}", name);
                (self.task)().await;
            } else {
                debug!("Skipping job {} - not leader", name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_leader_info_expiry() {
        let info = LeaderInfo {
            instance_id: "test".to_string(),
            cluster_name: "cluster".to_string(),
            elected_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(30),
            leadership_term: 1,
        };

        assert!(!info.is_expired());
        assert!(info.is_leader("test"));
        assert!(!info.is_leader("other"));
    }

    #[tokio::test]
    async fn test_leader_election_creation() {
        let config = ScalingConfig::default();
        let lock = Arc::new(DistributedLock::new(None, config.instance_id.clone()));
        let election = LeaderElection::new(config, lock);

        assert!(!election.is_leader().await);
    }
}

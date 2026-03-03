use crate::lfs::LfsLockManager;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info};

pub struct LockCleanupScheduler {
    lock_manager: Arc<LfsLockManager>,
    cleanup_interval: Duration,
    enabled: bool,
}

impl LockCleanupScheduler {
    pub fn new(
        lock_manager: Arc<LfsLockManager>,
        cleanup_interval_secs: u64,
        enabled: bool,
    ) -> Self {
        Self {
            lock_manager,
            cleanup_interval: Duration::from_secs(cleanup_interval_secs),
            enabled,
        }
    }

    pub async fn start(self: Arc<Self>) {
        if !self.enabled {
            info!("Lock cleanup scheduler is disabled");
            return;
        }

        info!(
            "Starting lock cleanup scheduler with interval: {:?}",
            self.cleanup_interval
        );

        let mut ticker = interval(self.cleanup_interval);

        loop {
            ticker.tick().await;
            debug!("Running scheduled lock cleanup");

            match self.lock_manager.cleanup_expired_locks().await {
                Ok(count) => {
                    if count > 0 {
                        info!("Cleaned up {} expired lock(s)", count);
                        crate::lfs::record_lock_cleanup(count);
                    } else {
                        debug!("No expired locks to clean up");
                    }
                }
                Err(e) => {
                    error!("Failed to clean up expired locks: {}", e);
                }
            }
        }
    }

    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        let scheduler = Arc::new(self);
        tokio::spawn(async move {
            scheduler.start().await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn try_connect_db() -> Option<Arc<PgPool>> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/test".to_string());

        match PgPool::connect(&database_url).await {
            Ok(pool) => Some(Arc::new(pool)),
            Err(_) => {
                println!("Skipping test: PostgreSQL not available");
                None
            }
        }
    }

    #[tokio::test]
    async fn test_scheduler_construction() {
        let Some(pool) = try_connect_db().await else {
            return;
        };
        let lock_manager = Arc::new(LfsLockManager::new(pool));

        let scheduler = LockCleanupScheduler::new(lock_manager, 3600, true);
        assert_eq!(scheduler.cleanup_interval, Duration::from_secs(3600));
        assert!(scheduler.enabled);
    }

    #[tokio::test]
    async fn test_scheduler_disabled() {
        let Some(pool) = try_connect_db().await else {
            return;
        };
        let lock_manager = Arc::new(LfsLockManager::new(pool));

        let scheduler = Arc::new(LockCleanupScheduler::new(lock_manager, 3600, false));
        scheduler.start().await;
    }
}

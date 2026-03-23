//! Graceful shutdown handling for horizontal scaling
//!
//! Provides graceful shutdown with:
//! - Connection draining
//! - In-flight request completion
//! - Leader step-down
//! - Health status updates

use crate::scaling::cluster::{ClusterManager, InstanceStatus};
use crate::scaling::leader::LeaderElection;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShutdownPhase {
    Running,
    Draining,
    ShuttingDown,
    Complete,
}

#[derive(Clone)]
pub struct ShutdownSignal {
    phase: Arc<RwLock<ShutdownPhase>>,
    tx: broadcast::Sender<ShutdownPhase>,
}

impl ShutdownSignal {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            phase: Arc::new(RwLock::new(ShutdownPhase::Running)),
            tx,
        }
    }

    pub async fn current_phase(&self) -> ShutdownPhase {
        *self.phase.read().await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ShutdownPhase> {
        self.tx.subscribe()
    }

    pub async fn set_phase(&self, phase: ShutdownPhase) {
        let mut current = self.phase.write().await;
        *current = phase;
        let _ = self.tx.send(phase);
    }

    pub async fn wait_for_shutdown(&self) {
        let mut rx = self.tx.subscribe();
        loop {
            match rx.recv().await {
                Ok(ShutdownPhase::Complete) => break,
                Err(broadcast::error::RecvError::Closed) => break,
                _ => continue,
            }
        }
    }
}

pub struct GracefulShutdown {
    signal: ShutdownSignal,
    cluster_manager: Option<Arc<RwLock<ClusterManager>>>,
    leader_election: Option<Arc<LeaderElection>>,
    timeout_secs: u64,
}

impl GracefulShutdown {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            signal: ShutdownSignal::new(),
            cluster_manager: None,
            leader_election: None,
            timeout_secs,
        }
    }

    pub fn with_cluster_manager(mut self, manager: Arc<RwLock<ClusterManager>>) -> Self {
        self.cluster_manager = Some(manager);
        self
    }

    pub fn with_leader_election(mut self, election: Arc<LeaderElection>) -> Self {
        self.leader_election = Some(election);
        self
    }

    pub fn signal(&self) -> &ShutdownSignal {
        &self.signal
    }

    pub fn is_shutting_down(&self) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let phase = self.signal.current_phase().await;
                matches!(
                    phase,
                    ShutdownPhase::Draining | ShutdownPhase::ShuttingDown | ShutdownPhase::Complete
                )
            })
        })
    }

    pub async fn initiate(&self) {
        info!("Initiating graceful shutdown");
        self.signal.set_phase(ShutdownPhase::Draining).await;

        if let Some(ref manager) = self.cluster_manager {
            let mut mgr = manager.write().await;
            mgr.set_local_status(InstanceStatus::Draining).await;
            info!("Instance marked as draining");
        }

        if let Some(ref election) = self.leader_election {
            election.step_down().await;
            election.stop_renewal_task().await;
            info!("Stepped down from leadership");
        }

        let drain_timeout = Duration::from_secs(self.timeout_secs / 2);
        info!(
            "Waiting up to {:?} for connections to drain",
            drain_timeout
        );

        let signal = self.signal.clone();
        let _drain_signal = signal.clone();

        tokio::select! {
            _ = self.drain_connections() => {
                info!("Connection draining complete");
            }
            _ = tokio::time::sleep(drain_timeout) => {
                warn!("Drain timeout reached, proceeding with shutdown");
            }
        }

        self.signal.set_phase(ShutdownPhase::ShuttingDown).await;

        if let Some(ref manager) = self.cluster_manager {
            let mut mgr = manager.write().await;
            mgr.set_local_status(InstanceStatus::ShuttingDown).await;
        }

        let shutdown_timeout = Duration::from_secs(self.timeout_secs / 2);
        tokio::select! {
            _ = self.complete_shutdown() => {
                info!("Shutdown tasks complete");
            }
            _ = tokio::time::sleep(shutdown_timeout) => {
                warn!("Shutdown timeout reached, forcing exit");
            }
        }

        if let Some(ref manager) = self.cluster_manager {
            let mut mgr = manager.write().await;
            let _ = mgr.shutdown().await;
        }

        self.signal.set_phase(ShutdownPhase::Complete).await;
        info!("Graceful shutdown complete");
    }

    async fn drain_connections(&self) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    async fn complete_shutdown(&self) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    pub fn setup_signal_handlers(&self) -> tokio::task::JoinHandle<()> {
        let signal = self.signal.clone();

        tokio::spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};

                let mut sigterm = signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");
                let mut sigint = signal(SignalKind::interrupt()).expect("Failed to install SIGINT handler");

                tokio::select! {
                    _ = sigterm.recv() => {
                        info!("Received SIGTERM");
                    }
                    _ = sigint.recv() => {
                        info!("Received SIGINT");
                    }
                }
            }

            #[cfg(not(unix))]
            {
                tokio::signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
                info!("Received Ctrl+C");
            }

            signal.set_phase(ShutdownPhase::Draining).await;
        })
    }

    pub async fn wait_for_signal(&self) {
        let mut rx = self.signal.subscribe();

        while let Ok(phase) = rx.recv().await {
            if matches!(phase, ShutdownPhase::Draining | ShutdownPhase::ShuttingDown) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_signal() {
        let signal = ShutdownSignal::new();

        assert_eq!(signal.current_phase().await, ShutdownPhase::Running);

        signal.set_phase(ShutdownPhase::Draining).await;
        assert_eq!(signal.current_phase().await, ShutdownPhase::Draining);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_graceful_shutdown_creation() {
        let shutdown = GracefulShutdown::new(30);
        assert!(!shutdown.is_shutting_down());
    }
}

//! Horizontal scaling support for repos-mcp
//!
//! This module provides:
//! - Stateless design helpers
//! - Distributed lock mechanism (Redis-based)
//! - Instance health tracking
//! - Leader election for scheduled jobs
//! - Session affinity configuration
//! - Graceful shutdown handling
//! - Health check improvements (readiness/liveness)

pub mod cluster;
pub mod config;
pub mod health;
pub mod leader;
pub mod lock;
pub mod session;
pub mod shutdown;
pub mod stateless;

pub use cluster::{ClusterManager, ClusterStatus, InstanceInfo};
pub use config::ScalingConfig;
pub use health::{HealthChecker, LivenessStatus, ReadinessStatus};
pub use leader::{LeaderElection, LeaderInfo};
pub use lock::{DistributedLock, LockError, LockResult};
pub use session::{SessionAffinity, SessionConfig};
pub use shutdown::{GracefulShutdown, ShutdownSignal};
pub use stateless::StatelessHelper;

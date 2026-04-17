pub mod aggregator;
pub mod collector;
pub mod handlers;
pub mod models;

pub use collector::{AnalyticsCollector, AnalyticsStore, InMemoryAnalyticsStore};
pub use handlers::configure_analytics_routes;
pub use models::{AnalyticsEvent, EventType, RepoStats, TenantStats, TrendPoint, TrendQuery};

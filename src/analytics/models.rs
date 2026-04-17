//! Analytics data models — events, aggregations, and trend snapshots.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A single analytics event fired when a git operation occurs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsEvent {
    pub id: Uuid,
    pub event_type: EventType,
    pub tenant_id: String,
    pub repository_id: String,
    /// Actor (username or service account) who triggered the event.
    pub actor: Option<String>,
    /// Extra key/value metadata (branch, ref, bytes_transferred, etc.)
    pub metadata: HashMap<String, String>,
    pub occurred_at: DateTime<Utc>,
}

impl AnalyticsEvent {
    pub fn new(
        event_type: EventType,
        tenant_id: impl Into<String>,
        repository_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            tenant_id: tenant_id.into(),
            repository_id: repository_id.into(),
            actor: None,
            metadata: HashMap::new(),
            occurred_at: Utc::now(),
        }
    }

    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// The type of git/platform operation being recorded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// `git clone` or initial fetch.
    Clone,
    /// `git fetch` (subsequent).
    Fetch,
    /// `git push` (receive-pack).
    Push,
    /// A commit was made.
    Commit,
    /// A branch was created.
    BranchCreate,
    /// A branch was deleted.
    BranchDelete,
    /// A tag was created.
    TagCreate,
    /// An LFS object was uploaded.
    LfsUpload,
    /// An LFS object was downloaded.
    LfsDownload,
    /// A repository was created.
    RepoCreate,
    /// A repository was deleted.
    RepoDelete,
    /// An API request (general).
    ApiRequest,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            EventType::Clone => "clone",
            EventType::Fetch => "fetch",
            EventType::Push => "push",
            EventType::Commit => "commit",
            EventType::BranchCreate => "branch_create",
            EventType::BranchDelete => "branch_delete",
            EventType::TagCreate => "tag_create",
            EventType::LfsUpload => "lfs_upload",
            EventType::LfsDownload => "lfs_download",
            EventType::RepoCreate => "repo_create",
            EventType::RepoDelete => "repo_delete",
            EventType::ApiRequest => "api_request",
        };
        write!(f, "{}", s)
    }
}

/// Aggregated statistics for a single repository over a time window.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoStats {
    pub repository_id: String,
    pub tenant_id: String,
    /// Total number of commits recorded.
    pub commit_count: u64,
    /// Total number of clones.
    pub clone_count: u64,
    /// Total number of pushes.
    pub push_count: u64,
    /// Total number of fetches.
    pub fetch_count: u64,
    /// Total LFS bytes uploaded.
    pub lfs_bytes_uploaded: u64,
    /// Total LFS bytes downloaded.
    pub lfs_bytes_downloaded: u64,
    /// Unique contributors (actors who pushed).
    pub unique_contributors: u64,
    /// Most active branch (by push count).
    pub most_active_branch: Option<String>,
    /// Window start timestamp.
    pub window_start: Option<DateTime<Utc>>,
    /// Window end timestamp.
    pub window_end: Option<DateTime<Utc>>,
}

/// Aggregated tenant-level statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TenantStats {
    pub tenant_id: String,
    pub total_repositories: u64,
    pub total_commits: u64,
    pub total_clones: u64,
    pub total_pushes: u64,
    pub active_contributors: u64,
    /// Top-N repositories by activity.
    pub top_repositories: Vec<RepoActivity>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
}

/// Repository activity ranking entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoActivity {
    pub repository_id: String,
    pub event_count: u64,
}

/// A trend data point: event count in a bucketed time interval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub bucket_start: DateTime<Utc>,
    pub event_type: EventType,
    pub count: u64,
}

/// Time bucket granularity for trend queries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrendBucket {
    Hourly,
    Daily,
    Weekly,
}

/// Query parameters for trend analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendQuery {
    pub repository_id: Option<String>,
    pub tenant_id: Option<String>,
    pub event_types: Vec<EventType>,
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub bucket: TrendBucket,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analytics_event_builder() {
        let event = AnalyticsEvent::new(EventType::Push, "tenant-1", "org/repo")
            .with_actor("alice")
            .with_meta("branch", "main")
            .with_meta("commits", "3");

        assert_eq!(event.event_type, EventType::Push);
        assert_eq!(event.tenant_id, "tenant-1");
        assert_eq!(event.actor.as_deref(), Some("alice"));
        assert_eq!(event.metadata["branch"], "main");
        assert_eq!(event.metadata["commits"], "3");
    }

    #[test]
    fn test_event_type_display() {
        assert_eq!(EventType::Clone.to_string(), "clone");
        assert_eq!(EventType::Push.to_string(), "push");
        assert_eq!(EventType::LfsUpload.to_string(), "lfs_upload");
    }

    #[test]
    fn test_repo_stats_default() {
        let stats = RepoStats::default();
        assert_eq!(stats.commit_count, 0);
        assert_eq!(stats.clone_count, 0);
        assert!(stats.most_active_branch.is_none());
    }
}

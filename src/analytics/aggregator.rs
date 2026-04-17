//! Analytics aggregator — computes RepoStats, TenantStats, and trend series from raw events.

use crate::analytics::models::{
    AnalyticsEvent, EventType, RepoActivity, RepoStats, TenantStats, TrendBucket, TrendPoint,
    TrendQuery,
};
use std::collections::{HashMap, HashSet};

/// Aggregate a slice of events into per-repository statistics.
pub fn aggregate_repo_stats(events: &[AnalyticsEvent]) -> HashMap<String, RepoStats> {
    let mut stats: HashMap<String, RepoStats> = HashMap::new();

    for event in events {
        let entry = stats
            .entry(event.repository_id.clone())
            .or_insert_with(|| RepoStats {
                repository_id: event.repository_id.clone(),
                tenant_id: event.tenant_id.clone(),
                ..Default::default()
            });

        match event.event_type {
            EventType::Commit => entry.commit_count += 1,
            EventType::Clone => entry.clone_count += 1,
            EventType::Push => {
                entry.push_count += 1;
                if let Some(actor) = &event.actor {
                    entry.unique_contributors += 1;
                    let _ = actor; // contributor counted separately below
                }
            }
            EventType::Fetch => entry.fetch_count += 1,
            EventType::LfsUpload => {
                let bytes: u64 = event
                    .metadata
                    .get("bytes")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                entry.lfs_bytes_uploaded += bytes;
            }
            EventType::LfsDownload => {
                let bytes: u64 = event
                    .metadata
                    .get("bytes")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                entry.lfs_bytes_downloaded += bytes;
            }
            _ => {}
        }
    }

    // Compute accurate unique contributors per repo (push actors)
    let mut contributors: HashMap<String, HashSet<String>> = HashMap::new();
    for event in events {
        if event.event_type == EventType::Push {
            if let Some(actor) = &event.actor {
                contributors
                    .entry(event.repository_id.clone())
                    .or_default()
                    .insert(actor.clone());
            }
        }
    }
    for (repo_id, actors) in &contributors {
        if let Some(s) = stats.get_mut(repo_id) {
            s.unique_contributors = actors.len() as u64;
        }
    }

    // Compute most active branch per repo
    let mut branch_counts: HashMap<String, HashMap<String, u64>> = HashMap::new();
    for event in events {
        if event.event_type == EventType::Push {
            if let Some(branch) = event.metadata.get("branch") {
                *branch_counts
                    .entry(event.repository_id.clone())
                    .or_default()
                    .entry(branch.clone())
                    .or_insert(0) += 1;
            }
        }
    }
    for (repo_id, branches) in &branch_counts {
        if let Some(s) = stats.get_mut(repo_id) {
            s.most_active_branch = branches
                .iter()
                .max_by_key(|(_, &count)| count)
                .map(|(b, _)| b.clone());
        }
    }

    stats
}

/// Aggregate a slice of events into tenant-level statistics.
pub fn aggregate_tenant_stats(events: &[AnalyticsEvent], top_n: usize) -> TenantStats {
    if events.is_empty() {
        return TenantStats::default();
    }

    let tenant_id = events[0].tenant_id.clone();
    let mut total_commits = 0u64;
    let mut total_clones = 0u64;
    let mut total_pushes = 0u64;
    let mut repo_event_counts: HashMap<String, u64> = HashMap::new();
    let mut repo_set: HashSet<String> = HashSet::new();
    let mut contributors: HashSet<String> = HashSet::new();

    for event in events {
        repo_set.insert(event.repository_id.clone());
        *repo_event_counts
            .entry(event.repository_id.clone())
            .or_insert(0) += 1;

        match event.event_type {
            EventType::Commit => total_commits += 1,
            EventType::Clone => total_clones += 1,
            EventType::Push => {
                total_pushes += 1;
                if let Some(actor) = &event.actor {
                    contributors.insert(actor.clone());
                }
            }
            _ => {}
        }
    }

    let mut top_repos: Vec<RepoActivity> = repo_event_counts
        .into_iter()
        .map(|(id, count)| RepoActivity {
            repository_id: id,
            event_count: count,
        })
        .collect();
    top_repos.sort_by(|a, b| b.event_count.cmp(&a.event_count));
    top_repos.truncate(top_n);

    TenantStats {
        tenant_id,
        total_repositories: repo_set.len() as u64,
        total_commits,
        total_clones,
        total_pushes,
        active_contributors: contributors.len() as u64,
        top_repositories: top_repos,
        window_start: events.iter().map(|e| e.occurred_at).min(),
        window_end: events.iter().map(|e| e.occurred_at).max(),
    }
}

/// Build trend series from events matching a [`TrendQuery`].
pub fn build_trend(events: &[AnalyticsEvent], query: &TrendQuery) -> Vec<TrendPoint> {
    use chrono::DurationRound;

    // Collect matching events
    let matching: Vec<&AnalyticsEvent> = events
        .iter()
        .filter(|e| {
            e.occurred_at >= query.since
                && e.occurred_at <= query.until
                && (query.event_types.is_empty() || query.event_types.contains(&e.event_type))
                && query
                    .repository_id
                    .as_ref()
                    .map(|r| *r == e.repository_id)
                    .unwrap_or(true)
                && query
                    .tenant_id
                    .as_ref()
                    .map(|t| *t == e.tenant_id)
                    .unwrap_or(true)
        })
        .collect();

    // Bucket and count
    let duration = match query.bucket {
        TrendBucket::Hourly => chrono::Duration::hours(1),
        TrendBucket::Daily => chrono::Duration::days(1),
        TrendBucket::Weekly => chrono::Duration::weeks(1),
    };

    let mut bucket_counts: HashMap<(chrono::DateTime<chrono::Utc>, String), u64> = HashMap::new();

    for event in &matching {
        let bucket_start = event
            .occurred_at
            .duration_trunc(duration)
            .unwrap_or(event.occurred_at);
        *bucket_counts
            .entry((bucket_start, event.event_type.to_string()))
            .or_insert(0) += 1;
    }

    // Expand to TrendPoints, one per (bucket, event_type)
    let mut points: Vec<TrendPoint> = bucket_counts
        .into_iter()
        .filter_map(|((bucket_start, event_type_str), count)| {
            // Re-map string back to EventType; skip unknowns
            let event_type = matching
                .iter()
                .find(|e| e.event_type.to_string() == event_type_str)
                .map(|e| e.event_type.clone())?;
            Some(TrendPoint {
                bucket_start,
                event_type,
                count,
            })
        })
        .collect();

    points.sort_by(|a, b| a.bucket_start.cmp(&b.bucket_start));
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::models::{AnalyticsEvent, EventType, TrendBucket, TrendQuery};
    use chrono::Utc;

    fn push_event(repo: &str, tenant: &str, actor: &str, branch: &str) -> AnalyticsEvent {
        AnalyticsEvent::new(EventType::Push, tenant, repo)
            .with_actor(actor)
            .with_meta("branch", branch)
    }

    fn clone_event(repo: &str, tenant: &str) -> AnalyticsEvent {
        AnalyticsEvent::new(EventType::Clone, tenant, repo)
    }

    #[test]
    fn test_aggregate_repo_stats_basic() {
        let events = vec![
            push_event("org/repo", "t1", "alice", "main"),
            push_event("org/repo", "t1", "bob", "main"),
            push_event("org/repo", "t1", "alice", "dev"),
            clone_event("org/repo", "t1"),
        ];
        let stats = aggregate_repo_stats(&events);
        let s = &stats["org/repo"];
        assert_eq!(s.push_count, 3);
        assert_eq!(s.clone_count, 1);
        assert_eq!(s.unique_contributors, 2); // alice + bob
        assert_eq!(s.most_active_branch.as_deref(), Some("main")); // 2 pushes to main
    }

    #[test]
    fn test_aggregate_repo_stats_lfs_bytes() {
        let events = vec![
            AnalyticsEvent::new(EventType::LfsUpload, "t1", "org/repo").with_meta("bytes", "1024"),
            AnalyticsEvent::new(EventType::LfsUpload, "t1", "org/repo").with_meta("bytes", "512"),
            AnalyticsEvent::new(EventType::LfsDownload, "t1", "org/repo")
                .with_meta("bytes", "2048"),
        ];
        let stats = aggregate_repo_stats(&events);
        let s = &stats["org/repo"];
        assert_eq!(s.lfs_bytes_uploaded, 1536);
        assert_eq!(s.lfs_bytes_downloaded, 2048);
    }

    #[test]
    fn test_aggregate_tenant_stats() {
        let events = vec![
            push_event("org/repo-a", "t1", "alice", "main"),
            push_event("org/repo-b", "t1", "bob", "main"),
            clone_event("org/repo-a", "t1"),
            AnalyticsEvent::new(EventType::Commit, "t1", "org/repo-a"),
        ];
        let stats = aggregate_tenant_stats(&events, 5);
        assert_eq!(stats.tenant_id, "t1");
        assert_eq!(stats.total_repositories, 2);
        assert_eq!(stats.total_pushes, 2);
        assert_eq!(stats.total_clones, 1);
        assert_eq!(stats.total_commits, 1);
        assert_eq!(stats.active_contributors, 2);
        assert_eq!(stats.top_repositories.len(), 2);
    }

    #[test]
    fn test_build_trend_daily() {
        let now = Utc::now();
        let events = vec![
            {
                let mut e = AnalyticsEvent::new(EventType::Push, "t1", "org/repo");
                e.occurred_at = now;
                e
            },
            {
                let mut e = AnalyticsEvent::new(EventType::Push, "t1", "org/repo");
                e.occurred_at = now;
                e
            },
            {
                let mut e = AnalyticsEvent::new(EventType::Clone, "t1", "org/repo");
                e.occurred_at = now;
                e
            },
        ];
        let query = TrendQuery {
            repository_id: Some("org/repo".to_string()),
            tenant_id: None,
            event_types: vec![EventType::Push],
            since: now - chrono::Duration::days(1),
            until: now + chrono::Duration::days(1),
            bucket: TrendBucket::Daily,
        };
        let points = build_trend(&events, &query);
        assert_eq!(points.len(), 1); // one daily bucket
        assert_eq!(points[0].count, 2); // two push events
        assert_eq!(points[0].event_type, EventType::Push);
    }

    #[test]
    fn test_build_trend_empty_result_outside_window() {
        let now = Utc::now();
        let old_event = {
            let mut e = AnalyticsEvent::new(EventType::Push, "t1", "org/repo");
            e.occurred_at = now - chrono::Duration::days(10);
            e
        };
        let query = TrendQuery {
            repository_id: None,
            tenant_id: None,
            event_types: vec![],
            since: now - chrono::Duration::days(1),
            until: now,
            bucket: TrendBucket::Daily,
        };
        let points = build_trend(&[old_event], &query);
        assert!(points.is_empty());
    }
}

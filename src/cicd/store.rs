//! In-memory build report storage (production would use SQLite / PostgreSQL via sqlx).

use crate::cicd::models::BuildReport;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Build report not found: {0}")]
    NotFound(Uuid),
    #[error("Lock poisoned")]
    LockPoisoned,
}

pub trait BuildReportStore: Send + Sync {
    fn upsert(&self, report: BuildReport) -> Result<BuildReport, StoreError>;
    fn get(&self, id: &Uuid) -> Result<Option<BuildReport>, StoreError>;
    fn list_by_repo(&self, repository_id: &str) -> Result<Vec<BuildReport>, StoreError>;
    fn list_by_commit(
        &self,
        repository_id: &str,
        commit_sha: &str,
    ) -> Result<Vec<BuildReport>, StoreError>;
    fn delete(&self, id: &Uuid) -> Result<(), StoreError>;
}

/// Thread-safe in-memory build report store.
#[derive(Default)]
pub struct InMemoryBuildReportStore {
    records: RwLock<HashMap<Uuid, BuildReport>>,
}

impl InMemoryBuildReportStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl BuildReportStore for InMemoryBuildReportStore {
    fn upsert(&self, report: BuildReport) -> Result<BuildReport, StoreError> {
        let mut map = self.records.write().map_err(|_| StoreError::LockPoisoned)?;
        map.insert(report.id, report.clone());
        Ok(report)
    }

    fn get(&self, id: &Uuid) -> Result<Option<BuildReport>, StoreError> {
        let map = self.records.read().map_err(|_| StoreError::LockPoisoned)?;
        Ok(map.get(id).cloned())
    }

    fn list_by_repo(&self, repository_id: &str) -> Result<Vec<BuildReport>, StoreError> {
        let map = self.records.read().map_err(|_| StoreError::LockPoisoned)?;
        let mut results: Vec<_> = map
            .values()
            .filter(|r| r.repository_id == repository_id)
            .cloned()
            .collect();
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(results)
    }

    fn list_by_commit(
        &self,
        repository_id: &str,
        commit_sha: &str,
    ) -> Result<Vec<BuildReport>, StoreError> {
        let map = self.records.read().map_err(|_| StoreError::LockPoisoned)?;
        let mut results: Vec<_> = map
            .values()
            .filter(|r| r.repository_id == repository_id && r.commit_sha == commit_sha)
            .cloned()
            .collect();
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(results)
    }

    fn delete(&self, id: &Uuid) -> Result<(), StoreError> {
        let mut map = self.records.write().map_err(|_| StoreError::LockPoisoned)?;
        map.remove(id).ok_or(StoreError::NotFound(*id))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cicd::models::{BuildStatus, CiProvider};

    fn make_report(repo: &str, sha: &str) -> BuildReport {
        BuildReport::new(
            sha.to_string(),
            "main".to_string(),
            repo.to_string(),
            CiProvider::GithubActions,
            "run-1".to_string(),
        )
    }

    #[test]
    fn test_upsert_and_get() {
        let store = InMemoryBuildReportStore::new();
        let r = make_report("org/repo", "abc123");
        let id = r.id;
        store.upsert(r).unwrap();
        let got = store.get(&id).unwrap().unwrap();
        assert_eq!(got.id, id);
    }

    #[test]
    fn test_get_missing_returns_none() {
        let store = InMemoryBuildReportStore::new();
        assert!(store.get(&Uuid::new_v4()).unwrap().is_none());
    }

    #[test]
    fn test_upsert_updates_existing() {
        let store = InMemoryBuildReportStore::new();
        let r = make_report("org/repo", "sha1");
        let id = r.id;
        store.upsert(r).unwrap();
        let updated = make_report("org/repo", "sha1");
        let updated = BuildReport { id, ..updated }.with_status(BuildStatus::Success);
        store.upsert(updated).unwrap();
        let got = store.get(&id).unwrap().unwrap();
        assert_eq!(got.status, BuildStatus::Success);
    }

    #[test]
    fn test_list_by_repo() {
        let store = InMemoryBuildReportStore::new();
        store.upsert(make_report("org/repo-a", "s1")).unwrap();
        store.upsert(make_report("org/repo-a", "s2")).unwrap();
        store.upsert(make_report("org/repo-b", "s3")).unwrap();
        assert_eq!(store.list_by_repo("org/repo-a").unwrap().len(), 2);
        assert_eq!(store.list_by_repo("org/repo-b").unwrap().len(), 1);
        assert_eq!(store.list_by_repo("org/repo-c").unwrap().len(), 0);
    }

    #[test]
    fn test_list_by_commit() {
        let store = InMemoryBuildReportStore::new();
        store.upsert(make_report("org/repo", "commit-a")).unwrap();
        store.upsert(make_report("org/repo", "commit-a")).unwrap();
        store.upsert(make_report("org/repo", "commit-b")).unwrap();
        let by_a = store.list_by_commit("org/repo", "commit-a").unwrap();
        assert_eq!(by_a.len(), 2);
    }

    #[test]
    fn test_delete() {
        let store = InMemoryBuildReportStore::new();
        let r = make_report("org/repo", "sha");
        let id = r.id;
        store.upsert(r).unwrap();
        store.delete(&id).unwrap();
        assert!(store.get(&id).unwrap().is_none());
    }
}

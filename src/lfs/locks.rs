use crate::lfs::{
    CreateLockRequest, CreateLockResponse, DeleteLockResponse, LfsLock, LfsLockOwner,
    ListLocksResponse, LockError, VerifyLocksResponse,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum LockManagerError {
    #[error("Lock already exists: {0}")]
    AlreadyLocked(String),
    #[error("Lock not found: {0}")]
    NotFound(Uuid),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Storage error: {0}")]
    Storage(String),
}

impl From<LockManagerError> for LockError {
    fn from(err: LockManagerError) -> Self {
        match err {
            LockManagerError::AlreadyLocked(p) => LockError::already_locked(&p),
            LockManagerError::NotFound(id) => LockError::not_found(&id),
            LockManagerError::Forbidden(r) => LockError::forbidden(&r),
            LockManagerError::Storage(e) => LockError::forbidden(&e),
        }
    }
}

/// Lock storage backend. InMemoryLockStorage is the default.
/// Future backends: git-refs, Redis for clustering.
pub trait LockStorage: Send + Sync {
    fn create_lock(
        &self,
        tenant: &str,
        repo: &str,
        request: CreateLockRequest,
        owner_name: &str,
        owner_id: Option<Uuid>,
        expiration_secs: Option<u64>,
    ) -> Result<CreateLockResponse, LockManagerError>;
    fn list_locks(
        &self,
        tenant: &str,
        repo: &str,
        path_filter: Option<&str>,
        id_filter: Option<Uuid>,
        refspec: Option<&str>,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<ListLocksResponse, LockManagerError>;
    fn delete_lock(
        &self,
        tenant: &str,
        repo: &str,
        lock_id: Uuid,
        requesting_user: &str,
        force: bool,
    ) -> Result<DeleteLockResponse, LockManagerError>;
    fn verify_locks(
        &self,
        tenant: &str,
        repo: &str,
        requesting_user: &str,
        refspec: Option<&str>,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<VerifyLocksResponse, LockManagerError>;
    fn cleanup_expired_locks(&self) -> Result<u64, LockManagerError>;
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct StoredLock {
    id: Uuid,
    tenant: String,
    repo: String,
    path: String,
    ref_name: Option<String>,
    owner_name: String,
    owner_id: Option<Uuid>,
    locked_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}

pub struct InMemoryLockStorage {
    locks: RwLock<HashMap<Uuid, StoredLock>>,
}

impl InMemoryLockStorage {
    pub fn new() -> Self {
        Self {
            locks: RwLock::new(HashMap::new()),
        }
    }
}
impl Default for InMemoryLockStorage {
    fn default() -> Self {
        Self::new()
    }
}

fn to_lock(l: &StoredLock) -> LfsLock {
    LfsLock {
        id: l.id,
        path: l.path.clone(),
        locked_at: l.locked_at,
        owner: LfsLockOwner {
            name: l.owner_name.clone(),
        },
    }
}

fn wlock(
    locks: &RwLock<HashMap<Uuid, StoredLock>>,
) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<Uuid, StoredLock>>, LockManagerError> {
    locks
        .write()
        .map_err(|e| LockManagerError::Storage(e.to_string()))
}
fn rlock(
    locks: &RwLock<HashMap<Uuid, StoredLock>>,
) -> Result<std::sync::RwLockReadGuard<'_, HashMap<Uuid, StoredLock>>, LockManagerError> {
    locks
        .read()
        .map_err(|e| LockManagerError::Storage(e.to_string()))
}

impl LockStorage for InMemoryLockStorage {
    fn create_lock(
        &self,
        tenant: &str,
        repo: &str,
        request: CreateLockRequest,
        owner_name: &str,
        owner_id: Option<Uuid>,
        expiration_secs: Option<u64>,
    ) -> Result<CreateLockResponse, LockManagerError> {
        let mut locks = wlock(&self.locks)?;
        let now = Utc::now();
        for l in locks.values() {
            if l.tenant == tenant
                && l.repo == repo
                && l.path == request.path
                && (l.expires_at.is_none() || l.expires_at.as_ref() > Some(&now))
            {
                return Err(LockManagerError::AlreadyLocked(request.path.clone()));
            }
        }
        let id = Uuid::new_v4();
        let stored = StoredLock {
            id,
            tenant: tenant.into(),
            repo: repo.into(),
            path: request.path.clone(),
            ref_name: request.git_ref.as_ref().map(|r| r.name.clone()),
            owner_name: owner_name.into(),
            owner_id,
            locked_at: now,
            expires_at: expiration_secs.map(|s| now + chrono::Duration::seconds(s as i64)),
        };
        let lock = to_lock(&stored);
        locks.insert(id, stored);
        Ok(CreateLockResponse { lock })
    }

    fn list_locks(
        &self,
        tenant: &str,
        repo: &str,
        path_filter: Option<&str>,
        id_filter: Option<Uuid>,
        refspec: Option<&str>,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<ListLocksResponse, LockManagerError> {
        let locks = rlock(&self.locks)?;
        let limit = limit.unwrap_or(100).min(1000) as usize;
        let now = Utc::now();
        let cursor_id = cursor
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| LockManagerError::Forbidden("Invalid cursor".into()))?;
        let mut matching: Vec<&StoredLock> = locks
            .values()
            .filter(|l| {
                l.tenant == tenant
                    && l.repo == repo
                    && (l.expires_at.is_none() || l.expires_at.as_ref() > Some(&now))
                    && path_filter.map_or(true, |p| l.path == p)
                    && id_filter.map_or(true, |id| l.id == id)
                    && refspec.map_or(true, |r| l.ref_name.as_deref() == Some(r))
                    && cursor_id.map_or(true, |cid| l.id > cid)
            })
            .collect();
        matching.sort_by_key(|l| l.id);
        let has_more = matching.len() > limit;
        matching.truncate(limit);
        let lock_list: Vec<LfsLock> = matching.iter().map(|l| to_lock(*l)).collect();
        let next_cursor = if has_more {
            lock_list.last().map(|l| l.id.to_string())
        } else {
            None
        };
        Ok(ListLocksResponse {
            locks: lock_list,
            next_cursor,
        })
    }

    fn delete_lock(
        &self,
        tenant: &str,
        repo: &str,
        lock_id: Uuid,
        requesting_user: &str,
        force: bool,
    ) -> Result<DeleteLockResponse, LockManagerError> {
        let mut locks = wlock(&self.locks)?;
        let stored = locks
            .get(&lock_id)
            .ok_or(LockManagerError::NotFound(lock_id))?;
        if stored.tenant != tenant || stored.repo != repo {
            return Err(LockManagerError::NotFound(lock_id));
        }
        if !force && stored.owner_name != requesting_user {
            return Err(LockManagerError::Forbidden(
                "Cannot unlock file locked by another user".into(),
            ));
        }
        let lock = to_lock(stored);
        locks.remove(&lock_id);
        Ok(DeleteLockResponse { lock })
    }

    fn verify_locks(
        &self,
        tenant: &str,
        repo: &str,
        requesting_user: &str,
        refspec: Option<&str>,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<VerifyLocksResponse, LockManagerError> {
        let result = self.list_locks(tenant, repo, None, None, refspec, limit, cursor)?;
        let (ours, theirs): (Vec<LfsLock>, Vec<LfsLock>) = result
            .locks
            .into_iter()
            .partition(|l| l.owner.name == requesting_user);
        Ok(VerifyLocksResponse {
            ours,
            theirs,
            next_cursor: result.next_cursor,
        })
    }

    fn cleanup_expired_locks(&self) -> Result<u64, LockManagerError> {
        let mut locks = wlock(&self.locks)?;
        let now = Utc::now();
        let before = locks.len();
        locks.retain(|_, l| l.expires_at.is_none() || l.expires_at.as_ref() > Some(&now));
        Ok((before - locks.len()) as u64)
    }
}

pub type LfsLockManager = InMemoryLockStorage;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_create_and_list() {
        let m = InMemoryLockStorage::new();
        let r = m
            .create_lock(
                "t",
                "r",
                CreateLockRequest {
                    path: "a.txt".into(),
                    git_ref: None,
                },
                "alice",
                None,
                None,
            )
            .unwrap();
        assert_eq!(r.lock.path, "a.txt");
        let list = m
            .list_locks("t", "r", None, None, None, None, None)
            .unwrap();
        assert_eq!(list.locks.len(), 1);
    }
    #[test]
    fn test_duplicate_fails() {
        let m = InMemoryLockStorage::new();
        m.create_lock(
            "t",
            "r",
            CreateLockRequest {
                path: "x".into(),
                git_ref: None,
            },
            "a",
            None,
            None,
        )
        .unwrap();
        let err = m.create_lock(
            "t",
            "r",
            CreateLockRequest {
                path: "x".into(),
                git_ref: None,
            },
            "b",
            None,
            None,
        );
        assert!(matches!(err, Err(LockManagerError::AlreadyLocked(_))));
    }
    #[test]
    fn test_delete_and_verify() {
        let m = InMemoryLockStorage::new();
        let r1 = m
            .create_lock(
                "t",
                "r",
                CreateLockRequest {
                    path: "m".into(),
                    git_ref: None,
                },
                "alice",
                None,
                None,
            )
            .unwrap();
        m.create_lock(
            "t",
            "r",
            CreateLockRequest {
                path: "t".into(),
                git_ref: None,
            },
            "bob",
            None,
            None,
        )
        .unwrap();
        let v = m.verify_locks("t", "r", "alice", None, None, None).unwrap();
        assert_eq!(v.ours.len(), 1);
        assert_eq!(v.theirs.len(), 1);
        m.delete_lock("t", "r", r1.lock.id, "alice", false).unwrap();
        let list = m
            .list_locks("t", "r", None, None, None, None, None)
            .unwrap();
        assert_eq!(list.locks.len(), 1);
    }
    #[test]
    fn test_non_owner_delete_fails_without_force() {
        let m = InMemoryLockStorage::new();
        let r = m
            .create_lock(
                "t",
                "r",
                CreateLockRequest {
                    path: "p".into(),
                    git_ref: None,
                },
                "alice",
                None,
                None,
            )
            .unwrap();
        let err = m.delete_lock("t", "r", r.lock.id, "bob", false);
        assert!(matches!(err, Err(LockManagerError::Forbidden(_))));
        let ok = m.delete_lock("t", "r", r.lock.id, "bob", true);
        assert!(ok.is_ok());
    }
}

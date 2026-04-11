use crate::lfs::{
    CreateLockRequest, CreateLockResponse, DeleteLockResponse, LfsLock, LfsLockOwner,
    ListLocksResponse, LockError, VerifyLocksResponse,
};
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum LockManagerError {
    #[error("Lock already exists: {0}")]
    AlreadyLocked(String),

    #[error("Lock not found: {0}")]
    NotFound(Uuid),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl From<LockManagerError> for LockError {
    fn from(err: LockManagerError) -> Self {
        match err {
            LockManagerError::AlreadyLocked(path) => LockError::already_locked(&path),
            LockManagerError::NotFound(id) => LockError::not_found(&id),
            LockManagerError::Forbidden(reason) => LockError::forbidden(&reason),
            LockManagerError::Database(e) => LockError::forbidden(&e.to_string()),
        }
    }
}

pub struct LfsLockManager {
    pool: Arc<PgPool>,
}

impl LfsLockManager {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn create_lock(
        &self,
        tenant: &str,
        repo: &str,
        request: CreateLockRequest,
        owner_name: &str,
        owner_id: Option<Uuid>,
        expiration_secs: Option<u64>,
    ) -> Result<CreateLockResponse, LockManagerError> {
        let ref_name = request.git_ref.as_ref().map(|r| r.name.as_str());
        let expires_at =
            expiration_secs.map(|secs| Utc::now() + chrono::Duration::seconds(secs as i64));

        let row = sqlx::query(
            r#"
            INSERT INTO lfs_locks (tenant, repo, path, ref_name, owner_name, owner_id, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, path, locked_at, owner_name
            "#,
        )
        .bind(tenant)
        .bind(repo)
        .bind(&request.path)
        .bind(ref_name)
        .bind(owner_name)
        .bind(owner_id)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.is_unique_violation() {
                    return LockManagerError::AlreadyLocked(request.path.clone());
                }
            }
            LockManagerError::Database(e)
        })?;

        let lock = LfsLock {
            id: row.get("id"),
            path: row.get("path"),
            locked_at: row.get("locked_at"),
            owner: LfsLockOwner {
                name: row.get("owner_name"),
            },
        };

        Ok(CreateLockResponse { lock })
    }

    pub async fn list_locks(
        &self,
        tenant: &str,
        repo: &str,
        path_filter: Option<&str>,
        id_filter: Option<Uuid>,
        refspec: Option<&str>,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<ListLocksResponse, LockManagerError> {
        let limit = limit.unwrap_or(100).min(1000);

        let mut query = String::from(
            r#"
            SELECT id, path, locked_at, owner_name
            FROM lfs_locks
            WHERE tenant = $1 AND repo = $2
            AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        );

        let mut bind_idx = 3;
        if path_filter.is_some() {
            query.push_str(&format!(" AND path = ${}", bind_idx));
            bind_idx += 1;
        }
        if id_filter.is_some() {
            query.push_str(&format!(" AND id = ${}", bind_idx));
            bind_idx += 1;
        }
        if refspec.is_some() {
            query.push_str(&format!(" AND ref_name = ${}", bind_idx));
            bind_idx += 1;
        }
        if cursor.is_some() {
            query.push_str(&format!(" AND id > ${}", bind_idx));
            bind_idx += 1;
        }

        query.push_str(&format!(" ORDER BY id LIMIT ${}", bind_idx));

        let mut sql_query = sqlx::query(&query).bind(tenant).bind(repo);

        if let Some(path) = path_filter {
            sql_query = sql_query.bind(path);
        }
        if let Some(id) = id_filter {
            sql_query = sql_query.bind(id);
        }
        if let Some(refspec) = refspec {
            sql_query = sql_query.bind(refspec);
        }
        if let Some(cursor) = cursor {
            let cursor_uuid = Uuid::parse_str(cursor)
                .map_err(|_| LockManagerError::Forbidden("Invalid cursor".to_string()))?;
            sql_query = sql_query.bind(cursor_uuid);
        }

        sql_query = sql_query.bind(limit as i64);

        let rows = sql_query.fetch_all(&*self.pool).await?;

        let locks: Vec<LfsLock> = rows
            .iter()
            .map(|row| LfsLock {
                id: row.get("id"),
                path: row.get("path"),
                locked_at: row.get("locked_at"),
                owner: LfsLockOwner {
                    name: row.get("owner_name"),
                },
            })
            .collect();

        let next_cursor = if locks.len() == limit as usize {
            locks.last().map(|l| l.id.to_string())
        } else {
            None
        };

        Ok(ListLocksResponse { locks, next_cursor })
    }

    pub async fn delete_lock(
        &self,
        tenant: &str,
        repo: &str,
        lock_id: Uuid,
        requesting_user: &str,
        force: bool,
    ) -> Result<DeleteLockResponse, LockManagerError> {
        let row = sqlx::query(
            r#"
            SELECT id, path, locked_at, owner_name
            FROM lfs_locks
            WHERE tenant = $1 AND repo = $2 AND id = $3
            "#,
        )
        .bind(tenant)
        .bind(repo)
        .bind(lock_id)
        .fetch_optional(&*self.pool)
        .await?;

        let row = row.ok_or(LockManagerError::NotFound(lock_id))?;

        let owner_name: String = row.get("owner_name");

        if !force && owner_name != requesting_user {
            return Err(LockManagerError::Forbidden(
                "Cannot unlock file locked by another user".to_string(),
            ));
        }

        let lock = LfsLock {
            id: row.get("id"),
            path: row.get("path"),
            locked_at: row.get("locked_at"),
            owner: LfsLockOwner { name: owner_name },
        };

        sqlx::query(
            r#"
            DELETE FROM lfs_locks
            WHERE id = $1
            "#,
        )
        .bind(lock_id)
        .execute(&*self.pool)
        .await?;

        Ok(DeleteLockResponse { lock })
    }

    pub async fn verify_locks(
        &self,
        tenant: &str,
        repo: &str,
        requesting_user: &str,
        refspec: Option<&str>,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<VerifyLocksResponse, LockManagerError> {
        let result = self
            .list_locks(tenant, repo, None, None, refspec, limit, cursor)
            .await?;

        let (ours, theirs): (Vec<LfsLock>, Vec<LfsLock>) = result
            .locks
            .into_iter()
            .partition(|lock| lock.owner.name == requesting_user);

        Ok(VerifyLocksResponse {
            ours,
            theirs,
            next_cursor: result.next_cursor,
        })
    }

    pub async fn cleanup_expired_locks(&self) -> Result<u64, LockManagerError> {
        let result = sqlx::query(
            r#"
            DELETE FROM lfs_locks
            WHERE expires_at IS NOT NULL AND expires_at < $1
            "#,
        )
        .bind(Utc::now())
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reuse shared test DB setup helpers to avoid duplication across tests.
    use crate::lfs::test_utils::{cleanup_test_db, setup_test_db};

    #[tokio::test]
    async fn test_create_lock_success() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());
        let request = CreateLockRequest {
            path: "designs/logo.psd".to_string(),
            git_ref: None,
        };

        let response = manager
            .create_lock("tenant1", "repo1", request, "alice", None, None)
            .await
            .expect("Failed to create lock");

        assert_eq!(response.lock.path, "designs/logo.psd");
        assert_eq!(response.lock.owner.name, "alice");
        assert!(response.lock.id != Uuid::nil());

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_create_lock_with_expiration() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());
        let request = CreateLockRequest {
            path: "video.mp4".to_string(),
            git_ref: None,
        };

        let response = manager
            .create_lock("tenant1", "repo1", request, "bob", None, Some(3600))
            .await
            .expect("Failed to create lock with expiration");

        assert_eq!(response.lock.path, "video.mp4");
        assert_eq!(response.lock.owner.name, "bob");

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_create_lock_duplicate_fails() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());
        let request1 = CreateLockRequest {
            path: "document.pdf".to_string(),
            git_ref: None,
        };

        manager
            .create_lock("tenant1", "repo1", request1, "alice", None, None)
            .await
            .expect("Failed to create first lock");

        let request2 = CreateLockRequest {
            path: "document.pdf".to_string(),
            git_ref: None,
        };

        let result = manager
            .create_lock("tenant1", "repo1", request2, "bob", None, None)
            .await;

        assert!(matches!(result, Err(LockManagerError::AlreadyLocked(_))));

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_list_locks_all() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());

        for i in 1..=3 {
            let request = CreateLockRequest {
                path: format!("file{}.txt", i),
                git_ref: None,
            };
            manager
                .create_lock("tenant1", "repo1", request, "alice", None, None)
                .await
                .expect("Failed to create lock");
        }

        let response = manager
            .list_locks("tenant1", "repo1", None, None, None, None, None)
            .await
            .expect("Failed to list locks");

        assert_eq!(response.locks.len(), 3);

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_list_locks_by_path() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());

        let request = CreateLockRequest {
            path: "specific.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request, "alice", None, None)
            .await
            .expect("Failed to create lock");

        let request = CreateLockRequest {
            path: "other.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request, "bob", None, None)
            .await
            .expect("Failed to create lock");

        let response = manager
            .list_locks(
                "tenant1",
                "repo1",
                Some("specific.txt"),
                None,
                None,
                None,
                None,
            )
            .await
            .expect("Failed to list locks by path");

        assert_eq!(response.locks.len(), 1);
        assert_eq!(response.locks[0].path, "specific.txt");

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_list_locks_with_limit() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());

        for i in 1..=5 {
            let request = CreateLockRequest {
                path: format!("file{}.txt", i),
                git_ref: None,
            };
            manager
                .create_lock("tenant1", "repo1", request, "alice", None, None)
                .await
                .expect("Failed to create lock");
        }

        let response = manager
            .list_locks("tenant1", "repo1", None, None, None, Some(3), None)
            .await
            .expect("Failed to list locks with limit");

        assert_eq!(response.locks.len(), 3);
        assert!(response.next_cursor.is_some());

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_delete_lock_as_owner() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());
        let request = CreateLockRequest {
            path: "to_delete.txt".to_string(),
            git_ref: None,
        };

        let create_response = manager
            .create_lock("tenant1", "repo1", request, "alice", None, None)
            .await
            .expect("Failed to create lock");

        let lock_id = create_response.lock.id;

        let delete_response = manager
            .delete_lock("tenant1", "repo1", lock_id, "alice", false)
            .await
            .expect("Failed to delete lock as owner");

        assert_eq!(delete_response.lock.id, lock_id);

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_delete_lock_as_non_owner_without_force_fails() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());
        let request = CreateLockRequest {
            path: "protected.txt".to_string(),
            git_ref: None,
        };

        let create_response = manager
            .create_lock("tenant1", "repo1", request, "alice", None, None)
            .await
            .expect("Failed to create lock");

        let lock_id = create_response.lock.id;

        let result = manager
            .delete_lock("tenant1", "repo1", lock_id, "bob", false)
            .await;

        assert!(matches!(result, Err(LockManagerError::Forbidden(_))));

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_delete_lock_with_force_succeeds() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());
        let request = CreateLockRequest {
            path: "forced.txt".to_string(),
            git_ref: None,
        };

        let create_response = manager
            .create_lock("tenant1", "repo1", request, "alice", None, None)
            .await
            .expect("Failed to create lock");

        let lock_id = create_response.lock.id;

        let delete_response = manager
            .delete_lock("tenant1", "repo1", lock_id, "admin", true)
            .await
            .expect("Failed to force delete lock");

        assert_eq!(delete_response.lock.id, lock_id);

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_delete_lock_not_found() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());
        let fake_id = Uuid::new_v4();

        let result = manager
            .delete_lock("tenant1", "repo1", fake_id, "alice", false)
            .await;

        assert!(matches!(result, Err(LockManagerError::NotFound(_))));

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_verify_locks_partitions_correctly() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());

        let request1 = CreateLockRequest {
            path: "mine1.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request1, "alice", None, None)
            .await
            .expect("Failed to create lock");

        let request2 = CreateLockRequest {
            path: "mine2.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request2, "alice", None, None)
            .await
            .expect("Failed to create lock");

        let request3 = CreateLockRequest {
            path: "theirs.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request3, "bob", None, None)
            .await
            .expect("Failed to create lock");

        let response = manager
            .verify_locks("tenant1", "repo1", "alice", None, None, None)
            .await
            .expect("Failed to verify locks");

        assert_eq!(response.ours.len(), 2);
        assert_eq!(response.theirs.len(), 1);
        assert_eq!(response.theirs[0].owner.name, "bob");

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired_locks() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());

        let request1 = CreateLockRequest {
            path: "expired.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request1, "alice", None, Some(1))
            .await
            .expect("Failed to create expiring lock");

        let request2 = CreateLockRequest {
            path: "permanent.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request2, "bob", None, None)
            .await
            .expect("Failed to create permanent lock");

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let count = manager
            .cleanup_expired_locks()
            .await
            .expect("Failed to cleanup expired locks");

        assert_eq!(count, 1);

        let remaining = manager
            .list_locks("tenant1", "repo1", None, None, None, None, None)
            .await
            .expect("Failed to list remaining locks");

        assert_eq!(remaining.locks.len(), 1);
        assert_eq!(remaining.locks[0].path, "permanent.txt");

        cleanup_test_db(&pool).await;
    }

    #[tokio::test]
    async fn test_list_locks_excludes_expired() {
        let Some(pool) = setup_test_db().await else {
            return;
        };
        cleanup_test_db(&pool).await;

        let manager = LfsLockManager::new(pool.clone());

        let request1 = CreateLockRequest {
            path: "expired.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request1, "alice", None, Some(1))
            .await
            .expect("Failed to create expiring lock");

        let request2 = CreateLockRequest {
            path: "active.txt".to_string(),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request2, "bob", None, None)
            .await
            .expect("Failed to create active lock");

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let response = manager
            .list_locks("tenant1", "repo1", None, None, None, None, None)
            .await
            .expect("Failed to list locks");

        assert_eq!(response.locks.len(), 1);
        assert_eq!(response.locks[0].path, "active.txt");

        cleanup_test_db(&pool).await;
    }
}

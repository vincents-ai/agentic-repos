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
    ) -> Result<CreateLockResponse, LockManagerError> {
        let ref_name = request.git_ref.as_ref().map(|r| r.name.as_str());

        let row = sqlx::query(
            r#"
            INSERT INTO lfs_locks (tenant, repo, path, ref_name, owner_name, owner_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, path, locked_at, owner_name
            "#,
        )
        .bind(tenant)
        .bind(repo)
        .bind(&request.path)
        .bind(ref_name)
        .bind(owner_name)
        .bind(owner_id)
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

    #[tokio::test]
    async fn test_lock_manager_requires_pool() {
        let pool = Arc::new(
            PgPool::connect("postgresql://localhost/test")
                .await
                .unwrap_or_else(|_| panic!("Test requires PostgreSQL")),
        );
        let _manager = LfsLockManager::new(pool);
    }
}

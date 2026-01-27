use crate::lfs::audit::{AuditEvent, AuditEvent as LfsAuditEvent, AuditEventType as LfsAuditEventType, AuditError, AuditStorage};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostgresAuditEvent {
    pub id: Uuid,
    pub event_type: String,
    pub tenant: Option<String>,
    pub user_id: Option<Uuid>,
    pub repository_id: Option<String>,
    pub operation: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
}

pub struct PostgresAuditStorage {
    pool: PgPool,
}

impl PostgresAuditStorage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditStorage for PostgresAuditStorage {
    async fn store(&self, event: LfsAuditEvent) -> Result<(), AuditError> {
        let id = Uuid::new_v4();
        
        let details_json = if event.metadata.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&event.metadata).map_err(|e| {
                AuditError::StorageError(format!("Failed to serialize metadata: {}", e))
            })?)
        };

        sqlx::query(
            r#"
            INSERT INTO audit_logs (
                id, event_type, tenant, user_id, repository_id, operation, 
                resource_type, resource_id, details, ip_address, user_agent, status, error_message
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(id)
        .bind(event.event_type.to_string())
        .bind(if event.tenant.is_empty() { None } else { Some(event.tenant.clone()) })
        .bind(event.user.and_then(|u| Uuid::parse_str(u).ok()))
        .bind(if event.repo.is_empty() { None } else { Some(event.repo.clone()) })
        .bind(format!("{:?}", event.id))
        .bind(event.size.map(|_| "object".to_string()))
        .bind(event.oid)
        .bind(event.ip_address)
        .bind(None)
        .bind(if event.success { "success".to_string() } else { "failed".to_string() })
        .bind(event.error_message)
        .bind(details_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AuditError::StorageError(format!("Database error: {}", e)))?;

        Ok(())
    }

    async fn query(
        &self,
        tenant: Option<&str>,
        repo: Option<&str>,
        start_time: Option<chrono::DateTime<Utc>>,
        end_time: Option<chrono::DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<LfsAuditEvent>, AuditError> {
        let mut query = String::from(
            r#"
            SELECT id, event_type, tenant, user_id::text, repository_id, operation, 
                   resource_type, resource_id, details, ip_address, user_agent, status, error_message, created_at
            FROM audit_logs
            WHERE 1=1
            "#,
        );
        
        let mut param_idx = 1i32;

        if let Some(t) = tenant {
            if !t.is_empty() {
                query.push_str(&format!(" AND tenant = ${}", param_idx));
                param_idx += 1;
            }
        }

        if let Some(r) = repo {
            if !r.is_empty() {
                query.push_str(&format!(" AND repository_id = ${}", param_idx));
                param_idx += 1;
            }
        }

        if let Some(start) = start_time {
            query.push_str(&format!(" AND created_at >= ${}", param_idx));
            param_idx += 1;
        }

        if let Some(end) = end_time {
            query.push_str(&format!(" AND created_at <= ${}", param_idx));
            param_idx += 1;
        }

        query.push_str(" ORDER BY created_at DESC");
        
        if limit > 0 {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        let rows = sqlx::query_as::<PostgresAuditEvent, _>(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AuditError::StorageError(format!("Database error: {}", e)))?;

        let events: Vec<LfsAuditEvent> = rows
            .into_iter()
            .map(|row| {
                let details = row.details
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();

                LfsAuditEvent {
                    id: row.id.to_string(),
                    timestamp: row.created_at,
                    event_type: parse_event_type(&row.event_type),
                    tenant: row.tenant.unwrap_or_default(),
                    repo: row.repository_id.unwrap_or_default(),
                    user: row.user_id.map(|id| id.to_string()),
                    ip_address: row.ip_address,
                    oid: row.resource_id,
                    size: None,
                    success: row.status == "success",
                    error_message: row.error_message,
                    metadata: details,
                }
            })
            .collect();

        Ok(events)
    }
}

fn parse_event_type(s: &str) -> LfsAuditEventType {
    match s {
        "lfs_download" => LfsAuditEventType::LfsDownload,
        "lfs_upload" => LfsAuditEventType::LfsUpload,
        "lfs_verify" => LfsAuditEventType::LfsVerify,
        "lock_create" => LfsAuditEventType::LockCreate,
        "lock_delete" => LfsAuditEventType::LockDelete,
        "lock_verify" => LfsAuditEventType::LockVerify,
        _ => LfsAuditEventType::LfsDownload, // Default fallback
    }
}

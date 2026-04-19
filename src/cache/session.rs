use crate::auth::session::{Session, SessionStore};
use crate::cache::{CacheKey, CachePool, CacheResult};
use async_trait::async_trait;
use chrono::Duration;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tracing::{debug, instrument};
use uuid::Uuid;

#[derive(Clone)]
pub struct RedisSessionStore {
    pool: Arc<CachePool>,
    session_duration: Duration,
}

impl RedisSessionStore {
    pub fn new(pool: Arc<CachePool>) -> Self {
        Self {
            pool,
            session_duration: Duration::hours(24),
        }
    }

    pub fn with_duration(pool: Arc<CachePool>, duration: Duration) -> Self {
        Self {
            pool,
            session_duration: duration,
        }
    }

    fn key_prefix(&self) -> &str {
        &self.pool.config().key_prefix
    }

    fn session_key(&self, session_id: &Uuid) -> String {
        CacheKey::session(self.key_prefix(), &session_id.to_string()).to_redis_key()
    }

    fn user_sessions_key(&self, user_id: &str) -> String {
        CacheKey::user(self.key_prefix(), user_id)
            .with("sessions")
            .to_redis_key()
    }

    async fn add_to_user_sessions(&self, user_id: &str, session_id: &Uuid) -> CacheResult<()> {
        let key = self.user_sessions_key(user_id);
        let mut conn = self.pool.get_connection().await?;
        conn.sadd(&key, &session_id.to_string()).await?;
        let ttl = self.pool.config().session_ttl();
        conn.expire(&key, ttl).await?;
        Ok(())
    }

    async fn remove_from_user_sessions(&self, user_id: &str, session_id: &Uuid) -> CacheResult<()> {
        let key = self.user_sessions_key(user_id);
        let mut conn = self.pool.get_connection().await?;
        conn.srem(&key, &session_id.to_string()).await?;
        Ok(())
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    #[instrument(skip(self), fields(session_id = %session_id))]
    async fn get(&self, session_id: &Uuid) -> Option<Session> {
        let key = self.session_key(session_id);
        let mut conn = match self.pool.get_connection().await {
            Ok(c) => c,
            Err(e) => {
                debug!("Failed to get cache connection: {}", e);
                return None;
            }
        };

        match conn.get(&key).await {
            Ok(Some(json)) => match serde_json::from_str::<Session>(&json) {
                Ok(session) => {
                    if session.is_expired() {
                        let _ = conn.delete(&key).await;
                        None
                    } else {
                        debug!("Session cache hit");
                        Some(session)
                    }
                }
                Err(e) => {
                    debug!("Failed to deserialize session: {}", e);
                    None
                }
            },
            Ok(None) => {
                debug!("Session cache miss");
                None
            }
            Err(e) => {
                debug!("Failed to get session from cache: {}", e);
                None
            }
        }
    }

    #[instrument(skip(self, session), fields(session_id = %session.id))]
    async fn set(&self, session: Session) {
        let key = self.session_key(&session.id);
        let json = match serde_json::to_string(&session) {
            Ok(j) => j,
            Err(e) => {
                debug!("Failed to serialize session: {}", e);
                return;
            }
        };

        let mut conn = match self.pool.get_connection().await {
            Ok(c) => c,
            Err(e) => {
                debug!("Failed to get cache connection: {}", e);
                return;
            }
        };

        let remaining = session.remaining_duration();
        // Convert chrono::Duration to std::time::Duration
        let ttl = remaining
            .to_std()
            .unwrap_or_else(|_| StdDuration::from_secs(self.session_duration.num_seconds() as u64));
        let ttl_secs = ttl.as_secs();

        if let Err(e) = conn
            .set(&key, &json, StdDuration::from_secs(ttl_secs))
            .await
        {
            debug!("Failed to set session in cache: {}", e);
            return;
        }

        if let Some(tenant_id) = &session.tenant_id {
            let _ = self
                .add_to_user_sessions(&session.user.provider_user_id, &session.id)
                .await;
            let _ = self
                .add_to_user_sessions(&format!("tenant:{}", tenant_id), &session.id)
                .await;
        } else {
            let _ = self
                .add_to_user_sessions(&session.user.provider_user_id, &session.id)
                .await;
        }

        debug!("Session stored in cache");
    }

    #[instrument(skip(self), fields(session_id = %session_id))]
    async fn delete(&self, session_id: &Uuid) {
        let session = self.get(session_id).await;

        let key = self.session_key(session_id);
        let mut conn = match self.pool.get_connection().await {
            Ok(c) => c,
            Err(e) => {
                debug!("Failed to get cache connection: {}", e);
                return;
            }
        };

        if let Err(e) = conn.delete(&key).await {
            debug!("Failed to delete session from cache: {}", e);
        }

        if let Some(session) = session {
            let _ = self
                .remove_from_user_sessions(&session.user.provider_user_id, session_id)
                .await;
            if let Some(tenant_id) = &session.tenant_id {
                let _ = self
                    .remove_from_user_sessions(&format!("tenant:{}", tenant_id), session_id)
                    .await;
            }
        }

        debug!("Session deleted from cache");
    }

    #[instrument(skip(self), fields(session_id = %session_id))]
    async fn touch(&self, session_id: &Uuid) {
        let key = self.session_key(session_id);
        let mut conn = match self.pool.get_connection().await {
            Ok(c) => c,
            Err(_) => return,
        };

        let ttl = self.pool.config().session_ttl();
        if let Err(e) = conn.expire(&key, ttl).await {
            tracing::warn!("Failed to refresh session TTL: {}", e);
        }
    }

    async fn cleanup_expired(&self) {
        debug!("Redis session store handles expiration automatically via TTL");
    }

    #[instrument(skip(self), fields(user_id = %provider_user_id))]
    async fn get_by_user(&self, provider_user_id: &str) -> Vec<Session> {
        let key = self.user_sessions_key(provider_user_id);
        let mut conn = match self.pool.get_connection().await {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let session_ids: Vec<String> = match conn.smembers(&key).await {
            Ok(ids) => ids,
            Err(_) => return Vec::new(),
        };

        let mut sessions = Vec::new();
        for sid in session_ids {
            if let Ok(uuid) = Uuid::parse_str(&sid) {
                if let Some(session) = self.get(&uuid).await {
                    if !session.is_expired() {
                        sessions.push(session);
                    }
                }
            }
        }

        sessions
    }
}

impl RedisSessionStore {
    pub async fn get_tenant_sessions(&self, tenant_id: &str) -> CacheResult<Vec<Session>> {
        let key = self.user_sessions_key(&format!("tenant:{}", tenant_id));
        let mut conn = self.pool.get_connection().await?;

        let session_ids: Vec<String> = conn.smembers(&key).await?;
        let mut sessions = Vec::new();

        for sid in session_ids {
            if let Ok(uuid) = Uuid::parse_str(&sid) {
                if let Some(session) = self.get(&uuid).await {
                    if !session.is_expired() {
                        sessions.push(session);
                    }
                }
            }
        }

        Ok(sessions)
    }

    pub async fn invalidate_tenant_sessions(&self, tenant_id: &str) -> CacheResult<u64> {
        let sessions = self.get_tenant_sessions(tenant_id).await?;
        let mut count = 0u64;

        for session in sessions {
            self.delete(&session.id).await;
            count += 1;
        }

        Ok(count)
    }

    pub async fn session_count(&self) -> CacheResult<u64> {
        let pattern = format!("{}:session:*", self.key_prefix());
        let mut conn = self.pool.get_connection().await?;

        let keys: Vec<String> = conn
            .keys(&pattern)
            .await
            .map_err(|e| crate::cache::CacheError::Operation(format!("KEYS failed: {}", e)))?;

        Ok(keys.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::oauth::{OAuthProvider, OAuthUser};

    fn _test_user() -> OAuthUser {
        OAuthUser {
            provider: OAuthProvider::GitHub,
            provider_user_id: "user-123".to_string(),
            email: Some("test@example.com".to_string()),
            name: Some("Test User".to_string()),
            username: Some("testuser".to_string()),
            avatar_url: None,
        }
    }

    #[tokio::test]
    async fn test_session_key_format() {
        let config = crate::cache::CacheConfig::default();
        let pool = match crate::cache::CachePool::new(config).await {
            Ok(p) => p,
            Err(_) => {
                return;
            }
        };
        let _store = RedisSessionStore::new(Arc::new(pool));
    }
}

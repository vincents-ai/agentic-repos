use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::OAuthUser;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub user: OAuthUser,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub tenant_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl Session {
    pub fn new(user: OAuthUser, duration: Duration) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user,
            created_at: now,
            expires_at: now + duration,
            last_activity: now,
            tenant_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_tenant(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }

    pub fn remaining_duration(&self) -> Duration {
        let remaining = self.expires_at - Utc::now();
        if remaining > Duration::zero() {
            remaining
        } else {
            Duration::zero()
        }
    }
}

#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn get(&self, session_id: &Uuid) -> Option<Session>;
    async fn set(&self, session: Session);
    async fn delete(&self, session_id: &Uuid);
    async fn touch(&self, session_id: &Uuid);
    async fn cleanup_expired(&self);
    async fn get_by_user(&self, provider_user_id: &str) -> Vec<Session>;
}

#[allow(dead_code)]
pub struct InMemorySessionStore {
    sessions: Arc<RwLock<HashMap<Uuid, Session>>>,
    default_duration: Duration,
}

impl InMemorySessionStore {
    pub fn new(default_duration: Duration) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            default_duration,
        }
    }

    pub fn with_capacity(capacity: usize, default_duration: Duration) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::with_capacity(capacity))),
            default_duration,
        }
    }
}

#[async_trait::async_trait]
impl SessionStore for InMemorySessionStore {
    async fn get(&self, session_id: &Uuid) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .filter(|s| !s.is_expired())
            .cloned()
    }

    async fn set(&self, session: Session) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id, session);
    }

    async fn delete(&self, session_id: &Uuid) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    async fn touch(&self, session_id: &Uuid) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.touch();
        }
    }

    async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, session| !session.is_expired());
    }

    async fn get_by_user(&self, provider_user_id: &str) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.user.provider_user_id == provider_user_id && !s.is_expired())
            .cloned()
            .collect()
    }
}

pub struct SessionManager<S: SessionStore> {
    store: S,
    session_duration: Duration,
}

impl<S: SessionStore> SessionManager<S> {
    pub fn new(store: S, session_duration: Duration) -> Self {
        Self {
            store,
            session_duration,
        }
    }

    pub async fn create_session(&self, user: OAuthUser) -> Session {
        let session = Session::new(user, self.session_duration);
        self.store.set(session.clone()).await;
        session
    }

    pub async fn create_session_with_tenant(&self, user: OAuthUser, tenant_id: String) -> Session {
        let session = Session::new(user, self.session_duration).with_tenant(tenant_id);
        self.store.set(session.clone()).await;
        session
    }

    pub async fn get_session(&self, session_id: &Uuid) -> Option<Session> {
        let session = self.store.get(session_id).await?;
        if session.is_expired() {
            self.store.delete(session_id).await;
            None
        } else {
            self.store.touch(session_id).await;
            Some(session)
        }
    }

    pub async fn validate_session(&self, session_id: &Uuid) -> bool {
        self.get_session(session_id).await.is_some()
    }

    pub async fn invalidate_session(&self, session_id: &Uuid) {
        self.store.delete(session_id).await;
    }

    pub async fn invalidate_user_sessions(&self, provider_user_id: &str) {
        let sessions = self.store.get_by_user(provider_user_id).await;
        for session in sessions {
            self.store.delete(&session.id).await;
        }
    }

    pub async fn cleanup(&self) {
        self.store.cleanup_expired().await;
    }
}

impl InMemorySessionStore {
    pub fn session_count(&self) -> usize {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sessions = self.sessions.read().await;
                sessions.len()
            })
        })
    }
}

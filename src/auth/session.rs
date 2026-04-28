//! Session management — provider-agnostic user identity.
//!
//! `SessionUser` is the minimal identity stored in a session. It's decoupled
//! from any specific auth provider (OAuth, LDAP, SAML, SSH). Providers in adp
//! convert their user types to `SessionUser` when creating sessions.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Provider-agnostic user identity stored in a session.
///
/// Created by whatever auth provider the user authenticated through
/// (OAuth, LDAP, SAML, or SSH key). agentic-repos doesn't care which
/// provider was used — it just needs a stable identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionUser {
    /// Unique user identifier (provider-specific, e.g. "github:12345", "ldap:jsmith").
    pub user_id: String,
    /// Human-readable username.
    pub username: Option<String>,
    /// Email address.
    pub email: Option<String>,
    /// Display name (full name).
    pub display_name: Option<String>,
    /// Tenant ID for multi-tenant deployments.
    pub tenant_id: Option<String>,
}

impl SessionUser {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            username: None,
            email: None,
            display_name: None,
            tenant_id: None,
        }
    }

    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub user: SessionUser,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

impl Session {
    pub fn new(user: SessionUser, duration: Duration) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user,
            created_at: now,
            expires_at: now + duration,
            last_activity: now,
            metadata: HashMap::new(),
        }
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
    async fn get_by_user(&self, user_id: &str) -> Vec<Session>;
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

    async fn get_by_user(&self, user_id: &str) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.user.user_id == user_id && !s.is_expired())
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

    pub async fn create_session(&self, user: SessionUser) -> Session {
        let session = Session::new(user, self.session_duration);
        self.store.set(session.clone()).await;
        session
    }

    pub async fn create_session_with_tenant(&self, user: SessionUser, tenant_id: String) -> Session {
        let session = Session::new(user.with_tenant(&tenant_id), self.session_duration);
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

    pub async fn invalidate_user_sessions(&self, user_id: &str) {
        let sessions = self.store.get_by_user(user_id).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user() -> SessionUser {
        SessionUser::new("test:user123")
            .with_username("testuser")
            .with_email("test@example.com")
    }

    #[test]
    fn session_user_builder() {
        let user = test_user();
        assert_eq!(user.user_id, "test:user123");
        assert_eq!(user.username.as_deref(), Some("testuser"));
        assert_eq!(user.email.as_deref(), Some("test@example.com"));
        assert!(user.tenant_id.is_none());
    }

    #[test]
    fn session_user_with_tenant() {
        let user = test_user().with_tenant("acme");
        assert_eq!(user.tenant_id.as_deref(), Some("acme"));
    }

    #[test]
    fn session_creation() {
        let user = test_user();
        let session = Session::new(user, Duration::hours(1));
        assert!(!session.is_expired());
        assert_eq!(session.user.user_id, "test:user123");
    }

    #[test]
    fn session_expiry() {
        let user = test_user();
        let mut session = Session::new(user, Duration::hours(0));
        session.expires_at = Utc::now() - Duration::seconds(1);
        assert!(session.is_expired());
    }

    #[tokio::test]
    async fn in_memory_store_crud() {
        let store = InMemorySessionStore::new(Duration::hours(1));
        let user = test_user();
        let session = Session::new(user, Duration::hours(1));

        store.set(session.clone()).await;
        let retrieved = store.get(&session.id).await;
        assert!(retrieved.is_some());

        store.delete(&session.id).await;
        let gone = store.get(&session.id).await;
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn session_manager_lifecycle() {
        let store = InMemorySessionStore::new(Duration::hours(1));
        let manager = SessionManager::new(store, Duration::hours(1));
        let user = test_user();

        let session = manager.create_session(user).await;
        let retrieved = manager.get_session(&session.id).await;
        assert!(retrieved.is_some());

        manager.invalidate_session(&session.id).await;
        let gone = manager.get_session(&session.id).await;
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn get_by_user() {
        let store = InMemorySessionStore::new(Duration::hours(1));
        let user = test_user();
        let session = Session::new(user, Duration::hours(1));
        store.set(session).await;

        let found = store.get_by_user("test:user123").await;
        assert_eq!(found.len(), 1);

        let missing = store.get_by_user("nobody").await;
        assert!(missing.is_empty());
    }
}

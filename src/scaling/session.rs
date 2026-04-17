//! Session affinity configuration for load balancer integration
//!
//! Provides sticky session support for WebSocket connections
//! and stateful operations that need to route to the same instance.

use crate::scaling::config::SessionAffinityConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub session_id: String,
    pub instance_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
}

impl SessionConfig {
    pub fn new(session_id: String, instance_id: String, ttl_secs: u64) -> Self {
        let now = chrono::Utc::now();
        Self {
            session_id,
            instance_id,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(ttl_secs as i64),
            metadata: HashMap::new(),
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    pub fn touch(&mut self, ttl_secs: u64) {
        self.expires_at = chrono::Utc::now() + chrono::Duration::seconds(ttl_secs as i64);
    }
}

pub struct SessionAffinity {
    config: SessionAffinityConfig,
    instance_id: String,
    sessions: Arc<RwLock<HashMap<String, SessionConfig>>>,
}

impl SessionAffinity {
    pub fn new(config: SessionAffinityConfig, instance_id: String) -> Self {
        Self {
            config,
            instance_id,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn cookie_name(&self) -> &str {
        &self.config.cookie_name
    }

    pub fn header_name(&self) -> &str {
        &self.config.header_name
    }

    pub async fn create_session(&self, session_id: Option<String>) -> SessionConfig {
        let id = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let session =
            SessionConfig::new(id.clone(), self.instance_id.clone(), self.config.ttl_secs);

        let mut sessions = self.sessions.write().await;
        sessions.insert(id, session.clone());

        debug!(
            "Created session {} for instance {}",
            session.session_id, self.instance_id
        );

        session
    }

    pub async fn get_session(&self, session_id: &str) -> Option<SessionConfig> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    pub async fn validate_session(&self, session_id: &str) -> Option<SessionConfig> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            if session.is_expired() {
                sessions.remove(session_id);
                debug!("Session {} expired", session_id);
                return None;
            }

            if session.instance_id == self.instance_id {
                session.touch(self.config.ttl_secs);
                return Some(session.clone());
            }

            debug!(
                "Session {} belongs to different instance {}",
                session_id, session.instance_id
            );
        }

        None
    }

    pub async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            debug!("Removed session {}", session_id);
        }
    }

    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        let before = sessions.len();

        sessions.retain(|_, session| !session.is_expired());

        let removed = before - sessions.len();
        if removed > 0 {
            info!("Cleaned up {} expired sessions", removed);
        }
    }

    pub fn get_cookie_value(&self, session: &SessionConfig) -> String {
        format!(
            "{}={}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax",
            self.config.cookie_name, session.session_id, self.config.ttl_secs
        )
    }

    pub fn get_header_value(&self, _session: &SessionConfig) -> String {
        format!("{}={}", self.config.header_name, self.instance_id)
    }

    pub async fn active_session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    pub fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let sessions = Arc::clone(&self.sessions);
        let interval_secs = self.config.ttl_secs / 2;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs.max(60)));
            loop {
                interval.tick().await;

                let mut sessions = sessions.write().await;
                let before = sessions.len();
                sessions.retain(|_, session| !session.is_expired());
                let removed = before - sessions.len();

                if removed > 0 {
                    debug!("Cleanup task removed {} expired sessions", removed);
                }
            }
        })
    }
}

pub fn extract_session_from_cookie(cookie_header: &str, cookie_name: &str) -> Option<String> {
    cookie_header
        .split(';')
        .filter_map(|cookie| {
            let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
            if parts.len() == 2 && parts[0] == cookie_name {
                Some(parts[1].to_string())
            } else {
                None
            }
        })
        .next()
}

pub fn extract_session_from_header(header_value: &str) -> Option<String> {
    let parts: Vec<&str> = header_value.splitn(2, '=').collect();
    if parts.len() == 2 {
        Some(parts[1].to_string())
    } else {
        Some(header_value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_creation() {
        let config = SessionAffinityConfig::default();
        let affinity = SessionAffinity::new(config, "instance-1".to_string());

        let session = affinity.create_session(None).await;
        assert_eq!(session.instance_id, "instance-1");
        assert!(!session.is_expired());
    }

    #[tokio::test]
    async fn test_session_validation() {
        let config = SessionAffinityConfig::default();
        let affinity = SessionAffinity::new(config, "instance-1".to_string());

        let _session = affinity
            .create_session(Some("test-session".to_string()))
            .await;

        let validated = affinity.validate_session("test-session").await;
        assert!(validated.is_some());
    }

    #[tokio::test]
    async fn test_session_wrong_instance() {
        let config = SessionAffinityConfig::default();
        let affinity1 = SessionAffinity::new(config.clone(), "instance-1".to_string());
        let affinity2 = SessionAffinity::new(config, "instance-2".to_string());

        let _session = affinity1
            .create_session(Some("test-session".to_string()))
            .await;

        let validated = affinity2.validate_session("test-session").await;
        assert!(validated.is_none());
    }

    #[test]
    fn test_extract_session_from_cookie() {
        let cookie = "SESSION_ID=abc123; Path=/; other=value";
        let session = extract_session_from_cookie(cookie, "SESSION_ID");
        assert_eq!(session, Some("abc123".to_string()));
    }
}

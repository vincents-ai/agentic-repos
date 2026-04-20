use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("HTTP client error: {0}")]
    HttpClient(String),
    #[error("Token exchange failed: {0}")]
    TokenExchange(String),
    #[error("Token refresh failed: {0}")]
    TokenRefresh(String),
    #[error("User info failed: {0}")]
    UserInfo(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("CSRF state mismatch or expired")]
    CsrfStateMismatch,
    #[error("Missing CSRF state parameter")]
    CsrfStateMissing,
}

use chrono::{DateTime, Duration, Utc};
use oauth2::{
    basic::BasicClient, basic::BasicTokenType, reqwest::async_http_client, AuthUrl,
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, RefreshToken, Scope,
    StandardTokenResponse, TokenResponse, TokenUrl,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub provider: OAuthProvider,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub authorize_url: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    #[serde(default)]
    pub userinfo_url: Option<String>,
    #[serde(default)]
    pub issuer_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProvider {
    GitHub,
    Google,
    Okta,
    Custom,
}

impl OAuthProvider {
    pub fn authorize_url(&self) -> &str {
        match self {
            OAuthProvider::GitHub => "https://github.com/login/oauth/authorize",
            OAuthProvider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            OAuthProvider::Okta => "/oauth2/v1/authorize",
            OAuthProvider::Custom => "",
        }
    }

    pub fn token_url(&self) -> &str {
        match self {
            OAuthProvider::GitHub => "https://github.com/login/oauth/access_token",
            OAuthProvider::Google => "https://oauth2.googleapis.com/token",
            OAuthProvider::Okta => "/oauth2/v1/token",
            OAuthProvider::Custom => "",
        }
    }

    pub fn userinfo_url(&self) -> &str {
        match self {
            OAuthProvider::GitHub => "https://api.github.com/user",
            OAuthProvider::Google => "https://openidconnect.googleapis.com/v1/userinfo",
            OAuthProvider::Okta => "/oauth2/v1/userinfo",
            OAuthProvider::Custom => "",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_at: DateTime<Utc>,
    pub scope: Option<String>,
}

impl OAuthToken {
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    pub fn from_oauth2_response(
        response: StandardTokenResponse<oauth2::EmptyExtraTokenFields, BasicTokenType>,
    ) -> Self {
        let expires_at = response
            .expires_in()
            .map(|d| {
                Utc::now()
                    + chrono::Duration::from_std(d)
                        .unwrap_or_else(|_| chrono::Duration::seconds(3600))
            })
            .unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(3600));

        Self {
            access_token: response.access_token().secret().to_string(),
            refresh_token: response
                .refresh_token()
                .map(|t: &oauth2::RefreshToken| t.secret().to_string()),
            token_type: "Bearer".to_string(),
            expires_at,
            scope: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUser {
    pub provider: OAuthProvider,
    pub provider_user_id: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub url: Url,
    pub csrf_token: CsrfToken,
    pub redirect_url: Option<String>,
}

// ---------------------------------------------------------------------------
// CSRF State Store
// ---------------------------------------------------------------------------

/// Metadata stored alongside each CSRF state token during the OAuth authorize phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthStateEntry {
    pub provider: OAuthProvider,
    pub redirect_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Trait for persisting and verifying OAuth CSRF state tokens.
#[async_trait::async_trait]
pub trait OAuthStateStore: Send + Sync {
    /// Store a state token with its associated entry.
    async fn set(&self, state: &str, entry: OAuthStateEntry);
    /// Verify and atomically consume a state token. Returns `None` if not found or expired.
    async fn verify_and_consume(&self, state: &str) -> Option<OAuthStateEntry>;
    /// Remove expired entries.
    async fn cleanup_expired(&self);
}

/// Default in-memory implementation of [`OAuthStateStore`].
#[allow(dead_code)]
pub struct InMemoryOAuthStateStore {
    states: Arc<RwLock<HashMap<String, OAuthStateEntry>>>,
    ttl: Duration,
}

impl InMemoryOAuthStateStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }
}

#[async_trait::async_trait]
impl OAuthStateStore for InMemoryOAuthStateStore {
    async fn set(&self, state: &str, entry: OAuthStateEntry) {
        let mut states = self.states.write().await;
        states.insert(state.to_string(), entry);
    }

    async fn verify_and_consume(&self, state: &str) -> Option<OAuthStateEntry> {
        let mut states = self.states.write().await;
        let entry = states.remove(state)?;
        // Reject if expired
        if Utc::now() - entry.created_at > self.ttl {
            None
        } else {
            Some(entry)
        }
    }

    async fn cleanup_expired(&self) {
        let mut states = self.states.write().await;
        let ttl = self.ttl;
        states.retain(|_, entry| Utc::now() - entry.created_at <= ttl);
    }
}

// ---------------------------------------------------------------------------
// OAuth Service
// ---------------------------------------------------------------------------

pub struct OAuthService {
    config: OAuthConfig,
    http_client: Client,
    state_store: Arc<dyn OAuthStateStore>,
}

impl OAuthService {
    pub fn new(
        config: OAuthConfig,
        state_store: Arc<dyn OAuthStateStore>,
    ) -> Result<Self, AuthError> {
        let http_client = Client::builder()
            .user_agent("agentic-repos-oauth")
            .build()
            .map_err(|e| AuthError::HttpClient(e.to_string()))?;

        Ok(Self {
            config,
            http_client,
            state_store,
        })
    }

    /// Convenience constructor with the default in-memory state store (10 min TTL).
    pub fn with_default_state_store(config: OAuthConfig) -> Result<Self, AuthError> {
        let store = Arc::new(InMemoryOAuthStateStore::new(Duration::minutes(10)));
        Self::new(config, store)
    }

    /// Generate an authorization URL with a CSRF state token that is persisted for later
    /// verification in [`Self::exchange_code_with_state`].
    pub fn get_authorization_url(
        &self,
        redirect_url: Option<String>,
    ) -> Result<AuthorizationRequest, AuthError> {
        let client = self.build_oauth_client()?;

        let mut builder = client.authorize_url(CsrfToken::new_random);

        for scope in &self.config.scopes {
            builder = builder.add_scope(Scope::new(scope.clone()));
        }

        let (url, csrf_token) = builder.url();

        Ok(AuthorizationRequest {
            url,
            csrf_token,
            redirect_url,
        })
    }

    /// Persist the CSRF state so it can be verified on callback.
    pub async fn store_csrf_state(&self, csrf_token: &CsrfToken, redirect_url: Option<String>) {
        let entry = OAuthStateEntry {
            provider: self.config.provider.clone(),
            redirect_url,
            created_at: Utc::now(),
        };
        self.state_store.set(csrf_token.secret(), entry).await;
    }

    /// Exchange an authorization code **after** validating the CSRF state parameter.
    /// The state is consumed (single-use) on successful verification.
    pub async fn exchange_code_with_state(
        &self,
        code: &str,
        state: &str,
    ) -> Result<OAuthToken, AuthError> {
        if state.is_empty() {
            return Err(AuthError::CsrfStateMissing);
        }

        let _entry = self
            .state_store
            .verify_and_consume(state)
            .await
            .ok_or(AuthError::CsrfStateMismatch)?;

        self.exchange_code(code).await
    }

    pub async fn exchange_code(&self, code: &str) -> Result<OAuthToken, AuthError> {
        let client = self.build_oauth_client()?;

        let token_response = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .request_async(async_http_client)
            .await
            .map_err(|e| AuthError::TokenExchange(e.to_string()))?;

        Ok(OAuthToken::from_oauth2_response(token_response))
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthToken, AuthError> {
        let client = self.build_oauth_client()?;

        let token_response = client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .request_async(async_http_client)
            .await
            .map_err(|e| AuthError::TokenRefresh(e.to_string()))?;

        Ok(OAuthToken::from_oauth2_response(token_response))
    }

    pub async fn get_user_info(&self, token: &OAuthToken) -> Result<OAuthUser, AuthError> {
        let userinfo_url = self.get_userinfo_endpoint();

        let response = self
            .http_client
            .get(userinfo_url)
            .bearer_auth(&token.access_token)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| AuthError::UserInfo(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AuthError::UserInfo(format!(
                "Failed to get user info: status {}",
                response.status()
            )));
        }

        let user = match &self.config.provider {
            OAuthProvider::GitHub => {
                let github_user: GitHubUser = response
                    .json()
                    .await
                    .map_err(|e| AuthError::UserInfo(e.to_string()))?;

                OAuthUser {
                    provider: OAuthProvider::GitHub,
                    provider_user_id: github_user.id.to_string(),
                    username: Some(github_user.login),
                    email: github_user.email,
                    name: github_user.name,
                    avatar_url: github_user.avatar_url,
                }
            }
            OAuthProvider::Google => {
                let google_user: GoogleUser = response
                    .json()
                    .await
                    .map_err(|e| AuthError::UserInfo(e.to_string()))?;

                OAuthUser {
                    provider: OAuthProvider::Google,
                    provider_user_id: google_user.sub,
                    username: None,
                    email: Some(google_user.email),
                    name: Some(google_user.name),
                    avatar_url: google_user.picture,
                }
            }
            OAuthProvider::Okta => {
                let okta_user: OktaUser = response
                    .json()
                    .await
                    .map_err(|e| AuthError::UserInfo(e.to_string()))?;

                OAuthUser {
                    provider: OAuthProvider::Okta,
                    provider_user_id: okta_user.sub,
                    username: okta_user.preferred_username,
                    email: okta_user.email,
                    name: okta_user.name,
                    avatar_url: okta_user.picture,
                }
            }
            OAuthProvider::Custom => {
                let custom_user: CustomUserInfo = response
                    .json()
                    .await
                    .map_err(|e| AuthError::UserInfo(e.to_string()))?;

                OAuthUser {
                    provider: OAuthProvider::Custom,
                    provider_user_id: custom_user.sub,
                    username: custom_user.username,
                    email: custom_user.email,
                    name: custom_user.name,
                    avatar_url: custom_user.picture,
                }
            }
        };

        Ok(user)
    }

    pub async fn validate_token(&self, token: &OAuthToken) -> Result<bool, AuthError> {
        if token.is_expired() {
            return Ok(false);
        }

        let user = self.get_user_info(token).await?;
        Ok(!user.provider_user_id.is_empty())
    }

    fn build_oauth_client(&self) -> Result<BasicClient, AuthError> {
        let auth_url = self.get_authorize_endpoint();
        let auth_url =
            AuthUrl::new(auth_url).map_err(|e| AuthError::Configuration(e.to_string()))?;

        let redirect_url = RedirectUrl::new(self.config.redirect_uri.clone())
            .map_err(|e| AuthError::Configuration(e.to_string()))?;

        let token_url = self.get_token_endpoint();
        let token_url =
            TokenUrl::new(token_url).map_err(|e| AuthError::Configuration(e.to_string()))?;

        let client = BasicClient::new(
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
            auth_url,
            Some(token_url),
        )
        .set_redirect_uri(redirect_url);

        Ok(client)
    }

    fn get_authorize_endpoint(&self) -> String {
        if let Some(ref url) = self.config.authorize_url {
            url.clone()
        } else if self.config.provider == OAuthProvider::Okta {
            if let Some(ref issuer) = self.config.issuer_url {
                format!(
                    "{}{}",
                    issuer.trim_end_matches('/'),
                    self.config.provider.authorize_url()
                )
            } else {
                self.config.provider.authorize_url().to_string()
            }
        } else {
            self.config.provider.authorize_url().to_string()
        }
    }

    fn get_token_endpoint(&self) -> String {
        if let Some(ref url) = self.config.token_url {
            url.clone()
        } else if self.config.provider == OAuthProvider::Okta {
            if let Some(ref issuer) = self.config.issuer_url {
                format!(
                    "{}{}",
                    issuer.trim_end_matches('/'),
                    self.config.provider.token_url()
                )
            } else {
                self.config.provider.token_url().to_string()
            }
        } else {
            self.config.provider.token_url().to_string()
        }
    }

    fn get_userinfo_endpoint(&self) -> String {
        if let Some(ref url) = self.config.userinfo_url {
            url.clone()
        } else if self.config.provider == OAuthProvider::Okta {
            if let Some(ref issuer) = self.config.issuer_url {
                format!(
                    "{}{}",
                    issuer.trim_end_matches('/'),
                    self.config.provider.userinfo_url()
                )
            } else {
                self.config.provider.userinfo_url().to_string()
            }
        } else {
            self.config.provider.userinfo_url().to_string()
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    id: u64,
    login: String,
    email: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleUser {
    sub: String,
    email: String,
    name: String,
    picture: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OktaUser {
    sub: String,
    email: Option<String>,
    name: Option<String>,
    preferred_username: Option<String>,
    picture: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CustomUserInfo {
    sub: String,
    username: Option<String>,
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_provider_urls_github() {
        let provider = OAuthProvider::GitHub;
        assert!(provider.authorize_url().contains("github.com"));
        assert!(provider.token_url().contains("github.com"));
        assert!(provider.userinfo_url().contains("github.com"));
    }

    #[test]
    fn test_oauth_provider_urls_google() {
        let provider = OAuthProvider::Google;
        assert!(provider.authorize_url().contains("google"));
        assert!(provider.token_url().contains("google"));
        assert!(provider.userinfo_url().contains("google"));
    }

    #[test]
    fn test_oauth_provider_urls_okta() {
        let provider = OAuthProvider::Okta;
        assert!(provider.authorize_url().contains("oauth2"));
        assert!(provider.token_url().contains("oauth2"));
    }

    #[test]
    fn test_oauth_provider_custom_empty() {
        let provider = OAuthProvider::Custom;
        assert_eq!(provider.authorize_url(), "");
        assert_eq!(provider.token_url(), "");
        assert_eq!(provider.userinfo_url(), "");
    }

    #[test]
    fn test_oauth_token_is_expired_future() {
        let token = OAuthToken {
            access_token: "abc".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            scope: None,
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn test_oauth_token_is_expired_past() {
        let token = OAuthToken {
            access_token: "abc".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: Utc::now() - chrono::Duration::hours(1),
            scope: None,
        };
        assert!(token.is_expired());
    }

    #[test]
    fn test_oauth_config_deserialize() {
        let json = serde_json::json!({
            "provider": "github",
            "client_id": "my-client-id",
            "client_secret": "my-secret",
            "redirect_uri": "http://localhost:8080/callback",
            "scopes": ["user:email"]
        });
        let config: OAuthConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.provider, OAuthProvider::GitHub);
        assert_eq!(config.client_id, "my-client-id");
        assert_eq!(config.scopes, vec!["user:email"]);
    }

    #[test]
    fn test_oauth_config_custom_urls() {
        let json = serde_json::json!({
            "provider": "custom",
            "client_id": "id",
            "client_secret": "secret",
            "redirect_uri": "http://localhost/callback",
            "scopes": [],
            "authorize_url": "https://auth.example.com/authorize",
            "token_url": "https://auth.example.com/token",
            "userinfo_url": "https://auth.example.com/userinfo"
        });
        let config: OAuthConfig = serde_json::from_value(json).unwrap();
        assert_eq!(
            config.authorize_url,
            Some("https://auth.example.com/authorize".to_string())
        );
    }

    // ----- CSRF State Store tests -----

    #[tokio::test]
    async fn test_csrf_state_store_set_and_verify() {
        let store = InMemoryOAuthStateStore::new(Duration::minutes(10));

        let entry = OAuthStateEntry {
            provider: OAuthProvider::GitHub,
            redirect_url: Some("https://app.example.com/dashboard".to_string()),
            created_at: Utc::now(),
        };

        store.set("test-state-token", entry.clone()).await;

        let retrieved = store.verify_and_consume("test-state-token").await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.provider, OAuthProvider::GitHub);
        assert_eq!(
            retrieved.redirect_url,
            Some("https://app.example.com/dashboard".to_string())
        );
    }

    #[tokio::test]
    async fn test_csrf_state_store_single_use() {
        let store = InMemoryOAuthStateStore::new(Duration::minutes(10));

        let entry = OAuthStateEntry {
            provider: OAuthProvider::GitHub,
            redirect_url: None,
            created_at: Utc::now(),
        };

        store.set("single-use-token", entry).await;

        // First consume should succeed
        let first = store.verify_and_consume("single-use-token").await;
        assert!(first.is_some());

        // Second consume should fail (token was consumed)
        let second = store.verify_and_consume("single-use-token").await;
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn test_csrf_state_store_expired() {
        let store = InMemoryOAuthStateStore::new(Duration::seconds(1));

        let entry = OAuthStateEntry {
            provider: OAuthProvider::Google,
            redirect_url: None,
            created_at: Utc::now() - Duration::seconds(5), // Created 5s ago, TTL is 1s
        };

        store.set("expired-token", entry).await;

        let retrieved = store.verify_and_consume("expired-token").await;
        assert!(
            retrieved.is_none(),
            "Expired state should not be verifiable"
        );
    }

    #[tokio::test]
    async fn test_csrf_state_store_unknown_token() {
        let store = InMemoryOAuthStateStore::new(Duration::minutes(10));

        let retrieved = store.verify_and_consume("nonexistent-token").await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_csrf_state_store_cleanup() {
        let store = InMemoryOAuthStateStore::new(Duration::seconds(1));

        let expired_entry = OAuthStateEntry {
            provider: OAuthProvider::GitHub,
            redirect_url: None,
            created_at: Utc::now() - Duration::seconds(5),
        };
        let valid_entry = OAuthStateEntry {
            provider: OAuthProvider::Google,
            redirect_url: None,
            created_at: Utc::now(),
        };

        store.set("expired", expired_entry).await;
        store.set("valid", valid_entry).await;

        store.cleanup_expired().await;

        // Expired should be gone
        assert!(store.verify_and_consume("expired").await.is_none());
        // Valid should still be there
        assert!(store.verify_and_consume("valid").await.is_some());
    }

    #[tokio::test]
    async fn test_exchange_code_with_state_missing_state() {
        let config = OAuthConfig {
            provider: OAuthProvider::GitHub,
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            scopes: vec![],
            authorize_url: None,
            token_url: None,
            userinfo_url: None,
            issuer_url: None,
        };
        let service = OAuthService::with_default_state_store(config).unwrap();

        let result = service.exchange_code_with_state("some-code", "").await;
        assert!(matches!(result, Err(AuthError::CsrfStateMissing)));
    }

    #[tokio::test]
    async fn test_exchange_code_with_state_mismatch() {
        let config = OAuthConfig {
            provider: OAuthProvider::GitHub,
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            scopes: vec![],
            authorize_url: None,
            token_url: None,
            userinfo_url: None,
            issuer_url: None,
        };
        let service = OAuthService::with_default_state_store(config).unwrap();

        let result = service
            .exchange_code_with_state("some-code", "bogus-state")
            .await;
        assert!(matches!(result, Err(AuthError::CsrfStateMismatch)));
    }

    #[tokio::test]
    async fn test_authorization_url_stores_csrf_state() {
        let config = OAuthConfig {
            provider: OAuthProvider::GitHub,
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            scopes: vec!["user:email".to_string()],
            authorize_url: None,
            token_url: None,
            userinfo_url: None,
            issuer_url: None,
        };
        let service = OAuthService::with_default_state_store(config).unwrap();

        let auth_req = service
            .get_authorization_url(Some("https://app.example.com/dash".to_string()))
            .unwrap();

        // The URL should contain a state parameter
        assert!(auth_req.url.query().unwrap().contains("state="));

        // Store the CSRF state
        service
            .store_csrf_state(&auth_req.csrf_token, auth_req.redirect_url.clone())
            .await;

        // Now the state should be verifiable
        let state_param = auth_req.csrf_token.secret();
        let result = service.state_store.verify_and_consume(state_param).await;
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.provider, OAuthProvider::GitHub);
        assert_eq!(
            entry.redirect_url,
            Some("https://app.example.com/dash".to_string())
        );
    }
}

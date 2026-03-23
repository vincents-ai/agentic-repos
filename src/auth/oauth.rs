use crate::error::AuthError;
use chrono::{DateTime, Utc};
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthorizationCode, AuthUrl, ClientId,
    ClientSecret, CsrfToken, RedirectUrl, RefreshToken, Scope, StandardTokenResponse, TokenUrl,
    basic::BasicTokenType, TokenResponse,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
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

    pub fn from_oauth2_response(response: StandardTokenResponse<oauth2::EmptyExtraTokenFields, BasicTokenType>) -> Self {
        let expires_at = response
            .expires_in()
            .map(|d| Utc::now() + chrono::Duration::from_std(d).unwrap_or_else(|_| chrono::Duration::seconds(3600)))
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
}

pub struct OAuthService {
    config: OAuthConfig,
    http_client: Client,
}

impl OAuthService {
    pub fn new(config: OAuthConfig) -> Result<Self, AuthError> {
        let http_client = Client::builder()
            .user_agent("repos-mcp-oauth")
            .build()
            .map_err(|e| AuthError::HttpClient(e.to_string()))?;

        Ok(Self {
            config,
            http_client,
        })
    }

    pub fn get_authorization_url(&self, _state: Option<String>) -> Result<AuthorizationRequest, AuthError> {
        let client = self.build_oauth_client()?;

        let mut builder = client.authorize_url(CsrfToken::new_random);

        for scope in &self.config.scopes {
            builder = builder.add_scope(Scope::new(scope.clone()));
        }

        let (url, csrf_token) = builder.url();

        Ok(AuthorizationRequest {
            url,
            csrf_token,
        })
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
        let auth_url = AuthUrl::new(auth_url)
            .map_err(|e| AuthError::Configuration(e.to_string()))?;

        let redirect_url = RedirectUrl::new(self.config.redirect_uri.clone())
            .map_err(|e| AuthError::Configuration(e.to_string()))?;

        let token_url = self.get_token_endpoint();
        let token_url = TokenUrl::new(token_url)
            .map_err(|e| AuthError::Configuration(e.to_string()))?;

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
                format!("{}{}", issuer.trim_end_matches('/'), self.config.provider.authorize_url())
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
                format!("{}{}", issuer.trim_end_matches('/'), self.config.provider.token_url())
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
                format!("{}{}", issuer.trim_end_matches('/'), self.config.provider.userinfo_url())
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

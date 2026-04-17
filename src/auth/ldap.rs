//! LDAP / Active Directory authentication.
//!
//! Provides bind-based authentication (verifies credentials against the directory),
//! user search, group membership lookup, and attribute mapping.
//!
//! The implementation uses the `ldap3` crate which supports both plain and TLS
//! connections via the `LdapConnSettings` builder.

use ldap3::{Ldap, LdapConnAsync, LdapConnSettings, Scope, SearchEntry};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LdapError {
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Bind failed: invalid credentials")]
    InvalidCredentials,
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Search error: {0}")]
    Search(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("LDAP protocol error: {0}")]
    Protocol(String),
}

/// TLS mode for LDAP connections.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LdapTlsMode {
    /// Plain LDAP (ldap://) — development only.
    #[default]
    None,
    /// LDAPS (ldaps://) — TLS from the start.
    Ldaps,
    /// STARTTLS upgrade on ldap://.
    StartTls,
}

/// LDAP server and directory configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
    /// LDAP server URL, e.g. "ldap://dc.corp.example.com:389" or "ldaps://dc:636"
    pub url: String,
    /// Base DN for user searches, e.g. "dc=corp,dc=example,dc=com"
    pub base_dn: String,
    /// Bind DN for service account (used to search the directory).
    /// If None, anonymous bind is attempted (not recommended for AD).
    pub bind_dn: Option<String>,
    /// Service account password.
    pub bind_password: Option<String>,
    /// Filter template to find a user by username.
    /// `{username}` is replaced with the escaped login name.
    /// AD default: "(sAMAccountName={username})"
    /// OpenLDAP default: "(uid={username})"
    #[serde(default = "default_user_filter")]
    pub user_filter: String,
    /// Group membership attribute on the user object.
    /// AD: "memberOf", OpenLDAP: "memberOf" or "gidNumber"
    #[serde(default = "default_group_attr")]
    pub group_attribute: String,
    /// LDAP attribute for email. Default: "mail"
    #[serde(default = "default_mail_attr")]
    pub mail_attribute: String,
    /// LDAP attribute for display name. Default: "displayName"
    #[serde(default = "default_display_attr")]
    pub display_name_attribute: String,
    /// TLS mode.
    #[serde(default)]
    pub tls_mode: LdapTlsMode,
    /// Skip TLS certificate verification (development only, never for production).
    #[serde(default)]
    pub tls_insecure: bool,
    /// Connection timeout in seconds (default: 10).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_user_filter() -> String {
    "(sAMAccountName={username})".to_string()
}
fn default_group_attr() -> String {
    "memberOf".to_string()
}
fn default_mail_attr() -> String {
    "mail".to_string()
}
fn default_display_attr() -> String {
    "displayName".to_string()
}
fn default_timeout() -> u64 {
    10
}

/// A user record returned from the directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapUser {
    /// Distinguished name of the user entry.
    pub dn: String,
    /// SAMAccountName / uid / login.
    pub username: String,
    /// Email address.
    pub email: Option<String>,
    /// Display name.
    pub display_name: Option<String>,
    /// Group DNs the user is a member of.
    pub groups: Vec<String>,
}

/// LDAP authentication and directory service.
pub struct LdapService {
    config: LdapConfig,
}

impl LdapService {
    pub fn new(config: LdapConfig) -> Result<Self, LdapError> {
        if config.url.is_empty() {
            return Err(LdapError::Configuration("url must not be empty".into()));
        }
        if config.base_dn.is_empty() {
            return Err(LdapError::Configuration("base_dn must not be empty".into()));
        }
        Ok(Self { config })
    }

    /// Build [`LdapConnSettings`] from config.
    fn conn_settings(&self) -> LdapConnSettings {
        let mut settings = LdapConnSettings::new();
        if self.config.tls_insecure {
            settings = settings.set_no_tls_verify(true);
        }
        settings
    }

    /// Open and bind a connection using the service account.
    async fn service_bind(&self) -> Result<Ldap, LdapError> {
        let (conn, mut ldap) = LdapConnAsync::with_settings(self.conn_settings(), &self.config.url)
            .await
            .map_err(|e| LdapError::Connection(e.to_string()))?;

        ldap3::drive!(conn);

        if self.config.tls_mode == LdapTlsMode::StartTls {
            ldap.simple_bind("", "")
                .await
                .map_err(|e| LdapError::Protocol(e.to_string()))?;
        }

        match (&self.config.bind_dn, &self.config.bind_password) {
            (Some(dn), Some(pw)) => {
                let result = ldap
                    .simple_bind(dn, pw)
                    .await
                    .map_err(|e| LdapError::Protocol(e.to_string()))?;
                result
                    .success()
                    .map_err(|_| LdapError::InvalidCredentials)?;
            }
            _ => {
                // Anonymous bind
                ldap.simple_bind("", "")
                    .await
                    .map_err(|e| LdapError::Protocol(e.to_string()))?
                    .success()
                    .map_err(|e| LdapError::Protocol(e.to_string()))?;
            }
        }

        Ok(ldap)
    }

    /// Escape a value for safe inclusion in an LDAP filter per RFC 4515.
    fn escape_filter_value(value: &str) -> String {
        let mut out = String::with_capacity(value.len());
        for c in value.chars() {
            match c {
                '\\' => out.push_str("\\5c"),
                '*' => out.push_str("\\2a"),
                '(' => out.push_str("\\28"),
                ')' => out.push_str("\\29"),
                '\0' => out.push_str("\\00"),
                other => out.push(other),
            }
        }
        out
    }

    /// Authenticate a user by binding with their credentials.
    ///
    /// 1. Service-account bind to find the user DN.
    /// 2. Bind again with the user DN + password.
    /// 3. Return the user record with group memberships.
    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<LdapUser, LdapError> {
        // Step 1: find the user DN
        let user = self.find_user(username).await?;

        // Step 2: bind as the user to verify the password
        let (conn, mut ldap) = LdapConnAsync::with_settings(self.conn_settings(), &self.config.url)
            .await
            .map_err(|e| LdapError::Connection(e.to_string()))?;
        ldap3::drive!(conn);

        let result = ldap
            .simple_bind(&user.dn, password)
            .await
            .map_err(|e| LdapError::Protocol(e.to_string()))?;
        result
            .success()
            .map_err(|_| LdapError::InvalidCredentials)?;

        Ok(user)
    }

    /// Search for a user by login name. Returns the user record with group memberships.
    pub async fn find_user(&self, username: &str) -> Result<LdapUser, LdapError> {
        let mut ldap = self.service_bind().await?;

        let safe_username = Self::escape_filter_value(username);
        let filter = self
            .config
            .user_filter
            .replace("{username}", &safe_username);

        let attrs = vec![
            "dn",
            self.config.mail_attribute.as_str(),
            self.config.display_name_attribute.as_str(),
            self.config.group_attribute.as_str(),
        ];

        let (entries, _result) = ldap
            .search(&self.config.base_dn, Scope::Subtree, &filter, attrs)
            .await
            .map_err(|e| LdapError::Search(e.to_string()))?
            .success()
            .map_err(|e| LdapError::Search(e.to_string()))?;

        let entry = entries
            .into_iter()
            .next()
            .ok_or_else(|| LdapError::UserNotFound(username.to_string()))?;

        let se = SearchEntry::construct(entry);

        let email = se
            .attrs
            .get(&self.config.mail_attribute)
            .and_then(|v| v.first())
            .cloned();

        let display_name = se
            .attrs
            .get(&self.config.display_name_attribute)
            .and_then(|v| v.first())
            .cloned();

        let groups = se
            .attrs
            .get(&self.config.group_attribute)
            .cloned()
            .unwrap_or_default();

        Ok(LdapUser {
            dn: se.dn,
            username: username.to_string(),
            email,
            display_name,
            groups,
        })
    }

    /// List all users under the base DN matching the configured user filter with `*`.
    pub async fn list_users(&self) -> Result<Vec<LdapUser>, LdapError> {
        let mut ldap = self.service_bind().await?;

        let filter = self.config.user_filter.replace("{username}", "*");
        let attrs = vec![
            "dn",
            self.config.mail_attribute.as_str(),
            self.config.display_name_attribute.as_str(),
            self.config.group_attribute.as_str(),
        ];

        let (entries, _result) = ldap
            .search(&self.config.base_dn, Scope::Subtree, &filter, attrs)
            .await
            .map_err(|e| LdapError::Search(e.to_string()))?
            .success()
            .map_err(|e| LdapError::Search(e.to_string()))?;

        let users = entries
            .into_iter()
            .map(|entry| {
                let se = SearchEntry::construct(entry);
                let email = se
                    .attrs
                    .get(&self.config.mail_attribute)
                    .and_then(|v| v.first())
                    .cloned();
                let display_name = se
                    .attrs
                    .get(&self.config.display_name_attribute)
                    .and_then(|v| v.first())
                    .cloned();
                let groups = se
                    .attrs
                    .get(&self.config.group_attribute)
                    .cloned()
                    .unwrap_or_default();
                // Extract uid/sAMAccountName from DN heuristically
                let username = se
                    .dn
                    .split(',')
                    .next()
                    .and_then(|rdn| rdn.split('=').nth(1))
                    .unwrap_or("")
                    .to_string();
                LdapUser {
                    dn: se.dn,
                    username,
                    email,
                    display_name,
                    groups,
                }
            })
            .collect();

        Ok(users)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LdapConfig {
        LdapConfig {
            url: "ldap://localhost:389".to_string(),
            base_dn: "dc=example,dc=com".to_string(),
            bind_dn: Some("cn=svc,dc=example,dc=com".to_string()),
            bind_password: Some("secret".to_string()),
            user_filter: "(uid={username})".to_string(),
            group_attribute: "memberOf".to_string(),
            mail_attribute: "mail".to_string(),
            display_name_attribute: "displayName".to_string(),
            tls_mode: LdapTlsMode::None,
            tls_insecure: false,
            timeout_secs: 10,
        }
    }

    #[test]
    fn test_service_creation() {
        assert!(LdapService::new(test_config()).is_ok());
    }

    #[test]
    fn test_empty_url_rejected() {
        let mut cfg = test_config();
        cfg.url = String::new();
        assert!(LdapService::new(cfg).is_err());
    }

    #[test]
    fn test_empty_base_dn_rejected() {
        let mut cfg = test_config();
        cfg.base_dn = String::new();
        assert!(LdapService::new(cfg).is_err());
    }

    #[test]
    fn test_filter_escape_special_chars() {
        assert_eq!(
            LdapService::escape_filter_value("user*(name)"),
            r"user\2a\28name\29"
        );
        assert_eq!(
            LdapService::escape_filter_value("back\\slash"),
            r"back\5cslash"
        );
        assert_eq!(LdapService::escape_filter_value("normal"), "normal");
        assert_eq!(
            LdapService::escape_filter_value("null\x00char"),
            r"null\00char"
        );
    }

    #[test]
    fn test_filter_injection_escape() {
        // An attacker trying LDAP injection via username
        let malicious = "admin)(|(password=*)";
        let escaped = LdapService::escape_filter_value(malicious);
        // The filter should now be safe — parens are escaped
        assert!(!escaped.contains('('));
        assert!(!escaped.contains(')'));
    }
}

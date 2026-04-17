use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub url: String,
    pub key_prefix: String,
    pub default_ttl_secs: u64,
    pub max_connections: u32,
    pub connection_timeout_secs: u64,
    pub response_timeout_secs: u64,
    pub retry_attempts: u32,
    pub retry_delay_ms: u64,
    pub tls: Option<TlsConfig>,
    pub cache_ttls: CacheTtls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub ca_cert_path: Option<String>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub skip_verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheTtls {
    pub repository_metadata_secs: u64,
    pub session_secs: u64,
    pub api_response_secs: u64,
    pub lfs_object_secs: u64,
    pub permission_secs: u64,
    pub user_info_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "redis://127.0.0.1:6379".to_string(),
            key_prefix: "agentic-repos".to_string(),
            default_ttl_secs: 3600,
            max_connections: 10,
            connection_timeout_secs: 5,
            response_timeout_secs: 3,
            retry_attempts: 3,
            retry_delay_ms: 100,
            tls: None,
            cache_ttls: CacheTtls::default(),
        }
    }
}

impl Default for CacheTtls {
    fn default() -> Self {
        Self {
            repository_metadata_secs: 300,
            session_secs: 86400,
            api_response_secs: 60,
            lfs_object_secs: 600,
            permission_secs: 300,
            user_info_secs: 3600,
        }
    }
}

impl CacheConfig {
    pub fn new(url: String) -> Self {
        Self {
            enabled: true,
            url,
            ..Self::default()
        }
    }

    pub fn with_prefix(mut self, prefix: String) -> Self {
        self.key_prefix = prefix;
        self
    }

    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    pub fn with_default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl_secs = ttl.as_secs();
        self
    }

    pub fn default_ttl(&self) -> Duration {
        Duration::from_secs(self.default_ttl_secs)
    }

    pub fn repository_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttls.repository_metadata_secs)
    }

    pub fn session_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttls.session_secs)
    }

    pub fn api_response_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttls.api_response_secs)
    }

    pub fn lfs_object_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttls.lfs_object_secs)
    }

    pub fn permission_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttls.permission_secs)
    }

    pub fn user_info_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttls.user_info_secs)
    }

    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.connection_timeout_secs)
    }

    pub fn response_timeout(&self) -> Duration {
        Duration::from_secs(self.response_timeout_secs)
    }

    pub fn redis_url(&self) -> &str {
        &self.url
    }

    pub fn pool_config(&self) -> deadpool_redis::Config {
        deadpool_redis::Config::from_url(self.url.clone())
    }
}

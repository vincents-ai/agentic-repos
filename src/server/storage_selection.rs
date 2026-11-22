//! Choose the storage backend from `refs/adp/config/server`.
//!
//! `Server::new` used to build a [`LocalStorage`] unconditionally. The server
//! config already carries `storage_backend` and the S3 settings, so this module
//! turns that config into one backend, or into a named startup error.
//!
//! Rules:
//!
//! - `local` behaves exactly as before: the root is the process's storage root.
//! - `s3` needs a bucket, an endpoint and both credentials. A missing setting
//!   is an error; there is no silent fallback to local, because a fallback
//!   writes repository data to a second place and hides the loss.
//! - S3 clients use path-style addressing. SeaweedFS, MinIO and Garage do not
//!   serve virtual-hosted-style buckets, and AWS S3 accepts path style too.
//!
//! Credentials are read from the environment only. They are never written into
//! the git ref that is replicated with the repositories.

use crate::adp::config::ServerConfig;
use crate::storage::local::LocalStorage;
use crate::storage::s3::S3Storage;
use crate::storage::StorageBackend;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Region used when the config does not name one. S3-compatible stores such
/// as SeaweedFS accept any non-empty value.
pub const DEFAULT_S3_REGION: &str = "us-east-1";

/// Environment variables that carry S3 credentials. The standard AWS names
/// are used so an existing environment file works unchanged.
pub const S3_ACCESS_KEY_ENV: &str = "AWS_ACCESS_KEY_ID";
pub const S3_SECRET_KEY_ENV: &str = "AWS_SECRET_ACCESS_KEY";

/// S3 credentials supplied by the process environment.
#[derive(Clone, PartialEq, Eq)]
pub struct S3Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
}

impl std::fmt::Debug for S3Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the secret, including in an error or a log line.
        f.debug_struct("S3Credentials")
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .finish()
    }
}

impl S3Credentials {
    /// Read both credentials from the environment, or name the missing one.
    pub fn from_env() -> Result<Self, StorageInitError> {
        let access_key_id =
            non_empty_env(S3_ACCESS_KEY_ENV).ok_or(StorageInitError::MissingS3AccessKey)?;
        let secret_access_key =
            non_empty_env(S3_SECRET_KEY_ENV).ok_or(StorageInitError::MissingS3SecretKey)?;
        Ok(Self {
            access_key_id,
            secret_access_key,
        })
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// A startup error caused by the storage configuration.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StorageInitError {
    #[error(
        "storage backend \"s3\" is selected but s3_bucket is missing; \
         refusing to start (no local fallback)"
    )]
    MissingS3Bucket,

    #[error(
        "storage backend \"s3\" is selected but s3_endpoint is missing; \
         refusing to start (no local fallback)"
    )]
    MissingS3Endpoint,

    #[error(
        "storage backend \"s3\" is selected but {S3_ACCESS_KEY_ENV} is missing; \
         refusing to start (no local fallback)"
    )]
    MissingS3AccessKey,

    #[error(
        "storage backend \"s3\" is selected but {S3_SECRET_KEY_ENV} is missing; \
         refusing to start (no local fallback)"
    )]
    MissingS3SecretKey,

    #[error("storage backend \"{0}\" is not supported; expected \"local\" or \"s3\"")]
    UnsupportedBackend(String),
}

/// The validated backend choice. This is what the tests compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageSelection {
    Local,
    S3 {
        bucket: String,
        region: String,
        endpoint: String,
        /// Always true; kept explicit so the value is visible in logs and tests.
        path_style: bool,
    },
}

/// Validate the config and choose a backend.
///
/// This is pure: it reads `config` and the credentials it is given, never the
/// environment. [`build_storage`] is the environment-aware wrapper.
pub fn select_storage_backend(
    config: &ServerConfig,
    credentials: Option<&S3Credentials>,
) -> Result<StorageSelection, StorageInitError> {
    match config.storage_backend.trim() {
        "local" => Ok(StorageSelection::Local),
        "s3" => {
            let bucket = non_empty(&config.s3_bucket).ok_or(StorageInitError::MissingS3Bucket)?;
            let endpoint =
                non_empty(&config.s3_endpoint).ok_or(StorageInitError::MissingS3Endpoint)?;
            let credentials = credentials.ok_or(StorageInitError::MissingS3AccessKey)?;
            if credentials.access_key_id.is_empty() {
                return Err(StorageInitError::MissingS3AccessKey);
            }
            if credentials.secret_access_key.is_empty() {
                return Err(StorageInitError::MissingS3SecretKey);
            }
            let region =
                non_empty(&config.s3_region).unwrap_or_else(|| DEFAULT_S3_REGION.to_string());
            Ok(StorageSelection::S3 {
                bucket,
                region,
                endpoint,
                path_style: true,
            })
        }
        other => Err(StorageInitError::UnsupportedBackend(other.to_string())),
    }
}

/// Build the backend named by `config`.
///
/// `local_root` is the process storage root (`AGENTIC_REPOS_STORAGE_ROOT` or
/// `./repositories`). It is the local backend root, and it is also where the
/// system repo lives while the server config is read.
pub fn build_storage(
    config: &ServerConfig,
    local_root: PathBuf,
) -> Result<Arc<dyn StorageBackend>, StorageInitError> {
    let credentials = if config.storage_backend.trim() == "s3" {
        Some(S3Credentials::from_env()?)
    } else {
        None
    };

    match select_storage_backend(config, credentials.as_ref())? {
        StorageSelection::Local => {
            tracing::info!(root = %local_root.display(), "storage backend: local");
            Ok(Arc::new(LocalStorage::new(local_root)))
        }
        StorageSelection::S3 {
            bucket,
            region,
            endpoint,
            path_style,
        } => {
            let credentials = credentials.expect("s3 selection requires credentials");
            tracing::info!(
                bucket = %bucket,
                region = %region,
                endpoint = %endpoint,
                path_style,
                "storage backend: s3"
            );
            let client = s3_client(&region, &endpoint, &credentials, path_style);
            Ok(Arc::new(S3Storage::new(client, bucket)))
        }
    }
}

fn s3_client(
    region: &str,
    endpoint: &str,
    credentials: &S3Credentials,
    path_style: bool,
) -> aws_sdk_s3::Client {
    let config = aws_sdk_s3::config::Builder::new()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(region.to_string()))
        .endpoint_url(endpoint)
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            credentials.access_key_id.clone(),
            credentials.secret_access_key.clone(),
            None,
            None,
            "agentic-repos-static",
        ))
        .force_path_style(path_style)
        .build();
    aws_sdk_s3::Client::from_conf(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s3_config() -> ServerConfig {
        ServerConfig {
            storage_backend: "s3".to_string(),
            s3_bucket: Some("adp-repos".to_string()),
            s3_region: Some("us-east-1".to_string()),
            s3_endpoint: Some("http://127.0.0.1:8333".to_string()),
            ..ServerConfig::default()
        }
    }

    fn credentials() -> S3Credentials {
        S3Credentials {
            access_key_id: "test-key".to_string(),
            secret_access_key: "test-secret".to_string(),
        }
    }

    #[test]
    fn default_config_selects_local() {
        let config = ServerConfig::default();
        assert_eq!(
            select_storage_backend(&config, None).unwrap(),
            StorageSelection::Local
        );
    }

    #[test]
    fn complete_s3_config_selects_s3_with_path_style() {
        let config = s3_config();
        assert_eq!(
            select_storage_backend(&config, Some(&credentials())).unwrap(),
            StorageSelection::S3 {
                bucket: "adp-repos".to_string(),
                region: "us-east-1".to_string(),
                endpoint: "http://127.0.0.1:8333".to_string(),
                path_style: true,
            }
        );
    }

    #[test]
    fn s3_without_region_defaults_to_us_east_1() {
        let mut config = s3_config();
        config.s3_region = None;
        match select_storage_backend(&config, Some(&credentials())).unwrap() {
            StorageSelection::S3 { region, .. } => assert_eq!(region, DEFAULT_S3_REGION),
            other => panic!("expected s3, got {other:?}"),
        }
    }

    #[test]
    fn incomplete_s3_config_refuses_by_name() {
        let cases = [
            ("s3_bucket", StorageInitError::MissingS3Bucket),
            ("s3_endpoint", StorageInitError::MissingS3Endpoint),
        ];
        for (field, expected) in cases {
            let mut config = s3_config();
            match field {
                "s3_bucket" => config.s3_bucket = None,
                "s3_endpoint" => config.s3_endpoint = None,
                _ => unreachable!(),
            }
            let error = select_storage_backend(&config, Some(&credentials())).unwrap_err();
            assert_eq!(error, expected);
            assert!(error.to_string().contains("refusing to start"));
        }
    }

    #[test]
    fn missing_credentials_refuse_by_name() {
        let config = s3_config();
        assert_eq!(
            select_storage_backend(&config, None).unwrap_err(),
            StorageInitError::MissingS3AccessKey
        );

        let empty_access = S3Credentials {
            access_key_id: String::new(),
            secret_access_key: "test-secret".to_string(),
        };
        assert_eq!(
            select_storage_backend(&config, Some(&empty_access)).unwrap_err(),
            StorageInitError::MissingS3AccessKey
        );

        let empty_secret = S3Credentials {
            access_key_id: "test-key".to_string(),
            secret_access_key: String::new(),
        };
        assert_eq!(
            select_storage_backend(&config, Some(&empty_secret)).unwrap_err(),
            StorageInitError::MissingS3SecretKey
        );
    }

    #[test]
    fn unknown_backend_is_named() {
        let mut config = ServerConfig::default();
        config.storage_backend = "garage".to_string();
        let error = select_storage_backend(&config, None).unwrap_err();
        assert_eq!(
            error,
            StorageInitError::UnsupportedBackend("garage".to_string())
        );
        assert!(error.to_string().contains("garage"));
    }

    #[test]
    fn credentials_debug_redacts_secrets() {
        let rendered = format!("{:?}", credentials());
        assert!(!rendered.contains("test-key"));
        assert!(!rendered.contains("test-secret"));
    }
}

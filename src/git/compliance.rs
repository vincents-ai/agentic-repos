use crate::storage::StorageBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::debug;

/// Compliance tag types for data classification
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ComplianceTag {
    #[serde(rename = "GDPR")]
    Gdpr,
    #[serde(rename = "HIPAA")]
    Hipaa,
    #[serde(rename = "SOC2")]
    Soc2,
    #[serde(rename = "ISO27001")]
    Iso27001,
    #[serde(rename = "PUBLIC")]
    Public,
}

impl std::fmt::Display for ComplianceTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComplianceTag::Gdpr => write!(f, "GDPR"),
            ComplianceTag::Hipaa => write!(f, "HIPAA"),
            ComplianceTag::Soc2 => write!(f, "SOC2"),
            ComplianceTag::Iso27001 => write!(f, "ISO27001"),
            ComplianceTag::Public => write!(f, "PUBLIC"),
        }
    }
}

impl std::str::FromStr for ComplianceTag {
    type Err = ComplianceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GDPR" => Ok(ComplianceTag::Gdpr),
            "HIPAA" => Ok(ComplianceTag::Hipaa),
            "SOC2" | "SOC_2" => Ok(ComplianceTag::Soc2),
            "ISO27001" | "ISO_27001" => Ok(ComplianceTag::Iso27001),
            "PUBLIC" => Ok(ComplianceTag::Public),
            _ => Err(ComplianceError::UnknownTag(s.to_string())),
        }
    }
}

/// S3 bucket configuration with compliance settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3BucketConfig {
    pub name: String,
    pub region: String,
    #[serde(default)]
    pub compliance: Vec<ComplianceTag>,
    #[serde(default)]
    pub encryption: String, // "AES256" or "aws:kms"
    #[serde(default)]
    pub kms_key_id: Option<String>,
    #[serde(default)]
    pub replication: Option<ReplicationConfig>,
    #[serde(default)]
    pub retention: Option<RetentionConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    pub enabled: bool,
    pub target_region: String,
    pub target_bucket: String,
    #[serde(default)]
    pub role_arn: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    pub mode: RetentionMode,
    pub days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum RetentionMode {
    #[serde(rename = "GOVERNANCE")]
    Governance,
    #[serde(rename = "COMPLIANCE")]
    Compliance,
}

/// Repository compliance configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepositoryCompliance {
    pub tags: Vec<ComplianceTag>,
    #[serde(default)]
    pub data_classification: DataClassification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "level")]
pub enum DataClassification {
    #[serde(rename = "public")]
    Public,
    #[serde(rename = "internal")]
    Internal,
    #[serde(rename = "confidential")]
    Confidential,
    #[serde(rename = "restricted")]
    Restricted,
}

impl Default for DataClassification {
    fn default() -> Self {
        DataClassification::Internal
    }
}

/// Configuration for compliance-aware storage routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceStorageConfig {
    pub default_bucket: String,
    pub buckets: Vec<S3BucketConfig>,
    /// Mapping from compliance tag to bucket name
    #[serde(default)]
    pub compliance_routing: HashMap<String, String>,
    /// Default routing for unknown tags
    #[serde(default)]
    pub fallback_bucket: String,
}

impl Default for ComplianceStorageConfig {
    fn default() -> Self {
        Self {
            default_bucket: "default".to_string(),
            buckets: vec![S3BucketConfig {
                name: "default".to_string(),
                region: "us-east-1".to_string(),
                compliance: vec![],
                encryption: "AES256".to_string(),
                kms_key_id: None,
                replication: None,
                retention: None,
            }],
            compliance_routing: HashMap::new(),
            fallback_bucket: "default".to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ComplianceError {
    #[error("Unknown compliance tag: {0}")]
    UnknownTag(String),
    #[error("No bucket found for compliance tags: {0:?}")]
    NoMatchingBucket(Vec<ComplianceTag>),
    #[error("Bucket configuration error: {0}")]
    ConfigError(String),
    #[error("Storage error: {0}")]
    StorageError(#[from] crate::storage::StorageError),
}

pub type ComplianceResult<T> = std::result::Result<T, ComplianceError>;

/// Service for routing repositories to compliance-aware S3 buckets
#[allow(dead_code)]
pub struct ComplianceAwareStorage<S: StorageBackend> {
    config: ComplianceStorageConfig,
    storage: Arc<S>,
    bucket_map: HashMap<String, S3BucketConfig>,
}

impl<S: StorageBackend> ComplianceAwareStorage<S> {
    pub fn new(storage: Arc<S>, config: ComplianceStorageConfig) -> Self {
        let bucket_map: HashMap<String, S3BucketConfig> = config
            .buckets
            .iter()
            .map(|b| (b.name.clone(), b.clone()))
            .collect();

        Self {
            config,
            storage,
            bucket_map,
        }
    }

    /// Get the appropriate bucket for a repository based on its compliance tags
    pub fn get_bucket_for_repository(
        &self,
        compliance: &RepositoryCompliance,
    ) -> ComplianceResult<&S3BucketConfig> {
        // If no compliance tags, use default bucket
        if compliance.tags.is_empty() {
            return self
                .bucket_map
                .get(&self.config.default_bucket)
                .ok_or_else(|| {
                    ComplianceError::ConfigError("Default bucket not found".to_string())
                });
        }

        // Try to find a bucket matching the compliance tags
        // Priority: find exact match for all tags, then partial match
        for tag in &compliance.tags {
            let tag_str = tag.to_string();
            if let Some(bucket_name) = self.config.compliance_routing.get(&tag_str) {
                if let Some(bucket) = self.bucket_map.get(bucket_name) {
                    debug!(
                        "Routing repository to bucket '{}' via compliance tag '{}'",
                        bucket.name, tag_str
                    );
                    return Ok(bucket);
                }
            }
        }

        // Fall back to matching bucket by compliance
        for bucket in self.config.buckets.iter() {
            let has_matching_tag = compliance
                .tags
                .iter()
                .any(|tag| bucket.compliance.contains(tag));

            if has_matching_tag {
                debug!(
                    "Routing repository to bucket '{}' (compliance match)",
                    bucket.name
                );
                return Ok(bucket);
            }
        }

        // Final fallback
        let fallback = self
            .bucket_map
            .get(&self.config.fallback_bucket)
            .or(self.bucket_map.get(&self.config.default_bucket))
            .ok_or_else(|| {
                ComplianceError::ConfigError("No fallback bucket configured".to_string())
            })?;

        debug!("Routing repository to fallback bucket '{}'", fallback.name);
        Ok(fallback)
    }

    /// Get bucket by name
    pub fn get_bucket_by_name(&self, name: &str) -> Option<&S3BucketConfig> {
        self.bucket_map.get(name)
    }

    /// Check if a bucket supports a specific compliance tag
    pub fn bucket_supports_compliance(&self, bucket_name: &str, tag: &ComplianceTag) -> bool {
        self.bucket_map
            .get(bucket_name)
            .map(|b| b.compliance.contains(tag))
            .unwrap_or(false)
    }

    /// List all configured buckets
    pub fn list_buckets(&self) -> Vec<&S3BucketConfig> {
        self.bucket_map.values().collect()
    }

    /// Get encryption settings for a bucket
    pub fn get_encryption_config(&self, bucket_name: &str) -> (String, Option<String>) {
        self.bucket_map
            .get(bucket_name)
            .map(|b| (b.encryption.clone(), b.kms_key_id.clone()))
            .unwrap_or(("AES256".to_string(), None))
    }

    /// Get retention configuration for a bucket
    pub fn get_retention_config(&self, bucket_name: &str) -> Option<&RetentionConfig> {
        self.bucket_map
            .get(bucket_name)
            .and_then(|b| b.retention.as_ref())
    }
}

/// Builder for compliance storage configuration
pub struct ComplianceStorageConfigBuilder {
    config: ComplianceStorageConfig,
}

impl ComplianceStorageConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: ComplianceStorageConfig::default(),
        }
    }

    pub fn default_bucket(mut self, name: impl Into<String>) -> Self {
        self.config.default_bucket = name.into();
        self
    }

    pub fn add_bucket(mut self, bucket: S3BucketConfig) -> Self {
        self.config.buckets.push(bucket);
        self
    }

    pub fn add_compliance_routing(
        mut self,
        tag: ComplianceTag,
        bucket_name: impl Into<String>,
    ) -> Self {
        self.config
            .compliance_routing
            .insert(tag.to_string(), bucket_name.into());
        self
    }

    pub fn fallback_bucket(mut self, name: impl Into<String>) -> Self {
        self.config.fallback_bucket = name.into();
        self
    }

    pub fn build(self) -> ComplianceStorageConfig {
        self.config
    }
}

impl Default for ComplianceStorageConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility function to create GDPR-compliant bucket configuration
pub fn create_gdpr_bucket_config(region: &str, name: &str) -> S3BucketConfig {
    S3BucketConfig {
        name: name.to_string(),
        region: region.to_string(),
        compliance: vec![ComplianceTag::Gdpr],
        encryption: "aws:kms".to_string(),
        kms_key_id: None, // Set from environment
        replication: None,
        retention: Some(RetentionConfig {
            mode: RetentionMode::Compliance,
            days: 2555, // 7 years for GDPR
        }),
    }
}

/// Utility function to create public bucket configuration
pub fn create_public_bucket_config(region: &str, name: &str) -> S3BucketConfig {
    S3BucketConfig {
        name: name.to_string(),
        region: region.to_string(),
        compliance: vec![ComplianceTag::Public],
        encryption: "AES256".to_string(),
        kms_key_id: None,
        replication: Some(ReplicationConfig {
            enabled: true,
            target_region: "us-west-2".to_string(),
            target_bucket: format!("{}-backup", name),
            role_arn: None,
        }),
        retention: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local::LocalStorage;

    #[test]
    fn test_compliance_tag_parsing() {
        assert_eq!(
            "GDPR".parse::<ComplianceTag>().unwrap(),
            ComplianceTag::Gdpr
        );
        assert_eq!(
            "HIPAA".parse::<ComplianceTag>().unwrap(),
            ComplianceTag::Hipaa
        );
        assert_eq!(
            "SOC2".parse::<ComplianceTag>().unwrap(),
            ComplianceTag::Soc2
        );
        assert!("UNKNOWN".parse::<ComplianceTag>().is_err());
    }

    #[test]
    fn test_bucket_routing() {
        let config = ComplianceStorageConfigBuilder::new()
            .add_bucket(create_gdpr_bucket_config("eu-west-1", "gdpr-eu"))
            .add_bucket(create_public_bucket_config("us-east-1", "public-us"))
            .add_compliance_routing(ComplianceTag::Gdpr, "gdpr-eu")
            .add_compliance_routing(ComplianceTag::Public, "public-us")
            .build();

        let storage = Arc::new(LocalStorage::new(std::path::PathBuf::from("/tmp")));
        let compliance_storage = ComplianceAwareStorage::new(storage, config);

        // Test GDPR routing
        let gdpr_compliance = RepositoryCompliance {
            tags: vec![ComplianceTag::Gdpr],
            data_classification: DataClassification::Confidential,
        };
        let bucket = compliance_storage
            .get_bucket_for_repository(&gdpr_compliance)
            .unwrap();
        assert_eq!(bucket.name, "gdpr-eu");

        // Test public routing
        let public_compliance = RepositoryCompliance {
            tags: vec![ComplianceTag::Public],
            data_classification: DataClassification::Public,
        };
        let bucket = compliance_storage
            .get_bucket_for_repository(&public_compliance)
            .unwrap();
        assert_eq!(bucket.name, "public-us");
    }
}

use crate::features::FeatureFlags;
use config::{Config as ConfigBuilder, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::from_file("config.yaml")
    }

    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path_without_ext = path.trim_end_matches(".yaml").trim_end_matches(".yml");

        let settings = ConfigBuilder::builder()
            .add_source(File::with_name(path_without_ext).required(false))
            .build();

        match settings {
            Ok(config) => {
                println!("Loading config from {}", path);
                let app_config: AppConfig = config.try_deserialize()?;
                println!(
                    "Config loaded: host={}, port={}",
                    app_config.server.host, app_config.server.port
                );
                Ok(app_config)
            }
            Err(e) => {
                println!(
                    "Config file {} not found or invalid ({}), using defaults",
                    path, e
                );
                Ok(Self::default())
            }
        }
    }

    pub fn get_feature_flags(&self) -> FeatureFlags {
        match &self.features {
            Some(config_flags) => {
                let env_flags = FeatureFlags::load_from_env();
                let config_feature_flags = FeatureFlags::load_from_config(config_flags);

                FeatureFlags {
                    web_ui: env_flags.web_ui || config_feature_flags.web_ui,
                    ldap_auth: env_flags.ldap_auth || config_feature_flags.ldap_auth,
                    saml_auth: env_flags.saml_auth || config_feature_flags.saml_auth,
                    oauth_auth: env_flags.oauth_auth || config_feature_flags.oauth_auth,
                    audit_logging: env_flags.audit_logging || config_feature_flags.audit_logging,
                    audit_tracking: env_flags.audit_tracking || config_feature_flags.audit_tracking,
                    repo_templates: env_flags.repo_templates || config_feature_flags.repo_templates,
                    webhooks: env_flags.webhooks || config_feature_flags.webhooks,
                    multi_region: env_flags.multi_region || config_feature_flags.multi_region,
                    horizontal_scaling: env_flags.horizontal_scaling
                        || config_feature_flags.horizontal_scaling,
                    redis_cache: env_flags.redis_cache || config_feature_flags.redis_cache,
                    load_balancing: env_flags.load_balancing || config_feature_flags.load_balancing,
                    monitoring: env_flags.monitoring || config_feature_flags.monitoring,
                    security_scanning: env_flags.security_scanning
                        || config_feature_flags.security_scanning,
                    soc2_compliance: env_flags.soc2_compliance
                        || config_feature_flags.soc2_compliance,
                    gdpr_compliance: env_flags.gdpr_compliance
                        || config_feature_flags.gdpr_compliance,
                    hipaa_compliance: env_flags.hipaa_compliance
                        || config_feature_flags.hipaa_compliance,
                    rbac: env_flags.rbac || config_feature_flags.rbac,
                    backup_recovery: env_flags.backup_recovery
                        || config_feature_flags.backup_recovery,
                    github_actions: env_flags.github_actions || config_feature_flags.github_actions,
                    gitlab_ci: env_flags.gitlab_ci || config_feature_flags.gitlab_ci,
                    jenkins: env_flags.jenkins || config_feature_flags.jenkins,
                    graphql_api: env_flags.graphql_api || config_feature_flags.graphql_api,
                    advanced_webhooks: env_flags.advanced_webhooks
                        || config_feature_flags.advanced_webhooks,
                    analytics: env_flags.analytics || config_feature_flags.analytics,
                }
            }
            None => FeatureFlags::load_from_env(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            features: None,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8090,
        }
    }
}

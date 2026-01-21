use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub web_ui: bool,
    pub ldap_auth: bool,
    pub saml_auth: bool,
    pub oauth_auth: bool,
    pub audit_logging: bool,
    pub audit_tracking: bool,
    pub repo_templates: bool,
    pub webhooks: bool,
    pub multi_region: bool,
    pub horizontal_scaling: bool,
    pub redis_cache: bool,
    pub load_balancing: bool,
    pub monitoring: bool,
    pub security_scanning: bool,
    pub soc2_compliance: bool,
    pub gdpr_compliance: bool,
    pub hipaa_compliance: bool,
    pub rbac: bool,
    pub backup_recovery: bool,
    pub github_actions: bool,
    pub gitlab_ci: bool,
    pub jenkins: bool,
    pub graphql_api: bool,
    pub advanced_webhooks: bool,
    pub analytics: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            web_ui: false,
            ldap_auth: false,
            saml_auth: false,
            oauth_auth: false,
            audit_logging: false,
            audit_tracking: false,
            repo_templates: false,
            webhooks: false,
            multi_region: false,
            horizontal_scaling: false,
            redis_cache: false,
            load_balancing: false,
            monitoring: false,
            security_scanning: false,
            soc2_compliance: false,
            gdpr_compliance: false,
            hipaa_compliance: false,
            rbac: false,
            backup_recovery: false,
            github_actions: false,
            gitlab_ci: false,
            jenkins: false,
            graphql_api: false,
            advanced_webhooks: false,
            analytics: false,
        }
    }
}

impl FeatureFlags {
    pub fn load_from_env() -> Self {
        Self {
            web_ui: std::env::var("FEATURE_WEB_UI").unwrap_or_default() == "true",
            ldap_auth: std::env::var("FEATURE_LDAP_AUTH").unwrap_or_default() == "true",
            saml_auth: std::env::var("FEATURE_SAML_AUTH").unwrap_or_default() == "true",
            oauth_auth: std::env::var("FEATURE_OAUTH_AUTH").unwrap_or_default() == "true",
            audit_logging: std::env::var("FEATURE_AUDIT_LOGGING").unwrap_or_default() == "true",
            audit_tracking: std::env::var("FEATURE_AUDIT_TRACKING").unwrap_or_default() == "true",
            repo_templates: std::env::var("FEATURE_REPO_TEMPLATES").unwrap_or_default() == "true",
            webhooks: std::env::var("FEATURE_WEBHOOKS").unwrap_or_default() == "true",
            multi_region: std::env::var("FEATURE_MULTI_REGION").unwrap_or_default() == "true",
            horizontal_scaling: std::env::var("FEATURE_HORIZONTAL_SCALING").unwrap_or_default()
                == "true",
            redis_cache: std::env::var("FEATURE_REDIS_CACHE").unwrap_or_default() == "true",
            load_balancing: std::env::var("FEATURE_LOAD_BALANCING").unwrap_or_default() == "true",
            monitoring: std::env::var("FEATURE_MONITORING").unwrap_or_default() == "true",
            security_scanning: std::env::var("FEATURE_SECURITY_SCANNING").unwrap_or_default()
                == "true",
            soc2_compliance: std::env::var("FEATURE_SOC2_COMPLIANCE").unwrap_or_default() == "true",
            gdpr_compliance: std::env::var("FEATURE_GDPR_COMPLIANCE").unwrap_or_default() == "true",
            hipaa_compliance: std::env::var("FEATURE_HIPAA_COMPLIANCE").unwrap_or_default()
                == "true",
            rbac: std::env::var("FEATURE_RBAC").unwrap_or_default() == "true",
            backup_recovery: std::env::var("FEATURE_BACKUP_RECOVERY").unwrap_or_default() == "true",
            github_actions: std::env::var("FEATURE_GITHUB_ACTIONS").unwrap_or_default() == "true",
            gitlab_ci: std::env::var("FEATURE_GITLAB_CI").unwrap_or_default() == "true",
            jenkins: std::env::var("FEATURE_JENKINS").unwrap_or_default() == "true",
            graphql_api: std::env::var("FEATURE_GRAPHQL_API").unwrap_or_default() == "true",
            advanced_webhooks: std::env::var("FEATURE_ADVANCED_WEBHOOKS").unwrap_or_default()
                == "true",
            analytics: std::env::var("FEATURE_ANALYTICS").unwrap_or_default() == "true",
        }
    }

    pub fn load_from_config(config: &HashMap<String, bool>) -> Self {
        Self {
            web_ui: *config.get("web_ui").unwrap_or(&false),
            ldap_auth: *config.get("ldap_auth").unwrap_or(&false),
            saml_auth: *config.get("saml_auth").unwrap_or(&false),
            oauth_auth: *config.get("oauth_auth").unwrap_or(&false),
            audit_logging: *config.get("audit_logging").unwrap_or(&false),
            audit_tracking: *config.get("audit_tracking").unwrap_or(&false),
            repo_templates: *config.get("repo_templates").unwrap_or(&false),
            webhooks: *config.get("webhooks").unwrap_or(&false),
            multi_region: *config.get("multi_region").unwrap_or(&false),
            horizontal_scaling: *config.get("horizontal_scaling").unwrap_or(&false),
            redis_cache: *config.get("redis_cache").unwrap_or(&false),
            load_balancing: *config.get("load_balancing").unwrap_or(&false),
            monitoring: *config.get("monitoring").unwrap_or(&false),
            security_scanning: *config.get("security_scanning").unwrap_or(&false),
            soc2_compliance: *config.get("soc2_compliance").unwrap_or(&false),
            gdpr_compliance: *config.get("gdpr_compliance").unwrap_or(&false),
            hipaa_compliance: *config.get("hipaa_compliance").unwrap_or(&false),
            rbac: *config.get("rbac").unwrap_or(&false),
            backup_recovery: *config.get("backup_recovery").unwrap_or(&false),
            github_actions: *config.get("github_actions").unwrap_or(&false),
            gitlab_ci: *config.get("gitlab_ci").unwrap_or(&false),
            jenkins: *config.get("jenkins").unwrap_or(&false),
            graphql_api: *config.get("graphql_api").unwrap_or(&false),
            advanced_webhooks: *config.get("advanced_webhooks").unwrap_or(&false),
            analytics: *config.get("analytics").unwrap_or(&false),
        }
    }

    pub fn is_commercial_feature_enabled(&self) -> bool {
        self.web_ui
            || self.ldap_auth
            || self.saml_auth
            || self.oauth_auth
            || self.audit_logging
            || self.audit_tracking
            || self.repo_templates
            || self.webhooks
            || self.multi_region
            || self.horizontal_scaling
            || self.redis_cache
            || self.load_balancing
            || self.monitoring
            || self.security_scanning
            || self.soc2_compliance
            || self.gdpr_compliance
            || self.hipaa_compliance
            || self.rbac
            || self.backup_recovery
            || self.github_actions
            || self.gitlab_ci
            || self.jenkins
            || self.graphql_api
            || self.advanced_webhooks
            || self.analytics
    }
}

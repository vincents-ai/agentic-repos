use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Retention policy rules for repository cleanup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub version: String,
    pub rules: Vec<RetentionRule>,
    #[serde(default)]
    pub global_settings: GlobalRetentionSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionRule {
    pub name: String,
    pub pattern: String, // Git ref pattern (e.g., "refs/heads/feature/**")
    pub retention: RetentionConfig,
    #[serde(default)]
    pub notify_before_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum RetentionConfig {
    #[serde(rename = "permanent")]
    Permanent,
    #[serde(rename = "max_age")]
    MaxAge {
        days: u32,
        #[serde(default)]
        auto_delete: bool,
    },
    #[serde(rename = "min_versions")]
    MinVersions {
        count: u32,
        #[serde(default)]
        keep_tags: bool,
    },
    #[serde(rename = "archive")]
    Archive {
        #[serde(default)]
        archive_ref: String,
        #[serde(default)]
        archive_days: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRetentionSettings {
    #[serde(default)]
    pub default_rule: Option<RetentionRule>,
    #[serde(default)]
    pub protected_refs: Vec<String>,
    #[serde(default)]
    pub retention_check_interval_hours: u32,
}

impl Default for GlobalRetentionSettings {
    fn default() -> Self {
        Self {
            default_rule: None,
            protected_refs: vec![
                "refs/heads/main".to_string(),
                "refs/heads/master".to_string(),
                "refs/tags/".to_string(),
            ],
            retention_check_interval_hours: 24,
        }
    }
}

/// Reference metadata for retention decisions
#[derive(Debug, Clone)]
pub struct RefMetadata {
    pub name: String, // Full ref name (e.g., "refs/heads/feature/my-branch")
    pub ref_type: RefType,
    pub created_at: u64, // Unix timestamp
    pub updated_at: u64,
    pub is_protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefType {
    Branch,
    Tag,
    Other,
}

/// Repository compliance configuration with retention
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepositoryRetention {
    /// Source of the retention policy
    pub source: PolicySource,
    /// Git ref where policy is stored (if source is GitRef)
    pub policy_ref: String,
    /// Embedded policy (if source is Embedded)
    #[serde(default)]
    pub policy: Option<RetentionPolicy>,
    /// Whether to inherit from tenant/global defaults
    #[serde(default)]
    pub inherit_from_tenant: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source")]
#[derive(Default)]
pub enum PolicySource {
    #[serde(rename = "git_ref")]
    GitRef { ref_name: String },
    #[serde(rename = "embedded")]
    #[default]
    Embedded,
    #[serde(rename = "database")]
    Database,
    #[serde(rename = "api")]
    Api,
}

/// Retention evaluation result
#[derive(Debug, Clone)]
pub struct RetentionDecision {
    pub ref_name: String,
    pub should_delete: bool,
    pub should_archive: bool,
    pub archive_ref: Option<String>,
    pub reason: String,
    pub notify_days: Option<u32>,
}

/// Service for evaluating and applying retention policies
#[derive(Debug)]
pub struct RetentionService {
    policy_cache: std::sync::RwLock<HashMap<String, RetentionPolicy>>,
}

impl RetentionService {
    pub fn new() -> Self {
        Self {
            policy_cache: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Evaluate retention for a single ref
    pub fn evaluate_retention(
        &self,
        policy: &RetentionPolicy,
        ref_meta: &RefMetadata,
    ) -> RetentionDecision {
        // Check if ref is protected
        if ref_meta.is_protected || self.is_protected_ref(&policy.global_settings, &ref_meta.name) {
            return RetentionDecision {
                ref_name: ref_meta.name.clone(),
                should_delete: false,
                should_archive: false,
                archive_ref: None,
                reason: "Protected ref".to_string(),
                notify_days: None,
            };
        }

        // Find matching rule
        for rule in &policy.rules {
            if self.matches_pattern(&rule.pattern, &ref_meta.name) {
                let decision = self.apply_rule(rule, ref_meta);
                if decision.should_delete || decision.should_archive {
                    return decision;
                }
            }
        }

        // Apply default rule if no match
        if let Some(default_rule) = &policy.global_settings.default_rule {
            let decision = self.apply_rule(default_rule, ref_meta);
            if decision.should_delete || decision.should_archive {
                return decision;
            }
        }

        // Default: keep everything
        RetentionDecision {
            ref_name: ref_meta.name.clone(),
            should_delete: false,
            should_archive: false,
            archive_ref: None,
            reason: "No deletion rule matched".to_string(),
            notify_days: None,
        }
    }

    /// Apply a retention rule to a ref
    fn apply_rule(&self, rule: &RetentionRule, ref_meta: &RefMetadata) -> RetentionDecision {
        let age_days = self.calculate_age_days(ref_meta.created_at);

        match &rule.retention {
            RetentionConfig::Permanent => RetentionDecision {
                ref_name: ref_meta.name.clone(),
                should_delete: false,
                should_archive: false,
                archive_ref: None,
                reason: "Permanent retention".to_string(),
                notify_days: None,
            },
            RetentionConfig::MaxAge { days, auto_delete } => {
                if *days == 0 {
                    return RetentionDecision {
                        ref_name: ref_meta.name.clone(),
                        should_delete: false,
                        should_archive: false,
                        archive_ref: None,
                        reason: "Max age is 0 (unlimited)".to_string(),
                        notify_days: None,
                    };
                }

                if age_days >= *days as i64 {
                    let notify = if *auto_delete && rule.notify_before_days > 0 {
                        Some(rule.notify_before_days)
                    } else {
                        None
                    };

                    RetentionDecision {
                        ref_name: ref_meta.name.clone(),
                        should_delete: *auto_delete,
                        should_archive: false,
                        archive_ref: None,
                        reason: format!("Age {} days exceeds limit of {} days", age_days, days),
                        notify_days: notify,
                    }
                } else {
                    RetentionDecision {
                        ref_name: ref_meta.name.clone(),
                        should_delete: false,
                        should_archive: false,
                        archive_ref: None,
                        reason: format!("Age {} days within limit of {} days", age_days, days),
                        notify_days: None,
                    }
                }
            }
            RetentionConfig::MinVersions { count: _, .. } => {
                // This would require context of all versions - simplified here
                RetentionDecision {
                    ref_name: ref_meta.name.clone(),
                    should_delete: false,
                    should_archive: false,
                    archive_ref: None,
                    reason: "Min versions rule (would need version count context)".to_string(),
                    notify_days: None,
                }
            }
            RetentionConfig::Archive {
                archive_ref,
                archive_days,
            } => {
                if age_days >= *archive_days as i64 {
                    RetentionDecision {
                        ref_name: ref_meta.name.clone(),
                        should_delete: false,
                        should_archive: true,
                        archive_ref: Some(archive_ref.clone()),
                        reason: format!("Archiving after {} days", archive_days),
                        notify_days: Some(7),
                    }
                } else {
                    RetentionDecision {
                        ref_name: ref_meta.name.clone(),
                        should_delete: false,
                        should_archive: false,
                        archive_ref: None,
                        reason: format!("Within archive window of {} days", archive_days),
                        notify_days: None,
                    }
                }
            }
        }
    }

    /// Check if ref matches a glob pattern
    fn matches_pattern(&self, pattern: &str, ref_name: &str) -> bool {
        // Simple glob matching - can be enhanced with regex or glob crate
        if let Some(prefix) = pattern.strip_suffix("/**") {
            ref_name.starts_with(prefix) || ref_name.contains(&format!("{}/", prefix))
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            ref_name.starts_with(prefix)
        } else {
            ref_name == pattern
        }
    }

    /// Check if ref is globally protected
    fn is_protected_ref(&self, settings: &GlobalRetentionSettings, ref_name: &str) -> bool {
        settings.protected_refs.iter().any(|p| {
            if p.ends_with('/') {
                ref_name.starts_with(p)
            } else {
                ref_name == p
            }
        })
    }

    /// Calculate age in days from Unix timestamp
    fn calculate_age_days(&self, created_at: u64) -> i64 {
        let created = UNIX_EPOCH + Duration::from_secs(created_at);
        let now = SystemTime::now();
        let duration = now.duration_since(created).unwrap_or(Duration::ZERO);
        duration.as_secs() as i64 / 86400
    }

    /// Cache a policy
    pub fn cache_policy(&self, repo_id: &str, policy: RetentionPolicy) {
        let mut cache = self.policy_cache.write().unwrap();
        cache.insert(repo_id.to_string(), policy);
    }

    /// Get cached policy
    pub fn get_cached_policy(&self, repo_id: &str) -> Option<RetentionPolicy> {
        let cache = self.policy_cache.read().unwrap();
        cache.get(repo_id).cloned()
    }

    /// Clear policy cache
    pub fn clear_cache(&self) {
        let mut cache = self.policy_cache.write().unwrap();
        cache.clear();
    }
}

impl Default for RetentionService {
    fn default() -> Self {
        Self::new()
    }
}

/// Default retention policy template
pub fn default_retention_policy() -> RetentionPolicy {
    RetentionPolicy {
        version: "1.0".to_string(),
        rules: vec![
            RetentionRule {
                name: "main-branch".to_string(),
                pattern: "refs/heads/main".to_string(),
                retention: RetentionConfig::Permanent,
                notify_before_days: 0,
            },
            RetentionRule {
                name: "feature-branches".to_string(),
                pattern: "refs/heads/feature/**".to_string(),
                retention: RetentionConfig::MaxAge {
                    days: 90,
                    auto_delete: true,
                },
                notify_before_days: 7,
            },
            RetentionRule {
                name: "bugfix-branches".to_string(),
                pattern: "refs/heads/bugfix/**".to_string(),
                retention: RetentionConfig::MaxAge {
                    days: 30,
                    auto_delete: true,
                },
                notify_before_days: 3,
            },
            RetentionRule {
                name: "release-tags".to_string(),
                pattern: "refs/tags/release-*".to_string(),
                retention: RetentionConfig::MinVersions {
                    count: 5,
                    keep_tags: true,
                },
                notify_before_days: 14,
            },
        ],
        global_settings: GlobalRetentionSettings::default(),
    }
}

/// Embedded retention policy for .git/repos-mcp/retention.json
pub const EMBEDDED_POLICY_FILENAME: &str = "retention.json";
pub const DEFAULT_POLICY_REF: &str = "refs/policies/retention";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        let service = RetentionService::new();

        // Exact match
        assert!(service.matches_pattern("refs/heads/main", "refs/heads/main"));

        // Prefix match
        assert!(service.matches_pattern("refs/heads/feature/*", "refs/heads/feature/my-branch"));

        // Recursive match
        assert!(service.matches_pattern("refs/heads/feature/**", "refs/heads/feature/sub/branch"));

        // No match
        assert!(!service.matches_pattern("refs/heads/main", "refs/heads/develop"));
    }

    #[test]
    fn test_retention_evaluation() {
        let policy = default_retention_policy();
        let service = RetentionService::new();

        // Main branch should be permanent
        let main_meta = RefMetadata {
            name: "refs/heads/main".to_string(),
            ref_type: RefType::Branch,
            created_at: 0,
            updated_at: 0,
            is_protected: false,
        };

        let decision = service.evaluate_retention(&policy, &main_meta);
        assert!(!decision.should_delete);
        assert_eq!(decision.reason, "Protected ref");

        // Old feature branch should be deleted
        let old_feature = RefMetadata {
            name: "refs/heads/feature/old-branch".to_string(),
            ref_type: RefType::Branch,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 100 * 86400, // 100 days ago
            updated_at: 0,
            is_protected: false,
        };

        let decision = service.evaluate_retention(&policy, &old_feature);
        assert!(decision.should_delete);
        assert_eq!(decision.notify_days, Some(7));
    }

    #[test]
    fn test_age_calculation() {
        let service = RetentionService::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 10 days ago
        let ten_days_ago = now - 10 * 86400;
        let age = service.calculate_age_days(ten_days_ago);
        assert!(age >= 9 && age <= 10);
    }
}

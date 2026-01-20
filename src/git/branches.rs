use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Branch limit configuration for repositories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchLimitConfig {
    pub max_branches: u32,
    pub max_tags: u32,
    #[serde(default)]
    pub patterns: Vec<BranchPatternLimit>,
    #[serde(default)]
    pub branch_naming_rules: Vec<BranchNamingRule>,
}

impl Default for BranchLimitConfig {
    fn default() -> Self {
        Self {
            max_branches: 100,
            max_tags: 0, // 0 = unlimited
            patterns: vec![],
            branch_naming_rules: vec![
                BranchNamingRule {
                    pattern: "refs/heads/*".to_string(),
                    naming_regex: Some(r"^[a-zA-Z0-9/_-]+$".to_string()),
                    max_length: 100,
                    reserved_names: vec!["main".to_string(), "master".to_string()],
                },
                BranchNamingRule {
                    pattern: "refs/tags/*".to_string(),
                    naming_regex: Some(r"^[vV]?\d+\.\d+.*$".to_string()),
                    max_length: 100,
                    reserved_names: vec!["latest".to_string()],
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchPatternLimit {
    pub pattern: String, // e.g., "refs/heads/feature/**"
    pub max_per_repo: u32,
    #[serde(default)]
    pub max_age_days: u32,
    #[serde(default)]
    pub auto_delete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchNamingRule {
    pub pattern: String,
    #[serde(default)]
    pub naming_regex: Option<String>,
    pub max_length: u32,
    #[serde(default)]
    pub reserved_names: Vec<String>,
}

/// Branch validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchValidationResult {
    pub is_valid: bool,
    pub branch_name: String,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl BranchValidationResult {
    pub fn valid(name: &str) -> Self {
        Self {
            is_valid: true,
            branch_name: name.to_string(),
            errors: vec![],
            warnings: vec![],
        }
    }

    pub fn invalid(name: &str, error: &str) -> Self {
        Self {
            is_valid: false,
            branch_name: name.to_string(),
            errors: vec![error.to_string()],
            warnings: vec![],
        }
    }

    pub fn with_warning(mut self, warning: &str) -> Self {
        self.warnings.push(warning.to_string());
        self
    }

    pub fn add_error(&mut self, error: &str) {
        self.is_valid = false;
        self.errors.push(error.to_string());
    }
}

/// Branch statistics for a repository
#[derive(Debug, Clone, Default)]
pub struct BranchStats {
    pub total_branches: u32,
    pub feature_branches: u32,
    pub bugfix_branches: u32,
    pub release_branches: u32,
    pub total_tags: u32,
    pub oldest_branch_age_days: Option<u64>,
    pub newest_branch_age_days: Option<u64>,
}

/// Service for managing branch limits
#[derive(Debug)]
pub struct BranchLimitService {
    config: BranchLimitConfig,
    branch_counts: HashMap<String, u32>, // repo_id -> branch count
}

impl BranchLimitService {
    pub fn new(config: Option<BranchLimitConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            branch_counts: HashMap::new(),
        }
    }

    /// Validate a branch name before creation
    pub fn validate_branch_name(&self, branch_name: &str, repo_id: &str) -> BranchValidationResult {
        let mut result = BranchValidationResult::valid(branch_name);

        // Check naming rules
        for rule in &self.config.branch_naming_rules {
            if self.matches_pattern(&rule.pattern, branch_name) {
                // Check max length
                if branch_name.len() > rule.max_length as usize {
                    result.add_error(&format!(
                        "Branch name exceeds maximum length of {} characters",
                        rule.max_length
                    ));
                }

                // Check naming regex
                if let Some(ref regex_str) = rule.naming_regex {
                    if let Ok(re) = regex::Regex::new(regex_str) {
                        if !re.is_match(branch_name) {
                            result.add_error(&format!(
                                "Branch name does not match required pattern: {}",
                                regex_str
                            ));
                        }
                    }
                }

                // Check reserved names
                if rule.reserved_names.iter().any(|r| {
                    r.trim_start_matches("refs/heads/")
                        .trim_start_matches("refs/tags/")
                        == branch_name
                            .trim_start_matches("refs/heads/")
                            .trim_start_matches("refs/tags/")
                }) {
                    result.add_error("Branch name is reserved");
                }
            }
        }

        // Check if creating this branch would exceed limits
        let current_count = self.branch_counts.get(repo_id).unwrap_or(&0);
        if *current_count >= self.config.max_branches {
            result.add_error(&format!(
                "Repository has reached maximum branch limit of {}",
                self.config.max_branches
            ));
        }

        // Check pattern-specific limits
        for pattern_limit in &self.config.patterns {
            if self.matches_pattern(&pattern_limit.pattern, branch_name) {
                // This is a simplified check - in production you'd track pattern counts
                debug!(
                    "Branch '{}' matches pattern '{}'",
                    branch_name, pattern_limit.pattern
                );
            }
        }

        result
    }

    /// Check if branch creation is allowed
    pub fn can_create_branch(
        &self,
        repo_id: &str,
        branch_name: &str,
    ) -> Result<bool, BranchLimitError> {
        let validation = self.validate_branch_name(branch_name, repo_id);

        if !validation.is_valid {
            Err(BranchLimitError::ValidationFailed(
                validation.errors.join(", "),
            ))
        } else {
            let current_count = self.branch_counts.get(repo_id).unwrap_or(&0);
            if *current_count >= self.config.max_branches {
                Err(BranchLimitError::BranchLimitExceeded {
                    repo_id: repo_id.to_string(),
                    current: *current_count,
                    limit: self.config.max_branches,
                })
            } else {
                Ok(true)
            }
        }
    }

    /// Increment branch count after successful creation
    pub fn increment_branch_count(&mut self, repo_id: &str) {
        *self.branch_counts.entry(repo_id.to_string()).or_insert(0) += 1;
        debug!(
            "Branch count for repo '{}' incremented to {}",
            repo_id, self.branch_counts[repo_id]
        );
    }

    /// Decrement branch count after deletion
    pub fn decrement_branch_count(&mut self, repo_id: &str) {
        if let Some(count) = self.branch_counts.get_mut(repo_id) {
            if *count > 0 {
                *count -= 1;
                debug!(
                    "Branch count for repo '{}' decremented to {}",
                    repo_id, count
                );
            }
        }
    }

    /// Get current branch count for a repo
    pub fn get_branch_count(&self, repo_id: &str) -> u32 {
        self.branch_counts.get(repo_id).unwrap_or(&0).clone()
    }

    /// Check pattern matching
    fn matches_pattern(&self, pattern: &str, name: &str) -> bool {
        if pattern.ends_with("/**") {
            let prefix = &pattern[..pattern.len() - 3];
            name.starts_with(prefix) || name.contains(&format!("{}/", prefix))
        } else if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            name.starts_with(prefix)
        } else {
            name == pattern
        }
    }

    /// Get branch statistics
    pub fn get_stats(&self, _repo_id: &str) -> BranchStats {
        // This would be enhanced with actual repository scanning
        BranchStats::default()
    }
}

#[derive(Debug, Error)]
pub enum BranchLimitError {
    #[error("Branch limit exceeded for repo {repo_id}: {current}/{limit}")]
    BranchLimitExceeded {
        repo_id: String,
        current: u32,
        limit: u32,
    },
    #[error("Branch validation failed: {0}")]
    ValidationFailed(String),
    #[error("Branch name invalid: {0}")]
    InvalidBranchName(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Pricing tier with branch limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTier {
    pub name: String,
    pub max_repos: i32,
    pub max_repo_size_mb: i64,
    pub max_branches: u32,
    pub max_tags: u32,
    pub rate_limit_rpm: u32,
    pub retention_enabled: bool,
    pub compliance_enabled: bool,
    #[serde(default)]
    pub custom_features: Vec<String>,
}

impl Default for PricingTier {
    fn default() -> Self {
        Self {
            name: "free".to_string(),
            max_repos: 5,
            max_repo_size_mb: 5,
            max_branches: 100,
            max_tags: 0,
            rate_limit_rpm: 60,
            retention_enabled: false,
            compliance_enabled: false,
            custom_features: vec![],
        }
    }
}

/// Predefined pricing tiers
pub fn pricing_tiers() -> HashMap<String, PricingTier> {
    let mut tiers = HashMap::new();

    tiers.insert(
        "free".to_string(),
        PricingTier {
            name: "free".to_string(),
            max_repos: 5,
            max_repo_size_mb: 5,
            max_branches: 10,
            max_tags: 0,
            rate_limit_rpm: 60,
            retention_enabled: false,
            compliance_enabled: false,
            custom_features: vec![],
        },
    );

    tiers.insert(
        "pro".to_string(),
        PricingTier {
            name: "pro".to_string(),
            max_repos: 50,
            max_repo_size_mb: 1000,
            max_branches: 100,
            max_tags: 0,
            rate_limit_rpm: 300,
            retention_enabled: true,
            compliance_enabled: true,
            custom_features: vec!["webhooks".to_string()],
        },
    );

    tiers.insert(
        "enterprise".to_string(),
        PricingTier {
            name: "enterprise".to_string(),
            max_repos: -1, // Unlimited
            max_repo_size_mb: -1,
            max_branches: 0, // Unlimited (0 in config means unlimited)
            max_tags: 0,
            rate_limit_rpm: 0, // Unlimited
            retention_enabled: true,
            compliance_enabled: true,
            custom_features: vec![
                "webhooks".to_string(),
                "sla".to_string(),
                "audit".to_string(),
            ],
        },
    );

    tiers
}

/// Tier assignment service
#[derive(Debug, Default)]
pub struct TierService {
    tiers: HashMap<String, PricingTier>,
    tenant_tiers: HashMap<String, String>, // tenant_id -> tier_name
}

impl TierService {
    pub fn new() -> Self {
        Self {
            tiers: pricing_tiers(),
            tenant_tiers: HashMap::new(),
        }
    }

    pub fn get_tier(&self, tenant_id: &str) -> Option<&PricingTier> {
        self.tenant_tiers
            .get(tenant_id)
            .and_then(|tier_name| self.tiers.get(tier_name))
    }

    pub fn assign_tier(
        &mut self,
        tenant_id: &str,
        tier_name: &str,
    ) -> Result<(), BranchLimitError> {
        if !self.tiers.contains_key(tier_name) {
            return Err(BranchLimitError::ConfigError(format!(
                "Unknown tier: {}",
                tier_name
            )));
        }
        self.tenant_tiers
            .insert(tenant_id.to_string(), tier_name.to_string());
        Ok(())
    }

    pub fn get_branch_limit(&self, tenant_id: &str) -> u32 {
        self.get_tier(tenant_id)
            .map(|t| t.max_branches)
            .unwrap_or(10) // Default for unknown tenants
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_name_validation() {
        let config = BranchLimitConfig::default();
        let service = BranchLimitService::new(Some(config));

        // Valid branch name
        assert!(
            service
                .validate_branch_name("refs/heads/feature/new-feature", "repo-1")
                .is_valid
        );

        // Reserved name
        let result = service.validate_branch_name("refs/heads/main", "repo-1");
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("reserved")));
    }

    #[test]
    fn test_branch_count_increment() {
        let config = BranchLimitConfig {
            max_branches: 5,
            ..Default::default()
        };
        let mut service = BranchLimitService::new(Some(config));

        assert_eq!(service.get_branch_count("repo-1"), 0);

        service.increment_branch_count("repo-1");
        assert_eq!(service.get_branch_count("repo-1"), 1);

        service.increment_branch_count("repo-1");
        assert_eq!(service.get_branch_count("repo-1"), 2);

        service.decrement_branch_count("repo-1");
        assert_eq!(service.get_branch_count("repo-1"), 1);
    }

    #[test]
    fn test_pricing_tiers() {
        let tiers = pricing_tiers();

        assert!(tiers.contains_key("free"));
        assert!(tiers.contains_key("pro"));
        assert!(tiers.contains_key("enterprise"));

        let free = &tiers["free"];
        assert_eq!(free.max_repos, 5);
        assert_eq!(free.max_branches, 10);

        let enterprise = &tiers["enterprise"];
        assert_eq!(enterprise.max_repos, -1); // Unlimited
    }
}

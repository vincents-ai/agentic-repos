//! Secret pattern definitions for the security scanner.
//!
//! Each pattern is a compiled regex paired with a type, severity, and human-readable
//! description. Patterns are loaded eagerly at scanner construction time.

use regex::Regex;
use std::str::FromStr;

/// Well-known secret pattern types.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SecretPatternType {
    AwsAccessKeyId,
    AwsSecretAccessKey,
    GithubToken,
    GitlabToken,
    JwtToken,
    PrivateKey,
    Password,
    ApiKey,
    GenericSecret,
    /// User-defined custom pattern.
    Custom(String),
}

/// A compiled secret-detection pattern.
pub struct SecretPatternDef {
    pub pattern_type: SecretPatternType,
    pub regex: Regex,
    pub severity: Severity,
    pub description: String,
}

/// Severity level for a secret match.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

impl FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "critical" => Ok(Severity::Critical),
            "high" => Ok(Severity::High),
            "medium" => Ok(Severity::Medium),
            "low" => Ok(Severity::Low),
            _ => Err(format!("unknown severity: {}", s)),
        }
    }
}

/// Build the default set of built-in secret patterns.
pub fn builtin_patterns() -> Vec<SecretPatternDef> {
    vec![
        SecretPatternDef {
            pattern_type: SecretPatternType::AwsAccessKeyId,
            regex: Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap(),
            severity: Severity::Critical,
            description: "AWS Access Key ID".to_string(),
        },
        SecretPatternDef {
            pattern_type: SecretPatternType::AwsSecretAccessKey,
            regex: Regex::new(
                r"(?i)aws[_-]?secret[_-]?access[_-]?key[_-]?[=:]\s*[A-Za-z0-9/+=]{40}",
            )
            .unwrap(),
            severity: Severity::Critical,
            description: "AWS Secret Access Key".to_string(),
        },
        SecretPatternDef {
            pattern_type: SecretPatternType::GithubToken,
            regex: Regex::new(r"(?i)ghp_[A-Za-z0-9]{36}").unwrap(),
            severity: Severity::Critical,
            description: "GitHub Personal Access Token".to_string(),
        },
        SecretPatternDef {
            pattern_type: SecretPatternType::GitlabToken,
            regex: Regex::new(r"(?i)glpat-[A-Za-z0-9_-]{20}").unwrap(),
            severity: Severity::Critical,
            description: "GitLab Personal Access Token".to_string(),
        },
        SecretPatternDef {
            pattern_type: SecretPatternType::JwtToken,
            regex: Regex::new(r"eyJ[A-Za-z0-9_-]*\.eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]*").unwrap(),
            severity: Severity::High,
            description: "JWT Token".to_string(),
        },
        SecretPatternDef {
            pattern_type: SecretPatternType::PrivateKey,
            regex: Regex::new(r"-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----").unwrap(),
            severity: Severity::Critical,
            description: "Private Key".to_string(),
        },
        SecretPatternDef {
            pattern_type: SecretPatternType::Password,
            regex: Regex::new(r"(?i)(?:password|passwd|pwd)[_-]?\s*[=:]\s*\S{8,}").unwrap(),
            severity: Severity::High,
            description: "Password".to_string(),
        },
        SecretPatternDef {
            pattern_type: SecretPatternType::ApiKey,
            regex: Regex::new(r"(?i)(?:api[_-]?key|apikey)[_-]?\s*[=:]\s*[A-Za-z0-9_-]{20,}")
                .unwrap(),
            severity: Severity::High,
            description: "API Key".to_string(),
        },
        SecretPatternDef {
            pattern_type: SecretPatternType::GenericSecret,
            regex: Regex::new(r"(?i)(?:secret|token|auth)[_-]?\s*[=:]\s*[A-Za-z0-9_-]{16,}")
                .unwrap(),
            severity: Severity::Medium,
            description: "Generic Secret".to_string(),
        },
    ]
}

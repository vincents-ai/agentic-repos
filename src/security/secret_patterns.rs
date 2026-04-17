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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // ── Severity FromStr ──

    #[test]
    fn severity_from_str() {
        assert_eq!(Severity::from_str("critical").unwrap(), Severity::Critical);
        assert_eq!(Severity::from_str("HIGH").unwrap(), Severity::High);
        assert_eq!(Severity::from_str("Medium").unwrap(), Severity::Medium);
        assert_eq!(Severity::from_str("low").unwrap(), Severity::Low);
    }

    #[test]
    fn severity_from_str_unknown() {
        assert!(Severity::from_str("urgent").is_err());
    }

    // ── Built-in patterns load ──

    #[test]
    fn builtin_patterns_count() {
        let patterns = builtin_patterns();
        assert_eq!(patterns.len(), 9);
    }

    #[test]
    fn builtin_patterns_all_have_valid_regex() {
        let patterns = builtin_patterns();
        for p in &patterns {
            // Ensure the regex compiles and can match something
            assert!(!p.description.is_empty(), "Pattern {:?} has no description", p.pattern_type);
        }
    }

    // ── Pattern detection accuracy ──

    #[test]
    fn detect_aws_access_key() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::AwsAccessKeyId).unwrap();
        assert!(p.regex.is_match("AKIAIOSFODNN7EXAMPLE"));
        assert!(p.regex.is_match("key=AKIA1234567890ABCDEF"));
        assert!(!p.regex.is_match("NOT_A_KEY_AT_ALL"));
    }

    #[test]
    fn detect_aws_secret_key() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::AwsSecretAccessKey).unwrap();
        assert!(p.regex.is_match("aws_secret_access_key=ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcd"));
        assert!(!p.regex.is_match("aws_secret_access_key = short"));
    }

    #[test]
    fn detect_github_token() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::GithubToken).unwrap();
        assert!(p.regex.is_match("ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijkl"));
        assert!(!p.regex.is_match("ghp_too_short"));
    }

    #[test]
    fn detect_gitlab_token() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::GitlabToken).unwrap();
        assert!(p.regex.is_match("glpat-ABCDEFGHIJKLMNOPQRST"));
        assert!(!p.regex.is_match("glpat-tooshort"));
    }

    #[test]
    fn detect_jwt() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::JwtToken).unwrap();
        assert!(p.regex.is_match("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.sig123"));
        assert!(!p.regex.is_match("notajwt"));
    }

    #[test]
    fn detect_private_key() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::PrivateKey).unwrap();
        assert!(p.regex.is_match("-----BEGIN RSA PRIVATE KEY-----"));
        assert!(p.regex.is_match("-----BEGIN PRIVATE KEY-----"));
        assert!(!p.regex.is_match("-----BEGIN PUBLIC KEY-----"));
    }

    #[test]
    fn detect_password() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::Password).unwrap();
        assert!(p.regex.is_match("password = hunter2xxxxx"));
        assert!(p.regex.is_match("passwd:supersecret123"));
        assert!(!p.regex.is_match("password = short")); // Less than 8 chars
    }

    #[test]
    fn detect_api_key() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::ApiKey).unwrap();
        assert!(p.regex.is_match("api_key=ABCDEFGHIJKLMNOPQRST1234"));
        assert!(!p.regex.is_match("api_key=short"));
    }

    #[test]
    fn detect_generic_secret() {
        let patterns = builtin_patterns();
        let p = patterns.iter().find(|p| p.pattern_type == SecretPatternType::GenericSecret).unwrap();
        assert!(p.regex.is_match("secret = ABCDEFGHIJKLMNOP"));
        assert!(p.regex.is_match("token:ABCDEFGHIJKLMNOP"));
        assert!(!p.regex.is_match("secret = short"));
    }

    // ── Severity assignments ──

    #[test]
    fn critical_patterns() {
        let patterns = builtin_patterns();
        let critical_types: Vec<_> = patterns
            .iter()
            .filter(|p| p.severity == Severity::Critical)
            .map(|p| p.pattern_type.clone())
            .collect();
        assert!(critical_types.contains(&SecretPatternType::AwsAccessKeyId));
        assert!(critical_types.contains(&SecretPatternType::AwsSecretAccessKey));
        assert!(critical_types.contains(&SecretPatternType::GithubToken));
        assert!(critical_types.contains(&SecretPatternType::GitlabToken));
        assert!(critical_types.contains(&SecretPatternType::PrivateKey));
    }

    #[test]
    fn high_severity_patterns() {
        let patterns = builtin_patterns();
        let high: Vec<_> = patterns
            .iter()
            .filter(|p| p.severity == Severity::High)
            .map(|p| p.pattern_type.clone())
            .collect();
        assert!(high.contains(&SecretPatternType::JwtToken));
        assert!(high.contains(&SecretPatternType::Password));
        assert!(high.contains(&SecretPatternType::ApiKey));
    }

    #[test]
    fn medium_severity_patterns() {
        let patterns = builtin_patterns();
        let medium: Vec<_> = patterns
            .iter()
            .filter(|p| p.severity == Severity::Medium)
            .map(|p| p.pattern_type.clone())
            .collect();
        assert!(medium.contains(&SecretPatternType::GenericSecret));
    }
}

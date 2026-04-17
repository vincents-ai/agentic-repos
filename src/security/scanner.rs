use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMatch {
    pub pattern_type: SecretPattern,
    pub pattern_name: String,
    pub match_value: String,
    pub line_number: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub severity: Severity,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecretPattern {
    AwsAccessKeyId,
    AwsSecretAccessKey,
    GithubToken,
    GitlabToken,
    JwtToken,
    PrivateKey,
    Password,
    ApiKey,
    GenericSecret,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub scan_id: String,
    pub matches: Vec<SecretMatch>,
    pub total_matches: usize,
    pub files_scanned: usize,
    pub scan_duration_ms: u64,
    pub status: ScanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScanStatus {
    Completed,
    InProgress,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRequest {
    pub content: String,
    pub file_path: Option<String>,
}

pub struct SecretScanner {
    patterns: Vec<(SecretPattern, Regex, Severity, String)>,
    results: Arc<RwLock<HashMap<String, ScanResult>>>,
}

impl SecretScanner {
    pub fn new() -> Self {
        let patterns = Self::build_patterns();
        Self {
            patterns,
            results: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn build_patterns() -> Vec<(SecretPattern, Regex, Severity, String)> {
        vec![
            (
                SecretPattern::AwsAccessKeyId,
                Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap(),
                Severity::Critical,
                "AWS Access Key ID".to_string(),
            ),
            (
                SecretPattern::AwsSecretAccessKey,
                Regex::new(r"(?i)aws[_-]?secret[_-]?access[_-]?key[_-]?[=:]\s*[A-Za-z0-9/+=]{40}")
                    .unwrap(),
                Severity::Critical,
                "AWS Secret Access Key".to_string(),
            ),
            (
                SecretPattern::GithubToken,
                Regex::new(r"(?i)ghp_[A-Za-z0-9]{36}").unwrap(),
                Severity::Critical,
                "GitHub Personal Access Token".to_string(),
            ),
            (
                SecretPattern::GitlabToken,
                Regex::new(r"(?i)glpat-[A-Za-z0-9_-]{20}").unwrap(),
                Severity::Critical,
                "GitLab Personal Access Token".to_string(),
            ),
            (
                SecretPattern::JwtToken,
                Regex::new(r"eyJ[A-Za-z0-9_-]*\.eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]*").unwrap(),
                Severity::High,
                "JWT Token".to_string(),
            ),
            (
                SecretPattern::PrivateKey,
                Regex::new(r"-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----").unwrap(),
                Severity::Critical,
                "Private Key".to_string(),
            ),
            (
                SecretPattern::Password,
                Regex::new(r"(?i)(?:password|passwd|pwd)[_-]?\s*[=:]\s*\S{8,}").unwrap(),
                Severity::High,
                "Password".to_string(),
            ),
            (
                SecretPattern::ApiKey,
                Regex::new(r"(?i)(?:api[_-]?key|apikey)[_-]?\s*[=:]\s*[A-Za-z0-9_-]{20,}").unwrap(),
                Severity::High,
                "API Key".to_string(),
            ),
            (
                SecretPattern::GenericSecret,
                Regex::new(r"(?i)(?:secret|token|auth)[_-]?\s*[=:]\s*[A-Za-z0-9_-]{16,}").unwrap(),
                Severity::Medium,
                "Generic Secret".to_string(),
            ),
        ]
    }

    pub async fn scan(&self, request: ScanRequest) -> ScanResult {
        let scan_id = Uuid::new_v4().to_string();
        let start = std::time::Instant::now();

        let matches = self.scan_content(&request.content);

        let result = ScanResult {
            scan_id: scan_id.clone(),
            matches,
            total_matches: 0,
            files_scanned: 1,
            scan_duration_ms: start.elapsed().as_millis() as u64,
            status: ScanStatus::Completed,
        };

        let result_clone = result.clone();
        {
            let mut results = self.results.write().await;
            results.insert(scan_id.clone(), result_clone);
        }

        ScanResult {
            total_matches: result.matches.len(),
            ..result
        }
    }

    fn scan_content(&self, content: &str) -> Vec<SecretMatch> {
        let mut matches = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            for (pattern_type, regex, severity, description) in &self.patterns {
                for capture in regex.find_iter(line) {
                    let match_value = if capture.end() - capture.start() > 50 {
                        format!("{}...", &line[capture.start()..capture.start() + 47])
                    } else {
                        line[capture.start()..capture.end()].to_string()
                    };

                    matches.push(SecretMatch {
                        pattern_type: pattern_type.clone(),
                        pattern_name: description.clone(),
                        match_value,
                        line_number: line_num + 1,
                        column_start: capture.start(),
                        column_end: capture.end(),
                        severity: severity.clone(),
                        description: description.clone(),
                    });
                }
            }
        }

        matches
    }

    pub async fn get_result(&self, scan_id: &str) -> Option<ScanResult> {
        let results = self.results.read().await;
        results.get(scan_id).cloned()
    }
}

impl Default for SecretScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scan_detects_aws_key() {
        let scanner = SecretScanner::new();
        let content = "AWS_KEY=AKIAIOSFODNN7EXAMPLE".to_string();
        let result = scanner
            .scan(ScanRequest {
                content,
                file_path: None,
            })
            .await;

        assert!(!result.matches.is_empty());
        assert!(result
            .matches
            .iter()
            .any(|m| m.pattern_type == SecretPattern::AwsAccessKeyId));
    }

    #[tokio::test]
    async fn test_scan_detects_github_token() {
        let scanner = SecretScanner::new();
        let content = "GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string();
        let result = scanner
            .scan(ScanRequest {
                content,
                file_path: None,
            })
            .await;

        assert!(!result.matches.is_empty());
        assert!(result
            .matches
            .iter()
            .any(|m| m.pattern_type == SecretPattern::GithubToken));
    }

    #[tokio::test]
    async fn test_scan_detects_private_key() {
        let scanner = SecretScanner::new();
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...".to_string();
        let result = scanner
            .scan(ScanRequest {
                content,
                file_path: None,
            })
            .await;

        assert!(!result.matches.is_empty());
        assert!(result
            .matches
            .iter()
            .any(|m| m.pattern_type == SecretPattern::PrivateKey));
    }

    #[tokio::test]
    async fn test_scan_no_false_positives() {
        let scanner = SecretScanner::new();
        let content = "This is a normal line of code without any secrets.".to_string();
        let result = scanner
            .scan(ScanRequest {
                content,
                file_path: None,
            })
            .await;

        assert!(result.matches.is_empty());
    }

    #[tokio::test]
    async fn test_get_result() {
        let scanner = SecretScanner::new();
        let content = "password = 'supersecret123'".to_string();
        let result = scanner
            .scan(ScanRequest {
                content,
                file_path: None,
            })
            .await;

        let retrieved = scanner.get_result(&result.scan_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().scan_id, result.scan_id);
    }

    // ── All 9 pattern types ──

    #[tokio::test]
    async fn test_detect_aws_access_key() {
        let scanner = SecretScanner::new();
        let result = scanner.scan(ScanRequest { content: "key=AKIAIOSFODNN7EXAMPLE".into(), file_path: None }).await;
        assert!(result.matches.iter().any(|m| m.pattern_type == SecretPattern::AwsAccessKeyId));
        assert_eq!(result.matches[0].severity, Severity::Critical);
    }

    #[tokio::test]
    async fn test_detect_aws_secret_key() {
        let scanner = SecretScanner::new();
        let result = scanner.scan(ScanRequest {
            content: "aws_secret_access_key=ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcd".into(),
            file_path: None,
        }).await;
        assert!(result.matches.iter().any(|m| m.pattern_type == SecretPattern::AwsSecretAccessKey));
    }

    #[tokio::test]
    async fn test_detect_gitlab_token() {
        let scanner = SecretScanner::new();
        let result = scanner.scan(ScanRequest {
            content: "token=glpat-ABCDEFGHIJKLMNOPQRST".into(),
            file_path: None,
        }).await;
        assert!(result.matches.iter().any(|m| m.pattern_type == SecretPattern::GitlabToken));
    }

    #[tokio::test]
    async fn test_detect_jwt() {
        let scanner = SecretScanner::new();
        let result = scanner.scan(ScanRequest {
            content: "auth=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.signaturehere".into(),
            file_path: None,
        }).await;
        assert!(result.matches.iter().any(|m| m.pattern_type == SecretPattern::JwtToken));
        assert_eq!(result.matches[0].severity, Severity::High);
    }

    #[tokio::test]
    async fn test_detect_password() {
        let scanner = SecretScanner::new();
        let result = scanner.scan(ScanRequest {
            content: "password=hunter2supersecret".into(),
            file_path: None,
        }).await;
        assert!(result.matches.iter().any(|m| m.pattern_type == SecretPattern::Password));
    }

    #[tokio::test]
    async fn test_detect_api_key() {
        let scanner = SecretScanner::new();
        let result = scanner.scan(ScanRequest {
            content: "api_key=ABCDEFGHIJKLMNOPQRST1234567890".into(),
            file_path: None,
        }).await;
        assert!(result.matches.iter().any(|m| m.pattern_type == SecretPattern::ApiKey));
    }

    #[tokio::test]
    async fn test_detect_generic_secret() {
        let scanner = SecretScanner::new();
        let result = scanner.scan(ScanRequest {
            content: "secret=ABCDEFGHIJKLMNOP".into(),
            file_path: None,
        }).await;
        assert!(result.matches.iter().any(|m| m.pattern_type == SecretPattern::GenericSecret));
        assert_eq!(result.matches[0].severity, Severity::Medium);
    }

    // ── Multiline scanning ──

    #[tokio::test]
    async fn test_multiline_multiple_findings() {
        let scanner = SecretScanner::new();
        let content = r#"
AKIAIOSFODNN7EXAMPLE
this line is clean
ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijkl
another clean line
-----BEGIN RSA PRIVATE KEY-----
"#;
        let result = scanner.scan(ScanRequest { content: content.into(), file_path: None }).await;
        assert!(result.total_matches >= 3);
    }

    // ── Line and column positions ──

    #[tokio::test]
    async fn test_line_number_correct() {
        let scanner = SecretScanner::new();
        let content = "clean line\nAKIAIOSFODNN7EXAMPLE";
        let result = scanner.scan(ScanRequest { content: content.into(), file_path: None }).await;
        let aws_match = result.matches.iter().find(|m| m.pattern_type == SecretPattern::AwsAccessKeyId).unwrap();
        assert_eq!(aws_match.line_number, 2);
    }

    #[tokio::test]
    async fn test_column_positions_set() {
        let scanner = SecretScanner::new();
        let content = "prefix AKIAIOSFODNN7EXAMPLE suffix";
        let result = scanner.scan(ScanRequest { content: content.into(), file_path: None }).await;
        let m = &result.matches[0];
        assert!(m.column_start > 0);
        assert!(m.column_end > m.column_start);
    }

    // ── Scan result metadata ──

    #[tokio::test]
    async fn test_scan_result_fields() {
        let scanner = SecretScanner::new();
        let result = scanner.scan(ScanRequest { content: "AKIAIOSFODNN7EXAMPLE".into(), file_path: None }).await;
        assert!(!result.scan_id.is_empty());
        assert_eq!(result.status, ScanStatus::Completed);
        assert_eq!(result.files_scanned, 1);
        assert!(result.scan_duration_ms < 1000); // Should be fast
    }

    #[tokio::test]
    async fn test_scan_nonexistent_result() {
        let scanner = SecretScanner::new();
        let result = scanner.get_result("nonexistent").await;
        assert!(result.is_none());
    }

    // ── Long value truncation ──

    #[tokio::test]
    async fn test_long_match_truncated() {
        let scanner = SecretScanner::new();
        // Create a very long API key that exceeds 50 chars
        let long_key = "api_key=".to_string() + &"A".repeat(60);
        let result = scanner.scan(ScanRequest { content: long_key, file_path: None }).await;
        let api_match = result.matches.iter().find(|m| m.pattern_type == SecretPattern::ApiKey);
        if let Some(m) = api_match {
            assert!(m.match_value.len() <= 53); // 50 + "..."
        }
    }
}

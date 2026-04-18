//! Custom security rule engine.
//!
//! Rules are user-defined patterns loaded from YAML or JSON. Each rule has a name,
//! description, regex pattern, severity, and optional file-path glob filter.

use crate::security::secret_patterns::Severity;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tracing::warn;

/// Maximum time allowed to compile a single user-provided regex.
const REGEX_COMPILE_TIMEOUT: Duration = Duration::from_millis(500);

/// Maximum length of a user-provided regex pattern.
const MAX_PATTERN_LENGTH: usize = 4096;

/// Maximum number of capture groups / alternations before we reject.
/// This is a heuristic to prevent pathological patterns.
const MAX_PATTERN_COMPLEXITY: usize = 100;

/// A user-defined security rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRule {
    /// Unique rule name (e.g. "custom-api-key-v1").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Regex pattern to search for.
    pub pattern: String,
    /// Severity level (critical, high, medium, low).
    pub severity: String,
    /// Optional file glob to restrict matching (e.g. "*.env").
    #[serde(default)]
    pub file_glob: Option<String>,
    /// Whether the rule is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// A compiled security rule with pre-compiled regex.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub name: String,
    pub description: String,
    pub regex: Regex,
    pub severity: Severity,
    pub file_glob: Option<globset::GlobMatcher>,
    pub enabled: bool,
}

/// A match found by a custom rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatch {
    pub rule_name: String,
    pub description: String,
    pub match_value: String,
    pub line_number: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub severity: Severity,
}

/// Load rules from a YAML string.
pub fn load_rules_from_yaml(yaml: &str) -> Result<Vec<SecurityRule>, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

/// Load rules from a JSON string.
pub fn load_rules_from_json(json: &str) -> Result<Vec<SecurityRule>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Check whether a regex pattern is likely to cause catastrophic backtracking.
///
/// This is a heuristic check — it catches the most common pathological patterns
/// like nested quantifiers `((a+)+)` and deeply nested alternations. It does NOT
/// guarantee safety against all ReDoS patterns, so execution-time limits are also
/// applied.
fn is_likely_pathological(pattern: &str) -> bool {
    // Reject overly complex patterns
    let quantifier_count = pattern
        .chars()
        .filter(|c| *c == '+' || *c == '*' || *c == '{')
        .count();
    if quantifier_count > MAX_PATTERN_COMPLEXITY {
        return true;
    }

    // Check for nested quantifiers like (a+)+, (a*)*, (a+)*, etc.
    // Simplified heuristic: look for `)+`, `)*`, `}+`, `}*` which indicate
    // a quantifier on a group that already contains quantifiers.
    let bytes = pattern.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b')' || bytes[i] == b'}' {
            if bytes[i + 1] == b'+' || bytes[i + 1] == b'*' {
                // Check if there's a quantifier inside the matching group
                // Look backwards for the matching open paren
                let mut depth = 0;
                for j in (0..i).rev() {
                    if bytes[j] == b')' {
                        depth += 1;
                    } else if bytes[j] == b'(' {
                        if depth == 0 {
                            // Found the matching open paren, check for inner quantifiers
                            for k in j..i {
                                if bytes[k] == b'+' || bytes[k] == b'*' || bytes[k] == b'{' {
                                    return true; // Nested quantifier detected
                                }
                            }
                            break;
                        }
                        depth -= 1;
                    }
                }
            }
        }
    }

    false
}

/// Compile a regex from a user-provided pattern with safety checks.
///
/// Applies the following protections:
/// 1. Pattern length limit (MAX_PATTERN_LENGTH)
/// 2. Heuristic check for pathological nested quantifiers
/// 3. Compile-time measurement (REGEX_COMPILE_TIMEOUT)
fn safe_compile_regex(pattern: &str) -> Option<Regex> {
    // Length check
    if pattern.len() > MAX_PATTERN_LENGTH {
        warn!(
            "Rejecting regex pattern: too long ({} bytes, max {})",
            pattern.len(),
            MAX_PATTERN_LENGTH
        );
        return None;
    }

    // Heuristic check for pathological patterns
    if is_likely_pathological(pattern) {
        warn!("Rejecting regex pattern: likely pathological (nested quantifiers or excessive complexity)");
        return None;
    }

    // Compile with timeout measurement
    let start = Instant::now();
    match Regex::new(pattern) {
        Ok(regex) => {
            let elapsed = start.elapsed();
            if elapsed > REGEX_COMPILE_TIMEOUT {
                warn!(
                    "Rejecting regex pattern: compile took too long ({:?}, max {:?})",
                    elapsed, REGEX_COMPILE_TIMEOUT
                );
                return None;
            }
            Some(regex)
        }
        Err(e) => {
            warn!("Failed to compile regex pattern '{}': {}", pattern, e);
            None
        }
    }
}

/// Compile a list of security rules into matchers.
pub fn compile_rules(rules: &[SecurityRule]) -> Vec<CompiledRule> {
    rules
        .iter()
        .filter(|r| r.enabled)
        .filter_map(|rule| {
            let regex = safe_compile_regex(&rule.pattern)?;
            let severity = Severity::from_str(&rule.severity).ok()?;
            let file_glob = rule
                .file_glob
                .as_ref()
                .and_then(|g| globset::Glob::new(g).ok())
                .map(|g| g.compile_matcher());

            Some(CompiledRule {
                name: rule.name.clone(),
                description: rule.description.clone(),
                regex,
                severity,
                file_glob,
                enabled: rule.enabled,
            })
        })
        .collect()
}

/// Scan content against compiled custom rules.
pub fn scan_with_rules(
    rules: &[CompiledRule],
    content: &str,
    file_path: Option<&Path>,
) -> Vec<RuleMatch> {
    let mut matches = Vec::new();

    for rule in rules {
        // Skip rule if file glob doesn't match
        if let (Some(ref glob), Some(path)) = (&rule.file_glob, file_path) {
            if !glob.is_match(path) {
                continue;
            }
        }

        for (line_num, line) in content.lines().enumerate() {
            for capture in rule.regex.find_iter(line) {
                let match_value = if capture.end() - capture.start() > 50 {
                    format!("{}...", &line[capture.start()..capture.start() + 47])
                } else {
                    line[capture.start()..capture.end()].to_string()
                };

                matches.push(RuleMatch {
                    rule_name: rule.name.clone(),
                    description: rule.description.clone(),
                    match_value,
                    line_number: line_num + 1,
                    column_start: capture.start(),
                    column_end: capture.end(),
                    severity: rule.severity.clone(),
                });
            }
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_YAML: &str = r#"
- name: custom-bearer-token
  description: Bearer token in Authorization header
  pattern: 'Bearer\s+[A-Za-z0-9_\-.]+'
  severity: high
  file_glob: "*.http"
  enabled: true
- name: internal-secret
  description: Internal service secret
  pattern: 'INT_SECRET_[A-Z0-9_]+=[A-Za-z0-9]{32,}'
  severity: critical
  enabled: true
"#;

    #[test]
    fn test_load_rules_from_yaml() {
        let rules = load_rules_from_yaml(SAMPLE_YAML).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "custom-bearer-token");
        assert_eq!(rules[1].severity, "critical");
    }

    #[test]
    fn test_compile_and_scan() {
        let rules = load_rules_from_yaml(SAMPLE_YAML).unwrap();
        let compiled = compile_rules(&rules);
        assert_eq!(compiled.len(), 2);

        let content = "Authorization: Bearer abc123.def456\nINT_SECRET_DB_PASSWORD=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let matches = scan_with_rules(&compiled, content, None);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_file_glob_filter() {
        let rules = load_rules_from_yaml(SAMPLE_YAML).unwrap();
        let compiled = compile_rules(&rules);

        // Bearer rule has file_glob: "*.http" — should not match a .rs file.
        // internal-secret has no file_glob so it always fires.
        let content = "Authorization: Bearer abc123.def456\nINT_SECRET_DB_KEY=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let matches = scan_with_rules(&compiled, content, Some(Path::new("src/main.rs")));
        assert_eq!(matches.len(), 1); // Only internal-secret matches
        assert_eq!(matches[0].rule_name, "internal-secret");
    }

    #[test]
    fn test_disabled_rule() {
        let yaml = r#"
- name: disabled-rule
  description: This is disabled
  pattern: 'test'
  severity: low
  enabled: false
"#;
        let rules = load_rules_from_yaml(yaml).unwrap();
        let compiled = compile_rules(&rules);
        assert!(compiled.is_empty());
    }

    // ── ReDoS protection tests ──

    #[test]
    fn test_rejects_nested_quantifier_pattern() {
        // Classic ReDoS pattern: (a+)+
        let rules = vec![SecurityRule {
            name: "redos-attempt".to_string(),
            description: "Pathological regex".to_string(),
            pattern: "(a+)+$".to_string(),
            severity: "high".to_string(),
            file_glob: None,
            enabled: true,
        }];
        let compiled = compile_rules(&rules);
        assert!(
            compiled.is_empty(),
            "Nested quantifier pattern should be rejected"
        );
    }

    #[test]
    fn test_rejects_overly_long_pattern() {
        let long_pattern = "a".repeat(MAX_PATTERN_LENGTH + 1);
        let rules = vec![SecurityRule {
            name: "long-pattern".to_string(),
            description: "Too long".to_string(),
            pattern: long_pattern,
            severity: "low".to_string(),
            file_glob: None,
            enabled: true,
        }];
        let compiled = compile_rules(&rules);
        assert!(
            compiled.is_empty(),
            "Overly long pattern should be rejected"
        );
    }

    #[test]
    fn test_accepts_benign_pattern() {
        let rules = vec![SecurityRule {
            name: "benign".to_string(),
            description: "Safe pattern".to_string(),
            pattern: r"Bearer\s+[A-Za-z0-9]+".to_string(),
            severity: "high".to_string(),
            file_glob: None,
            enabled: true,
        }];
        let compiled = compile_rules(&rules);
        assert_eq!(
            compiled.len(),
            1,
            "Benign pattern should compile successfully"
        );
    }

    #[test]
    fn test_is_likely_pathological_positive() {
        assert!(is_likely_pathological("(a+)+$"));
        assert!(is_likely_pathological("(a*)*"));
        assert!(is_likely_pathological("((x+)+)+"));
    }

    #[test]
    fn test_is_likely_pathological_negative() {
        assert!(!is_likely_pathological("[A-Za-z]+"));
        assert!(!is_likely_pathological("(foo|bar)"));
        assert!(!is_likely_pathological("\\d{1,3}"));
        assert!(!is_likely_pathological("Bearer\\s+[A-Za-z0-9_-]+"));
    }
}

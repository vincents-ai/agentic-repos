//! CODEOWNERS file parser for monorepo path-based access control.

use crate::error::RbacError;

/// A single CODEOWNERS entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersEntry {
    /// The glob pattern for matching paths.
    pub pattern: String,
    /// The owners (usernames, team names, or email addresses).
    pub owners: Vec<String>,
    /// Line number in the CODEOWNERS file.
    pub line: usize,
    /// Whether the pattern is a negation (prefixed with `!`).
    pub is_negation: bool,
}

/// Parsed CODEOWNERS file.
#[derive(Debug, Clone, Default)]
pub struct CodeownersParser {
    /// Entries in order (last matching entry wins, per GitHub semantics).
    entries: Vec<CodeownersEntry>,
}

impl CodeownersParser {
    /// Parse a CODEOWNERS file from string content.
    pub fn parse(content: &str) -> Result<Self, RbacError> {
        let mut entries = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1; // 1-indexed
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Split pattern from owners
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let pattern = parts[0].to_string();
            let is_negation = pattern.starts_with('!');
            let owners: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

            // Validate pattern is not empty after removing negation prefix
            let pattern_check = if is_negation {
                &pattern[1..]
            } else {
                &pattern
            };
            if pattern_check.is_empty() {
                return Err(RbacError::CodeownersParseError {
                    line: line_num,
                    reason: "empty pattern".to_string(),
                });
            }

            entries.push(CodeownersEntry {
                pattern,
                owners,
                line: line_num,
                is_negation,
            });
        }

        Ok(Self { entries })
    }

    /// Find owners for a given path.
    ///
    /// Returns the owners from the **last** matching entry (GitHub semantics).
    /// Negation patterns (`!`) exclude paths from ownership.
    pub fn owners_for_path(&self, path: &str) -> Vec<String> {
        let mut owners = Vec::new();
        let mut excluded = false;

        for entry in &self.entries {
            if entry.is_negation {
                let neg_pattern = &entry.pattern[1..]; // Remove '!'
                if Self::path_matches_pattern(path, neg_pattern) {
                    excluded = true;
                }
            } else if Self::path_matches_pattern(path, &entry.pattern) {
                excluded = false;
                owners = entry.owners.clone();
            }
        }

        if excluded {
            Vec::new()
        } else {
            owners
        }
    }

    /// Check if a path is owned by a specific owner.
    pub fn is_owned_by(&self, path: &str, owner: &str) -> bool {
        self.owners_for_path(path)
            .iter()
            .any(|o| o.eq_ignore_ascii_case(owner))
    }

    /// Get all entries.
    pub fn entries(&self) -> &[CodeownersEntry] {
        &self.entries
    }

    /// Simple glob matching for CODEOWNERS patterns.
    /// Supports `*` (any segment) and `**` (any path depth).
    fn path_matches_pattern(path: &str, pattern: &str) -> bool {
        // Normalize: strip leading /
        let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
        let path = path.strip_prefix('/').unwrap_or(path);

        // Strip trailing / from pattern for matching
        let pattern_dir = pattern.strip_suffix('/').unwrap_or(pattern);

        if pattern.contains("**") {
            // ** matches any number of directories
            Self::glob_match_doublestar(path, pattern_dir)
        } else if pattern.contains('*') {
            // Simple single-star glob
            Self::glob_match_single(path, pattern_dir)
        } else {
            // Exact match or directory prefix
            path == pattern_dir || path.starts_with(&format!("{}/", pattern_dir))
        }
    }

    fn glob_match_single(path: &str, pattern: &str) -> bool {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            // One wildcard: prefix*suffix
            let (prefix, suffix) = (parts[0], parts[1]);
            path.starts_with(prefix) && path.ends_with(suffix) && path.len() >= prefix.len() + suffix.len()
        } else {
            path == pattern
        }
    }

    fn glob_match_doublestar(path: &str, pattern: &str) -> bool {
        // Split on ** and check each segment matches
        let segments: Vec<&str> = pattern.split("**").collect();

        if segments.len() == 1 {
            return path == segments[0];
        }

        let mut path_remaining = path;

        for (i, segment) in segments.iter().enumerate() {
            if segment.is_empty() {
                continue;
            }

            let segment = segment.strip_prefix('/').unwrap_or(segment);
            let segment = segment.strip_suffix('/').unwrap_or(segment);

            if i == 0 {
                // First segment: path must start with it
                if !path_remaining.starts_with(segment) {
                    return false;
                }
                path_remaining = &path_remaining[segment.len()..];
                // Skip past the next /
                if let Some(pos) = path_remaining.find('/') {
                    path_remaining = &path_remaining[pos..];
                }
            } else if i == segments.len() - 1 {
                // Last segment: path must end with it
                if segment.is_empty() {
                    return true;
                }
                return path_remaining.ends_with(segment) || path_remaining.contains(segment);
            } else {
                // Middle segment: must be found somewhere in remaining path
                if let Some(pos) = path_remaining.find(segment) {
                    path_remaining = &path_remaining[pos + segment.len()..];
                    if let Some(pos) = path_remaining.find('/') {
                        path_remaining = &path_remaining[pos..];
                    }
                } else {
                    return false;
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_codeowners() {
        let content = "\
# These owners will be the default owners
*       @core-team

# Backend services
/src/backend/    @backend-team

# Frontend
/src/frontend/   @frontend-team alice@example.com

# Docs
/docs/           @docs-team
";
        let parser = CodeownersParser::parse(content).unwrap();
        assert_eq!(parser.entries().len(), 4);

        // Default owners for any file
        let owners = parser.owners_for_path("random.txt");
        assert_eq!(owners, vec!["@core-team"]);

        // Backend takes priority over default
        let owners = parser.owners_for_path("src/backend/main.rs");
        assert_eq!(owners, vec!["@backend-team"]);

        // Frontend
        let owners = parser.owners_for_path("src/frontend/App.svelte");
        assert_eq!(owners, vec!["@frontend-team", "alice@example.com"]);

        // Docs
        let owners = parser.owners_for_path("docs/api.md");
        assert_eq!(owners, vec!["@docs-team"]);
    }

    #[test]
    fn parse_negation_pattern() {
        let content = "\
*               @all-team
!/internal/     @all-team
/src/           @src-team
";
        let parser = CodeownersParser::parse(content).unwrap();

        // Normal match
        let owners = parser.owners_for_path("src/main.rs");
        assert_eq!(owners, vec!["@src-team"]);

        // Default match
        let owners = parser.owners_for_path("README.md");
        assert_eq!(owners, vec!["@all-team"]);
    }

    #[test]
    fn parse_empty_and_comments() {
        let content = "\
# Just a comment

# Another comment
";
        let parser = CodeownersParser::parse(content).unwrap();
        assert!(parser.entries().is_empty());
    }

    #[test]
    fn parse_error_on_empty_negation() {
        let content = "! \n";
        let result = CodeownersParser::parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn is_owned_by() {
        let content = "/src/backend/   @backend-team alice@example.com\n";
        let parser = CodeownersParser::parse(content).unwrap();

        assert!(parser.is_owned_by("src/backend/main.rs", "@backend-team"));
        assert!(parser.is_owned_by("src/backend/main.rs", "alice@example.com"));
        assert!(!parser.is_owned_by("src/backend/main.rs", "@other-team"));
    }

    #[test]
    fn doublestar_pattern() {
        let content = "/src/**/tests/   @test-team\n";
        let parser = CodeownersParser::parse(content).unwrap();

        assert!(parser.is_owned_by("src/crates/foo/tests/test.rs", "@test-team"));
    }

    #[test]
    fn exact_match_or_prefix() {
        let content = "/Makefile   @build-team\n";
        let parser = CodeownersParser::parse(content).unwrap();

        // Exact match
        let owners = parser.owners_for_path("Makefile");
        assert_eq!(owners, vec!["@build-team"]);
    }
}

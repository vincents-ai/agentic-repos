//! Glob pattern matching for ADP ref-namespace schemas.
//!
//! Supported syntax:
//! - `*`  — matches any single path segment (no `/`)
//! - `**` — matches any sequence of segments including separators
//! - All other characters are literal.
//!
//! Examples:
//! - `refs/engram/task/*` matches `refs/engram/task/abc123`
//! - `refs/engram/**` matches `refs/engram/task/abc123` and
//!   `refs/engram/session/xyz`
//! - `refs/heads/main` matches only `refs/heads/main`

use crate::adp::schema::RefSchema;

/// Returns `true` if `refname` is matched by `pattern`.
///
/// `*` matches a single path segment (no forward slashes).
/// `**` matches any number of characters including `/`.
pub fn matches_pattern(pattern: &str, refname: &str) -> bool {
    match_impl(pattern, refname)
}

fn match_impl(pattern: &str, text: &str) -> bool {
    // Base cases
    if pattern.is_empty() {
        return text.is_empty();
    }

    // Check for ** at the start of pattern
    if let Some(rest_pattern) = pattern.strip_prefix("**/") {
        // ** followed by /: try matching rest at every position
        if match_impl(rest_pattern, text) {
            return true;
        }
        // Consume segments from text one at a time
        let mut remaining = text;
        while let Some(slash_pos) = remaining.find('/') {
            remaining = &remaining[slash_pos + 1..];
            if match_impl(rest_pattern, remaining) {
                return true;
            }
        }
        return false;
    }

    if pattern == "**" {
        // ** at end matches everything
        return true;
    }

    // Check for * (single-segment wildcard) at the start of pattern
    if let Some(rest_pattern) = pattern.strip_prefix("*/") {
        // * matches one segment (no slash)
        if let Some(slash_pos) = text.find('/') {
            let segment = &text[..slash_pos];
            if !segment.contains('/') {
                return match_impl(rest_pattern, &text[slash_pos + 1..]);
            }
        }
        return false;
    }

    if pattern == "*" {
        // * matches the remaining text as long as it has no slash
        return !text.contains('/');
    }

    // Literal prefix
    if let Some(sep) = pattern.find("/*") {
        // Split at the first wildcard boundary
        let literal = &pattern[..sep + 1]; // includes the slash
        if let Some(rest_text) = text.strip_prefix(literal) {
            let rest_pattern = &pattern[sep + 1..];
            return match_impl(rest_pattern, rest_text);
        }
        return false;
    }

    // Pure literal match
    text == pattern
}

/// Returns a specificity score for a pattern.
///
/// Higher is more specific.  Literal characters each contribute 1 point;
/// `*` wildcards contribute 0; `**` wildcards contribute a penalty of -1.
/// This means more-specific patterns (fewer wildcards, more literal characters)
/// win during schema resolution.
pub fn specificity_score(pattern: &str) -> u32 {
    let mut score: i64 = 0;
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' {
            if chars.peek() == Some(&'*') {
                // ** — strong wildcard, penalise
                chars.next();
                score -= 1;
            }
            // * — weak wildcard, neutral (0 points)
        } else {
            score += 1;
        }
    }
    score.max(0) as u32
}

/// Finds the `RefSchema` in `schemas` whose `namespace_pattern` matches
/// `refname` with the highest specificity score.
///
/// Returns `None` if no schema matches.
pub fn resolve_schema<'a>(schemas: &'a [RefSchema], refname: &str) -> Option<&'a RefSchema> {
    schemas
        .iter()
        .filter(|s| matches_pattern(&s.namespace_pattern, refname))
        .max_by_key(|s| specificity_score(&s.namespace_pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_match() {
        assert!(matches_pattern("refs/heads/main", "refs/heads/main"));
        assert!(!matches_pattern("refs/heads/main", "refs/heads/dev"));
    }

    #[test]
    fn test_star_single_segment() {
        assert!(matches_pattern("refs/heads/*", "refs/heads/main"));
        assert!(matches_pattern("refs/heads/*", "refs/heads/feature-x"));
        assert!(!matches_pattern("refs/heads/*", "refs/heads/a/b"));
        assert!(!matches_pattern("refs/heads/*", "refs/tags/v1.0"));
    }

    #[test]
    fn test_double_star_multi_segment() {
        assert!(matches_pattern("refs/engram/**", "refs/engram/task/abc"));
        assert!(matches_pattern(
            "refs/engram/**",
            "refs/engram/session/xyz/sub"
        ));
        assert!(matches_pattern("refs/engram/**", "refs/engram/single"));
        assert!(!matches_pattern("refs/engram/**", "refs/heads/main"));
    }

    #[test]
    fn test_specificity_score() {
        let s1 = specificity_score("refs/engram/task/*");
        let s2 = specificity_score("refs/engram/**");
        // More literal chars → higher score
        assert!(
            s1 > s2,
            "literal-heavy pattern should score higher: {} vs {}",
            s1,
            s2
        );
    }

    #[test]
    fn test_resolve_schema_most_specific_wins() {
        use chrono::Utc;
        let now = Utc::now();
        let generic = RefSchema {
            id: "generic".to_string(),
            namespace_pattern: "refs/engram/**".to_string(),
            title: "Generic".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: now,
            updated_at: now,
            schema: serde_json::Value::Null,
            ui: None,
        };
        let specific = RefSchema {
            id: "task".to_string(),
            namespace_pattern: "refs/engram/task/*".to_string(),
            title: "Task".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: now,
            updated_at: now,
            schema: serde_json::Value::Null,
            ui: None,
        };

        let schemas = vec![generic, specific];
        let result = resolve_schema(&schemas, "refs/engram/task/abc123");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "task");
    }

    #[test]
    fn test_resolve_schema_no_match() {
        use chrono::Utc;
        let now = Utc::now();
        let schema = RefSchema {
            id: "task".to_string(),
            namespace_pattern: "refs/engram/task/*".to_string(),
            title: "Task".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: now,
            updated_at: now,
            schema: serde_json::Value::Null,
            ui: None,
        };

        let schemas = vec![schema];
        let result = resolve_schema(&schemas, "refs/heads/main");
        assert!(result.is_none());
    }
}

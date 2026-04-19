use std::fmt;

/// Characters that are dangerous in Redis key parts.
/// - `:` is the namespace separator
/// - `\n`/`\r` enable RESP injection
/// - `\0` is null byte
/// - ` ` and `*` enable pattern injection in SCAN
const DANGEROUS_CHARS: &[char] = &[':', '\n', '\r', '\0', ' ', '*'];

/// Sanitize a key part by replacing dangerous characters with underscores.
fn sanitize_key_part(part: &str) -> String {
    let sanitized: String = part
        .chars()
        .map(|c| {
            if DANGEROUS_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if sanitized.is_empty() {
        "_empty_".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    parts: Vec<String>,
}

impl CacheKey {
    pub fn new(prefix: &str) -> Self {
        Self {
            parts: vec![sanitize_key_part(prefix)],
        }
    }

    pub fn tenant(prefix: &str, tenant_id: &str) -> Self {
        Self {
            parts: vec![
                sanitize_key_part(prefix),
                "tenant".to_string(),
                sanitize_key_part(tenant_id),
            ],
        }
    }

    pub fn repository(prefix: &str, tenant_id: &str, repo_name: &str) -> Self {
        Self {
            parts: vec![
                sanitize_key_part(prefix),
                "repo".to_string(),
                sanitize_key_part(tenant_id),
                sanitize_key_part(repo_name),
            ],
        }
    }

    pub fn session(prefix: &str, session_id: &str) -> Self {
        Self {
            parts: vec![
                sanitize_key_part(prefix),
                "session".to_string(),
                sanitize_key_part(session_id),
            ],
        }
    }

    pub fn user(prefix: &str, user_id: &str) -> Self {
        Self {
            parts: vec![sanitize_key_part(prefix), "user".to_string(), sanitize_key_part(user_id)],
        }
    }

    pub fn lfs_object(prefix: &str, tenant_id: &str, oid: &str) -> Self {
        Self {
            parts: vec![
                sanitize_key_part(prefix),
                "lfs".to_string(),
                sanitize_key_part(tenant_id),
                sanitize_key_part(oid),
            ],
        }
    }

    pub fn permission(prefix: &str, user_id: &str, resource: &str) -> Self {
        Self {
            parts: vec![
                sanitize_key_part(prefix),
                "perm".to_string(),
                sanitize_key_part(user_id),
                sanitize_key_part(resource),
            ],
        }
    }

    pub fn api_response(prefix: &str, endpoint: &str, params_hash: &str) -> Self {
        Self {
            parts: vec![
                sanitize_key_part(prefix),
                "api".to_string(),
                sanitize_key_part(endpoint),
                sanitize_key_part(params_hash),
            ],
        }
    }

    pub fn repo_list(prefix: &str, tenant_id: &str, page: u32) -> Self {
        Self {
            parts: vec![
                sanitize_key_part(prefix),
                "repos".to_string(),
                sanitize_key_part(tenant_id),
                format!("page:{}", page),
            ],
        }
    }

    pub fn branch_list(prefix: &str, tenant_id: &str, repo_name: &str) -> Self {
        Self {
            parts: vec![
                sanitize_key_part(prefix),
                "branches".to_string(),
                sanitize_key_part(tenant_id),
                sanitize_key_part(repo_name),
            ],
        }
    }

    pub fn push(&mut self, part: &str) -> &mut Self {
        self.parts.push(sanitize_key_part(part));
        self
    }

    pub fn with(mut self, part: &str) -> Self {
        self.parts.push(sanitize_key_part(part));
        self
    }

    pub fn to_redis_key(&self) -> String {
        self.parts.join(":")
    }

    pub fn pattern(&self) -> String {
        format!("{}:*", self.parts.join(":"))
    }

    pub fn tenant_pattern(prefix: &str, tenant_id: &str) -> String {
        format!("{}:tenant:{}:*", prefix, tenant_id)
    }

    pub fn repo_pattern(prefix: &str, tenant_id: &str, repo_name: &str) -> String {
        format!("{}:repo:{}:{}:*", prefix, tenant_id, repo_name)
    }

    pub fn user_pattern(prefix: &str, user_id: &str) -> String {
        format!("{}:user:{}:*", prefix, user_id)
    }
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_redis_key())
    }
}

impl AsRef<str> for CacheKey {
    fn as_ref(&self) -> &str {
        static EMPTY: &str = "";
        self.parts.last().map(|s| s.as_str()).unwrap_or(EMPTY)
    }
}

pub fn hash_params(params: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(params.as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_key() {
        let key = CacheKey::tenant("agentic-repos", "tenant-123");
        assert_eq!(key.to_redis_key(), "agentic-repos:tenant:tenant-123");
    }

    #[test]
    fn test_repository_key() {
        let key = CacheKey::repository("agentic-repos", "tenant-123", "my-repo");
        assert_eq!(key.to_redis_key(), "agentic-repos:repo:tenant-123:my-repo");
    }

    #[test]
    fn test_session_key() {
        let key = CacheKey::session("agentic-repos", "session-abc");
        assert_eq!(key.to_redis_key(), "agentic-repos:session:session-abc");
    }

    #[test]
    fn test_lfs_object_key() {
        let key = CacheKey::lfs_object("agentic-repos", "tenant-123", "oid123");
        assert_eq!(key.to_redis_key(), "agentic-repos:lfs:tenant-123:oid123");
    }

    #[test]
    fn test_permission_key() {
        let key = CacheKey::permission("agentic-repos", "user-1", "repo:tenant:repo1");
        // Colons in resource are sanitized to underscores
        assert_eq!(
            key.to_redis_key(),
            "agentic-repos:perm:user-1:repo_tenant_repo1"
        );
    }

    #[test]
    fn test_chained_key() {
        let key = CacheKey::new("agentic-repos").with("custom").with("path");
        assert_eq!(key.to_redis_key(), "agentic-repos:custom:path");
    }

    #[test]
    fn test_hash_params() {
        let hash1 = hash_params("page=1&limit=10");
        let hash2 = hash_params("page=1&limit=10");
        let hash3 = hash_params("page=2&limit=10");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn test_tenant_pattern() {
        let pattern = CacheKey::tenant_pattern("agentic-repos", "tenant-123");
        assert_eq!(pattern, "agentic-repos:tenant:tenant-123:*");
    }

    #[test]
    fn test_sanitize_replaces_dangerous_chars() {
        let key = CacheKey::tenant("prefix", "evil:tenant\n*\0");
        assert_eq!(key.to_redis_key(), "prefix:tenant:evil_tenant__");
    }

    #[test]
    fn test_sanitize_empty_string() {
        let key = CacheKey::user("prefix", "");
        assert_eq!(key.to_redis_key(), "prefix:user:_empty_");
    }
}

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    parts: Vec<String>,
}

impl CacheKey {
    pub fn new(prefix: &str) -> Self {
        Self {
            parts: vec![prefix.to_string()],
        }
    }

    pub fn tenant(prefix: &str, tenant_id: &str) -> Self {
        Self {
            parts: vec![
                prefix.to_string(),
                "tenant".to_string(),
                tenant_id.to_string(),
            ],
        }
    }

    pub fn repository(prefix: &str, tenant_id: &str, repo_name: &str) -> Self {
        Self {
            parts: vec![
                prefix.to_string(),
                "repo".to_string(),
                tenant_id.to_string(),
                repo_name.to_string(),
            ],
        }
    }

    pub fn session(prefix: &str, session_id: &str) -> Self {
        Self {
            parts: vec![
                prefix.to_string(),
                "session".to_string(),
                session_id.to_string(),
            ],
        }
    }

    pub fn user(prefix: &str, user_id: &str) -> Self {
        Self {
            parts: vec![prefix.to_string(), "user".to_string(), user_id.to_string()],
        }
    }

    pub fn lfs_object(prefix: &str, tenant_id: &str, oid: &str) -> Self {
        Self {
            parts: vec![
                prefix.to_string(),
                "lfs".to_string(),
                tenant_id.to_string(),
                oid.to_string(),
            ],
        }
    }

    pub fn permission(prefix: &str, user_id: &str, resource: &str) -> Self {
        Self {
            parts: vec![
                prefix.to_string(),
                "perm".to_string(),
                user_id.to_string(),
                resource.to_string(),
            ],
        }
    }

    pub fn api_response(prefix: &str, endpoint: &str, params_hash: &str) -> Self {
        Self {
            parts: vec![
                prefix.to_string(),
                "api".to_string(),
                endpoint.to_string(),
                params_hash.to_string(),
            ],
        }
    }

    pub fn repo_list(prefix: &str, tenant_id: &str, page: u32) -> Self {
        Self {
            parts: vec![
                prefix.to_string(),
                "repos".to_string(),
                tenant_id.to_string(),
                format!("page:{}", page),
            ],
        }
    }

    pub fn branch_list(prefix: &str, tenant_id: &str, repo_name: &str) -> Self {
        Self {
            parts: vec![
                prefix.to_string(),
                "branches".to_string(),
                tenant_id.to_string(),
                repo_name.to_string(),
            ],
        }
    }

    pub fn push(&mut self, part: &str) -> &mut Self {
        self.parts.push(part.to_string());
        self
    }

    pub fn with(mut self, part: &str) -> Self {
        self.parts.push(part.to_string());
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
        assert_eq!(
            key.to_redis_key(),
            "agentic-repos:perm:user-1:repo:tenant:repo1"
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
}

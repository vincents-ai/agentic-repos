//! Tenant namespace isolation

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Tenant namespace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantNamespace {
    pub tenant_id: String,
    pub base_path: PathBuf,
}

/// Validate that a name component is safe to use as a path segment.
///
/// Rejects:
/// - Empty strings
/// - Names containing path separators (`/`, `\`)
/// - Names containing path traversal sequences (`..`)
/// - Names that would resolve outside the expected directory
fn validate_path_component(name: &str, label: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{} must not be empty", label));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!("{} must not contain path separators", label));
    }
    if name == ".." || name.contains("..") {
        return Err(format!(
            "{} must not contain path traversal sequences",
            label
        ));
    }
    // Reject names that start with a dot (hidden files) or contain null bytes
    if name.starts_with('.') || name.contains('\0') {
        return Err(format!("{} contains invalid characters", label));
    }
    Ok(())
}

impl TenantNamespace {
    /// Create a new tenant namespace
    pub fn new(tenant_id: String, base_path: PathBuf) -> Result<Self, String> {
        validate_path_component(&tenant_id, "tenant_id")?;
        Ok(Self {
            tenant_id,
            base_path,
        })
    }

    /// Get repository path
    ///
    /// Validates `repo_name` to prevent path traversal attacks before constructing
    /// the path. Returns an error if the name contains traversal sequences, path
    /// separators, or other unsafe characters.
    pub fn repo_path(&self, repo_name: &str) -> Result<PathBuf, String> {
        validate_path_component(repo_name, "repo_name")?;

        let path = self
            .base_path
            .join(&self.tenant_id)
            .join("repos")
            .join(format!("{}.git", repo_name));

        // Defense-in-depth: lexically verify the path stays under the tenant root.
        // validate_path_component already rejects '..', '/', and '\\', so traversal
        // should be impossible — this catches logic errors in the join chain.
        let tenant_root = self.base_path.join(&self.tenant_id);
        if !path.starts_with(&tenant_root) {
            return Err(
                "repo_path resolves outside tenant namespace — possible path traversal".to_string(),
            );
        }

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_ns() -> TenantNamespace {
        TenantNamespace::new("tenant-1".to_string(), PathBuf::from("/data/repos")).unwrap()
    }

    #[test]
    fn test_valid_repo_path() {
        let ns = make_ns();
        let path = ns.repo_path("my-repo").unwrap();
        assert_eq!(
            path,
            PathBuf::from("/data/repos/tenant-1/repos/my-repo.git")
        );
    }

    #[test]
    fn test_path_traversal_dotdot() {
        let ns = make_ns();
        assert!(ns.repo_path("..").is_err());
        assert!(ns.repo_path("../etc").is_err());
        assert!(ns.repo_path("foo/../../../etc").is_err());
    }

    #[test]
    fn test_path_separator_rejected() {
        let ns = make_ns();
        assert!(ns.repo_path("foo/bar").is_err());
        assert!(ns.repo_path("foo\\bar").is_err());
    }

    #[test]
    fn test_empty_name_rejected() {
        let ns = make_ns();
        assert!(ns.repo_path("").is_err());
    }

    #[test]
    fn test_hidden_name_rejected() {
        let ns = make_ns();
        assert!(ns.repo_path(".hidden").is_err());
    }

    #[test]
    fn test_null_byte_rejected() {
        let ns = make_ns();
        assert!(ns.repo_path("foo\0bar").is_err());
    }

    #[test]
    fn test_tenant_id_validated() {
        assert!(TenantNamespace::new("valid-tenant".to_string(), PathBuf::from("/data")).is_ok());
        assert!(TenantNamespace::new("..".to_string(), PathBuf::from("/data")).is_err());
        assert!(TenantNamespace::new("a/b".to_string(), PathBuf::from("/data")).is_err());
        assert!(TenantNamespace::new("".to_string(), PathBuf::from("/data")).is_err());
    }
}

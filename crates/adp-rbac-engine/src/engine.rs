//! Core RBAC engine — pattern-matching enforcement for service keys.

use crate::checker::{AccessDecision, Operation, PermissionCheck};
use crate::codeowners::CodeownersParser;
use crate::error::RbacError;
use adp_proto::rbac::{KeyPermissions, ServiceType};
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::sync::RwLock;
use tracing::{debug, instrument};

/// The RBAC enforcement engine.
///
/// Compiles glob patterns from `KeyPermissions` into efficient `GlobSet`
/// matchers and provides fast path-based access checks.
pub struct RbacEngine {
    /// Cached compiled glob sets, keyed by key_id.
    compiled: RwLock<HashMap<String, CompiledPatterns>>,
    /// Optional CODEOWNERS parsers, keyed by repo identifier.
    codeowners: RwLock<HashMap<String, CodeownersParser>>,
}

struct CompiledPatterns {
    allowed_write: GlobSet,
    denied_write: GlobSet,
    allowed_read: GlobSet,
}

impl RbacEngine {
    /// Create a new engine.
    pub fn new() -> Self {
        Self {
            compiled: RwLock::new(HashMap::new()),
            codeowners: RwLock::new(HashMap::new()),
        }
    }

    /// Check write access for a path.
    #[instrument(skip(self, permissions), fields(key_id = %permissions.key_id, path = %path))]
    pub fn check_write(
        &self,
        permissions: &KeyPermissions,
        path: &str,
    ) -> AccessDecision {
        // 1. Check expiry
        if let Some(expires_at) = &permissions.expires_at {
            if *expires_at < Utc::now() {
                return AccessDecision::Denied(format!(
                    "key permissions expired at {}",
                    expires_at.to_rfc3339()
                ));
            }
        }

        // 2. Get or compile patterns
        let compiled = self.get_or_compile(permissions);

        // 3. Check denied patterns first (deny takes precedence)
        if compiled.denied_write.is_match(path) {
            debug!(path = path, "Write denied by denied_write pattern");
            return AccessDecision::Denied(format!(
                "path '{}' matches denied pattern for key '{}'",
                path, permissions.key_id
            ));
        }

        // 4. Check allowed patterns
        if compiled.allowed_write.is_match(path) {
            // 5. Check CODEOWNERS path restrictions if configured
            if let Some(restrictions) = &permissions.path_restrictions {
                if restrictions.enforce_codeowners {
                    // CODEOWNERS check would go here — requires repo context
                    // For now, just log
                    debug!(path = path, "CODEOWNERS enforcement active for path");
                }

                // Check explicit path restrictions
                if !restrictions.allowed_paths.is_empty() {
                    let allowed = restrictions.allowed_paths.iter().any(|p| path.starts_with(p) || path == p);
                    if !allowed {
                        return AccessDecision::Denied(format!(
                            "path '{}' not in allowed list for key '{}'",
                            path, permissions.key_id
                        ));
                    }
                }

                if restrictions.denied_paths.iter().any(|p| path.starts_with(p) || path == p) {
                    return AccessDecision::Denied(format!(
                        "path '{}' in explicit denied list for key '{}'",
                        path, permissions.key_id
                    ));
                }
            }

            debug!(path = path, "Write allowed");
            return AccessDecision::Allowed;
        }

        // 5. Default deny
        AccessDecision::Denied(format!(
            "path '{}' does not match any allowed write pattern for key '{}' (service_type={})",
            path, permissions.key_id, permissions.service_type
        ))
    }

    /// Check read access for a path.
    pub fn check_read(
        &self,
        permissions: &KeyPermissions,
        path: &str,
    ) -> AccessDecision {
        // Check expiry
        if let Some(expires_at) = &permissions.expires_at {
            if *expires_at < Utc::now() {
                return AccessDecision::Denied(format!(
                    "key permissions expired at {}",
                    expires_at.to_rfc3339()
                ));
            }
        }

        let compiled = self.get_or_compile(permissions);

        if compiled.allowed_read.is_match(path) {
            AccessDecision::Allowed
        } else {
            AccessDecision::Denied(format!(
                "path '{}' does not match any allowed read pattern for key '{}'",
                path, permissions.key_id
            ))
        }
    }

    /// Check both read and write access in one call.
    pub fn check(
        &self,
        permissions: &KeyPermissions,
        check: &PermissionCheck,
    ) -> AccessDecision {
        match check.operation {
            Operation::Read => self.check_read(permissions, &check.path),
            Operation::Write | Operation::Create => self.check_write(permissions, &check.path),
            Operation::Delete => {
                // Delete requires write access
                self.check_write(permissions, &check.path)
            }
        }
    }

    /// Invalidate the compiled patterns for a key (e.g. after permissions change).
    pub fn invalidate(&self, key_id: &str) {
        let mut compiled = self.compiled.write().unwrap();
        compiled.remove(key_id);
    }

    /// Register a CODEOWNERS parser for a repository.
    pub fn set_codeowners(&self, repo_id: &str, parser: CodeownersParser) {
        let mut codeowners = self.codeowners.write().unwrap();
        codeowners.insert(repo_id.to_string(), parser);
    }

    /// Get the CODEOWNERS parser for a repository.
    pub fn get_codeowners(&self, repo_id: &str) -> Option<CodeownersParser> {
        let codeowners = self.codeowners.read().unwrap();
        codeowners.get(repo_id).cloned()
    }

    /// Create default permissions for a service type.
    pub fn default_permissions_for(
        service_type: ServiceType,
        key_id: String,
        created_by: String,
    ) -> KeyPermissions {
        let now = Utc::now();
        KeyPermissions {
            key_id,
            service_type,
            service_subtype: None,
            allowed_write_patterns: service_type.default_write_patterns(),
            denied_write_patterns: service_type.default_denied_patterns(),
            allowed_read_patterns: vec!["**".to_string()],
            path_restrictions: None,
            created_by,
            created_at: now,
            expires_at: None,
            tags: vec![],
        }
    }

    /// Get or compile the glob patterns for a key.
    fn get_or_compile(&self, permissions: &KeyPermissions) -> CompiledPatterns {
        // Check cache
        {
            let compiled = self.compiled.read().unwrap();
            if let Some(c) = compiled.get(&permissions.key_id) {
                return CompiledPatterns {
                    allowed_write: c.allowed_write.clone(),
                    denied_write: c.denied_write.clone(),
                    allowed_read: c.allowed_read.clone(),
                };
            }
        }

        // Compile
        let new_compiled = Self::compile_patterns(permissions)
            .expect("failed to compile glob patterns");

        // Cache
        {
            let mut compiled = self.compiled.write().unwrap();
            compiled.insert(
                permissions.key_id.clone(),
                CompiledPatterns {
                    allowed_write: new_compiled.allowed_write.clone(),
                    denied_write: new_compiled.denied_write.clone(),
                    allowed_read: new_compiled.allowed_read.clone(),
                },
            );
        }

        new_compiled
    }

    /// Compile glob patterns from permissions into GlobSets.
    fn compile_patterns(permissions: &KeyPermissions) -> Result<CompiledPatterns, RbacError> {
        let allowed_write = Self::build_globset(&permissions.allowed_write_patterns)?;
        let denied_write = Self::build_globset(&permissions.denied_write_patterns)?;
        let allowed_read = Self::build_globset(&permissions.allowed_read_patterns)?;

        Ok(CompiledPatterns {
            allowed_write,
            denied_write,
            allowed_read,
        })
    }

    /// Build a GlobSet from a list of glob patterns.
    fn build_globset(patterns: &[String]) -> Result<GlobSet, RbacError> {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern).map_err(|e| RbacError::InvalidPattern {
                pattern: pattern.clone(),
                reason: e.to_string(),
            })?;
            builder.add(glob);
        }
        builder.build().map_err(|e| RbacError::Internal(e.to_string()))
    }
}

impl Default for RbacEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adp_proto::rbac::PathRestrictions;

    fn make_permissions(
        service_type: ServiceType,
        allowed_write: Vec<&str>,
        denied_write: Vec<&str>,
    ) -> KeyPermissions {
        KeyPermissions {
            key_id: "test-key".to_string(),
            service_type,
            service_subtype: None,
            allowed_write_patterns: allowed_write.into_iter().map(|s| s.to_string()).collect(),
            denied_write_patterns: denied_write.into_iter().map(|s| s.to_string()).collect(),
            allowed_read_patterns: vec!["**".to_string()],
            path_restrictions: None,
            created_by: "admin".to_string(),
            created_at: Utc::now(),
            expires_at: None,
            tags: vec![],
        }
    }

    #[test]
    fn default_permissions_agentic_loop() {
        let perms = RbacEngine::default_permissions_for(
            ServiceType::AgenticLoop,
            "SHA256:abc".to_string(),
            "admin".to_string(),
        );
        assert_eq!(perms.key_id, "SHA256:abc");
        assert_eq!(perms.service_type, ServiceType::AgenticLoop);
        assert!(perms.allowed_write_patterns.contains(&"refs/secrets/agentic-loop/*".to_string()));
    }

    #[test]
    fn write_allowed_in_namespace() {
        let engine = RbacEngine::new();
        let perms = make_permissions(
            ServiceType::AgenticLoop,
            vec!["refs/secrets/agentic-loop/*"],
            vec![],
        );

        let decision = engine.check_write(&perms, "refs/secrets/agentic-loop/instance-1/secret");
        assert!(decision.is_allowed());
    }

    #[test]
    fn write_denied_outside_namespace() {
        let engine = RbacEngine::new();
        let perms = make_permissions(
            ServiceType::AgenticLoop,
            vec!["refs/secrets/agentic-loop/*"],
            vec![],
        );

        let decision = engine.check_write(&perms, "refs/secrets/ci-runners/something");
        assert!(decision.is_denied());
    }

    #[test]
    fn write_denied_overrides_allowed() {
        let engine = RbacEngine::new();
        let perms = make_permissions(
            ServiceType::User,
            vec!["refs/secrets/**"],
            vec!["refs/secrets/agentic-loop/*"],
        );

        // User can write to users namespace
        assert!(engine.check_write(&perms, "refs/secrets/users/alice/key").is_allowed());

        // But denied from agentic-loop namespace
        let decision = engine.check_write(&perms, "refs/secrets/agentic-loop/instance-1/secret");
        assert!(decision.is_denied());
    }

    #[test]
    fn read_allowed_globstar() {
        let engine = RbacEngine::new();
        let perms = make_permissions(
            ServiceType::AgenticLoop,
            vec!["refs/secrets/agentic-loop/*"],
            vec![],
        );

        assert!(engine.check_read(&perms, "anything/at/all").is_allowed());
    }

    #[test]
    fn expired_permissions_denied() {
        let engine = RbacEngine::new();
        let mut perms = make_permissions(
            ServiceType::AgenticLoop,
            vec!["refs/secrets/agentic-loop/*"],
            vec![],
        );
        perms.expires_at = Some(Utc::now() - chrono::Duration::hours(1));

        let decision = engine.check_write(&perms, "refs/secrets/agentic-loop/instance-1/secret");
        assert!(decision.is_denied());
        assert!(decision.denial_reason().unwrap().contains("expired"));
    }

    #[test]
    fn path_restrictions_allowed_list() {
        let engine = RbacEngine::new();
        let mut perms = make_permissions(
            ServiceType::AgenticLoop,
            vec!["refs/secrets/agentic-loop/*"],
            vec![],
        );
        perms.path_restrictions = Some(PathRestrictions {
            codeowners_file: "CODEOWNERS".to_string(),
            allowed_paths: vec!["refs/secrets/agentic-loop/instance-1".to_string()],
            denied_paths: vec![],
            enforce_codeowners: false,
            require_owner_approval: false,
        });

        // Allowed path
        assert!(engine.check_write(&perms, "refs/secrets/agentic-loop/instance-1/key").is_allowed());

        // Not in allowed list
        assert!(engine.check_write(&perms, "refs/secrets/agentic-loop/instance-2/key").is_denied());
    }

    #[test]
    fn invalidate_clears_cache() {
        let engine = RbacEngine::new();
        let perms = make_permissions(
            ServiceType::AgenticLoop,
            vec!["refs/secrets/agentic-loop/*"],
            vec![],
        );

        // First check compiles
        assert!(engine.check_write(&perms, "refs/secrets/agentic-loop/test").is_allowed());

        // Invalidate
        engine.invalidate("test-key");

        // Re-check still works (recompiles)
        assert!(engine.check_write(&perms, "refs/secrets/agentic-loop/test").is_allowed());
    }

    #[test]
    fn check_with_permission_check() {
        let engine = RbacEngine::new();
        let perms = make_permissions(
            ServiceType::AgenticLoop,
            vec!["refs/secrets/agentic-loop/*"],
            vec![],
        );

        let check = PermissionCheck {
            path: "refs/secrets/agentic-loop/test".to_string(),
            operation: Operation::Write,
            service_type: ServiceType::AgenticLoop,
            key_id: "test-key".to_string(),
        };

        assert!(engine.check(&perms, &check).is_allowed());

        let check_denied = PermissionCheck {
            path: "refs/heads/main".to_string(),
            operation: Operation::Write,
            service_type: ServiceType::AgenticLoop,
            key_id: "test-key".to_string(),
        };

        assert!(engine.check(&perms, &check_denied).is_denied());
    }
}

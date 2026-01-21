use crate::git::GitConfig;
use russh_keys::ssh_key::PublicKey;
use std::collections::HashMap;
use std::fs;
use tracing::{debug, error, warn};

#[derive(Debug, Clone)]
pub struct AuthorizedKeysManager {
    authorized_keys: HashMap<String, Vec<String>>,
    config: GitConfig,
}

impl AuthorizedKeysManager {
    pub fn new(config: GitConfig) -> Self {
        Self {
            authorized_keys: HashMap::new(),
            config,
        }
    }

    /// Load authorized keys from file
    pub async fn load_authorized_keys(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let authorized_keys_path = &self.config.authorized_keys_path;

        if !authorized_keys_path.exists() {
            warn!(
                "Authorized keys file does not exist: {:?}",
                authorized_keys_path
            );
            debug!(
                "Creating empty authorized keys file at: {:?}",
                authorized_keys_path
            );

            if let Some(parent) = authorized_keys_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let example_content = "# Authorized SSH public keys
# Format: username:ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@example.com
# Example:
# git:ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC7... git@server.com
";
            fs::write(authorized_keys_path, example_content)?;
            return Ok(());
        }

        let content = fs::read_to_string(authorized_keys_path)
            .map_err(|e| format!("Failed to read authorized keys file: {}", e))?;

        self.parse_authorized_keys(&content)?;
        debug!(
            "Loaded {} users from authorized keys",
            self.authorized_keys.len()
        );

        Ok(())
    }

    /// Parse authorized keys file content
    fn parse_authorized_keys(&mut self, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.authorized_keys.clear();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() != 2 {
                warn!(
                    "Invalid line {} in authorized keys: missing ':' separator",
                    line_num + 1
                );
                continue;
            }

            let username = parts[0].trim();
            let key_part = parts[1].trim();

            if !key_part.starts_with("ssh-") {
                warn!(
                    "Invalid key format on line {}: key must start with 'ssh-'",
                    line_num + 1
                );
                continue;
            }

            self.authorized_keys
                .entry(username.to_string())
                .or_insert_with(Vec::new)
                .push(key_part.to_string());

            debug!("Added authorized key for user: {}", username);
        }

        Ok(())
    }

    /// Validate if a public key is authorized for the given user
    pub fn is_key_authorized(&self, user: &str, public_key: &PublicKey) -> bool {
        let Some(user_keys) = self.authorized_keys.get(user) else {
            debug!("User '{}' not found in authorized keys", user);
            return false;
        };

        let provided_key = match public_key.to_openssh() {
            Ok(key_str) => key_str,
            Err(e) => {
                error!("Failed to convert public key to OpenSSH format: {}", e);
                return false;
            }
        };

        for authorized_key in user_keys {
            if self.keys_match(&provided_key, authorized_key) {
                debug!("Public key authorized for user: {}", user);
                return true;
            }
        }

        debug!("Public key not authorized for user: {}", user);
        false
    }

    /// Compare two SSH public keys for equality
    fn keys_match(&self, provided_key: &str, authorized_key: &str) -> bool {
        let provided_parts: Vec<&str> = provided_key.split_whitespace().collect();
        let authorized_parts: Vec<&str> = authorized_key.split_whitespace().collect();

        if provided_parts.len() < 2 || authorized_parts.len() < 2 {
            return false;
        }

        provided_parts[0] == authorized_parts[0] && provided_parts[1] == authorized_parts[1]
    }

    /// Reload authorized keys from file
    pub async fn reload(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Reloading authorized keys");
        self.load_authorized_keys().await
    }

    /// Get list of authorized users
    pub fn get_authorized_users(&self) -> Vec<String> {
        self.authorized_keys.keys().cloned().collect()
    }

    /// Check if any users are configured
    pub fn has_authorized_users(&self) -> bool {
        !self.authorized_keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn create_test_config(authorized_keys_path: PathBuf) -> GitConfig {
        GitConfig {
            ssh_port: 2222,
            host_key_path: PathBuf::from("test_host_key"),
            authorized_keys_path,
            max_connections: 100,
            connection_timeout_secs: 30,
        }
    }

    #[tokio::test]
    async fn test_parse_authorized_keys() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = "# Test authorized keys file
git:ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC7vbZ... git@server.com
user1:ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGq... user1@client.com

# Another user
user2:ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQD8... user2@client.com";

        std::fs::write(temp_file.path(), content).unwrap();

        let config = create_test_config(temp_file.path().to_path_buf());
        let mut auth_manager = AuthorizedKeysManager::new(config);

        auth_manager.load_authorized_keys().await.unwrap();

        assert!(auth_manager.has_authorized_users());
        assert_eq!(auth_manager.get_authorized_users().len(), 3);
        assert!(auth_manager
            .get_authorized_users()
            .contains(&"git".to_string()));
        assert!(auth_manager
            .get_authorized_users()
            .contains(&"user1".to_string()));
        assert!(auth_manager
            .get_authorized_users()
            .contains(&"user2".to_string()));
    }

    #[tokio::test]
    async fn test_missing_authorized_keys_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nonexistent_path = temp_dir.path().join("nonexistent_authorized_keys");

        let config = create_test_config(nonexistent_path.clone());
        let mut auth_manager = AuthorizedKeysManager::new(config);

        auth_manager.load_authorized_keys().await.unwrap();

        assert!(nonexistent_path.exists());
        assert!(!auth_manager.has_authorized_users());
    }

    #[tokio::test]
    async fn test_invalid_key_format() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = "# Test file with invalid formats
user1:invalid-key-format
user2:ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... valid@key.com
user3:missing-colon-separator
user4:ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... valid@key2.com";

        std::fs::write(temp_file.path(), content).unwrap();

        let config = create_test_config(temp_file.path().to_path_buf());
        let mut auth_manager = AuthorizedKeysManager::new(config);

        auth_manager.load_authorized_keys().await.unwrap();

        assert_eq!(auth_manager.get_authorized_users().len(), 2);
        assert!(auth_manager
            .get_authorized_users()
            .contains(&"user2".to_string()));
        assert!(auth_manager
            .get_authorized_users()
            .contains(&"user4".to_string()));
    }
}

//! Per-repo AGE keypair management.
//!
//! Handles generation, encryption, storage, and retrieval of per-repo AGE keypairs.
//! Uses the two-layer key wrapping scheme from ADR-024:
//! - Layer 1: Root key encrypts per-repo private key
//! - Layer 2: Per-repo keypair used for SOPS secret encryption

use age::secrecy::ExposeSecret;
use age::x25519::{Identity, Recipient};
use std::io::{Read, Write};
use std::path::PathBuf;
use thiserror::Error;

/// Errors for secrets key management.
#[derive(Error, Debug)]
pub enum KeyManagerError {
    #[error("failed to generate keypair: {0}")]
    KeyGeneration(String),

    #[error("failed to encrypt: {0}")]
    Encryption(String),

    #[error("failed to decrypt: {0}")]
    Decryption(String),

    #[error("failed to read/write file: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid key format: {0}")]
    InvalidKey(String),

    #[error("root key not configured")]
    RootKeyNotConfigured,
}

/// Public identity information for a repo.
#[derive(Debug, Clone)]
pub struct RepoKeyPair {
    /// The repo identifier.
    pub repo_id: String,
    /// The public key (age bech32 string).
    pub public_key: String,
}

/// Per-repo key manager.
pub struct RepoKeyManager {
    /// Path to the root wrapping key on the server filesystem.
    root_key_path: PathBuf,
}

impl RepoKeyManager {
    /// Create a new key manager with the given root key path.
    pub fn new(root_key_path: PathBuf) -> Self {
        Self { root_key_path }
    }

    /// Generate a new per-repo AGE keypair.
    ///
    /// Returns the private key string (encrypted with root key) and public key string.
    pub fn generate_keypair(&self, repo_id: &str) -> Result<(Vec<u8>, String), KeyManagerError> {
        let identity = Identity::generate();
        let recipient = identity.to_public();

        let public_key_str = recipient.to_string();
        let private_key_str = identity.to_string().expose_secret().to_string();

        // Encrypt the private key string with the root key
        let encrypted_private_key = self.encrypt_with_root_key(private_key_str.as_bytes())?;

        tracing::info!(
            repo_id = repo_id,
            public_key = %public_key_str,
            "generated new AGE keypair for repo"
        );

        Ok((encrypted_private_key, public_key_str))
    }

    /// Encrypt data with the root key.
    fn encrypt_with_root_key(&self, data: &[u8]) -> Result<Vec<u8>, KeyManagerError> {
        let root_key_str = std::fs::read_to_string(&self.root_key_path)
            .map_err(|_| KeyManagerError::RootKeyNotConfigured)?;

        let root_identity: Identity = root_key_str
            .trim()
            .parse()
            .map_err(|e| KeyManagerError::InvalidKey(format!("failed to parse root key: {}", e)))?;
        let root_recipient = root_identity.to_public();

        let recipients: Vec<Box<dyn age::Recipient>> = vec![Box::new(root_recipient)];
        let encryptor = age::Encryptor::with_recipients(recipients.iter().map(|r| r.as_ref()))
            .map_err(|e| KeyManagerError::Encryption(e.to_string()))?;

        let mut encrypted = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .map_err(|e| KeyManagerError::Encryption(e.to_string()))?;

        writer
            .write_all(data)
            .map_err(|e| KeyManagerError::Encryption(e.to_string()))?;

        writer
            .finish()
            .map_err(|e| KeyManagerError::Encryption(e.to_string()))?;

        Ok(encrypted)
    }

    /// Decrypt data with the root key.
    fn decrypt_with_root_key(&self, encrypted_data: &[u8]) -> Result<Vec<u8>, KeyManagerError> {
        let root_key_str = std::fs::read_to_string(&self.root_key_path)
            .map_err(|_| KeyManagerError::RootKeyNotConfigured)?;

        let root_identity: Identity = root_key_str
            .trim()
            .parse()
            .map_err(|e| KeyManagerError::InvalidKey(format!("failed to parse root key: {}", e)))?;

        let decryptor = age::Decryptor::new(std::io::Cursor::new(encrypted_data))
            .map_err(|e| KeyManagerError::Decryption(format!("invalid encrypted data: {}", e)))?;

        let identities: Vec<Box<dyn age::Identity>> = vec![Box::new(root_identity)];
        let mut output = Vec::new();
        let mut reader = decryptor
            .decrypt(identities.iter().map(|i| i.as_ref()))
            .map_err(|e| KeyManagerError::Decryption(e.to_string()))?;

        reader
            .read_to_end(&mut output)
            .map_err(|e| KeyManagerError::Decryption(e.to_string()))?;

        Ok(output)
    }

    /// Decrypt a repo's private key.
    pub fn decrypt_private_key(&self, encrypted_key: &[u8]) -> Result<String, KeyManagerError> {
        let bytes = self.decrypt_with_root_key(encrypted_key)?;
        String::from_utf8(bytes)
            .map_err(|e| KeyManagerError::InvalidKey(format!("invalid UTF-8 private key: {}", e)))
    }

    /// Parse an age Identity from a private key string.
    pub fn parse_identity(private_key_str: &str) -> Result<Identity, KeyManagerError> {
        private_key_str
            .trim()
            .parse()
            .map_err(|e| KeyManagerError::InvalidKey(format!("invalid private key: {}", e)))
    }

    /// Parse a Recipient from a public key string.
    pub fn parse_recipient(public_key_str: &str) -> Result<Recipient, KeyManagerError> {
        public_key_str
            .trim()
            .parse()
            .map_err(|e| KeyManagerError::InvalidKey(format!("invalid public key: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_root_key(tmpdir: &TempDir) -> PathBuf {
        let key_path = tmpdir.path().join("wrapping-key.age");

        let identity = Identity::generate();
        let identity_str = identity.to_string();
        std::fs::write(&key_path, identity_str.expose_secret()).unwrap();

        key_path
    }

    #[test]
    fn test_keypair_generation() {
        let tmpdir = TempDir::new().unwrap();
        let root_key_path = create_test_root_key(&tmpdir);

        let manager = RepoKeyManager::new(root_key_path);
        let repo_id = "test-repo-123";

        let (encrypted, public_key) = manager.generate_keypair(repo_id).unwrap();

        // Should be able to decrypt the private key
        let private_key_str = manager.decrypt_private_key(&encrypted).unwrap();

        // Private key should be a valid age identity string
        let identity = RepoKeyManager::parse_identity(&private_key_str).unwrap();
        let derived_recipient = identity.to_public();
        assert_eq!(derived_recipient.to_string(), public_key);

        tracing::info!("generated keypair for {}: {}", repo_id, public_key);
    }
}

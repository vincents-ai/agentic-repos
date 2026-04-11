//! Per-repo AGE keypair management.
//!
//! Handles generation, encryption, storage, and retrieval of per-repo AGE keypairs.
//! Uses the two-layer key wrapping scheme from ADR-024:
//! - Layer 1: Root key encrypts per-repo private key
//! - Layer 2: Per-repo keypair used for SOPS secret encryption

use age::{
    identity::Identity,
    Armor, Decryptor, Encryptor, IdentityKey, SecKey, X25519,
};
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
    /// The public key (X25519 as base64).
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
    /// Returns the private key (encrypted with root key) and public key.
    pub fn generate_keypair(&self, repo_id: &str) -> Result<(Vec<u8>, String), KeyManagerError> {
        // Generate a new X25519 secret key
        let secret_key = X25519::generate();

        // Derive the public key
        let public_key = secret_key.public_key();

        // Encode public key as base64
        let public_key_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            public_key.as_bytes(),
        );

        // Encrypt the secret key with the root key
        let encrypted_private_key = self.encrypt_with_root_key(secret_key.as_bytes())?;

        tracing::info!(
            repo_id = repo_id,
            public_key = %public_key_b64,
            "generated new AGE keypair for repo"
        );

        Ok((encrypted_private_key, public_key_b64))
    }

    /// Encrypt data with the root key.
    fn encrypt_with_root_key(&self, data: &[u8]) -> Result<Vec<u8>, KeyManagerError> {
        // Read the root key
        let root_key_data = std::fs::read(&self.root_key_path)
            .map_err(|_| KeyManagerError::RootKeyNotConfigured)?;

        // Parse as age identity
        let identity: Identity<SecKey> = serde_json::from_slice(&root_key_data)
            .map_err(|e| KeyManagerError::InvalidKey(format!("failed to parse root key: {}", e)))?;

        // Create encryptor with the identity
        let encryptor = Encryptor::new_identity(&identity);

        // Encrypt the data
        let mut encrypted = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .map_err(|e| KeyManagerError::Encryption(e.to_string()))?;

        use std::io::Write;
        writer
            .write_all(data)
            .map_err(|e| KeyManagerError::Encryption(e.to_string()))?;

        writer
            .close()
            .map_err(|e| KeyManagerError::Encryption(e.to_string()))?;

        Ok(encrypted)
    }

    /// Decrypt data with the root key.
    fn decrypt_with_root_key(&self, encrypted_data: &[u8]) -> Result<Vec<u8>, KeyManagerError> {
        // Read the root key
        let root_key_data = std::fs::read(&self.root_key_path)
            .map_err(|_| KeyManagerError::RootKeyNotConfigured)?;

        // Parse as age identity
        let identity: Identity<SecKey> = serde_json::from_slice(&root_key_data)
            .map_err(|e| KeyManagerError::InvalidKey(format!("failed to parse root key: {}", e)))?;

        // Create decryptor
        let decryptor = Decryptor::new(encrypted_data)
            .map_err(|e| KeyManagerError::Decryption(format!("invalid encrypted data: {}", e)))?;

        match decryptor {
            Decryptor::Identity(dec) => {
                let mut output = Vec::new();
                let mut reader = dec
                    .decrypt(&identity, None)
                    .map_err(|e| KeyManagerError::Decryption(e.to_string()))?;

                use std::io::Read;
                reader
                    .read_to_end(&mut output)
                    .map_err(|e| KeyManagerError::Decryption(e.to_string()))?;

                Ok(output)
            }
            _ => Err(KeyManagerError::Decryption(
                "expected identity encrypted data".to_string(),
            )),
        }
    }

    /// Decrypt a repo's private key.
    pub fn decrypt_private_key(&self, encrypted_key: &[u8]) -> Result<Vec<u8>, KeyManagerError> {
        self.decrypt_with_root_key(encrypted_key)
    }

    /// Create an age identity from a decrypted private key.
    pub fn create_identity(&self, private_key_bytes: &[u8]) -> Result<Identity<SecKey>, KeyManagerError> {
        let secret_key = X25519::from_slice(private_key_bytes)
            .map_err(|e| KeyManagerError::InvalidKey(format!("invalid private key: {}", e)))?;

        let identity_key = IdentityKey::from(secret_key);
        Ok(identity_key.to_identity())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_root_key(tmpdir: &TempDir) -> PathBuf {
        let key_path = tmpdir.path().join("wrapping-key.age");
        
        // Generate a test identity
        let secret_key = X25519::generate();
        let identity_key = IdentityKey::from(secret_key);
        let identity = identity_key.to_identity();
        
        let identity_json = serde_json::to_vec(&identity).unwrap();
        std::fs::write(&key_path, identity_json).unwrap();
        
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
        let decrypted = manager.decrypt_private_key(&encrypted).unwrap();
        assert_eq!(decrypted.len(), 32); // X25519 secret key is 32 bytes
        
        // Public key should be base64 encoded
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD.decode(&public_key).unwrap();
        assert_eq!(decoded.len(), 32); // X25519 public key is 32 bytes
        
        tracing::info!("generated keypair for {}: {}", repo_id, public_key);
    }
}
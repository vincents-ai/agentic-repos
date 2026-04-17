//! SOPS-compatible secret encryption/decryption.
//!
//! Implements SOPS-style AGE encryption for secret blobs at refs/secrets/*.
//! Uses the per-repo public key to encrypt secrets (zero-knowledge submission).
//! Secrets are stored as JSON with SOPS-compatible format.

use crate::adp::secrets::key_manager::RepoKeyManager;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use thiserror::Error;

/// Errors for SOPS operations.
#[derive(Error, Debug)]
pub enum SopsError {
    #[error("encryption failed: {0}")]
    Encryption(String),

    #[error("decryption failed: {0}")]
    Decryption(String),

    #[error("invalid format: {0}")]
    InvalidFormat(String),

    #[error("no recipients configured")]
    NoRecipients,

    #[error("key not found: {0}")]
    KeyNotFound(String),
}

/// SOPS metadata structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopsMetadata {
    /// Age recipients (public keys).
    #[serde(rename = "age")]
    pub age_recipients: Vec<String>,

    /// Last modification timestamp.
    #[serde(rename = "lastModified", default)]
    pub last_modified: Option<String>,

    /// SOPS version.
    #[serde(rename = "version", default)]
    pub version: Option<String>,
}

/// SOPS encrypted file format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopsEncryptedFile {
    /// SOPS metadata.
    #[serde(rename = "sops")]
    pub sops: SopsMetadata,

    /// Encrypted data (base64-encoded).
    #[serde(rename = "data")]
    pub data: String,
}

/// Secrets manager for encrypting/decrypting secret blobs.
pub struct SecretsManager;

impl SecretsManager {
    /// Create a SOPS-compatible encrypted blob from plaintext.
    ///
    /// Takes the repo's public key (age bech32 recipient string) as the recipient.
    pub fn encrypt(plaintext: &[u8], recipient_public_key: &str) -> Result<String, SopsError> {
        let recipient = RepoKeyManager::parse_recipient(recipient_public_key)
            .map_err(|e| SopsError::InvalidFormat(e.to_string()))?;

        let recipients: Vec<Box<dyn age::Recipient>> = vec![Box::new(recipient)];
        let encryptor = age::Encryptor::with_recipients(recipients.iter().map(|r| r.as_ref()))
            .map_err(|e| SopsError::Encryption(e.to_string()))?;

        let mut ciphertext = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut ciphertext)
            .map_err(|e| SopsError::Encryption(e.to_string()))?;

        writer
            .write_all(plaintext)
            .map_err(|e| SopsError::Encryption(e.to_string()))?;

        writer
            .finish()
            .map_err(|e| SopsError::Encryption(e.to_string()))?;

        // Encode as base64
        let encrypted_data = base64::engine::general_purpose::STANDARD.encode(&ciphertext);

        // Create SOPS structure
        let sops_file = SopsEncryptedFile {
            sops: SopsMetadata {
                age_recipients: vec![recipient_public_key.to_string()],
                last_modified: Some(chrono::Utc::now().to_rfc3339()),
                version: Some("3.9.0".to_string()),
            },
            data: encrypted_data,
        };

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&sops_file)
            .map_err(|e| SopsError::Encryption(e.to_string()))?;

        Ok(json)
    }

    /// Decrypt a SOPS-compatible encrypted blob.
    ///
    /// Takes the private key string and the encrypted JSON.
    pub fn decrypt(encrypted_json: &str, private_key: &str) -> Result<String, SopsError> {
        let sops_file: SopsEncryptedFile = serde_json::from_str(encrypted_json)
            .map_err(|e| SopsError::InvalidFormat(format!("invalid SOPS JSON: {}", e)))?;

        if sops_file.sops.age_recipients.is_empty() {
            return Err(SopsError::NoRecipients);
        }

        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(&sops_file.data)
            .map_err(|e| {
                SopsError::InvalidFormat(format!("invalid encrypted data base64: {}", e))
            })?;

        let identity = RepoKeyManager::parse_identity(private_key)
            .map_err(|e| SopsError::InvalidFormat(e.to_string()))?;

        let decryptor = age::Decryptor::new(std::io::Cursor::new(&ciphertext))
            .map_err(|e| SopsError::Decryption(format!("invalid ciphertext: {}", e)))?;

        let identities: Vec<Box<dyn age::Identity>> = vec![Box::new(identity)];
        let mut output = Vec::new();
        let mut reader = decryptor
            .decrypt(identities.iter().map(|i| i.as_ref()))
            .map_err(|e| SopsError::Decryption(e.to_string()))?;

        reader
            .read_to_end(&mut output)
            .map_err(|e| SopsError::Decryption(e.to_string()))?;

        String::from_utf8(output)
            .map_err(|e| SopsError::Decryption(format!("invalid UTF-8: {}", e)))
    }

    /// Encrypt a map of key-value secrets.
    pub fn encrypt_map(
        secrets: &HashMap<String, String>,
        recipient_public_key: &str,
    ) -> Result<String, SopsError> {
        let json =
            serde_json::to_string(secrets).map_err(|e| SopsError::Encryption(e.to_string()))?;

        Self::encrypt(json.as_bytes(), recipient_public_key)
    }

    /// Decrypt to a map of key-value secrets.
    pub fn decrypt_map(
        encrypted_json: &str,
        private_key: &str,
    ) -> Result<HashMap<String, String>, SopsError> {
        let json = Self::decrypt(encrypted_json, private_key)?;

        let secrets: HashMap<String, String> = serde_json::from_str(&json)
            .map_err(|e| SopsError::InvalidFormat(format!("invalid secrets JSON: {}", e)))?;

        Ok(secrets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use age::secrecy::ExposeSecret;
    use age::x25519::Identity;

    fn generate_test_keypair() -> (String, String) {
        let identity = Identity::generate();
        let recipient = identity.to_public();
        let public_key_str = recipient.to_string();
        let private_key_str = identity.to_string().expose_secret().to_string();
        (public_key_str, private_key_str)
    }

    #[test]
    fn test_encrypt_decrypt() {
        let (public_key, private_key) = generate_test_keypair();

        let plaintext = b"my-secret-value";

        let encrypted = SecretsManager::encrypt(plaintext, &public_key).unwrap();

        let decrypted = SecretsManager::decrypt(&encrypted, &private_key).unwrap();
        assert_eq!(decrypted, "my-secret-value");
    }

    #[test]
    fn test_encrypt_decrypt_map() {
        let (public_key, private_key) = generate_test_keypair();

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "sk-abc123".to_string());
        secrets.insert(
            "database_url".to_string(),
            "postgres://localhost/db".to_string(),
        );

        let encrypted = SecretsManager::encrypt_map(&secrets, &public_key).unwrap();

        let decrypted = SecretsManager::decrypt_map(&encrypted, &private_key).unwrap();
        assert_eq!(decrypted.get("api_key").unwrap(), "sk-abc123");
        assert_eq!(
            decrypted.get("database_url").unwrap(),
            "postgres://localhost/db"
        );
    }
}

//! SOPS-compatible secret encryption/decryption.
//!
//! Implements SOPS-style AGE encryption for secret blobs at refs/secrets/*.
//! Uses the per-repo public key to encrypt secrets (zero-knowledge submission).
//! Secrets are stored as JSON with SOPS-compatible format.
//!
//! ## SOPS Format
//!
//! ```json
//! {
//!   "sops": {
//!     "age": ["age1..."],
//!     "lastModified": "2026-04-11T12:00:00Z",
//!     "version": "3.9.0"
//!   },
//!   "data": "base64-encoded-encrypted-data"
//! }
//! ```

use age::armor::Armored;
use age::recipient::Recipients;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Takes the repo's public key (base64-encoded X25519 key) as the recipient.
    pub fn encrypt(plaintext: &[u8], recipient_public_key: &str) -> Result<String, SopsError> {
        // Parse the public key from base64
        let public_key_bytes = base64::engine::general_purpose::STANDARD
            .decode(recipient_public_key)
            .map_err(|e| SopsError::InvalidFormat(format!("invalid public key base64: {}", e)))?;

        // Create X25519 public key
        let public_key = age::x25519::PublicKey::from_slice(&public_key_bytes)
            .map_err(|e| SopsError::InvalidFormat(format!("invalid public key: {}", e)))?;

        // Create recipient
        let recipient = age::recipient::Recipient::X25519(public_key);

        // Encrypt using age
        let encryptor = age::Encryptor::new_recipient(&recipient);

        let mut ciphertext = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut ciphertext)
            .map_err(|e| SopsError::Encryption(e.to_string()))?;

        use std::io::Write;
        writer
            .write_all(plaintext)
            .map_err(|e| SopsError::Encryption(e.to_string()))?;

        writer
            .close()
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
    /// Takes the decrypted private key bytes and the encrypted JSON.
    pub fn decrypt(encrypted_json: &str, private_key: &[u8]) -> Result<String, SopsError> {
        // Parse the JSON
        let sops_file: SopsEncryptedFile = serde_json::from_str(encrypted_json)
            .map_err(|e| SopsError::InvalidFormat(format!("invalid SOPS JSON: {}", e)))?;

        // Check recipients
        if sops_file.sops.age_recipients.is_empty() {
            return Err(SopsError::NoRecipients);
        }

        // Decode the encrypted data
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(&sops_file.data)
            .map_err(|e| SopsError::InvalidFormat(format!("invalid encrypted data base64: {}", e)))?;

        // Create identity from private key
        let secret_key = age::x25519::SecretKey::from_slice(private_key)
            .map_err(|e| SopsError::InvalidFormat(format!("invalid private key: {}", e)))?;

        let identity = age::identity::Identity::from(secret_key);

        // Decrypt
        let decryptor = age::Decryptor::new(&ciphertext)
            .map_err(|e| SopsError::Decryption(format!("invalid ciphertext: {}", e)))?;

        match decryptor {
            age::Decryptor::Recipient(dec) => {
                let mut output = Vec::new();
                let mut reader = dec
                    .decrypt(&identity, None)
                    .map_err(|e| SopsError::Decryption(e.to_string()))?;

                use std::io::Read;
                reader
                    .read_to_end(&mut output)
                    .map_err(|e| SopsError::Decryption(e.to_string()))?;

                String::from_utf8(output)
                    .map_err(|e| SopsError::Decryption(format!("invalid UTF-8: {}", e)))
            }
            _ => Err(SopsError::Decryption("unexpected decryptor type".to_string())),
        }
    }

    /// Encrypt a map of key-value secrets.
    ///
    /// Returns a SOPS-encrypted JSON string.
    pub fn encrypt_map(
        secrets: &HashMap<String, String>,
        recipient_public_key: &str,
    ) -> Result<String, SopsError> {
        let json = serde_json::to_string(secrets)
            .map_err(|e| SopsError::Encryption(e.to_string()))?;

        Self::encrypt(json.as_bytes(), recipient_public_key)
    }

    /// Decrypt to a map of key-value secrets.
    pub fn decrypt_map(
        encrypted_json: &str,
        private_key: &[u8],
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
    use age::x25519::{PublicKey, SecretKey};

    fn generate_test_keypair() -> (String, Vec<u8>) {
        let secret_key = SecretKey::generate();
        let public_key = PublicKey::from(&secret_key);

        let public_key_b64 = base64::engine::general_purpose::STANDARD.encode(public_key.as_bytes());
        let private_key_bytes = secret_key.as_bytes().to_vec();

        (public_key_b64, private_key_bytes)
    }

    #[test]
    fn test_encrypt_decrypt() {
        let (public_key, private_key) = generate_test_keypair();

        let plaintext = b"my-secret-value";
        
        let encrypted = SecretsManager::encrypt(plaintext, &public_key).unwrap();
        println!("encrypted: {}", encrypted);

        let decrypted = SecretsManager::decrypt(&encrypted, &private_key).unwrap();
        assert_eq!(decrypted, "my-secret-value");
    }

    #[test]
    fn test_encrypt_decrypt_map() {
        let (public_key, private_key) = generate_test_keypair();

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "sk-abc123".to_string());
        secrets.insert("database_url".to_string(), "postgres://localhost/db".to_string());

        let encrypted = SecretsManager::encrypt_map(&secrets, &public_key).unwrap();
        println!("encrypted map: {}", encrypted);

        let decrypted = SecretsManager::decrypt_map(&encrypted, &private_key).unwrap();
        assert_eq!(decrypted.get("api_key").unwrap(), "sk-abc123");
        assert_eq!(decrypted.get("database_url").unwrap(), "postgres://localhost/db");
    }
}
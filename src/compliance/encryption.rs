//! Data encryption at rest for HIPAA compliance.
//
//! Provides AES-256-GCM encryption/decryption for repository data at rest,
//! key management, and encryption verification.

#![allow(dead_code)]

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

/// Encryption key metadata — tracks key creation, rotation, and status.
/// The key material (key_b64) is excluded from serialization to prevent
/// accidental exposure in API responses or logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionKey {
    pub id: Uuid,
    /// Human-readable key name.
    pub name: String,
    /// Base64-encoded 256-bit key. Excluded from serialization.
    #[serde(skip_serializing)]
    pub key_b64: String,
    /// Whether this key is currently active.
    pub active: bool,
    /// Key creation timestamp.
    pub created_at: DateTime<Utc>,
    /// When this key was rotated (replaced by a new key).
    pub rotated_at: Option<DateTime<Utc>>,
    /// SHA-256 fingerprint of the key for verification.
    pub fingerprint: String,
}

impl Drop for EncryptionKey {
    fn drop(&mut self) {
        // Zeroize key material on drop to prevent lingering in memory
        // Use unsafe to get mutable access to the string bytes for zeroing
        unsafe {
            let bytes = self.key_b64.as_bytes_mut();
            for byte in bytes {
                std::ptr::write_volatile(byte, 0);
            }
        }
    }
}

impl EncryptionKey {
    /// Generate a new random AES-256 key.
    pub fn generate(name: impl Into<String>) -> Self {
        let mut key_bytes = [0u8; 32];
        rand_core::RngCore::fill_bytes(&mut OsRng, &mut key_bytes);

        let mut hasher = Sha256::new();
        hasher.update(key_bytes);
        let fingerprint = format!("{:x}", hasher.finalize());

        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            key_b64: BASE64.encode(key_bytes),
            active: true,
            created_at: Utc::now(),
            rotated_at: None,
            fingerprint,
        }
    }

    /// Decode the raw key bytes.
    pub fn decode_bytes(&self) -> Result<[u8; 32], EncryptionError> {
        BASE64
            .decode(&self.key_b64)
            .map_err(|e| EncryptionError::KeyDecode(e.to_string()))
            .and_then(|bytes| {
                let arr: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| EncryptionError::KeyDecode("wrong key length".to_string()))?;
                Ok(arr)
            })
    }
}

/// An encrypted data blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// ID of the key used for encryption.
    pub key_id: Uuid,
    /// Base64-encoded nonce (12 bytes for AES-GCM).
    pub nonce_b64: String,
    /// Base64-encoded ciphertext + tag.
    pub ciphertext_b64: String,
    /// Encryption timestamp.
    pub encrypted_at: DateTime<Utc>,
    // Note: No plaintext hash stored. AES-256-GCM's authentication tag
    // provides integrity verification without leaking plaintext information.
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("Encryption failed: {0}")]
    Encrypt(String),
    #[error("Decryption failed: {0}")]
    Decrypt(String),
    #[error("Key not found: {0}")]
    KeyNotFound(Uuid),
    #[error("Key decode error: {0}")]
    KeyDecode(String),
    #[error("Integrity check failed — plaintext hash mismatch")]
    IntegrityCheckFailed,
    #[error("No active key available")]
    NoActiveKey,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Lock poisoned")]
    LockPoisoned,
}

/// Trait for encryption key storage.
pub trait EncryptionKeyStore: Send + Sync {
    fn add_key(&self, key: EncryptionKey) -> Result<(), EncryptionError>;
    fn get_key(&self, id: Uuid) -> Result<Option<EncryptionKey>, EncryptionError>;
    fn get_active_key(&self) -> Result<Option<EncryptionKey>, EncryptionError>;
    fn list_keys(&self) -> Result<Vec<EncryptionKey>, EncryptionError>;
    fn deactivate_key(&self, id: Uuid) -> Result<(), EncryptionError>;
}

/// In-memory encryption key store.
pub struct InMemoryEncryptionKeyStore {
    keys: std::sync::RwLock<HashMap<Uuid, EncryptionKey>>,
}

impl InMemoryEncryptionKeyStore {
    pub fn new() -> Self {
        Self {
            keys: std::sync::RwLock::new(HashMap::new()),
        }
    }

    pub fn shared() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::new())
    }
}

impl Default for InMemoryEncryptionKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EncryptionKeyStore for InMemoryEncryptionKeyStore {
    fn add_key(&self, key: EncryptionKey) -> Result<(), EncryptionError> {
        let mut keys = self
            .keys
            .write()
            .map_err(|_| EncryptionError::LockPoisoned)?;
        keys.insert(key.id, key);
        Ok(())
    }

    fn get_key(&self, id: Uuid) -> Result<Option<EncryptionKey>, EncryptionError> {
        let keys = self
            .keys
            .read()
            .map_err(|_| EncryptionError::LockPoisoned)?;
        Ok(keys.get(&id).cloned())
    }

    fn get_active_key(&self) -> Result<Option<EncryptionKey>, EncryptionError> {
        let keys = self
            .keys
            .read()
            .map_err(|_| EncryptionError::LockPoisoned)?;
        Ok(keys.values().find(|k| k.active).cloned())
    }

    fn list_keys(&self) -> Result<Vec<EncryptionKey>, EncryptionError> {
        let keys = self
            .keys
            .read()
            .map_err(|_| EncryptionError::LockPoisoned)?;
        Ok(keys.values().cloned().collect())
    }

    fn deactivate_key(&self, id: Uuid) -> Result<(), EncryptionError> {
        let mut keys = self
            .keys
            .write()
            .map_err(|_| EncryptionError::LockPoisoned)?;
        if let Some(key) = keys.get_mut(&id) {
            key.active = false;
            key.rotated_at = Some(Utc::now());
        }
        Ok(())
    }
}

/// Encryption service — encrypts/decrypts data using managed keys.
pub struct EncryptionService {
    key_store: std::sync::Arc<dyn EncryptionKeyStore>,
}

impl EncryptionService {
    pub fn new(key_store: std::sync::Arc<dyn EncryptionKeyStore>) -> Self {
        Self { key_store }
    }

    /// Generate and store a new encryption key.
    pub fn create_key(&self, name: &str) -> Result<EncryptionKey, EncryptionError> {
        let key = EncryptionKey::generate(name);
        self.key_store.add_key(key.clone())?;
        Ok(key)
    }

    /// Encrypt plaintext bytes using the current active key.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData, EncryptionError> {
        let key = self
            .key_store
            .get_active_key()?
            .ok_or(EncryptionError::NoActiveKey)?;

        let key_bytes = key.decode_bytes()?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| EncryptionError::Encrypt(e.to_string()))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        rand_core::RngCore::fill_bytes(&mut OsRng, &mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::Encrypt(e.to_string()))?;

        Ok(EncryptedData {
            key_id: key.id,
            nonce_b64: BASE64.encode(nonce_bytes),
            ciphertext_b64: BASE64.encode(ciphertext),
            encrypted_at: Utc::now(),
        })
    }

    /// Decrypt an encrypted data blob.
    pub fn decrypt(&self, data: &EncryptedData) -> Result<Vec<u8>, EncryptionError> {
        let key = self
            .key_store
            .get_key(data.key_id)?
            .ok_or(EncryptionError::KeyNotFound(data.key_id))?;

        let key_bytes = key.decode_bytes()?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| EncryptionError::Decrypt(e.to_string()))?;

        let nonce_bytes = BASE64
            .decode(&data.nonce_b64)
            .map_err(|e| EncryptionError::Decrypt(e.to_string()))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = BASE64
            .decode(&data.ciphertext_b64)
            .map_err(|e| EncryptionError::Decrypt(e.to_string()))?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| EncryptionError::Decrypt(e.to_string()))?;

        // AES-256-GCM authentication tag already verified integrity.
        // If decrypt() succeeds, the ciphertext was not tampered with.

        Ok(plaintext)
    }

    /// Rotate the encryption key — deactivate current and create a new one.
    pub fn rotate_key(&self, new_key_name: &str) -> Result<EncryptionKey, EncryptionError> {
        // Deactivate current active key
        if let Some(current) = self.key_store.get_active_key()? {
            self.key_store.deactivate_key(current.id)?;
        }
        // Create new key
        self.create_key(new_key_name)
    }

    /// List all keys.
    pub fn list_keys(&self) -> Result<Vec<EncryptionKey>, EncryptionError> {
        self.key_store.list_keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> EncryptionService {
        EncryptionService::new(InMemoryEncryptionKeyStore::shared())
    }

    #[test]
    fn test_create_key() {
        let svc = make_service();
        let key = svc.create_key("test-key").unwrap();
        assert!(key.active);
        assert_eq!(key.name, "test-key");
        assert!(!key.fingerprint.is_empty());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let svc = make_service();
        svc.create_key("enc-key").unwrap();

        let plaintext = b"Patient record: John Doe, DOB 1980-01-15, MRN 123456";
        let encrypted = svc.encrypt(plaintext).unwrap();

        assert_ne!(encrypted.ciphertext_b64, BASE64.encode(plaintext));

        let decrypted = svc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_large_data() {
        let svc = make_service();
        svc.create_key("large-key").unwrap();

        let plaintext = vec![0u8; 1024 * 1024]; // 1MB
        let encrypted = svc.encrypt(&plaintext).unwrap();
        let decrypted = svc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_no_active_key() {
        let svc = make_service();
        let result = svc.encrypt(b"data");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No active key"));
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let svc = make_service();
        let key1 = svc.create_key("key1").unwrap();

        let encrypted = svc.encrypt(b"secret").unwrap();
        assert_eq!(encrypted.key_id, key1.id);

        // Rotate to new key
        let _key2 = svc.rotate_key("key2").unwrap();

        // Should still decrypt because key1 is stored
        let decrypted = svc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, b"secret");
    }

    #[test]
    fn test_key_rotation() {
        let svc = make_service();
        let key1 = svc.create_key("original").unwrap();

        let encrypted_with_key1 = svc.encrypt(b"data1").unwrap();

        let key2 = svc.rotate_key("rotated").unwrap();
        assert_ne!(key1.id, key2.id);

        let encrypted_with_key2 = svc.encrypt(b"data2").unwrap();
        assert_eq!(encrypted_with_key2.key_id, key2.id);

        // Both old and new data should still decrypt
        assert_eq!(svc.decrypt(&encrypted_with_key1).unwrap(), b"data1");
        assert_eq!(svc.decrypt(&encrypted_with_key2).unwrap(), b"data2");
    }

    #[test]
    fn test_key_fingerprint_deterministic() {
        let key = EncryptionKey::generate("test");
        let raw = key.decode_bytes().unwrap();

        let mut hasher = Sha256::new();
        hasher.update(raw);
        let expected = format!("{:x}", hasher.finalize());

        assert_eq!(key.fingerprint, expected);
    }

    #[test]
    fn test_list_keys() {
        let svc = make_service();
        svc.create_key("a").unwrap();
        svc.create_key("b").unwrap();
        let keys = svc.list_keys().unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_integrity_check() {
        let svc = make_service();
        svc.create_key("integrity").unwrap();

        let encrypted = svc.encrypt(b"original data").unwrap();

        // Tamper with the ciphertext — AES-GCM auth tag should reject it
        let mut tampered = encrypted.clone();
        let mut ct_bytes = BASE64.decode(&tampered.ciphertext_b64).unwrap();
        if !ct_bytes.is_empty() {
            ct_bytes[0] ^= 0xFF; // flip bits
        }
        tampered.ciphertext_b64 = BASE64.encode(ct_bytes);

        let result = svc.decrypt(&tampered);
        assert!(result.is_err());
    }
}

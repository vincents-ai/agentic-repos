//! Session PII encryption for Redis-stored session data.
//!
//! Uses AES-256-GCM to encrypt session JSON before writing to Redis.
//! Key sourced from `SESSION_ENCRYPTION_KEY` env var (base64-encoded, 32 bytes).
//! If not set, sessions are stored as plaintext JSON (for dev mode).

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::sync::OnceLock;

/// Magic header to detect encrypted session data.
const MAGIC: &[u8; 6] = b"SESENC";

static SESSION_KEY: OnceLock<Option<Vec<u8>>> = OnceLock::new();

fn get_key() -> Option<&'static [u8]> {
    SESSION_KEY
        .get_or_init(|| match std::env::var("SESSION_ENCRYPTION_KEY") {
            Ok(key_b64) => {
                match BASE64.decode(&key_b64) {
                    Ok(key_bytes) if key_bytes.len() == 32 => Some(key_bytes),
                    Ok(key_bytes) => {
                        tracing::error!(
                            "SESSION_ENCRYPTION_KEY must decode to 32 bytes, got {}",
                            key_bytes.len()
                        );
                        None
                    }
                    Err(e) => {
                        tracing::error!("SESSION_ENCRYPTION_KEY base64 decode failed: {}", e);
                        None
                    }
                }
            }
            Err(_) => {
                tracing::warn!(
                    "SESSION_ENCRYPTION_KEY not set — session PII will NOT be encrypted in Redis"
                );
                None
            }
        })
        .as_deref()
}

/// Encrypt session data. Returns plaintext if no key is configured.
/// Format: `[MAGIC(6)][NONCE(12)][CIPHERTEXT+TAG]`
pub fn encrypt_session(data: &[u8]) -> Result<Vec<u8>, String> {
    let key_bytes = match get_key() {
        Some(k) => k,
        None => return Ok(data.to_vec()),
    };

    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, data)
        .map_err(|e| format!("Session encryption failed: {}", e))?;

    let mut output = Vec::with_capacity(MAGIC.len() + 12 + ciphertext.len());
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

/// Decrypt session data. Returns plaintext if no magic header (backwards compat).
pub fn decrypt_session(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < MAGIC.len() + 12 + 16 {
        return Ok(data.to_vec());
    }

    if &data[..MAGIC.len()] != MAGIC {
        // Plaintext from before encryption was enabled
        return Ok(data.to_vec());
    }

    let key_bytes = match get_key() {
        Some(k) => k,
        None => {
            return Err(
                "Session data is encrypted but SESSION_ENCRYPTION_KEY is not set".to_string(),
            );
        }
    };

    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);

    let nonce = Nonce::from_slice(&data[MAGIC.len()..MAGIC.len() + 12]);
    let ciphertext = &data[MAGIC.len() + 12..];

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Session decryption failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_without_key() {
        // Without SESSION_ENCRYPTION_KEY set, data passes through unchanged
        let data = b"{\"user\":\"test\"}";
        let encrypted = encrypt_session(data).unwrap();
        assert_eq!(encrypted, data);
        let decrypted = decrypt_session(&encrypted).unwrap();
        assert_eq!(decrypted, data);
    }
}

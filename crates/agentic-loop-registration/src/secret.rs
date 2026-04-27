//! Secret generation for service registration.

use crate::error::RegistrationError;
use base64::Engine;
use rand::RngCore;

/// Generates cryptographically secure secrets for service registration.
pub struct SecretGenerator {
    /// Length of generated secrets in bytes (before base64 encoding).
    secret_length: usize,
}

impl SecretGenerator {
    /// Create a new generator with default secret length (32 bytes).
    pub fn new() -> Self {
        Self {
            secret_length: 32,
        }
    }

    /// Create a generator with a custom secret length.
    pub fn with_length(secret_length: usize) -> Self {
        Self { secret_length }
    }

    /// Generate a new random secret (base64-encoded).
    pub fn generate(&self) -> Result<String, RegistrationError> {
        let mut bytes = vec![0u8; self.secret_length];
        rand::rng().fill_bytes(&mut bytes);
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes))
    }

    /// Generate a new instance ID (UUID v4).
    pub fn generate_instance_id() -> uuid::Uuid {
        uuid::Uuid::new_v4()
    }
}

impl Default for SecretGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_unique_secrets() {
        let gen = SecretGenerator::new();
        let s1 = gen.generate().unwrap();
        let s2 = gen.generate().unwrap();
        assert_ne!(s1, s2);
        // Base64 URL-safe no-pad of 32 bytes = 43 chars
        assert_eq!(s1.len(), 43);
    }

    #[test]
    fn generate_instance_id_unique() {
        let id1 = SecretGenerator::generate_instance_id();
        let id2 = SecretGenerator::generate_instance_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn custom_length() {
        let gen = SecretGenerator::with_length(16);
        let secret = gen.generate().unwrap();
        // 16 bytes → 22 chars in URL_SAFE_NO_PAD
        assert_eq!(secret.len(), 22);
    }
}

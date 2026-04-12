//! SSH key management for Skynet v2 plugin

use crate::skynet::error::SkynetPluginResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, error};

/// SSH public key information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKey {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub public_key: String,
    pub key_type: SshKeyType,
    pub fingerprint: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub is_active: bool,
}

/// SSH key types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SshKeyType {
    #[serde(rename = "ssh-rsa")]
    Rsa,
    #[serde(rename = "ssh-ed25519")]
    Ed25519,
    #[serde(rename = "ecdsa-sha2-nistp256")]
    EcdsaNistp256,
    #[serde(rename = "ecdsa-sha2-nistp384")]
    EcdsaNistp384,
}

impl SshKeyType {
    pub fn as_str(&self) -> &str {
        match self {
            SshKeyType::Rsa => "ssh-rsa",
            SshKeyType::Ed25519 => "ssh-ed25519",
            SshKeyType::EcdsaNistp256 => "ecdsa-sha2-nistp256",
            SshKeyType::EcdsaNistp384 => "ecdsa-sha2-nistp384",
        }
    }
}

/// SSH key creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSshKeyRequest {
    pub user_id: String,
    pub name: String,
    pub public_key: String,
    pub expires_at: Option<String>,
}

/// SSH key update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSshKeyRequest {
    pub name: Option<String>,
    pub is_active: Option<bool>,
    pub expires_at: Option<String>,
}

/// SSH key manager
pub struct SshKeyManager;

impl SshKeyManager {
    /// Validate SSH public key format
    pub fn validate_public_key(key_str: &str) -> SkynetPluginResult<SshKeyType> {
        debug!("Validating SSH public key");

        let parts: Vec<&str> = key_str.split_whitespace().collect();
        if parts.is_empty() {
            error!("Invalid SSH public key format: empty");
            return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                "Empty public key".to_string(),
            ));
        }

        let key_type_str = parts[0];
        let key_type = match key_type_str {
            "ssh-rsa" => SshKeyType::Rsa,
            "ssh-ed25519" => SshKeyType::Ed25519,
            "ecdsa-sha2-nistp256" => SshKeyType::EcdsaNistp256,
            "ecdsa-sha2-nistp384" => SshKeyType::EcdsaNistp384,
            _ => {
                error!("Unsupported SSH key type: {}", key_type_str);
                return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                    format!("Unsupported key type: {}", key_type_str),
                ));
            }
        };

        if parts.len() < 2 {
            error!("Invalid SSH public key format: missing key data");
            return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                "Missing key data".to_string(),
            ));
        }

        // Validate base64 encoding of key data
        if base64_decode(parts[1]).is_err() {
            error!("Invalid SSH public key format: invalid base64");
            return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                "Invalid base64 encoding".to_string(),
            ));
        }

        Ok(key_type)
    }

    /// Compute SHA256 fingerprint of SSH public key
    pub fn compute_fingerprint(public_key: &str) -> SkynetPluginResult<String> {
        debug!("Computing SSH key fingerprint");

        let parts: Vec<&str> = public_key.split_whitespace().collect();
        if parts.len() < 2 {
            error!("Invalid SSH public key format for fingerprint");
            return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                "Invalid key format".to_string(),
            ));
        }

        let key_data = base64_decode(parts[1]).map_err(|e| {
            error!("Failed to decode SSH key: {}", e);
            crate::skynet::error::SkynetPluginError::InvalidSshKey(format!(
                "Failed to decode key: {}",
                e
            ))
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&key_data);
        let hash = hasher.finalize();

        // Format as OpenSSH fingerprint: SHA256:base64(hash)
        let fingerprint = format!("SHA256:{}", base64_encode(&hash).trim_end_matches('='));

        Ok(fingerprint)
    }

    /// Create a new SSH key entry
    pub async fn create_key(request: CreateSshKeyRequest) -> SkynetPluginResult<SshKey> {
        debug!("Creating SSH key for user: {}", request.user_id);

        // Validate public key
        let key_type = Self::validate_public_key(&request.public_key)?;

        // Compute fingerprint
        let fingerprint = Self::compute_fingerprint(&request.public_key)?;

        // Validate name
        if request.name.is_empty() || request.name.len() > 255 {
            error!("Invalid SSH key name");
            return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                "Name must be between 1 and 255 characters".to_string(),
            ));
        }

        // Validate expiration if provided
        if let Some(ref expires_at) = request.expires_at {
            if chrono::DateTime::parse_from_rfc3339(expires_at).is_err() {
                error!("Invalid expiration date format");
                return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                    "Invalid expiration date format".to_string(),
                ));
            }
        }

        let key = SshKey {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: request.user_id,
            name: request.name,
            public_key: request.public_key,
            key_type,
            fingerprint,
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: request.expires_at,
            last_used_at: None,
            is_active: true,
        };

        Ok(key)
    }

    /// Check if an SSH key has expired
    pub fn is_expired(key: &SshKey) -> bool {
        if let Some(expires_at) = &key.expires_at {
            if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                return chrono::Utc::now() > expiry.with_timezone(&chrono::Utc);
            }
        }
        false
    }

    /// Verify SSH key is valid for use
    pub fn is_valid(key: &SshKey) -> bool {
        key.is_active && !Self::is_expired(key)
    }

    /// Update SSH key metadata
    pub fn update_key(mut key: SshKey, request: UpdateSshKeyRequest) -> SkynetPluginResult<SshKey> {
        debug!("Updating SSH key: {}", key.id);

        if let Some(name) = request.name {
            if name.is_empty() || name.len() > 255 {
                error!("Invalid SSH key name");
                return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                    "Name must be between 1 and 255 characters".to_string(),
                ));
            }
            key.name = name;
        }

        if let Some(is_active) = request.is_active {
            key.is_active = is_active;
        }

        if let Some(expires_at) = request.expires_at {
            if chrono::DateTime::parse_from_rfc3339(&expires_at).is_err() {
                error!("Invalid expiration date format");
                return Err(crate::skynet::error::SkynetPluginError::InvalidSshKey(
                    "Invalid expiration date format".to_string(),
                ));
            }
            key.expires_at = Some(expires_at);
        }

        Ok(key)
    }

    /// Record key usage
    pub fn record_usage(mut key: SshKey) -> SshKey {
        key.last_used_at = Some(chrono::Utc::now().to_rfc3339());
        key
    }
}

/// Base64 encode helper
fn base64_encode(data: &[u8]) -> String {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b1 = data[i];
        let b2 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b3 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        let n = ((b1 as u32) << 16) | ((b2 as u32) << 8) | (b3 as u32);

        let c1 = (n >> 18) & 0x3f;
        let c2 = (n >> 12) & 0x3f;
        let c3 = (n >> 6) & 0x3f;
        let c4 = n & 0x3f;

        result.push(BASE64_CHARS[c1 as usize] as char);
        result.push(BASE64_CHARS[c2 as usize] as char);

        if i + 1 < data.len() {
            result.push(BASE64_CHARS[c3 as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(BASE64_CHARS[c4 as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

/// Base64 decode helper
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    const BASE64_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = Vec::new();
    let bytes = s.replace('=', "").as_bytes().to_vec();

    for chunk in bytes.chunks(4) {
        let mut n = 0u32;

        for (i, &c) in chunk.iter().enumerate() {
            if let Some(idx) = BASE64_CHARS.find(c as char) {
                n |= (idx as u32) << (6 * (3 - i));
            } else {
                return Err(format!("Invalid base64 character: {}", c as char));
            }
        }

        let len = chunk.len();
        result.push(((n >> 16) & 0xff) as u8);
        if len > 2 {
            result.push(((n >> 8) & 0xff) as u8);
        }
        if len > 3 {
            result.push((n & 0xff) as u8);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_key_type_strings() {
        assert_eq!(SshKeyType::Rsa.as_str(), "ssh-rsa");
        assert_eq!(SshKeyType::Ed25519.as_str(), "ssh-ed25519");
        assert_eq!(SshKeyType::EcdsaNistp256.as_str(), "ecdsa-sha2-nistp256");
        assert_eq!(SshKeyType::EcdsaNistp384.as_str(), "ecdsa-sha2-nistp384");
    }

    #[test]
    fn test_base64_encode_decode() {
        let data = b"Hello, World!";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data.to_vec());
    }

    #[test]
    fn test_key_expiration_check() {
        let mut key = SshKey {
            id: "test".to_string(),
            user_id: "user1".to_string(),
            name: "test-key".to_string(),
            public_key: "ssh-rsa AAAAB3NzaC1yc2E...".to_string(),
            key_type: SshKeyType::Rsa,
            fingerprint: "SHA256:abc123".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: None,
            last_used_at: None,
            is_active: true,
        };

        assert!(!SshKeyManager::is_expired(&key));
        assert!(SshKeyManager::is_valid(&key));

        // Set expiration to past
        key.expires_at = Some((chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339());
        assert!(SshKeyManager::is_expired(&key));
        assert!(!SshKeyManager::is_valid(&key));
    }
}

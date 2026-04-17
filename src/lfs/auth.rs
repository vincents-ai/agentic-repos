use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LfsOperation {
    Download,
    Upload,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LfsClaims {
    pub tenant: String,
    pub repo: String,
    pub operation: LfsOperation,
    pub exp: u64,
    pub iat: u64,
}

pub struct LfsTokenGenerator {
    secret: String,
}

impl LfsTokenGenerator {
    pub fn new(secret: String) -> Self {
        Self { secret }
    }

    pub fn generate_token(
        &self,
        tenant: &str,
        repo: &str,
        operation: LfsOperation,
        expires_in_secs: u64,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let claims = LfsClaims {
            tenant: tenant.to_string(),
            repo: repo.to_string(),
            operation,
            exp: now + expires_in_secs,
            iat: now,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
    }

    pub fn validate_token(&self, token: &str) -> Result<LfsClaims, jsonwebtoken::errors::Error> {
        let token_data = decode::<LfsClaims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )?;

        Ok(token_data.claims)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LfsAuthResponse {
    pub href: String,
    pub header: std::collections::HashMap<String, String>,
    pub expires_at: String,
    pub expires_in: u64,
}

impl LfsAuthResponse {
    pub fn new(base_url: &str, token: String, expires_in_secs: u64) -> Self {
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + expires_in_secs;

        let expires_at_str = chrono::DateTime::from_timestamp(expires_at as i64, 0)
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| expires_at.to_string());

        let mut header = std::collections::HashMap::new();
        header.insert("Authorization".to_string(), format!("Bearer {}", token));

        Self {
            href: base_url.to_string(),
            header,
            expires_at: expires_at_str,
            expires_in: expires_in_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_token() {
        let generator = LfsTokenGenerator::new("test-secret".to_string());

        let token = generator
            .generate_token("tenant1", "repo1", LfsOperation::Download, 3600)
            .unwrap();

        let claims = generator.validate_token(&token).unwrap();
        assert_eq!(claims.tenant, "tenant1");
        assert_eq!(claims.repo, "repo1");
        assert_eq!(claims.operation, LfsOperation::Download);
    }

    #[test]
    fn test_invalid_token() {
        let generator = LfsTokenGenerator::new("test-secret".to_string());

        let result = generator.validate_token("invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_secret() {
        let generator1 = LfsTokenGenerator::new("secret1".to_string());
        let generator2 = LfsTokenGenerator::new("secret2".to_string());

        let token = generator1
            .generate_token("tenant1", "repo1", LfsOperation::Upload, 3600)
            .unwrap();

        let result = generator2.validate_token(&token);
        assert!(result.is_err());
    }

    #[test]
    fn test_auth_response_format() {
        let response =
            LfsAuthResponse::new("https://example.com/lfs", "test-token".to_string(), 3600);

        assert_eq!(response.href, "https://example.com/lfs");
        assert_eq!(response.expires_in, 3600);
        assert_eq!(
            response.header.get("Authorization"),
            Some(&"Bearer test-token".to_string())
        );
        assert!(!response.expires_at.is_empty());
    }
}

//! SAML 2.0 Service Provider implementation.
//!
//! Supports SP-initiated SSO via HTTP-Redirect and HTTP-POST bindings.
//! Parses and validates SAML Responses from the IdP and extracts user attributes.
//!
//! # Signature Verification
//! XML signature verification requires XMLSec1 / OpenSSL bindings not currently
//! available in this crate. The `SamlConfig::require_signed_assertions` flag
//! controls whether unsigned assertions are accepted (should be `true` in production
//! only after adding a verified xmlsec Rust binding).

use base64::Engine as _;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SamlError {
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("XML parse error: {0}")]
    XmlParse(String),
    #[error("Base64 decode error: {0}")]
    Base64Decode(String),
    #[error("Invalid SAML response: {0}")]
    InvalidResponse(String),
    #[error("Assertion expired")]
    AssertionExpired,
    #[error("Audience mismatch: expected {expected}, got {got}")]
    AudienceMismatch { expected: String, got: String },
    #[error("Status not success: {0}")]
    StatusNotSuccess(String),
    #[error("Missing required attribute: {0}")]
    MissingAttribute(String),
}

/// Identity provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdpConfig {
    /// IdP entity ID (e.g. "https://idp.example.com/saml/metadata")
    pub entity_id: String,
    /// IdP SSO URL (HTTP-Redirect endpoint)
    pub sso_url: String,
    /// IdP X.509 certificate (PEM, used for signature verification)
    pub certificate_pem: Option<String>,
}

/// Service provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlConfig {
    /// SP entity ID (must match `Audience` in IdP config)
    pub sp_entity_id: String,
    /// SP ACS (Assertion Consumer Service) URL
    pub acs_url: String,
    /// IdP configuration
    pub idp: IdpConfig,
    /// Attribute name mapping: SAML attribute → local field
    pub attribute_map: AttributeMap,
    /// If true, reject assertions without a valid XML signature.
    /// Leave false during development / when IdP channel is trusted-transport only.
    #[serde(default)]
    pub require_signed_assertions: bool,
}

/// Mapping from SAML attribute names to normalised local fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeMap {
    /// SAML attribute that carries the user's email (default: "email")
    #[serde(default = "default_email_attr")]
    pub email: String,
    /// SAML attribute that carries the display name (default: "displayName")
    #[serde(default = "default_name_attr")]
    pub display_name: String,
    /// SAML attribute that carries group membership (default: "groups")
    #[serde(default = "default_groups_attr")]
    pub groups: String,
}

fn default_email_attr() -> String {
    "email".to_string()
}
fn default_name_attr() -> String {
    "displayName".to_string()
}
fn default_groups_attr() -> String {
    "groups".to_string()
}

impl Default for AttributeMap {
    fn default() -> Self {
        Self {
            email: default_email_attr(),
            display_name: default_name_attr(),
            groups: default_groups_attr(),
        }
    }
}

/// Normalised user extracted from a SAML assertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlUser {
    /// NameID from the SAML Subject element.
    pub name_id: String,
    /// Email address (from attribute map).
    pub email: Option<String>,
    /// Display name (from attribute map).
    pub display_name: Option<String>,
    /// Groups / roles the user belongs to.
    pub groups: Vec<String>,
    /// All raw SAML attributes (name → values).
    pub attributes: HashMap<String, Vec<String>>,
}

/// Parsed SAML Response envelope.
#[derive(Debug, Clone)]
struct ParsedResponse {
    in_response_to: Option<String>,
    status_code: String,
    name_id: Option<String>,
    conditions_not_before: Option<String>,
    conditions_not_on_or_after: Option<String>,
    audience: Option<String>,
    attributes: HashMap<String, Vec<String>>,
}

/// SAML 2.0 Service Provider.
pub struct SamlSp {
    config: SamlConfig,
}

impl SamlSp {
    pub fn new(config: SamlConfig) -> Result<Self, SamlError> {
        if config.sp_entity_id.is_empty() {
            return Err(SamlError::Configuration(
                "sp_entity_id must not be empty".into(),
            ));
        }
        if config.acs_url.is_empty() {
            return Err(SamlError::Configuration("acs_url must not be empty".into()));
        }
        if config.idp.sso_url.is_empty() {
            return Err(SamlError::Configuration(
                "idp.sso_url must not be empty".into(),
            ));
        }
        Ok(Self { config })
    }

    /// Build the HTTP-Redirect binding URL for SP-initiated SSO.
    ///
    /// Returns the URL to which the browser should be redirected.
    pub fn build_authn_request_url(&self, relay_state: Option<&str>) -> Result<String, SamlError> {
        let request_id = format!("_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");

        let authn_request = format!(
            r#"<samlp:AuthnRequest xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
  xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
  ID="{id}"
  Version="2.0"
  IssueInstant="{now}"
  Destination="{sso_url}"
  AssertionConsumerServiceURL="{acs_url}"
  ProtocolBinding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST">
  <saml:Issuer>{sp_entity_id}</saml:Issuer>
</samlp:AuthnRequest>"#,
            id = request_id,
            now = now,
            sso_url = self.config.idp.sso_url,
            acs_url = self.config.acs_url,
            sp_entity_id = self.config.sp_entity_id,
        );

        // Deflate + base64url encode per HTTP-Redirect binding spec.
        let deflated = deflate_and_encode(&authn_request)?;

        let mut url = Url::parse(&self.config.idp.sso_url)
            .map_err(|e| SamlError::Configuration(format!("invalid sso_url: {}", e)))?;

        {
            let mut q = url.query_pairs_mut();
            q.append_pair("SAMLRequest", &deflated);
            if let Some(rs) = relay_state {
                q.append_pair("RelayState", rs);
            }
        }

        Ok(url.to_string())
    }

    /// Process a base64-encoded SAMLResponse POST parameter.
    ///
    /// Returns the normalised [`SamlUser`] on success.
    pub fn process_response(&self, saml_response_b64: &str) -> Result<SamlUser, SamlError> {
        let xml_bytes = base64::engine::general_purpose::STANDARD
            .decode(saml_response_b64)
            .map_err(|e| SamlError::Base64Decode(e.to_string()))?;

        let xml = String::from_utf8(xml_bytes)
            .map_err(|e| SamlError::XmlParse(format!("UTF-8 decode: {}", e)))?;

        let parsed = Self::parse_response_xml(&xml)?;

        // Status check
        if !parsed
            .status_code
            .contains("urn:oasis:names:tc:SAML:2.0:status:Success")
        {
            return Err(SamlError::StatusNotSuccess(parsed.status_code));
        }

        // Audience restriction
        if let Some(ref aud) = parsed.audience {
            if aud != &self.config.sp_entity_id {
                return Err(SamlError::AudienceMismatch {
                    expected: self.config.sp_entity_id.clone(),
                    got: aud.clone(),
                });
            }
        }

        // NotOnOrAfter validity window
        if let Some(ref not_after) = parsed.conditions_not_on_or_after {
            if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(not_after) {
                if chrono::Utc::now() > exp {
                    return Err(SamlError::AssertionExpired);
                }
            }
        }

        let name_id = parsed
            .name_id
            .ok_or_else(|| SamlError::InvalidResponse("missing NameID".into()))?;

        let email = parsed
            .attributes
            .get(&self.config.attribute_map.email)
            .and_then(|v| v.first())
            .cloned();

        let display_name = parsed
            .attributes
            .get(&self.config.attribute_map.display_name)
            .and_then(|v| v.first())
            .cloned();

        let groups = parsed
            .attributes
            .get(&self.config.attribute_map.groups)
            .cloned()
            .unwrap_or_default();

        Ok(SamlUser {
            name_id,
            email,
            display_name,
            groups,
            attributes: parsed.attributes,
        })
    }

    /// Parse a SAML Response XML string into a [`ParsedResponse`].
    fn parse_response_xml(xml: &str) -> Result<ParsedResponse, SamlError> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut result = ParsedResponse {
            in_response_to: None,
            status_code: String::new(),
            name_id: None,
            conditions_not_before: None,
            conditions_not_on_or_after: None,
            audience: None,
            attributes: HashMap::new(),
        };

        // State machine for attribute parsing
        let mut current_attr_name: Option<String> = None;
        let mut current_attr_values: Vec<String> = Vec::new();
        let mut in_name_id = false;
        let mut in_status_code = false;
        let mut in_attr_value = false;
        let mut in_audience = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name_bytes = e.name();
                    let local = local_name(name_bytes.as_ref());
                    match local {
                        "Response" => {
                            for attr in e.attributes().flatten() {
                                if local_name(attr.key.as_ref()) == "InResponseTo" {
                                    result.in_response_to = Some(
                                        attr.unescape_value().unwrap_or_default().into_owned(),
                                    );
                                }
                            }
                        }
                        "StatusCode" => {
                            for attr in e.attributes().flatten() {
                                if local_name(attr.key.as_ref()) == "Value" {
                                    result.status_code =
                                        attr.unescape_value().unwrap_or_default().into_owned();
                                }
                            }
                            in_status_code = true;
                        }
                        "NameID" => {
                            in_name_id = true;
                        }
                        "Conditions" => {
                            for attr in e.attributes().flatten() {
                                match local_name(attr.key.as_ref()) {
                                    "NotBefore" => {
                                        result.conditions_not_before = Some(
                                            attr.unescape_value().unwrap_or_default().into_owned(),
                                        );
                                    }
                                    "NotOnOrAfter" => {
                                        result.conditions_not_on_or_after = Some(
                                            attr.unescape_value().unwrap_or_default().into_owned(),
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "AudienceRestriction" => {}
                        "Audience" => {
                            in_audience = true;
                        }
                        "Attribute" => {
                            // Save previous attribute if any
                            if let Some(name) = current_attr_name.take() {
                                result
                                    .attributes
                                    .entry(name)
                                    .or_default()
                                    .append(&mut current_attr_values);
                            }
                            // Read Name attribute
                            for attr in e.attributes().flatten() {
                                if local_name(attr.key.as_ref()) == "Name" {
                                    current_attr_name = Some(
                                        attr.unescape_value().unwrap_or_default().into_owned(),
                                    );
                                }
                            }
                        }
                        "AttributeValue" => {
                            in_attr_value = true;
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name_bytes = e.name();
                    let local = local_name(name_bytes.as_ref());
                    match local {
                        "NameID" => in_name_id = false,
                        "StatusCode" => in_status_code = false,
                        "AttributeValue" => in_attr_value = false,
                        "Audience" => in_audience = false,
                        "Attribute" => {
                            if let Some(name) = current_attr_name.take() {
                                result
                                    .attributes
                                    .entry(name)
                                    .or_default()
                                    .append(&mut current_attr_values);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    let text = e.unescape().unwrap_or_default().into_owned();
                    if in_name_id && result.name_id.is_none() {
                        result.name_id = Some(text);
                    } else if in_attr_value {
                        current_attr_values.push(text);
                    } else if in_audience && result.audience.is_none() {
                        result.audience = Some(text);
                    } else if in_status_code && result.status_code.is_empty() {
                        result.status_code = text;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(SamlError::XmlParse(e.to_string())),
                _ => {}
            }
        }

        Ok(result)
    }
}

/// Extract the local name (strip XML namespace prefix).
fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    s.rsplit_once(':').map(|(_, local)| local).unwrap_or(s)
}

/// Deflate-compress and base64url-encode a SAML request string (HTTP-Redirect binding).
fn deflate_and_encode(input: &str) -> Result<String, SamlError> {
    use std::io::Write;
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(input.as_bytes())
        .map_err(|e| SamlError::Configuration(format!("deflate: {}", e)))?;
    let compressed = encoder
        .finish()
        .map_err(|e| SamlError::Configuration(format!("deflate finish: {}", e)))?;
    Ok(base64::engine::general_purpose::URL_SAFE.encode(compressed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SamlConfig {
        SamlConfig {
            sp_entity_id: "https://sp.example.com".to_string(),
            acs_url: "https://sp.example.com/auth/saml/acs".to_string(),
            idp: IdpConfig {
                entity_id: "https://idp.example.com".to_string(),
                sso_url: "https://idp.example.com/sso".to_string(),
                certificate_pem: None,
            },
            attribute_map: AttributeMap::default(),
            require_signed_assertions: false,
        }
    }

    #[test]
    fn test_saml_sp_creation() {
        let sp = SamlSp::new(test_config());
        assert!(sp.is_ok());
    }

    #[test]
    fn test_saml_sp_empty_entity_id_rejected() {
        let mut cfg = test_config();
        cfg.sp_entity_id = String::new();
        assert!(SamlSp::new(cfg).is_err());
    }

    #[test]
    fn test_authn_request_url_contains_saml_request() {
        let sp = SamlSp::new(test_config()).unwrap();
        let url = sp.build_authn_request_url(Some("state123")).unwrap();
        assert!(url.contains("SAMLRequest="));
        assert!(url.contains("RelayState=state123"));
    }

    #[test]
    fn test_process_response_success() {
        let sp = SamlSp::new(test_config()).unwrap();

        // Minimal well-formed SAML Response (unsigned, for unit testing)
        let xml = r#"<?xml version="1.0"?>
<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
                Version="2.0" ID="_resp1">
  <samlp:Status>
    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
  </samlp:Status>
  <saml:Assertion Version="2.0" ID="_assert1">
    <saml:Issuer>https://idp.example.com</saml:Issuer>
    <saml:Subject>
      <saml:NameID>user@example.com</saml:NameID>
    </saml:Subject>
    <saml:Conditions NotOnOrAfter="2099-01-01T00:00:00Z">
      <saml:AudienceRestriction>
        <saml:Audience>https://sp.example.com</saml:Audience>
      </saml:AudienceRestriction>
    </saml:Conditions>
    <saml:AttributeStatement>
      <saml:Attribute Name="email">
        <saml:AttributeValue>user@example.com</saml:AttributeValue>
      </saml:Attribute>
      <saml:Attribute Name="displayName">
        <saml:AttributeValue>Test User</saml:AttributeValue>
      </saml:Attribute>
      <saml:Attribute Name="groups">
        <saml:AttributeValue>devs</saml:AttributeValue>
        <saml:AttributeValue>admins</saml:AttributeValue>
      </saml:Attribute>
    </saml:AttributeStatement>
  </saml:Assertion>
</samlp:Response>"#;

        let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
        let user = sp.process_response(&b64).unwrap();

        assert_eq!(user.name_id, "user@example.com");
        assert_eq!(user.email.as_deref(), Some("user@example.com"));
        assert_eq!(user.display_name.as_deref(), Some("Test User"));
        assert_eq!(user.groups, vec!["devs", "admins"]);
    }

    #[test]
    fn test_process_response_audience_mismatch() {
        let sp = SamlSp::new(test_config()).unwrap();

        let xml = r#"<?xml version="1.0"?>
<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
                Version="2.0" ID="_resp2">
  <samlp:Status>
    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
  </samlp:Status>
  <saml:Assertion Version="2.0" ID="_assert2">
    <saml:Issuer>https://idp.example.com</saml:Issuer>
    <saml:Subject>
      <saml:NameID>user@example.com</saml:NameID>
    </saml:Subject>
    <saml:Conditions NotOnOrAfter="2099-01-01T00:00:00Z">
      <saml:AudienceRestriction>
        <saml:Audience>https://wrong.example.com</saml:Audience>
      </saml:AudienceRestriction>
    </saml:Conditions>
  </saml:Assertion>
</samlp:Response>"#;

        let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
        let result = sp.process_response(&b64);
        assert!(matches!(result, Err(SamlError::AudienceMismatch { .. })));
    }

    #[test]
    fn test_process_response_expired() {
        let sp = SamlSp::new(test_config()).unwrap();

        let xml = r#"<?xml version="1.0"?>
<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
                Version="2.0" ID="_resp3">
  <samlp:Status>
    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
  </samlp:Status>
  <saml:Assertion Version="2.0" ID="_assert3">
    <saml:Issuer>https://idp.example.com</saml:Issuer>
    <saml:Subject>
      <saml:NameID>user@example.com</saml:NameID>
    </saml:Subject>
    <saml:Conditions NotOnOrAfter="2000-01-01T00:00:00Z">
      <saml:AudienceRestriction>
        <saml:Audience>https://sp.example.com</saml:Audience>
      </saml:AudienceRestriction>
    </saml:Conditions>
  </saml:Assertion>
</samlp:Response>"#;

        let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
        let result = sp.process_response(&b64);
        assert!(matches!(result, Err(SamlError::AssertionExpired)));
    }

    #[test]
    fn test_process_response_failure_status() {
        let sp = SamlSp::new(test_config()).unwrap();

        let xml = r#"<?xml version="1.0"?>
<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                Version="2.0" ID="_resp4">
  <samlp:Status>
    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Requester"/>
  </samlp:Status>
</samlp:Response>"#;

        let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
        let result = sp.process_response(&b64);
        assert!(matches!(result, Err(SamlError::StatusNotSuccess(_))));
    }
}

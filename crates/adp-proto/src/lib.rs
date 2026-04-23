//! # adp-proto
//!
//! Shared types and traits for the agentic-loop ↔ adp integration.
//!
//! This crate defines the contract between the two systems:
//! - **SSE events** pushed from adp to agentic-loop instances
//! - **Service types** for RBAC key classification
//! - **Key permissions** for namespace-based access control
//! - **Registration info** exchanged during the SSE handshake
//!
//! Types that already exist in `agentic-repos` (Permission, Role, User, RefSchema, UiHints)
//! are NOT duplicated here. This crate only contains new types introduced by the integration.

pub mod rbac;
pub mod registration;
pub mod sse;

pub use rbac::{KeyPermissions, PathRestrictions, ServiceType};
pub use registration::{InstanceCapabilities, RegistrationInfo, ServiceMetadata};
pub mod sealed {
    //! Sealed trait pattern for version-stable trait definitions.
    use std::fmt;

    /// Marker trait for types that can be serialized to the wire format.
    pub trait WireFormat: fmt::Debug + Clone + Send + Sync + serde::Serialize + for<'de> serde::Deserialize<'de> {}
    impl<T: fmt::Debug + Clone + Send + Sync + serde::Serialize + for<'de> serde::Deserialize<'de>> WireFormat for T {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_type_default_namespace() {
        assert_eq!(
            ServiceType::AgenticLoop.default_namespace(),
            "refs/secrets/agentic-loop/"
        );
        assert_eq!(
            ServiceType::CiRunner.default_namespace(),
            "refs/secrets/ci-runners/"
        );
        assert_eq!(
            ServiceType::Integration.default_namespace(),
            "refs/secrets/integrations/"
        );
        assert_eq!(ServiceType::User.default_namespace(), "refs/secrets/users/");
    }

    #[test]
    fn service_type_denied_patterns() {
        let denied = ServiceType::User.default_denied_patterns();
        assert!(denied.contains(&"refs/secrets/agentic-loop/*".to_string()));
        assert!(denied.contains(&"refs/secrets/ci-runners/*".to_string()));
        assert!(denied.contains(&"refs/secrets/integrations/*".to_string()));

        assert!(ServiceType::AgenticLoop.default_denied_patterns().is_empty());
        assert!(ServiceType::CiRunner.default_denied_patterns().is_empty());
    }

    #[test]
    fn sse_event_round_trip() {
        let event = sse::SseEvent::Registered {
            instance_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            ttl_remaining: 3600,
            ttl_expires_at: "2026-04-23T15:30:00Z".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: sse::SseEvent = serde_json::from_str(&json).unwrap();

        if let sse::SseEvent::Registered {
            instance_id,
            ttl_remaining,
            ..
        } = decoded
        {
            assert_eq!(instance_id, "550e8400-e29b-41d4-a716-446655440000");
            assert_eq!(ttl_remaining, 3600);
        } else {
            panic!("Expected Registered event");
        }
    }

    #[test]
    fn registration_info_serialization() {
        let info = RegistrationInfo {
            instance_id: uuid::Uuid::new_v4(),
            secret: "test-secret".to_string(),
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            service_type: ServiceType::AgenticLoop,
            metadata: ServiceMetadata {
                instance_name: "test-machine".to_string(),
                hostname: "localhost".to_string(),
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            capabilities: vec![InstanceCapabilities::Workflows],
            project_ids: vec!["default".to_string()],
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: RegistrationInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.secret, "test-secret");
        assert_eq!(decoded.service_type, ServiceType::AgenticLoop);
    }
}

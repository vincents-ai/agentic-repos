//! Access decision and permission check types.

use adp_proto::rbac::ServiceType;

/// The result of a permission check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessDecision {
    /// Access is allowed.
    Allowed,
    /// Access is denied with a reason.
    Denied(String),
}

impl AccessDecision {
    /// Returns true if access is allowed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, AccessDecision::Allowed)
    }

    /// Returns true if access is denied.
    pub fn is_denied(&self) -> bool {
        matches!(self, AccessDecision::Denied(_))
    }

    /// Returns the denial reason, if denied.
    pub fn denial_reason(&self) -> Option<&str> {
        match self {
            AccessDecision::Denied(reason) => Some(reason),
            AccessDecision::Allowed => None,
        }
    }
}

/// Context for a single permission check.
#[derive(Debug, Clone)]
pub struct PermissionCheck {
    /// The path being accessed (e.g. "refs/secrets/agentic-loop/instance-1/secret").
    pub path: String,
    /// The operation type.
    pub operation: Operation,
    /// The service type of the requesting key.
    pub service_type: ServiceType,
    /// The key identifier (fingerprint).
    pub key_id: String,
}

/// Type of git operation being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// Reading a ref or object.
    Read,
    /// Writing a ref or object.
    Write,
    /// Creating a new ref.
    Create,
    /// Deleting a ref.
    Delete,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operation::Read => write!(f, "read"),
            Operation::Write => write!(f, "write"),
            Operation::Create => write!(f, "create"),
            Operation::Delete => write!(f, "delete"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_decision_helpers() {
        let allowed = AccessDecision::Allowed;
        assert!(allowed.is_allowed());
        assert!(!allowed.is_denied());
        assert!(allowed.denial_reason().is_none());

        let denied = AccessDecision::Denied("no access".to_string());
        assert!(!denied.is_allowed());
        assert!(denied.is_denied());
        assert_eq!(denied.denial_reason(), Some("no access"));
    }

    #[test]
    fn operation_display() {
        assert_eq!(Operation::Read.to_string(), "read");
        assert_eq!(Operation::Write.to_string(), "write");
        assert_eq!(Operation::Create.to_string(), "create");
        assert_eq!(Operation::Delete.to_string(), "delete");
    }
}

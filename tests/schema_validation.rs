//! Tests for schema validation error types

use agentic_repos::adp::schema::SchemaValidationError;

#[test]
fn error_display() {
    let err = SchemaValidationError {
        path: "/title".to_string(),
        message: "Required field missing".to_string(),
        keyword: Some("required".to_string()),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("/title"));
    assert!(msg.contains("Required field missing"));
}

#[test]
fn error_fields_accessible() {
    let err = SchemaValidationError {
        path: "/data/attributes/name".to_string(),
        message: "expected string, got number".to_string(),
        keyword: Some("type".to_string()),
    };
    assert_eq!(err.path, "/data/attributes/name");
    assert_eq!(err.keyword.as_deref(), Some("type"));
    assert!(err.message.contains("string"));
}

#[test]
fn error_no_keyword() {
    let err = SchemaValidationError {
        path: "/".to_string(),
        message: "Root validation error".to_string(),
        keyword: None,
    };
    let msg = format!("{}", err);
    assert!(msg.contains("Root validation error"));
}

//! Tests for API model validation logic

use agentic_repos::api::models::{
    ApiError, CreateRepositoryRequest, CreateUserRequest, UpdateRepositoryRequest,
    UpdateUserRequest,
};

// ── CreateRepositoryRequest validation ──

#[test]
fn repo_create_valid() {
    let req = CreateRepositoryRequest {
        name: "my-repo".to_string(),
        description: Some("A test repo".to_string()),
        is_private: Some(false),
    };
    assert!(req.validate().is_ok());
}

#[test]
fn repo_create_empty_name_rejected() {
    let req = CreateRepositoryRequest {
        name: "".to_string(),
        description: None,
        is_private: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn repo_create_name_too_long_rejected() {
    let req = CreateRepositoryRequest {
        name: "x".repeat(101),
        description: None,
        is_private: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn repo_create_name_100_chars_ok() {
    let req = CreateRepositoryRequest {
        name: "x".repeat(100),
        description: None,
        is_private: None,
    };
    assert!(req.validate().is_ok());
}

#[test]
fn repo_create_description_too_long_rejected() {
    let req = CreateRepositoryRequest {
        name: "ok-name".to_string(),
        description: Some("x".repeat(1001)),
        is_private: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn repo_create_description_1000_chars_ok() {
    let req = CreateRepositoryRequest {
        name: "ok-name".to_string(),
        description: Some("x".repeat(1000)),
        is_private: None,
    };
    assert!(req.validate().is_ok());
}

// ── UpdateRepositoryRequest validation ──

#[test]
fn repo_update_valid() {
    let req = UpdateRepositoryRequest {
        description: Some("updated".to_string()),
        is_private: Some(true),
    };
    assert!(req.validate().is_ok());
}

#[test]
fn repo_update_description_too_long_rejected() {
    let req = UpdateRepositoryRequest {
        description: Some("x".repeat(1001)),
        is_private: None,
    };
    assert!(req.validate().is_err());
}

// ── CreateUserRequest validation ──

#[test]
fn user_create_valid() {
    let req = CreateUserRequest {
        username: "alice".to_string(),
        email: "alice@example.com".to_string(),
        ssh_public_key: Some("ssh-ed25519 AAAA".to_string()),
    };
    assert!(req.validate().is_ok());
}

#[test]
fn user_create_empty_username_rejected() {
    let req = CreateUserRequest {
        username: "".to_string(),
        email: "a@b.com".to_string(),
        ssh_public_key: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn user_create_username_too_long_rejected() {
    let req = CreateUserRequest {
        username: "x".repeat(101),
        email: "a@b.com".to_string(),
        ssh_public_key: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn user_create_empty_email_rejected() {
    let req = CreateUserRequest {
        username: "bob".to_string(),
        email: "".to_string(),
        ssh_public_key: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn user_create_email_no_at_rejected() {
    let req = CreateUserRequest {
        username: "bob".to_string(),
        email: "not-an-email".to_string(),
        ssh_public_key: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn user_create_email_too_long_rejected() {
    let req = CreateUserRequest {
        username: "bob".to_string(),
        email: format!("{}@example.com", "x".repeat(250)),
        ssh_public_key: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn user_create_ssh_key_too_large_rejected() {
    let req = CreateUserRequest {
        username: "bob".to_string(),
        email: "bob@test.com".to_string(),
        ssh_public_key: Some("x".repeat(100001)),
    };
    assert!(req.validate().is_err());
}

// ── UpdateUserRequest validation ──

#[test]
fn user_update_valid() {
    let req = UpdateUserRequest {
        email: Some("new@test.com".to_string()),
        ssh_public_key: Some("ssh-key".to_string()),
    };
    assert!(req.validate().is_ok());
}

#[test]
fn user_update_empty_email_rejected() {
    let req = UpdateUserRequest {
        email: Some("".to_string()),
        ssh_public_key: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn user_update_invalid_email_rejected() {
    let req = UpdateUserRequest {
        email: Some("no-at-sign".to_string()),
        ssh_public_key: None,
    };
    assert!(req.validate().is_err());
}

#[test]
fn user_update_ssh_key_too_large_rejected() {
    let req = UpdateUserRequest {
        email: None,
        ssh_public_key: Some("x".repeat(100001)),
    };
    assert!(req.validate().is_err());
}

// ── ApiError constructors ──

#[test]
fn api_error_bad_request() {
    let err = ApiError::bad_request("test message");
    assert_eq!(err.code, 400);
    assert_eq!(err.error, "bad_request");
    assert_eq!(err.message, "test message");
}

#[test]
fn api_error_not_found() {
    let err = ApiError::not_found("missing");
    assert_eq!(err.code, 404);
    assert_eq!(err.error, "not_found");
}

#[test]
fn api_error_conflict() {
    let err = ApiError::conflict("duplicate");
    assert_eq!(err.code, 409);
    assert_eq!(err.error, "conflict");
}

#[test]
fn api_error_internal_error_no_leak() {
    let err = ApiError::internal_error("sensitive db connection string");
    assert_eq!(err.code, 500);
    assert_eq!(err.message, "An internal error occurred");
    assert!(!err.message.contains("sensitive"));
}

#[test]
fn api_error_display() {
    let err = ApiError::new("test_error", "something went wrong", 422);
    let msg = format!("{}", err);
    assert!(msg.contains("test_error"));
    assert!(msg.contains("something went wrong"));
}

#[test]
fn api_error_serialization() {
    let err = ApiError::bad_request("invalid");
    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("bad_request"));
    assert!(json.contains("400"));
}

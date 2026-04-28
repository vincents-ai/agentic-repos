use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    pub operation: Operation,
    pub transfers: Option<Vec<String>>,
    pub objects: Vec<RequestObject>,
    #[serde(rename = "ref")]
    pub git_ref: Option<GitRef>,
    pub hash_algo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Download,
    Upload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestObject {
    pub oid: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRef {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse {
    pub transfer: Transfer,
    pub objects: Vec<ResponseObject>,
    pub hash_algo: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Transfer {
    #[default]
    Basic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseObject {
    pub oid: String,
    pub size: u64,
    pub authenticated: Option<bool>,
    pub actions: Option<Actions>,
    pub error: Option<ObjectError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actions {
    pub download: Option<Action>,
    pub upload: Option<Action>,
    pub verify: Option<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub href: String,
    pub header: Option<HashMap<String, String>>,
    pub expires_in: Option<u64>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectError {
    pub code: u16,
    pub message: String,
}

impl BatchResponse {
    pub fn new(objects: Vec<ResponseObject>) -> Self {
        Self {
            transfer: Transfer::Basic,
            objects,
            hash_algo: "sha256".to_string(),
        }
    }
}

impl ObjectError {
    pub fn not_found(oid: &str) -> Self {
        Self {
            code: 404,
            message: format!("Object {} not found", oid),
        }
    }

    pub fn invalid_size(oid: &str, expected: u64, actual: u64) -> Self {
        Self {
            code: 422,
            message: format!(
                "Object {} size mismatch: expected {}, got {}",
                oid, expected, actual
            ),
        }
    }

    pub fn too_large(oid: &str, max_size: u64) -> Self {
        Self {
            code: 413,
            message: format!("Object {} exceeds maximum size of {} bytes", oid, max_size),
        }
    }

    pub fn storage_error(oid: &str, error: &str) -> Self {
        Self {
            code: 500,
            message: format!("Storage error for object {}: {}", oid, error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsLock {
    pub id: Uuid,
    pub path: String,
    pub locked_at: DateTime<Utc>,
    pub owner: LfsLockOwner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsLockOwner {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLockRequest {
    pub path: String,
    #[serde(rename = "ref")]
    pub git_ref: Option<GitRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLockResponse {
    pub lock: LfsLock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListLocksResponse {
    pub locks: Vec<LfsLock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteLockRequest {
    #[serde(default)]
    pub force: bool,
    #[serde(rename = "ref")]
    pub git_ref: Option<GitRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteLockResponse {
    pub lock: LfsLock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyLocksRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(rename = "ref")]
    pub git_ref: Option<GitRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyLocksResponse {
    pub ours: Vec<LfsLock>,
    pub theirs: Vec<LfsLock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

impl LockError {
    pub fn already_locked(path: &str) -> Self {
        Self {
            message: format!("File {} is already locked", path),
            request_id: None,
            documentation_url: None,
        }
    }

    pub fn not_found(lock_id: &Uuid) -> Self {
        Self {
            message: format!("Lock {} not found", lock_id),
            request_id: None,
            documentation_url: None,
        }
    }

    pub fn forbidden(reason: &str) -> Self {
        Self {
            message: reason.to_string(),
            request_id: None,
            documentation_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_error_not_found() {
        let err = ObjectError::not_found("abc123");
        assert_eq!(err.code, 404);
        assert!(err.message.contains("abc123"));
    }

    #[test]
    fn test_object_error_too_large() {
        let err = ObjectError::too_large("abc123", 5_000_000_000);
        assert_eq!(err.code, 413);
        assert!(err.message.contains("5"));
    }

    #[test]
    fn test_object_error_invalid_size() {
        let err = ObjectError::invalid_size("abc123", 100, 200);
        assert_eq!(err.code, 422);
        assert!(err.message.contains("100"));
        assert!(err.message.contains("200"));
    }

    #[test]
    fn test_object_error_storage_error() {
        let err = ObjectError::storage_error("abc123", "disk full");
        assert_eq!(err.code, 500);
        assert!(err.message.contains("disk full"));
    }

    #[test]
    fn test_lock_error_already_locked() {
        let err = LockError::already_locked("/path/to/file");
        assert!(err.message.contains("already locked"));
    }

    #[test]
    fn test_lock_error_not_found() {
        let id = uuid::Uuid::new_v4();
        let err = LockError::not_found(&id);
        assert!(err.message.contains(&id.to_string()));
    }

    #[test]
    fn test_lock_error_forbidden() {
        let err = LockError::forbidden("not owner");
        assert!(err.message.contains("not owner"));
    }

    #[test]
    fn test_batch_response_new() {
        let resp = BatchResponse::new(vec![]);
        assert!(resp.objects.is_empty());
        assert_eq!(resp.hash_algo, "sha256");
    }
}

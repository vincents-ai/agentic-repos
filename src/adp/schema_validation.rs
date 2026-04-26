//! Schema-aware pre-receive hook for git push validation.
//!
//! Validates pushed refs against matching RefSchema before accepting.
//! Awaiting schema registry integration.

#![allow(dead_code)]
//! This is the first line of defense for data integrity.

use crate::adp::schema::{SchemaCache, SchemaValidationError};
use std::path::Path;

/// Errors that can occur during schema validation.
#[derive(Debug)]
pub enum SchemaValidationHookError {
    /// Validation failed - returns ref name and all errors
    ValidationFailed {
        ref_name: String,
        errors: Vec<SchemaValidationError>,
    },

    /// Invalid schema definition
    InvalidSchema { schema_id: String, details: String },

    /// Failed to read blob from repo
    BlobReadFailed { ref_name: String, details: String },

    /// JSON parse failed
    InvalidJson { ref_name: String, details: String },
}

impl std::fmt::Display for SchemaValidationHookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationFailed { ref_name, errors } => {
                write!(
                    f,
                    "schema validation failed for ref {}: {} errors",
                    ref_name,
                    errors.len()
                )?;
                for err in errors.iter().take(3) {
                    write!(f, "\n  - {}", err)?;
                }
                if errors.len() > 3 {
                    write!(f, "\n  ... and {} more", errors.len() - 3)?;
                }
                Ok(())
            }
            Self::InvalidSchema { schema_id, details } => {
                write!(
                    f,
                    "invalid schema in refs/adp/schema/{}: {}",
                    schema_id, details
                )
            }
            Self::BlobReadFailed { ref_name, details } => {
                write!(f, "failed to read blob for ref {}: {}", ref_name, details)
            }
            Self::InvalidJson { ref_name, details } => {
                write!(f, "invalid JSON in ref {}: {}", ref_name, details)
            }
        }
    }
}

impl std::error::Error for SchemaValidationHookError {}

/// Pre-receive hook that validates pushed refs against schemas.
///
/// Returns errors for any ref that fails validation. The caller should
/// reject the entire push if any error is returned.
pub fn validate_push(
    repo_id: &str,
    repo_path: &Path,
    schema_cache: &mut SchemaCache,
    ref_updates: &[(String, Option<String>, Option<String>)], // (ref_name, old_oid, new_oid)
) -> Result<Vec<(String, Vec<SchemaValidationError>)>, SchemaValidationHookError> {
    // ref_updates: (refname, old_oid, new_oid) - we care about new_oid
    let mut errors = Vec::new();

    for (refname, _old_oid, new_oid) in ref_updates {
        // Skip deletions or non-existent refs
        let new_oid = match new_oid {
            Some(oid) if !oid.is_empty() && oid != "0000000000000000000000000000000000000000" => {
                oid
            }
            _ => continue, // Skip deletes or null OIDs
        };

        // Skip git-internal refs - these are not data refs
        if !refname.starts_with("refs/engram/")
            && !refname.starts_with("refs/adp/")
            && !refname.starts_with("refs/webhooks/")
            && !refname.starts_with("refs/deploy/")
            && !refname.starts_with("refs/ci/")
            && !refname.starts_with("refs/team/")
        {
            // This is a git-internal ref (heads, tags, notes), not a data ref
            continue;
        }

        // Resolve schema for this ref
        let cached = match schema_cache.resolve(repo_id, refname) {
            Ok(Some(cached)) => cached,
            Ok(None) => continue, // No schema matches - untyped ref, skip
            Err(e) => {
                // Fail-closed: schema resolution errors block the push.
                // This prevents a corrupted/malicious schema from bypassing validation.
                tracing::error!(ref_name = %refname, error = %e, "schema resolution failed during push — blocking");
                return Err(SchemaValidationHookError::BlobReadFailed {
                    ref_name: refname.clone(),
                    details: format!("Schema resolution failed: {}", e),
                });
            }
        };

        // Read the blob from the new commit
        let data = match read_blob_data(repo_path, new_oid) {
            Ok(data) => data,
            Err(e) => {
                // Fail-closed: can't read blob data — block the push
                tracing::error!(ref_name = %refname, error = %e, "failed to read blob data during push — blocking");
                return Err(SchemaValidationHookError::BlobReadFailed {
                    ref_name: refname.clone(),
                    details: format!("Failed to read blob: {}", e),
                });
            }
        };

        // Parse as JSON
        let json: serde_json::Value = match serde_json::from_slice(&data) {
            Ok(v) => v,
            Err(e) => {
                // Schema-validated refs must be valid JSON
                tracing::error!(ref_name = %refname, error = %e, "invalid JSON in schema-validated ref — blocking");
                return Err(SchemaValidationHookError::InvalidJson {
                    ref_name: refname.clone(),
                    details: format!("Invalid JSON: {}", e),
                });
            }
        };

        // Validate against schema
        if let Err(validation_errors) = cached.validator.validate(&json) {
            if !validation_errors.is_empty() {
                errors.push((refname.clone(), validation_errors));
            }
        }
    }

    // If any validation errors, return them all
    if !errors.is_empty() {
        Err(SchemaValidationHookError::ValidationFailed {
            ref_name: errors.first().map(|(n, _)| n.clone()).unwrap_or_default(),
            errors: errors.iter().flat_map(|(_, e)| e.clone()).collect(),
        })
    } else {
        Ok(errors) // Empty = validation passed
    }
}

/// Read a blob from a repository at a specific OID.
fn read_blob_data(repo_path: &Path, oid: &str) -> Result<Vec<u8>, SchemaValidationHookError> {
    use gix::ObjectId;

    let object_id = ObjectId::from_hex(oid.as_bytes()).map_err(|e| {
        SchemaValidationHookError::BlobReadFailed {
            ref_name: oid.to_string(),
            details: format!("invalid OID: {}", e),
        }
    })?;

    let repo = gix::open(repo_path).map_err(|e| SchemaValidationHookError::BlobReadFailed {
        ref_name: oid.to_string(),
        details: format!("failed to open repo: {}", e),
    })?;

    let obj =
        repo.find_object(object_id)
            .map_err(|e| SchemaValidationHookError::BlobReadFailed {
                ref_name: oid.to_string(),
                details: format!("failed to find object: {}", e),
            })?;

    // Check it's a blob
    if obj.kind != gix::objs::Kind::Blob {
        return Err(SchemaValidationHookError::BlobReadFailed {
            ref_name: oid.to_string(),
            details: "not a blob".to_string(),
        });
    }

    // Get the blob and its data
    let blob = obj.into_blob();
    let data = blob.data.to_vec();
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adp::schema::SchemaValidationError;

    fn validation_err(msg: &str) -> SchemaValidationError {
        SchemaValidationError {
            path: "$".to_string(),
            message: msg.to_string(),
            keyword: Some("required".to_string()),
        }
    }

    #[test]
    fn error_display_validation_failed() {
        let err = SchemaValidationHookError::ValidationFailed {
            ref_name: "refs/engram/task/abc".to_string(),
            errors: vec![
                validation_err("title missing"),
                validation_err("id missing"),
                validation_err("status missing"),
                validation_err("priority missing"),
            ],
        };
        let msg = err.to_string();
        assert!(msg.contains("schema validation failed for ref refs/engram/task/abc"));
        assert!(msg.contains("4 errors"));
        assert!(msg.contains("and 1 more"));
    }

    #[test]
    fn error_display_validation_few_errors() {
        let err = SchemaValidationHookError::ValidationFailed {
            ref_name: "refs/test".to_string(),
            errors: vec![validation_err("x missing")],
        };
        let msg = err.to_string();
        assert!(msg.contains("1 errors"));
        assert!(!msg.contains("and"));
    }

    #[test]
    fn error_display_invalid_schema() {
        let err = SchemaValidationHookError::InvalidSchema {
            schema_id: "engram-task".to_string(),
            details: "missing fields".to_string(),
        };
        assert!(err.to_string().contains("invalid schema in refs/adp/schema/engram-task"));
    }

    #[test]
    fn error_display_blob_read_failed() {
        let err = SchemaValidationHookError::BlobReadFailed {
            ref_name: "refs/test".to_string(),
            details: "not found".to_string(),
        };
        assert!(err.to_string().contains("failed to read blob for ref refs/test"));
    }

    #[test]
    fn error_display_invalid_json() {
        let err = SchemaValidationHookError::InvalidJson {
            ref_name: "refs/test".to_string(),
            details: "unexpected token".to_string(),
        };
        assert!(err.to_string().contains("invalid JSON in ref refs/test"));
    }
}

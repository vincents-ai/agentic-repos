//! Entity lifecycle detection for schema-typed refs.
//!
//! Detects create/update/delete of typed entities by diffing old vs new refs
//! during a push. Emits events that can trigger webhooks.

use crate::adp::schema::SchemaCache;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Event types for entity lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EntityEventType {
    /// A new entity was created (ref didn't exist before).
    Created,
    /// An existing entity was updated (ref existed with different content).
    Updated,
    /// An entity was deleted (ref was removed or set to null).
    Deleted,
}

/// Represents a change to an entity during a push.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityChangeEvent {
    /// The type of change.
    pub event_type: EntityEventType,

    /// The schema ID that matched this ref (e.g. "engram-task").
    pub schema_id: String,

    /// The full ref name (e.g. "refs/engram/task/abc123").
    pub ref_name: String,

    /// The entity ID extracted from the ref (e.g. "abc123").
    pub entity_id: String,

    /// The new data (for created/updated events).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_data: Option<serde_json::Value>,

    /// The old data (for updated/deleted events).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_data: Option<serde_json::Value>,

    /// The actor who made the change (from caller identity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,

    /// Timestamp of the event.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Detect entity changes by diffing old vs new refs.
///
/// Takes:
/// - repo_id: identifier for the repo (for cache lookup)
/// - repo_path: path to the repository
/// - schema_cache: schema cache for resolving schemas
/// - old_refs: map of ref_name -> OID (before push)
/// - new_refs: map of ref_name -> OID (after push)
///
/// Returns a list of all entity changes that occurred.
pub fn detect_entity_changes(
    repo_id: &str,
    repo_path: &Path,
    schema_cache: &mut SchemaCache,
    old_refs: &[(String, Option<String>)], // (ref_name, old_oid)
    new_refs: &[(String, Option<String>)], // (ref_name, new_oid)
    actor: Option<&str>,
) -> Result<Vec<EntityChangeEvent>, String> {
    // Build maps for efficient lookup
    let old_map: std::collections::HashMap<&str, &str> = old_refs
        .iter()
        .filter_map(|(name, oid)| oid.as_ref().map(|o| (name.as_str(), o.as_str())))
        .collect();

    let new_map: std::collections::HashMap<&str, &str> = new_refs
        .iter()
        .filter_map(|(name, oid)| oid.as_ref().map(|o| (name.as_str(), o.as_str())))
        .collect();

    let mut events = Vec::new();
    let timestamp = chrono::Utc::now();

    // Find all refs that existed in old or new
    let all_refs: std::collections::HashSet<&str> =
        old_map.keys().chain(new_map.keys()).copied().collect();

    for refname in all_refs {
        let old_oid = old_map.get(refname);
        let new_oid = new_map.get(refname);

        // Determine the change type
        let (event_type, old_data, new_data) = match (old_oid, new_oid) {
            (None, Some(new)) => {
                // Created - new ref didn't exist before
                let new_data = read_blob_json(repo_path, new)?;
                (EntityEventType::Created, None, new_data)
            }
            (Some(old), None) => {
                // Deleted - ref was removed
                let old_data = read_blob_json(repo_path, old)?;
                (EntityEventType::Deleted, old_data, None)
            }
            (Some(old), Some(new)) if old != new => {
                // Updated - OID changed, need to compare content
                let old_json = read_blob_json(repo_path, old)?;
                let new_json = read_blob_json(repo_path, new)?;
                if old_json.is_some() && new_json.is_some() && old_json != new_json {
                    (EntityEventType::Updated, old_json, new_json)
                } else {
                    continue; // Same content or not JSON, no event needed
                }
            }
            _ => continue, // No change or same in both
        };

        // Try to resolve a schema for this ref
        if let Ok(Some(cached)) = schema_cache.resolve(repo_id, refname) {
            // Extract entity ID from ref name using the schema's namespace_pattern
            let entity_id = extract_entity_id(refname, &cached.schema.namespace_pattern);

            events.push(EntityChangeEvent {
                event_type,
                schema_id: cached.schema.id.clone(),
                ref_name: refname.to_string(),
                entity_id,
                new_data,
                old_data,
                actor: actor.map(String::from),
                timestamp,
            });
        }
        // If no schema matches, skip - this is an untyped ref
    }

    Ok(events)
}

/// Extract the entity ID from a ref name based on the namespace pattern.
///
/// Example: refname = "refs/engram/task/abc123", pattern = "refs/engram/task/*"
/// Returns: "abc123"
pub fn extract_entity_id(refname: &str, namespace_pattern: &str) -> String {
    // Handle the simple case: pattern ends with /* or /**
    if let Some(rest) = namespace_pattern
        .strip_suffix("/*")
        .and_then(|prefix| refname.strip_prefix(prefix))
    {
        return rest.trim_start_matches('/').to_string();
    }
    if let Some(rest) = namespace_pattern
        .strip_suffix("/**")
        .and_then(|prefix| refname.strip_prefix(prefix))
    {
        return rest.trim_start_matches('/').to_string();
    }
    // Fallback: use the last path component
    refname.split('/').next_back().unwrap_or(refname).to_string()
}

/// Read a blob and parse it as JSON.
fn read_blob_json(repo_path: &Path, oid: &str) -> Result<Option<serde_json::Value>, String> {
    use gix::ObjectId;

    let object_id =
        ObjectId::from_hex(oid.as_bytes()).map_err(|e| format!("invalid OID: {}", e))?;

    let repo = gix::open(repo_path).map_err(|e| format!("failed to open repo: {}", e))?;

    let obj = match repo.find_object(object_id) {
        Ok(o) => o,
        Err(_) => return Ok(None), // Object doesn't exist
    };

    // Check it's a blob
    if obj.kind != gix::objs::Kind::Blob {
        return Ok(None);
    }

    // Try to parse as JSON
    match serde_json::from_slice(&obj.data) {
        Ok(v) => Ok(Some(v)),
        Err(_) => Ok(None), // Not JSON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_entity_id_single_wildcard() {
        let id = extract_entity_id("refs/engram/task/abc123", "refs/engram/task/*");
        assert_eq!(id, "abc123");
    }

    #[test]
    fn test_extract_entity_id_double_wildcard() {
        let id = extract_entity_id("refs/engram/task/abc123/sub/xyz", "refs/engram/task/**");
        assert_eq!(id, "abc123/sub/xyz");
    }

    #[test]
    fn test_extract_entity_id_fallback() {
        let id = extract_entity_id("refs/engram/task/abc123", "refs/engram/task");
        // Falls back to last component
        assert_eq!(id, "abc123");
    }
}

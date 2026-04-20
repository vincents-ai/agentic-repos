//! ADP RefSchema types.
//!
//! A `RefSchema` is stored as a JSON blob at `refs/adp/schema/{id}` and
//! describes the structure of any git ref whose name matches
//! `namespace_pattern`, as well as rendering hints for the web UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// A ref-namespace schema definition stored at `refs/adp/schema/{id}`.
///
/// The `namespace_pattern` uses glob syntax where `*` matches a single path
/// segment and `**` matches across separators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefSchema {
    /// Unique slug identifier within this repo (e.g. `engram-task`).
    pub id: String,

    /// Glob pattern that matches the ref names this schema describes.
    /// Example: `refs/engram/task/*`
    pub namespace_pattern: String,

    /// Human-readable name (e.g. `Engram Task`).
    pub title: String,

    /// Optional description of what refs in this namespace represent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Semver string (e.g. `1.0.0`).
    pub version: String,

    /// Optional author string (e.g. `engram-cli/1.0.0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// When this schema was first created.
    pub created_at: DateTime<Utc>,

    /// When this schema was last updated.
    pub updated_at: DateTime<Utc>,

    /// JSON Schema (draft 2020-12) describing the blob pointed to by matched
    /// refs.
    pub schema: serde_json::Value,

    /// Optional UI rendering hints for the web UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiHints>,
}

/// UI rendering hints embedded in a `RefSchema`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiHints {
    /// View template name (e.g. `entity-list`, `entity-detail`, `task-board`,
    /// `pr-list`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,

    /// Field names to display in list view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub list_fields: Vec<String>,

    /// Field names to display in detail view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail_fields: Vec<String>,

    /// Per-field widget configuration keyed by field name.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, FieldConfig>,
}

/// Per-field widget configuration within `UiHints`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldConfig {
    /// Widget type (e.g. `text`, `badge`, `date`, `select`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,

    /// Colour map for enum/badge fields: value → CSS colour string.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub color_map: HashMap<String, String>,
}

// ── Schema Validator ──────────────────────────────────────────────────────

/// A single validation error from a schema check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaValidationError {
    /// JSON pointer to the offending element (e.g. `/data/attributes/title`).
    pub path: String,
    /// Human-readable description of what went wrong.
    pub message: String,
    /// The JSON Schema keyword that triggered the error (e.g. `type`, `required`).
    pub keyword: Option<String>,
}

impl std::fmt::Display for SchemaValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.keyword {
            Some(kw) => write!(f, "{}: {} ({})", self.path, self.message, kw),
            None => write!(f, "{}: {}", self.path, self.message),
        }
    }
}

/// Compiled JSON Schema validator.
///
/// Wraps the `jsonschema` crate's compiled validator in an `Arc` so it can
/// be shared across threads cheaply (schema compilation is expensive,
/// validation is cheap).
///
/// Memory budget: each compiled validator is ~1-5 KB depending on schema
/// complexity. Bounded by the LRU cache in `SchemaCache` (100 entries).
#[derive(Clone)]
pub struct SchemaValidator {
    inner: Arc<jsonschema::Validator>,
    /// The JSON Schema value this validator was compiled from (kept for cache key).
    schema_json: serde_json::Value,
}

impl SchemaValidator {
    /// Compile a JSON Schema into a validator.
    ///
    /// Supports drafts 4, 6, 7, 2019-09, 2020-12.
    /// Returns an error if the schema itself is invalid.
    pub fn compile(schema: &serde_json::Value) -> Result<Self, String> {
        let compiled =
            jsonschema::validator_for(schema).map_err(|e| format!("invalid JSON Schema: {}", e))?;
        Ok(Self {
            inner: Arc::new(compiled),
            schema_json: schema.clone(),
        })
    }

    /// Validate data against the compiled schema.
    ///
    /// Returns `Ok(())` if valid, or a list of all validation errors.
    /// Returns early errors (e.g. malformed JSON) as a single-item list.
    pub fn validate(&self, data: &serde_json::Value) -> Result<(), Vec<SchemaValidationError>> {
        // Use iter_errors to get all validation errors
        let errors = self.inner.iter_errors(data);
        let mut collected = Vec::new();
        for err in errors {
            collected.push(SchemaValidationError {
                path: err.instance_path().to_string(),
                message: err.to_string(),
                keyword: None,
            });
        }
        if collected.is_empty() {
            Ok(())
        } else {
            Err(collected)
        }
    }

    /// Returns the JSON Schema this validator was compiled from.
    pub fn schema_json(&self) -> &serde_json::Value {
        &self.schema_json
    }
}

impl std::fmt::Debug for SchemaValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaValidator")
            .field("schema_json", &self.schema_json)
            .finish()
    }
}

// ── Schema Cache (Two-Tier LRU) ─────────────────────────────────────────────

use lru::LruCache;

/// A cached schema with compiled validator, ready for use.
///
/// Memory: ~1-5KB per entry (RefSchema + compiled validator).
/// Bounded by LRU cache (100 entries = ~100KB max).
#[derive(Clone)]
pub struct CachedSchema {
    /// The schema definition.
    pub schema: RefSchema,
    /// Compiled validator for fast validation.
    pub validator: SchemaValidator,
    /// Timestamp for TTL tracking.
    pub cached_at: std::time::Instant,
}

/// Two-tier schema cache for horizontal scaling.
///
/// **Tier 1 (System)**: Schemas from `_adp/system` repo - loaded once, ~20KB constant.
/// **Tier 2 (Custom)**: Per-repo custom schemas - LRU cache, 100 entries, 60s TTL.
///
/// Memory budget: ~120KB regardless of repo count (ADR-032).
pub struct SchemaCache {
    /// Tier 1: System schemas (loaded once).
    system_schemas: Vec<CachedSchema>,
    /// Tier 2: Per-repo custom schemas (bounded LRU).
    custom_schemas: LruCache<(String, String), CachedSchema>, // (repo_id, schema_id) -> CachedSchema
    /// TTL for custom schema entries (60 seconds).
    ttl_seconds: u64,
}

impl SchemaCache {
    /// Create a new empty cache.
    ///
    /// - System schemas: load from system repo on first use
    /// - Custom schemas: bounded to 100 entries, TTL 60s
    pub fn new() -> Self {
        Self {
            system_schemas: Vec::new(),
            custom_schemas: LruCache::new(100.try_into().unwrap()),
            ttl_seconds: 60,
        }
    }

    /// Load system schemas from a system repo path.
    ///
    /// Called once on node startup or when system repo changes.
    pub fn load_system_schemas(
        &mut self,
        system_repo_path: &std::path::Path,
    ) -> Result<(), String> {
        let schemas = crate::adp::storage::AdpStore::list_schemas(system_repo_path)
            .map_err(|e| format!("failed to list system schemas: {}", e))?;

        let mut cached = Vec::new();
        for schema in schemas {
            let validator = SchemaValidator::compile(&schema.schema)
                .map_err(|e| format!("invalid system schema {}: {}", schema.id, e))?;
            cached.push(CachedSchema {
                schema,
                validator,
                cached_at: std::time::Instant::now(),
            });
        }
        self.system_schemas = cached;
        tracing::info!(count = self.system_schemas.len(), "loaded system schemas");
        Ok(())
    }

    /// Resolve a ref name to the most specific matching schema.
    ///
    /// 1. Check custom schemas (Tier 2) for exact (repo_id, schema_id) match
    /// 2. If miss, try to load from repo's refs/adp/schema/*
    /// 3. Merge with system schemas (Tier 1) as fallback
    /// 4. Use glob::resolve_schema to find most specific match
    pub fn resolve(
        &mut self,
        repo_id: &str,
        refname: &str,
    ) -> Result<Option<CachedSchema>, String> {
        // Collect all available schemas: system + custom (filtered to this repo)
        let mut all_schemas: Vec<RefSchema> = self
            .system_schemas
            .iter()
            .map(|s| s.schema.clone())
            .collect();

        // Add custom schemas for this repo
        for ((rid, _), cached) in self.custom_schemas.iter() {
            if rid == repo_id {
                all_schemas.push(cached.schema.clone());
            }
        }

        // Use glob::resolve_schema to find the most specific match
        let matched = crate::adp::glob::resolve_schema(&all_schemas, refname);

        if let Some(matched_schema) = matched {
            // Check custom cache first
            let key = (repo_id.to_string(), matched_schema.id.clone());
            if let Some(cached) = self.custom_schemas.get(&key) {
                // Check TTL
                let age = cached.cached_at.elapsed().as_secs();
                if age < self.ttl_seconds {
                    return Ok(Some(cached.clone()));
                }
                // Expired - remove it
                self.custom_schemas.pop(&key);
            }
            // Compile and cache
            let validator = SchemaValidator::compile(&matched_schema.schema)
                .map_err(|e| format!("invalid schema {}: {}", matched_schema.id, e))?;
            let cached = CachedSchema {
                schema: matched_schema.clone(),
                validator,
                cached_at: std::time::Instant::now(),
            };
            let key = (repo_id.to_string(), matched_schema.id.clone());
            let _ = self.custom_schemas.push(key, cached.clone());
            return Ok(Some(cached));
        }

        // No schema matches this ref
        Ok(None)
    }

    /// Get a schema by ID, checking both tiers.
    pub fn get(&mut self, repo_id: &str, schema_id: &str) -> Option<CachedSchema> {
        // Check custom cache first
        let key = (repo_id.to_string(), schema_id.to_string());
        if let Some(cached) = self.custom_schemas.get(&key) {
            // Check TTL
            let age = cached.cached_at.elapsed().as_secs();
            if age < self.ttl_seconds {
                return Some(cached.clone());
            }
            // Expired - remove it
            self.custom_schemas.pop(&key);
        }
        // Check system schemas
        self.system_schemas
            .iter()
            .find(|s| s.schema.id == schema_id)
            .cloned()
    }

    /// Invalidate a specific cached schema.
    pub fn invalidate(&mut self, repo_id: &str, schema_id: &str) {
        let key = (repo_id.to_string(), schema_id.to_string());
        self.custom_schemas.pop(&key);
    }

    /// Clear all custom schemas (system schemas persist).
    pub fn clear_custom(&mut self) {
        self.custom_schemas.clear();
    }

    /// Get the count of cached items (for metrics).
    pub fn len(&self) -> usize {
        self.custom_schemas.len() + self.system_schemas.len()
    }
}

impl Default for SchemaCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_round_trip() {
        let schema = RefSchema {
            id: "engram-task".to_string(),
            namespace_pattern: "refs/engram/task/*".to_string(),
            title: "Engram Task".to_string(),
            description: Some("An engram task entity".to_string()),
            version: "1.0.0".to_string(),
            author: Some("engram-cli/1.0.0".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            schema: serde_json::json!({ "type": "object" }),
            ui: Some(UiHints {
                view: Some("entity-list".to_string()),
                list_fields: vec!["title".to_string(), "status".to_string()],
                detail_fields: vec!["title".to_string(), "description".to_string()],
                fields: {
                    let mut m = HashMap::new();
                    m.insert(
                        "status".to_string(),
                        FieldConfig {
                            widget: Some("badge".to_string()),
                            color_map: {
                                let mut c = HashMap::new();
                                c.insert("done".to_string(), "green".to_string());
                                c
                            },
                        },
                    );
                    m
                },
            }),
        };

        let json = serde_json::to_string(&schema).unwrap();
        let decoded: RefSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "engram-task");
        assert_eq!(decoded.namespace_pattern, "refs/engram/task/*");
        assert!(decoded.ui.is_some());
    }

    #[test]
    fn test_schema_validator_valid() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "status": { "type": "string", "enum": ["todo", "in_progress", "done"] }
            },
            "required": ["title", "status"]
        });
        let validator = SchemaValidator::compile(&schema).expect("schema should compile");

        let valid_data = serde_json::json!({
            "title": "Test task",
            "status": "todo"
        });
        assert!(validator.validate(&valid_data).is_ok());
    }

    #[test]
    fn test_schema_validator_invalid() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "status": { "type": "string", "enum": ["todo", "in_progress", "done"] }
            },
            "required": ["title", "status"]
        });
        let validator = SchemaValidator::compile(&schema).expect("schema should compile");

        // Missing required field
        let invalid_data = serde_json::json!({
            "title": "Test task"
        });
        let result = validator.validate(&invalid_data);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(!errors.is_empty());
        // Error should mention 'status' field
        let msg = errors.first().map(|e| e.message.as_str()).unwrap_or("");
        assert!(
            msg.contains("status") || msg.contains("required"),
            "error message: {}",
            msg
        );
    }

    #[test]
    fn test_schema_validator_invalid_type() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "age": { "type": "integer", "minimum": 0 }
            }
        });
        let validator = SchemaValidator::compile(&schema).expect("schema should compile");

        // Wrong type
        let invalid_data = serde_json::json!({
            "age": "not a number"
        });
        let result = validator.validate(&invalid_data);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_schema_validator_compile_invalid_schema() {
        // Invalid JSON Schema - invalid keyword value
        // minimum requires a number, not a string
        let broken = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "minimum": "five" // invalid: minimum expects number
                }
            }
        });
        let result = SchemaValidator::compile(&broken);
        // This should fail - minimum requires a number, not a string
        assert!(
            result.is_err(),
            "Expected error for invalid schema but got: {:?}",
            result
        );
    }
}

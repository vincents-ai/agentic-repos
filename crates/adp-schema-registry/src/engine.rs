//! Schema registry engine — storage, lookup, and namespace matching.

use crate::error::SchemaError;
use crate::types::{RefSchema, SchemaChange, SchemaChangeType};
use crate::validation::ValidationResult;
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::sync::RwLock;

/// The schema registry — stores, indexes, and validates against schemas.
pub struct SchemaRegistry {
    /// Schemas keyed by ID.
    schemas: RwLock<HashMap<String, RefSchema>>,
    /// Compiled namespace matchers keyed by schema ID.
    namespace_matchers: RwLock<HashMap<String, GlobSet>>,
}

impl SchemaRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            schemas: RwLock::new(HashMap::new()),
            namespace_matchers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new schema.
    pub fn register(&self, schema: RefSchema) -> Result<SchemaChange, SchemaError> {
        // Check for existing
        {
            let schemas = self.schemas.read().unwrap();
            if schemas.contains_key(&schema.id) {
                return Err(SchemaError::AlreadyExists(schema.id.clone()));
            }
        }

        // Compile namespace pattern
        let glob = Glob::new(&schema.namespace_pattern)
            .map_err(|e| SchemaError::InvalidPattern(e.to_string()))?;
        let globset = GlobSetBuilder::new()
            .add(glob)
            .build()
            .map_err(|e| SchemaError::InvalidPattern(e.to_string()))?;

        let change = SchemaChange {
            schema_id: schema.id.clone(),
            change_type: SchemaChangeType::Created,
            timestamp: Utc::now(),
            changed_by: schema.author.clone().unwrap_or_default(),
        };

        let id = schema.id.clone();
        self.schemas
            .write()
            .unwrap()
            .insert(id, schema);
        self.namespace_matchers
            .write()
            .unwrap()
            .insert(change.schema_id.clone(), globset);

        Ok(change)
    }

    /// Update an existing schema.
    pub fn update(&self, schema: RefSchema) -> Result<SchemaChange, SchemaError> {
        let _existing = {
            let schemas = self.schemas.read().unwrap();
            schemas
                .get(&schema.id)
                .ok_or_else(|| SchemaError::NotFound(schema.id.clone()))?
                .clone()
        };

        // Recompile namespace pattern
        let glob = Glob::new(&schema.namespace_pattern)
            .map_err(|e| SchemaError::InvalidPattern(e.to_string()))?;
        let globset = GlobSetBuilder::new()
            .add(glob)
            .build()
            .map_err(|e| SchemaError::InvalidPattern(e.to_string()))?;

        let change = SchemaChange {
            schema_id: schema.id.clone(),
            change_type: SchemaChangeType::Updated,
            timestamp: Utc::now(),
            changed_by: schema.author.clone().unwrap_or_default(),
        };

        let id = schema.id.clone();
        self.schemas
            .write()
            .unwrap()
            .insert(id, schema);
        self.namespace_matchers
            .write()
            .unwrap()
            .insert(change.schema_id.clone(), globset);

        Ok(change)
    }

    /// Remove a schema.
    pub fn remove(&self, id: &str) -> Result<SchemaChange, SchemaError> {
        {
            let mut schemas = self.schemas.write().unwrap();
            schemas
                .remove(id)
                .ok_or_else(|| SchemaError::NotFound(id.to_string()))?;
        }
        self.namespace_matchers.write().unwrap().remove(id);

        Ok(SchemaChange {
            schema_id: id.to_string(),
            change_type: SchemaChangeType::Deleted,
            timestamp: Utc::now(),
            changed_by: String::new(),
        })
    }

    /// Get a schema by ID.
    pub fn get(&self, id: &str) -> Option<RefSchema> {
        self.schemas.read().unwrap().get(id).cloned()
    }

    /// List all schema IDs.
    pub fn list_ids(&self) -> Vec<String> {
        self.schemas.read().unwrap().keys().cloned().collect()
    }

    /// List all schemas.
    pub fn list_all(&self) -> Vec<RefSchema> {
        self.schemas.read().unwrap().values().cloned().collect()
    }

    /// Find the schema that matches a given ref path.
    pub fn find_for_path(&self, path: &str) -> Option<RefSchema> {
        let matchers = self.namespace_matchers.read().unwrap();
        let schemas = self.schemas.read().unwrap();

        for (id, globset) in matchers.iter() {
            if globset.is_match(path) {
                if let Some(schema) = schemas.get(id) {
                    return Some(schema.clone());
                }
            }
        }
        None
    }

    /// Validate a document against the schema for a given ref path.
    pub fn validate_for_path(
        &self,
        path: &str,
        document: &serde_json::Value,
    ) -> Result<ValidationResult, SchemaError> {
        let schema = self
            .find_for_path(path)
            .ok_or_else(|| SchemaError::NotFound(format!("no schema for path: {}", path)))?;

        Ok(ValidationResult::validate(document, &schema.schema))
    }

    /// Validate a document against a specific schema.
    pub fn validate(
        &self,
        schema_id: &str,
        document: &serde_json::Value,
    ) -> Result<ValidationResult, SchemaError> {
        let schema = self
            .get(schema_id)
            .ok_or_else(|| SchemaError::NotFound(schema_id.to_string()))?;

        Ok(ValidationResult::validate(document, &schema.schema))
    }

    /// Number of registered schemas.
    pub fn len(&self) -> usize {
        self.schemas.read().unwrap().len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.schemas.read().unwrap().is_empty()
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_schema(id: &str, pattern: &str) -> RefSchema {
        RefSchema {
            id: id.to_string(),
            namespace_pattern: pattern.to_string(),
            title: format!("Schema {}", id),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "status": {"type": "string"}
                },
                "required": ["title"]
            }),
            ui: None,
        }
    }

    #[test]
    fn register_and_get() {
        let registry = SchemaRegistry::new();
        let schema = make_schema("test-schema", "refs/test/*");

        let change = registry.register(schema.clone()).unwrap();
        assert_eq!(change.schema_id, "test-schema");
        assert_eq!(change.change_type, SchemaChangeType::Created);

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("test-schema").unwrap().title, "Schema test-schema");
    }

    #[test]
    fn register_duplicate_fails() {
        let registry = SchemaRegistry::new();
        registry.register(make_schema("dup", "refs/dup/*")).unwrap();
        let result = registry.register(make_schema("dup", "refs/dup/*"));
        assert!(result.is_err());
    }

    #[test]
    fn update_schema() {
        let registry = SchemaRegistry::new();
        registry.register(make_schema("up", "refs/up/*")).unwrap();

        let mut updated = make_schema("up", "refs/updated/*");
        updated.title = "Updated".to_string();
        let change = registry.update(updated).unwrap();
        assert_eq!(change.change_type, SchemaChangeType::Updated);
        assert_eq!(registry.get("up").unwrap().title, "Updated");
    }

    #[test]
    fn remove_schema() {
        let registry = SchemaRegistry::new();
        registry.register(make_schema("rm", "refs/rm/*")).unwrap();
        let change = registry.remove("rm").unwrap();
        assert_eq!(change.change_type, SchemaChangeType::Deleted);
        assert!(registry.is_empty());
    }

    #[test]
    fn find_for_path() {
        let registry = SchemaRegistry::new();
        registry.register(make_schema("tasks", "refs/engram/task/*")).unwrap();
        registry.register(make_schema("sessions", "refs/engram/session/*")).unwrap();

        let found = registry.find_for_path("refs/engram/task/abc-123").unwrap();
        assert_eq!(found.id, "tasks");

        let found = registry.find_for_path("refs/engram/session/def-456").unwrap();
        assert_eq!(found.id, "sessions");

        assert!(registry.find_for_path("refs/unknown/path").is_none());
    }

    #[test]
    fn validate_document() {
        let registry = SchemaRegistry::new();
        registry.register(make_schema("val", "refs/val/*")).unwrap();

        let result = registry.validate("val", &json!({"title": "test"})).unwrap();
        assert!(result.is_valid);

        let result = registry.validate("val", &json!({"no_title": true})).unwrap();
        assert!(!result.is_valid);
    }

    #[test]
    fn validate_for_path() {
        let registry = SchemaRegistry::new();
        registry.register(make_schema("val", "refs/val/*")).unwrap();

        let result = registry
            .validate_for_path("refs/val/test", &json!({"title": "hello"}))
            .unwrap();
        assert!(result.is_valid);
    }

    #[test]
    fn list_all_and_ids() {
        let registry = SchemaRegistry::new();
        registry.register(make_schema("a", "refs/a/*")).unwrap();
        registry.register(make_schema("b", "refs/b/*")).unwrap();

        assert_eq!(registry.list_ids().len(), 2);
        assert_eq!(registry.list_all().len(), 2);
    }
}

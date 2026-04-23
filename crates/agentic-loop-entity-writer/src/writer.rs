//! Entity writer — schema-validated entity CRUD against git refs.

use crate::error::EntityWriterError;
use adp_schema_registry::SchemaRegistry;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};
use uuid::Uuid;

/// A reference to an entity stored in git refs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRef {
    /// The full ref path (e.g. "refs/engram/task/abc-123").
    pub path: String,
    /// The entity ID (last segment of the path).
    pub entity_id: String,
    /// The schema ID this entity conforms to.
    pub schema_id: String,
    /// When this entity was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this entity was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Type of write operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOperation {
    /// Create a new entity.
    Create,
    /// Update an existing entity.
    Update,
    /// Delete an entity.
    Delete,
}

/// Entity writer with schema validation.
///
/// Validates all writes against the schema registry before committing.
pub struct EntityWriter {
    registry: SchemaRegistry,
}

impl EntityWriter {
    /// Create a new entity writer backed by the given schema registry.
    pub fn new(registry: SchemaRegistry) -> Self {
        Self { registry }
    }

    /// Create a new entity.
    ///
    /// Validates the entity data against the schema for the given path,
    /// then returns the entity reference for storage.
    #[instrument(skip(self, data))]
    pub fn create(
        &self,
        schema_id: &str,
        data: &serde_json::Value,
    ) -> Result<EntityRef, EntityWriterError> {
        // Validate against schema
        let result = self
            .registry
            .validate(schema_id, data)
            .map_err(|e| EntityWriterError::SchemaNotFound(e.to_string()))?;

        if !result.is_valid {
            return Err(EntityWriterError::ValidationFailed {
                schema_id: schema_id.to_string(),
                errors: result.errors,
            });
        }

        // Find schema to get namespace pattern
        let schema = self
            .registry
            .get(schema_id)
            .ok_or_else(|| EntityWriterError::SchemaNotFound(schema_id.to_string()))?;

        let entity_id = Uuid::new_v4().to_string();
        let path = format!(
            "{}/{}",
            schema.namespace_pattern.trim_end_matches("/*"),
            entity_id
        );

        let now = Utc::now();
        debug!(path = %path, schema_id = schema_id, "Entity created");

        Ok(EntityRef {
            path,
            entity_id,
            schema_id: schema_id.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Create an entity with a specific ID (for imports/migrations).
    pub fn create_with_id(
        &self,
        schema_id: &str,
        entity_id: &str,
        data: &serde_json::Value,
    ) -> Result<EntityRef, EntityWriterError> {
        // Validate entity_id is a valid format
        if entity_id.is_empty() || entity_id.contains('/') || entity_id.contains(' ') {
            return Err(EntityWriterError::InvalidEntityId(entity_id.to_string()));
        }

        let result = self
            .registry
            .validate(schema_id, data)
            .map_err(|e| EntityWriterError::SchemaNotFound(e.to_string()))?;

        if !result.is_valid {
            return Err(EntityWriterError::ValidationFailed {
                schema_id: schema_id.to_string(),
                errors: result.errors,
            });
        }

        let schema = self
            .registry
            .get(schema_id)
            .ok_or_else(|| EntityWriterError::SchemaNotFound(schema_id.to_string()))?;

        let path = format!(
            "{}/{}",
            schema.namespace_pattern.trim_end_matches("/*"),
            entity_id
        );

        let now = Utc::now();
        Ok(EntityRef {
            path,
            entity_id: entity_id.to_string(),
            schema_id: schema_id.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Update an existing entity.
    #[instrument(skip(self, data))]
    pub fn update(
        &self,
        schema_id: &str,
        entity_id: &str,
        data: &serde_json::Value,
    ) -> Result<EntityRef, EntityWriterError> {
        let result = self
            .registry
            .validate(schema_id, data)
            .map_err(|e| EntityWriterError::SchemaNotFound(e.to_string()))?;

        if !result.is_valid {
            return Err(EntityWriterError::ValidationFailed {
                schema_id: schema_id.to_string(),
                errors: result.errors,
            });
        }

        let schema = self
            .registry
            .get(schema_id)
            .ok_or_else(|| EntityWriterError::SchemaNotFound(schema_id.to_string()))?;

        let path = format!(
            "{}/{}",
            schema.namespace_pattern.trim_end_matches("/*"),
            entity_id
        );

        let now = Utc::now();
        debug!(path = %path, "Entity updated");

        Ok(EntityRef {
            path,
            entity_id: entity_id.to_string(),
            schema_id: schema_id.to_string(),
            created_at: now, // Would be preserved in real impl
            updated_at: now,
        })
    }

    /// Delete an entity (returns the path for ref deletion).
    pub fn delete(&self, schema_id: &str, entity_id: &str) -> Result<String, EntityWriterError> {
        let schema = self
            .registry
            .get(schema_id)
            .ok_or_else(|| EntityWriterError::SchemaNotFound(schema_id.to_string()))?;

        let path = format!(
            "{}/{}",
            schema.namespace_pattern.trim_end_matches("/*"),
            entity_id
        );

        debug!(path = %path, "Entity deleted");
        Ok(path)
    }

    /// Validate an entity without writing it.
    pub fn validate_only(
        &self,
        schema_id: &str,
        data: &serde_json::Value,
    ) -> Result<bool, EntityWriterError> {
        let result = self
            .registry
            .validate(schema_id, data)
            .map_err(|e| EntityWriterError::SchemaNotFound(e.to_string()))?;

        Ok(result.is_valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adp_schema_registry::RefSchema;
    use serde_json::json;

    fn setup_registry() -> SchemaRegistry {
        let registry = SchemaRegistry::new();
        let schema = RefSchema {
            id: "test-entity".to_string(),
            namespace_pattern: "refs/entities/test/*".to_string(),
            title: "Test Entity".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "priority": {"type": "string", "enum": ["low", "medium", "high"]}
                },
                "required": ["title"]
            }),
            ui: None,
        };
        registry.register(schema).unwrap();
        registry
    }

    #[test]
    fn create_valid_entity() {
        let writer = EntityWriter::new(setup_registry());
        let entity = writer
            .create("test-entity", &json!({"title": "Test", "priority": "high"}))
            .unwrap();

        assert!(entity.path.starts_with("refs/entities/test/"));
        assert_eq!(entity.schema_id, "test-entity");
    }

    #[test]
    fn create_invalid_entity_fails() {
        let writer = EntityWriter::new(setup_registry());
        let result = writer.create("test-entity", &json!({"no_title": true}));
        assert!(result.is_err());
        if let Err(EntityWriterError::ValidationFailed { schema_id, .. }) = result {
            assert_eq!(schema_id, "test-entity");
        } else {
            panic!("Expected ValidationFailed");
        }
    }

    #[test]
    fn create_unknown_schema_fails() {
        let writer = EntityWriter::new(setup_registry());
        let result = writer.create("nonexistent", &json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn create_with_id() {
        let writer = EntityWriter::new(setup_registry());
        let entity = writer
            .create_with_id("test-entity", "my-custom-id", &json!({"title": "Custom"}))
            .unwrap();
        assert_eq!(entity.entity_id, "my-custom-id");
        assert!(entity.path.ends_with("/my-custom-id"));
    }

    #[test]
    fn create_with_invalid_id_fails() {
        let writer = EntityWriter::new(setup_registry());
        assert!(writer
            .create_with_id("test-entity", "", &json!({"title": "T"}))
            .is_err());
        assert!(writer
            .create_with_id("test-entity", "has/slash", &json!({"title": "T"}))
            .is_err());
        assert!(writer
            .create_with_id("test-entity", "has space", &json!({"title": "T"}))
            .is_err());
    }

    #[test]
    fn update_entity() {
        let writer = EntityWriter::new(setup_registry());
        let entity = writer
            .update("test-entity", "abc-123", &json!({"title": "Updated"}))
            .unwrap();
        assert_eq!(entity.entity_id, "abc-123");
    }

    #[test]
    fn update_invalid_fails() {
        let writer = EntityWriter::new(setup_registry());
        let result = writer.update("test-entity", "abc", &json!({"missing": "title"}));
        assert!(result.is_err());
    }

    #[test]
    fn delete_entity() {
        let writer = EntityWriter::new(setup_registry());
        let path = writer.delete("test-entity", "abc-123").unwrap();
        assert_eq!(path, "refs/entities/test/abc-123");
    }

    #[test]
    fn validate_only() {
        let writer = EntityWriter::new(setup_registry());
        assert!(writer
            .validate_only("test-entity", &json!({"title": "T"}))
            .unwrap());
        assert!(!writer
            .validate_only("test-entity", &json!({}))
            .unwrap());
    }
}

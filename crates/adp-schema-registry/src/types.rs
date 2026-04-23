//! Schema types — mirrors agentic_repos::adp::schema for use without the full dependency.
//!
//! These are re-declarations so the schema registry crate is self-contained.
//! When integrating, the canonical types in agentic-repos should be used.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A ref-namespace schema definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefSchema {
    /// Unique slug identifier.
    pub id: String,
    /// Glob pattern matching ref names.
    pub namespace_pattern: String,
    /// Human-readable name.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Semver version.
    pub version: String,
    /// Optional author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// When last updated.
    pub updated_at: DateTime<Utc>,
    /// JSON Schema (draft 2020-12) for validating blobs.
    pub schema: serde_json::Value,
    /// Optional UI hints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiHints>,
}

/// UI rendering hints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiHints {
    /// View template name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    /// Primary display field path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_field: Option<String>,
    /// Fields to show in list views.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub list_fields: Vec<String>,
    /// Fields to show in detail views.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail_fields: Vec<String>,
    /// Sort configuration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort_by: Vec<SortField>,
    /// Filter definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<FilterDef>,
}

/// Sort field configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortField {
    /// Field path.
    pub field: String,
    /// Sort direction.
    #[serde(default)]
    pub direction: SortDirection,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

/// Filter definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterDef {
    /// Filter name.
    pub name: String,
    /// Field path.
    pub field: String,
    /// Filter type.
    #[serde(rename = "type")]
    pub filter_type: FilterType,
}

/// Filter type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    Text,
    Select,
    DateRange,
    NumericRange,
    Boolean,
}

/// A schema change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaChange {
    /// Schema ID that changed.
    pub schema_id: String,
    /// Type of change.
    pub change_type: SchemaChangeType,
    /// When the change occurred.
    pub timestamp: DateTime<Utc>,
    /// Who made the change.
    pub changed_by: String,
}

/// Type of schema change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaChangeType {
    Created,
    Updated,
    Deleted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_schema_round_trip() {
        let schema = RefSchema {
            id: "engram-task".to_string(),
            namespace_pattern: "refs/engram/task/*".to_string(),
            title: "Engram Task".to_string(),
            description: Some("A task entity".to_string()),
            version: "1.0.0".to_string(),
            author: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            schema: serde_json::json!({"type": "object"}),
            ui: None,
        };
        let json = serde_json::to_string(&schema).unwrap();
        let decoded: RefSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "engram-task");
    }
}

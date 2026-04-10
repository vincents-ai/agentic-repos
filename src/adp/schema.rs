//! ADP RefSchema types.
//!
//! A `RefSchema` is stored as a JSON blob at `refs/adp/schema/{id}` and
//! describes the structure of any git ref whose name matches
//! `namespace_pattern`, as well as rendering hints for the web UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

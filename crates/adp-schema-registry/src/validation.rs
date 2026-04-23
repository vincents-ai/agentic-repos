//! Schema validation using jsonschema.

/// Result of validating a JSON document against a schema.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the document is valid.
    pub is_valid: bool,
    /// Validation error messages (empty if valid).
    pub errors: Vec<String>,
}

impl ValidationResult {
    /// Create a valid result.
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
        }
    }

    /// Create an invalid result with errors.
    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            is_valid: false,
            errors,
        }
    }

    /// Validate a JSON document against a JSON Schema.
    pub fn validate(document: &serde_json::Value, schema: &serde_json::Value) -> Self {
        match jsonschema::validator_for(schema) {
            Ok(validator) => {
                if validator.is_valid(document) {
                    Self::valid()
                } else {
                    let error_msgs: Vec<String> =
                        validator.iter_errors(document).map(|e| e.to_string()).collect();
                    Self::invalid(error_msgs)
                }
            }
            Err(e) => Self::invalid(vec![format!("invalid schema: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_valid_document() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name"]
        });
        let doc = json!({"name": "test", "age": 42});
        let result = ValidationResult::validate(&doc, &schema);
        assert!(result.is_valid);
    }

    #[test]
    fn validate_invalid_document() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        });
        let doc = json!({"age": 42});
        let result = ValidationResult::validate(&doc, &schema);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn validate_invalid_schema() {
        let schema = json!("not a schema");
        let doc = json!({});
        let result = ValidationResult::validate(&doc, &schema);
        assert!(!result.is_valid);
    }
}

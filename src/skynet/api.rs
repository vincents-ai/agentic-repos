//! Plugin API methods and request/response handling

use serde::{Deserialize, Serialize};

/// API method descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMethod {
    pub name: String,
    pub description: String,
    pub params: Vec<ApiParam>,
}

/// API parameter descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiParam {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub description: String,
}

/// List all available API methods
pub fn list_api_methods() -> Vec<ApiMethod> {
    vec![
        ApiMethod {
            name: "repo.create".to_string(),
            description: "Create a new repository".to_string(),
            params: vec![ApiParam {
                name: "name".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: "Repository name".to_string(),
            }],
        },
        ApiMethod {
            name: "repo.delete".to_string(),
            description: "Delete a repository".to_string(),
            params: vec![ApiParam {
                name: "name".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: "Repository name".to_string(),
            }],
        },
        ApiMethod {
            name: "health.check".to_string(),
            description: "Check plugin health".to_string(),
            params: vec![],
        },
        ApiMethod {
            name: "metrics.get".to_string(),
            description: "Get plugin metrics".to_string(),
            params: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_api_methods_returns_expected() {
        let methods = list_api_methods();
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"repo.create"));
        assert!(names.contains(&"repo.delete"));
        assert!(names.contains(&"health.check"));
        assert!(names.contains(&"metrics.get"));
    }

    #[test]
    fn api_method_serialization() {
        let methods = list_api_methods();
        let json = serde_json::to_string(&methods).unwrap();
        let parsed: Vec<ApiMethod> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), methods.len());
    }

    #[test]
    fn create_repo_has_required_name_param() {
        let methods = list_api_methods();
        let create = methods.iter().find(|m| m.name == "repo.create").unwrap();
        assert_eq!(create.params.len(), 1);
        assert!(create.params[0].required);
        assert_eq!(create.params[0].name, "name");
    }
}

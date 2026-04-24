//! Error types for workflow parsing.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum YamlError {
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Missing required field: {field} in {context}")]
    MissingField { field: String, context: String },

    #[error("Invalid value for {field}: {value}")]
    InvalidValue { field: String, value: String },

    #[error("Circular dependency in jobs: {cycle}")]
    CircularDependency { cycle: String },

    #[error("Unknown job dependency: {job} depends on {dependency} which does not exist")]
    UnknownDependency { job: String, dependency: String },
}

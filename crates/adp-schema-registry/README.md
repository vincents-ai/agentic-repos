# adp-schema-registry

JSON Schema validation for ADP entities stored in git refs.

## Features

- **JSON Schema validation** via `jsonschema` 0.26
- **Namespace matching**: schemas bound to ref namespaces via glob patterns
- **CRUD operations**: register, update, delete, list schemas
- **Multiple error collection**: all validation errors returned, not just the first

## Usage

```rust
use adp_schema_registry::{SchemaRegistry, SchemaEntry};

let registry = SchemaRegistry::new();

// Register a schema
registry.register(SchemaEntry {
    id: "ci-workflow".into(),
    namespace_pattern: "refs/ci/workflows/*".into(),
    schema: serde_json::from_str(schema_json)?,
})?;

// Validate data against schema
let errors = registry.validate("ci-workflow", &workflow_data)?;
```

## Schema Location

Schemas are stored at `refs/adp/schema/{id}` in the ADP system repo. Data is validated before writing to its namespace.

## Tests

12 unit tests.

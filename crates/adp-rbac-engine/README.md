# adp-rbac-engine

Role-Based Access Control engine with globset pattern matching and CODEOWNERS-style path restrictions.

## Features

- **Globset pattern matching**: `src/**/*.rs`, `crates/*/lib.rs`, etc.
- **CODEOWNERS parser**: GitHub semantics — last matching pattern wins, deny overrides allow
- **Deny-overrides-allow**: explicit deny always takes precedence
- **Namespace restrictions**: service keys restricted to specific ref namespaces

## Usage

```rust
use adp_rbac_engine::{RbacEngine, RbacConfig};

let engine = RbacEngine::new(RbacConfig::default());

// Check write permission on a path
let allowed = engine.check_write("refs/secrets/ci/deploy-key", &permissions)?;

// Parse CODEOWNERS file
let owners = CodeownersParser::parse(&codeowners_content)?;
let reviewers = owners.owners_for_path("src/main.rs");
```

## Tests

18 unit tests.

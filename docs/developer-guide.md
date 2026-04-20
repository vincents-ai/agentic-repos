# Developer Guide — agentic-repos

Guide for developers contributing to or building agentic-repos.

## Table of Contents

1. [Development Environment](#development-environment)
2. [Building](#building)
3. [Project Structure](#project-structure)
4. [Module Guide](#module-guide)
5. [Testing](#testing)
6. [Code Style & Linting](#code-style--linting)
7. [Adding New Features](#adding-new-features)
8. [Commit Convention](#commit-convention)
9. [Architecture Decisions](#architecture-decisions)

## Development Environment

### Nix (recommended)

The project provides a Nix flake with a complete development environment:

```bash
# Enter dev shell (provides Rust, OpenSSL, cargo, clippy, rustfmt)
nix develop

# Build
nix develop --command cargo build

# Run tests
nix develop --command cargo test

# Check formatting
nix develop --command cargo fmt --check
```

### Manual Setup

If not using Nix, install:

- **Rust** (stable, latest) via [rustup](https://rustup.rs)
- **OpenSSL** development headers
- **pkg-config**
- **git**

## Building

### Debug build

```bash
cargo build
# Binary: target/debug/agentic-repos
```

### Release build

```bash
cargo build --release
# Binary: target/release/agentic-repos
```

### Nix (reproducible)

```bash
nix build
# Binary: result/bin/agentic-repos
```

### Build outputs

| Target | Output | Purpose |
|--------|--------|---------|
| Binary | `agentic-repos` | Standalone server |
| Shared library | `libagentic_repos.so` | Skylet v2 plugin (crate-type `cdylib`) |

## Project Structure

```
agentic-repos/
├── src/
│   ├── main.rs              # Entry point: CLI parsing, server start
│   ├── lib.rs               # Library root: module declarations
│   ├── server/              # Server initialization, route wiring
│   ├── api/                 # HTTP REST API (actix-web)
│   │   ├── mod.rs           # App configuration, state setup
│   │   ├── handlers.rs      # Shared handlers (health, info)
│   │   ├── models.rs        # Shared API models
│   │   ├── lfs.rs           # LFS batch/upload/download handlers
│   │   ├── lfs_locks.rs     # LFS file locking handlers
│   │   └── routes/          # Route configurators
│   │       ├── mod.rs       # Route scope wiring + auth middleware
│   │       ├── repositories.rs
│   │       ├── auth.rs
│   │       ├── cluster.rs
│   │       ├── adp.rs
│   │       └── secrets.rs
│   ├── auth/                # Authentication
│   │   ├── middleware.rs    # OAuth session middleware
│   │   ├── oauth.rs         # OAuth2 flow
│   │   └── saml.rs          # SAML SSO
│   ├── git/                 # Git operations + SSH server
│   │   ├── mod.rs           # GitService
│   │   ├── ssh.rs           # SSH server (russh)
│   │   └── protocol/        # Git wire protocol
│   │       ├── receive_pack.rs
│   │       └── upload_pack.rs (implied)
│   ├── lfs/                 # Git LFS subsystem
│   │   ├── pointer.rs       # LFS pointer parsing/validation
│   │   ├── quota.rs         # Storage quotas
│   │   ├── storage.rs       # LFS object storage backend
│   │   └── throttle.rs      # Bandwidth throttling
│   ├── rbac/                # Role-Based Access Control
│   │   ├── models.rs        # Roles, permissions, users
│   │   └── handlers.rs      # RBAC API handlers
│   ├── backup/              # Backup & restore
│   │   ├── handlers.rs      # Backup API endpoints
│   │   ├── service.rs       # Backup orchestration
│   │   ├── storage.rs       # Backup storage backend
│   │   ├── crypto.rs        # AES-256-GCM encryption
│   │   └── models.rs        # Backup data models
│   ├── cache/               # Redis caching layer
│   │   ├── mod.rs           # Cache manager
│   │   ├── pool.rs          # Connection pool
│   │   ├── session.rs       # Session cache
│   │   ├── permission.rs    # Permission cache
│   │   ├── keys.rs          # Cache key utilities
│   │   ├── crypto.rs        # Session data encryption
│   │   └── lfs.rs           # LFS-specific caching
│   ├── compliance/          # Compliance frameworks
│   │   ├── handlers.rs      # Compliance API handlers
│   │   ├── gdpr_hipaa_handlers.rs
│   │   ├── dora_nis2_handlers.rs
│   │   ├── audit.rs         # Audit logging
│   │   ├── reports.rs       # Report generation
│   │   ├── privacy.rs       # Data anonymization
│   │   └── encryption.rs    # Compliance encryption
│   ├── config/              # Configuration
│   │   └── mod.rs           # AppConfig parsing
│   ├── scaling/             # Horizontal scaling
│   │   ├── cluster.rs       # Cluster management
│   │   ├── leader.rs        # Leader election
│   │   ├── lock.rs          # Distributed locks
│   │   ├── session.rs       # Cluster sessions
│   │   └── config.rs        # Scaling config + validation
│   ├── security/            # Security scanning
│   │   ├── handlers.rs      # Security scan API
│   │   ├── scanner.rs       # Vulnerability scanner
│   │   └── rules.rs         # Scan rules
│   ├── skynet/              # Skylet v2 plugin (FFI)
│   │   ├── ffi.rs           # C FFI bridge
│   │   ├── interface.rs     # Plugin request handlers
│   │   ├── api.rs           # Method definitions
│   │   ├── multi_tenant/    # Namespace, permissions, quota
│   │   ├── rate_limit/      # Token bucket limiter
│   │   └── storage/         # Local + S3 backends
│   ├── web/                 # Web UI host (rust-embed)
│   ├── webhooks/            # Webhook delivery
│   ├── replication/         # Multi-region replication
│   ├── cicd/                # CI/CD integration
│   ├── sdlc/                # SDLC monitoring
│   └── adp/                 # ADP platform internals
│       ├── storage.rs       # Sidecar ref management
│       ├── schema.rs        # Schema definitions
│       ├── secrets/         # Secrets infrastructure
│       └── lfs_config.rs    # LFS configuration
├── features/                # BDD test scenarios (Cucumber/gherkin)
├── tests/                   # Integration tests
├── docs/                    # Documentation
├── k8s/                     # Kubernetes manifests
├── monitoring/              # Prometheus + Grafana
├── nix/                     # NixOS VM integration tests
├── migrations/              # SQL migrations
└── config-example.yaml      # Annotated configuration
```

## Module Guide

### Adding an HTTP endpoint

1. Create handler function in the appropriate module under `src/api/` or `src/<module>/handlers.rs`
2. Add route configuration in `src/api/routes/<module>.rs`
3. Wire the route scope in `src/api/routes/mod.rs`
4. Apply auth middleware at the scope level (see existing patterns)
5. Update `docs/API_REFERENCE.md`

### Adding a Skylet plugin method

1. Define method in `src/skynet/api.rs`
2. Implement handler in `src/skynet/interface.rs`
3. Add routing in `handle_request()`
4. Create BDD test in `features/`

### Auth pattern

Auth middleware is applied in the production combined configurators (`src/api/routes/mod.rs`), NOT in individual `configure_*_routes()` functions. This allows tests to use unwrapped route configurators:

```rust
// Production: auth applied at scope level
.service(web::scope("/backups")
    .wrap(settings_perm.clone())
    .configure(backup::handlers::configure_backup_routes))

// Tests: no auth wrapper
let app = test::init_service(
    web::scope("/backups")
        .configure(backup::handlers::configure_backup_routes)
);
```

For inline `require_auth()` functions in handlers, use `cfg!(test)` bypass:

```rust
fn require_auth(session: &Option<Session>) -> Result<(), ApiError> {
    if cfg!(test) {
        return Ok(()); // Bypass in tests
    }
    session.as_ref().ok_or(ApiError::unauthorized())?;
    Ok(())
}
```

### Error handling

All errors implement `std::error::Error + Send + Sync`. Use the `ApiError` type for HTTP responses:

- `ApiError::unauthorized()` — 401
- `ApiError::forbidden(reason)` — 403
- `ApiError::not_found(resource)` — 404
- `ApiError::bad_request(reason)` — 400
- `ApiError::internal_error(msg)` — 500 (logs full error, returns generic message)

### Logging

Use `tracing` (not `println!` / `eprintln!`):

```rust
use tracing::{info, warn, error, debug};

info!("Repository created: {}/{}", owner, repo);
warn!("Authentication failed for user: {}", user);
error!("Failed to process LFS upload: {}", err);
debug!("Cache hit for key: {}", key);
```

## Testing

### Test counts

- **934 unit tests** across 7 test files
- Integration tests in `tests/` directory

### Running tests

```bash
# All tests
nix develop --command cargo test

# Specific test module
nix develop --command cargo test --lib auth_integration

# Single test
nix develop --command cargo test --lib test_name

# Integration tests
nix develop --command cargo test --test git_protocol_integration
nix develop --command cargo test --test lfs_integration_test
```

### Test files

| File | Tests | Coverage |
|------|-------|----------|
| `src/security_tests.rs` | ~80 | Security scan, rules, validation |
| `src/git_integration_tests.rs` | ~50 | Git operations, protocol |
| `src/auth_integration_tests.rs` | ~41 | SSH keys, sessions, RBAC |
| `src/lfs_security_tests.rs` | ~49 | LFS path traversal, OID, quota |
| `src/access_control_tests.rs` | ~25 | Path traversal, injection, RBAC |
| `src/perf_tests.rs` | ~18 | Concurrent RBAC, SHA-256 perf, throttle |
| `src/git_protocol_tests.rs` | ~26 | Branches, compliance tags, ref namespaces |

### Test registration

All test modules are registered in `src/lib.rs`:

```rust
#[cfg(test)]
mod security_tests;
#[cfg(test)]
mod git_integration_tests;
// ...
```

### Test patterns

- **Auth bypass**: `cfg!(test)` pattern allows tests to skip auth without modifying production code
- **Test data**: Use `Arc::new(Mutex::new(...))` for shared state
- **Serial tests**: Use `#[serial_test::serial]` for tests that modify global state
- **Test timing**: SHA-256 of 10MB takes ~5.7s in debug — use 30s thresholds

### Skylet plugin BDD tests

```bash
plugin-test-harness bdd \
  --feature-path ./features \
  --plugin-path ./target/release/libagentic_repos.so

plugin-test-harness validate \
  --plugin-path ./target/release/libagentic_repos.so
```

## Code Style & Linting

### Pre-commit checks

Run before every commit:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

### Formatting

```bash
cargo fmt
```

**Important**: Do not run `cargo fmt` on files containing `".."` string checks — rustfmt incorrectly converts `".."` to `'..'` char literal.

### Clippy

```bash
cargo clippy -- -D warnings
```

All warnings are treated as errors. Fix clippy issues before committing.

## Adding New Features

### Checklist

1. Create feature module under `src/<feature>/`
2. Implement handler functions
3. Add route configuration
4. Add auth middleware at scope level in `src/api/routes/mod.rs`
5. Add feature flag in `config-example.yaml` under `features`
6. Write unit tests in `src/<feature>_tests.rs`
7. Register test module in `src/lib.rs`
8. Update `docs/API_REFERENCE.md`
9. Update `docs/configuration.md` if new config options added
10. Note frontend contract changes in engram knowledge

### Feature flags

Feature flags are in `src/features.rs` and `config-example.yaml`. New features should:

1. Default to `false`
2. Be documented in config-example.yaml
3. Be checked at the route or handler level

## Commit Convention

All commits must reference an engram task UUID:

```
<type>: <title> [<ENGRAM_TASK_UUID>]
```

Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `ci`

Examples:
```
feat: add LFS quota management [a1b2c3d4-e5f6-7890-abcd-ef1234567890]
fix: resolve SSH auth bypass on empty key [a1b2c3d4-e5f6-7890-abcd-ef1234567890]
docs: update API reference for backup endpoints [a1b2c3d4-e5f6-7890-abcd-ef1234567890]
```

`--no-verify` is **forbidden**. The pre-commit hook validates task linkage.

## Architecture Decisions

Significant architectural decisions are recorded as ADRs (Architecture Decision Records) in engram. Key decisions:

| ADR | Title | Summary |
|-----|-------|---------|
| ADR-018 | Full-fidelity handoff | Any agent picks up where another left off via engram |
| ADR-020 | gix over git2 | Pure Rust gitoxide replaces C FFI libgit2 |
| ADR-023 | gix migration plan | Parallel migration in engram + agentic-repos |
| ADR-024 | Secrets infrastructure | SOPS + AGE two-layer key wrapping |
| ADR-030 | AppConfigurator hook | Custom route injection for adp-web-ui |

Search engram for the full decision context:

```bash
engram ask query "ADR-024 secrets"
engram ask query "gix migration decision"
```

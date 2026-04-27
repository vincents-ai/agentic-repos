# Testing Guide for agentic-repos

## Overview

agentic-repos has 934 unit tests across 7 test modules, plus integration tests and Skylet plugin BDD validation. Tests run under the default `cargo test` and require no external services (all external dependencies are mocked).

## Quick Start

```bash
# Run all tests (preferred — uses nix dev shell)
nix develop --command cargo test

# Run specific test module
nix develop --command cargo test --lib auth_integration

# Run single test by name
nix develop --command cargo test --lib test_ssh_key_validation

# Integration tests
nix develop --command cargo test --test git_protocol_integration
nix develop --command cargo test --test lfs_integration_test
nix develop --command cargo test --test lfs_locks_integration
```

## Test Architecture

### Unit Tests (934 total)

Unit tests are `#[cfg(test)]` modules registered in `src/lib.rs`:

| Module | File | Count | Coverage |
|--------|------|-------|----------|
| Security | `src/security_tests.rs` | ~80 | Vulnerability scanning, rule evaluation, validation |
| Git integration | `src/git_integration_tests.rs` | ~50 | Git operations, wire protocol, refs |
| Auth integration | `src/auth_integration_tests.rs` | ~41 | SSH key auth, sessions, key rotation, user lifecycle |
| LFS security | `src/lfs_security_tests.rs` | ~49 | Path traversal, OID validation, SHA-256, quota, throttle |
| Access control | `src/access_control_tests.rs` | ~25 | Path traversal, command injection, RBAC, error sanitization, headers |
| Performance | `src/perf_tests.rs` | ~18 | Concurrent RBAC, SHA-256 throughput, throttle scalability |
| Git protocol | `src/git_protocol_tests.rs` | ~26 | Branch limits, compliance tags, command validation, ref namespaces |

### Integration Tests

Located in `tests/`:

| File | Coverage |
|------|----------|
| `git_protocol_integration.rs` | End-to-end Git protocol operations |
| `lfs_integration_test.rs` | LFS batch, upload, download, verify |
| `lfs_locks_integration.rs` | LFS file locking lifecycle |

### BDD Tests

Cucumber/gherkin scenarios in `features/` for Skylet plugin validation:

```bash
plugin-test-harness bdd \
  --feature-path ./features \
  --plugin-path ./target/release/libagentic_repos.so

plugin-test-harness validate \
  --plugin-path ./target/release/libagentic_repos.so
```

## Test Patterns

### Auth Bypass in Tests

Auth middleware is NOT applied in individual `configure_*_routes()` functions — it's applied at the scope level in the production route configurator. Tests use unwrapped route configurators:

```rust
// Production (src/api/routes/mod.rs):
.service(web::scope("/backups")
    .wrap(settings_perm.clone())
    .configure(backup::handlers::configure_backup_routes))

// Test:
let app = test::init_service(
    web::scope("/backups")
        .configure(backup::handlers::configure_backup_routes)
);
```

For handlers with inline `require_auth()`:

```rust
fn require_auth(session: &Option<Session>) -> Result<(), ApiError> {
    if cfg!(test) {
        return Ok(());  // Bypass in test builds
    }
    session.as_ref().ok_or(ApiError::unauthorized())?;
    Ok(())
}
```

### Test Data

- Use `Arc::new(Mutex::new(...))` for shared mutable state
- Mock external services with `mockall`
- Use `serial_test::serial` for tests that modify global state

### Timing

SHA-256 hashing of 10MB takes ~5.7s in debug mode under nix. Use 30-second thresholds for performance tests:

```rust
#[test]
fn test_sha256_throughput() {
    let start = Instant::now();
    // ... hash 10MB ...
    assert!(start.elapsed().as_secs() < 30);
}
```

### Test Naming

Test functions use `test_<module>_<scenario>` convention:

```rust
#[test]
fn test_lfs_oid_rejects_path_traversal() { ... }

#[test]
fn test_rbac_concurrent_permission_checks() { ... }
```

## Running Specific Test Categories

```bash
# Security-related tests only
nix develop --command cargo test --lib security

# Auth tests
nix develop --command cargo test --lib auth

# LFS tests (unit + integration)
nix develop --command cargo test lfs

# Performance tests
nix develop --command cargo test --lib perf

# Git protocol tests
nix develop --command cargo test --lib git_protocol

# Access control tests
nix develop --command cargo test --lib access_control
```

## Pre-commit Validation

The engram pre-commit hook validates:

1. Commit message contains a valid engram task UUID
2. The referenced task has context and reasoning relationships

```bash
# Validate a commit message before committing
engram validate commit --message "feat: add feature [UUID]" --dry-run
```

## CI/CD Integration

```bash
# Full validation pipeline
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

## Test Coverage by Domain

### Security (154 tests)

- Path traversal prevention (all user-supplied path components)
- Command injection prevention (SSH commands)
- Authentication bypass prevention
- ReDoS protection
- Error message sanitization (CWE-209)
- Security headers
- HMAC signature verification
- SAML assertion verification
- Rate limiting

### LFS (49 tests)

- LFS pointer parsing and validation
- OID format validation
- SHA-256 integrity verification
- Quota enforcement
- Bandwidth throttling
- File locking
- Path traversal in LFS storage

### RBAC (41+ tests)

- User lifecycle (create, update, delete)
- SSH key management (single, multi-key, rotation)
- Permission enforcement
- Role assignment and removal
- Session management

### Git Protocol (76 tests)

- Branch CRUD
- Compliance tagging
- Git command validation
- Ref namespace management
- Wire protocol operations

### Performance (18 tests)

- Concurrent RBAC checks (100+ concurrent)
- SHA-256 throughput (10MB in <30s)
- Throttle scalability
- Quota enforcement under load

## Adding New Tests

1. Create test file: `src/<domain>_tests.rs`
2. Register in `src/lib.rs`: `#[cfg(test)] mod <domain>_tests;`
3. Follow the auth bypass pattern for handler tests
4. Use descriptive test names: `test_<module>_<scenario>`
5. Run full suite to verify no regressions

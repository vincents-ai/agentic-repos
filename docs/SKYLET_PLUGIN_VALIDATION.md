# Skylet Plugin Validation Guide

## Overview

This guide documents the validation process for the repos-mcp Skylet plugin. The repos-mcp project is a Skylet v2 plugin that provides Git repository management capabilities through the Skylet runtime.

## What is the repos-mcp Skylet Plugin?

The repos-mcp plugin is a dynamic library (`librepos_mcp.so`) that implements the Skylet v2 ABI. It provides:

- **Repository Operations**: Create, list, delete, and query Git repositories
- **Multi-tenant Support**: Isolated repository access per tenant
- **SSH Authentication**: Secure Git access over SSH
- **Storage Backends**: S3-compatible and local storage support
- **Health Monitoring**: Plugin health checks and metrics

## Prerequisites

### 1. Build the Plugin

The plugin is built as part of the repos-mcp project:

```bash
# Build using nix (recommended)
nix build

# Or build using cargo directly
cargo build --release
```

The plugin will be available at:
- `target/release/librepos_mcp.so` (Linux)
- `target/release/librepos_mcp.dylib` (macOS)

### 2. Install Skylet Plugin Test Harness

The test harness is available in the skylet repository:

```bash
# Navigate to skylet repository
cd ~/code/skylet/

# Build the test harness
cd plugin-test-harness
cargo build --release

# The test harness binary will be at:
# target/release/plugin-test-harness
```

## Validation Methods

### Method 1: BDD Testing with Cucumber Features

The plugin includes comprehensive BDD test scenarios in the `features/` directory:

#### Test Categories

1. **Lifecycle Tests** (`01_lifecycle.feature`)
   - Plugin loading and initialization
   - Metadata validation
   - Health checks
   - Graceful shutdown

2. **Validation Tests** (`02_validation.feature`)
   - ABI v2 compliance
   - Capability exposure
   - Error handling

3. **Repository Operations** (`03_repository_operations.feature`)
   - repo.create
   - repo.list
   - repo.info
   - Error handling for missing parameters

4. **Health & Monitoring** (`04_health_monitoring.feature`)
   - health.check
   - metrics.get
   - Status reporting

5. **Storage Operations** (`05_storage_operations.feature`)
   - storage.list_backends
   - storage.list_buckets
   - storage.create_bucket
   - Error handling for invalid backends

#### Running BDD Tests

```bash
# From the repos-mcp directory
cd /home/shift/code/vincents-ai/repos-mcp

# Run all feature tests
~/code/skylet/target/release/plugin-test-harness bdd \
  --feature-path ./features \
  --plugin-path ./target/release/librepos_mcp.so

# Run specific feature file
~/code/skylet/target/release/plugin-test-harness bdd \
  --feature-path ./features/01_lifecycle.feature \
  --plugin-path ./target/release/librepos_mcp.so

# Run tests with specific tags (e.g., smoke tests only)
~/code/skylet/target/release/plugin-test-harness bdd \
  --feature-path ./features \
  --tags "@smoke" \
  --plugin-path ./target/release/librepos_mcp.so

# Generate JSON output for CI/CD
~/code/skylet/target/release/plugin-test-harness bdd \
  --feature-path ./features \
  --format json \
  --plugin-path ./target/release/librepos_mcp.so \
  > test-results.json

# Generate JUnit XML for test reporting
~/code/skylet/target/release/plugin-test-harness bdd \
  --feature-path ./features \
  --format junit \
  --plugin-path ./target/release/librepos_mcp.so \
  > test-results.xml
```

### Method 2: Direct API Testing

Test individual plugin actions directly:

```bash
# Test plugin info
~/code/skylet/target/release/plugin-test-harness execute \
  --plugin-path ./target/release/librepos_mcp.so \
  --action info \
  --args '{}'

# Test health check
~/code/skylet/target/release/plugin-test-harness execute \
  --plugin-path ./target/release/librepos_mcp.so \
  --action health.check \
  --args '{"id":"test-1","method":"health.check","params":{}}'

# Test repository creation
~/code/skylet/target/release/plugin-test-harness execute \
  --plugin-path ./target/release/librepos_mcp.so \
  --action repo.create \
  --args '{"id":"test-2","method":"repo.create","params":{"name":"my-repo","tenant_id":"tenant-1"}}'
```

### Method 3: Plugin Validation

Validate plugin meets Skylet requirements:

```bash
~/code/skylet/target/release/plugin-test-harness validate \
  --plugin-path ./target/release/librepos_mcp.so
```

### Method 4: Test Suite Configuration

Create a test suite configuration file:

```bash
# Create plugin-suite.toml
cat > plugin-suite.toml <<'EOF'
[[plugins]]
name = "repos-mcp"
path = "./target/release/librepos_mcp.so"
timeout_ms = 5000

# Additional configuration
config = """
storage.default_backend = "local"
log.level = "debug"
"""
EOF

# Run the test suite
~/code/skylet/target/release/plugin-test-harness suite \
  --config-file ./plugin-suite.toml
```

## Expected Test Results

### Successful Plugin Load

When the plugin loads successfully, you should see:
- Plugin metadata with name "git-operations"
- Version information (e.g., "0.1.0")
- ABI version "2.0"
- Capabilities list including: repo.create, repo.delete, repo.list, repo.info, lfs.operation

### Health Check Response

A successful health check should return:
```json
{
  "id": "health-1",
  "result": {
    "status": "healthy",
    "version": "0.1.0",
    "uptime_secs": 123,
    "active_repositories": 0,
    "active_sessions": 0,
    "storage_bytes": 0,
    "checked_at": "2026-03-07T..."
  }
}
```

### Repository Creation Response

Successful repository creation:
```json
{
  "id": "test-123",
  "result": {
    "name": "test-repo",
    "path": "/var/lib/repos-mcp/tenant-1/test-repo.git",
    "created_at": "2026-03-07T..."
  }
}
```

## Troubleshooting

### Plugin Not Found

```
Error: Cannot load plugin: No such file or directory
```

**Solution**: Ensure the plugin is built and the path is correct:
```bash
ls -la target/release/librepos_mcp.so
```

### Plugin Initialization Failed

```
Error: Plugin initialization failed
```

**Possible causes**:
1. Missing dependencies - Ensure all required libraries are available
2. Configuration errors - Check that config.yaml is valid
3. Permission issues - Ensure the plugin file is executable

**Debug**:
```bash
# Check plugin dependencies
ldd target/release/librepos_mcp.so

# Check file permissions
chmod +x target/release/librepos_mcp.so
```

### Test Harness Not Found

```
Error: plugin-test-harness: command not found
```

**Solution**: Build the test harness from the skylet repository:
```bash
cd ~/code/skylet/plugin-test-harness
cargo build --release
```

### ABI Version Mismatch

```
Error: ABI version mismatch
```

**Solution**: Ensure you're using the correct version of skylet-abi. Check Cargo.toml:
```toml
skylet-abi = { git = "ssh://git@github.com/vincents-ai/skylet.git", default-features = false }
```

## Integration with Skylet Runtime

### Loading the Plugin in Skylet

To load the plugin in the actual Skylet runtime:

1. **Copy plugin to Skylet plugins directory**:
   ```bash
   cp target/release/librepos_mcp.so ~/code/skylet/plugins/git-operations/
   ```

2. **Configure Skylet** (in `~/code/skylet/config.yaml`):
   ```yaml
   plugins:
     - name: git-operations
       path: ./plugins/git-operations/librepos_mcp.so
       enabled: true
       config:
         storage.default_backend: "local"
   ```

3. **Start Skylet**:
   ```bash
   cd ~/code/skylet
   cargo run --release
   ```

4. **Verify plugin is loaded**:
   Check Skylet logs for:
   ```
   INFO Plugin loaded: git-operations v0.1.0
   ```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Plugin Validation

on: [push, pull_request]

jobs:
  validate-plugin:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Build plugin
        run: nix build
        
      - name: Build test harness
        run: |
          git clone https://github.com/vincents-ai/skylet.git
          cd skylet/plugin-test-harness
          cargo build --release
          
      - name: Run BDD tests
        run: |
          ./skylet/target/release/plugin-test-harness bdd \
            --feature-path ./features \
            --format junit \
            --plugin-path ./target/release/librepos_mcp.so \
            > test-results.xml
            
      - name: Publish test results
        uses: EnricoMi/publish-unit-test-result-action@v2
        with:
          files: test-results.xml
```

## Test Coverage

The BDD test suite covers:

- ✅ Plugin lifecycle (load, init, shutdown)
- ✅ ABI v2 compliance
- ✅ Metadata validation
- ✅ Health checks
- ✅ Metrics collection
- ✅ Repository CRUD operations
- ✅ Storage backend operations
- ✅ Error handling
- ✅ Invalid input validation
- ✅ Multi-tenant isolation

## Additional Resources

- [Skylet Documentation](https://github.com/vincents-ai/skylet)
- [Skylet Plugin Development Guide](https://github.com/vincents-ai/skylet/blob/main/docs/PLUGIN_DEVELOPMENT.md)
- [Skylet ABI Specification](https://github.com/vincents-ai/skylet/blob/main/docs/PLUGIN_CONTRACT.md)
- [repos-mcp Architecture](./docs/ARCHITECTURE.md)

## Validation Checklist

Before considering the plugin fully validated, ensure:

- [ ] Plugin builds successfully
- [ ] All lifecycle tests pass
- [ ] All validation tests pass
- [ ] Repository operations work correctly
- [ ] Health checks return valid status
- [ ] Metrics are collected properly
- [ ] Storage operations function as expected
- [ ] Error handling works correctly
- [ ] Plugin can be loaded in Skylet runtime
- [ ] Plugin works with multiple concurrent requests
- [ ] Plugin handles graceful shutdown correctly

## Reporting Issues

If you encounter issues during validation:

1. Check the troubleshooting section above
2. Review Skylet logs for error messages
3. Verify plugin dependencies with `ldd`
4. Test with verbose logging enabled
5. Report issues at: https://github.com/vincents-ai/repos-mcp/issues

# NixOS VM Testing Framework for repos-mcp

This document describes the comprehensive NixOS VM testing framework for repos-mcp, which provides integration testing for SSH Git operations using isolated virtual machines.

## Overview

The VM testing framework creates isolated NixOS virtual machines to test:
- SSH server functionality 
- Git protocol operations over SSH
- Multi-client concurrent connections
- Authentication with SSH keys
- Service lifecycle and error handling

## Test Architecture

```
┌─────────────┐    SSH Port 2222    ┌─────────────┐
│   Client    │────────────────────▶│   Server    │
│   VM        │                     │   VM        │
│             │    SSH Port 22      │             │
│   - Git     │◀────────────────────│   - repos   │
│   - OpenSSH │                     │   - OpenSSH │
│   - Test    │                     │   - systemd │
│     Keys    │                     │             │
└─────────────┘                     └─────────────┘
```

## Available Tests

### 1. SSH Basic Connection Test (`ssh-basic`)

**Purpose**: Verifies basic SSH connectivity and service startup.

**What it tests**:
- ✅ repos-mcp service starts correctly
- ✅ SSH server accepts connections on port 2222
- ✅ SSH key authentication works
- ✅ Basic SSH commands execute successfully

**Usage**:
```bash
nix build .#checks.x86_64-linux.ssh-basic
```

### 2. Git Operations Test (`git-operations`)

**Purpose**: Tests Git protocol commands over SSH.

**What it tests**:
- ✅ `git-upload-pack` command (for cloning/fetching)
- ✅ `git-receive-pack` command (for pushing)
- ✅ Repository structure setup
- ✅ Git protocol message handling

**Usage**:
```bash
nix build .#checks.x86_64-linux.git-operations
```

### 3. Multi-Client Test (`multi-client`)

**Purpose**: Validates concurrent client connections.

**What it tests**:
- ✅ Multiple SSH clients connecting simultaneously
- ✅ Concurrent Git operations
- ✅ Server handles multiple sessions
- ✅ No race conditions or conflicts

**Usage**:
```bash
nix build .#checks.x86_64-linux.multi-client
```

### 4. Local Storage Backend Test (`storage-local`)

**Purpose**: Validates local filesystem storage backend.

**What it tests**:
- ✅ Local storage directory creation and permissions
- ✅ Git repository creation in local storage
- ✅ File persistence across service restarts
- ✅ Storage structure integrity
- ✅ Repository operations with local backend

**Usage**:
```bash
nix build .#checks.x86_64-linux.storage-local
```

### 5. S3 Storage Backend Test (`storage-s3`)

**Purpose**: Validates S3-compatible storage backend with MinIO.

**What it tests**:
- ✅ MinIO S3-compatible service startup
- ✅ Network connectivity to S3 backend
- ✅ S3 configuration and authentication
- ✅ Git operations with S3 storage
- ✅ Service restart with S3 backend
- ✅ S3 backend health and availability

**VM Architecture**:
```
Client VM ──SSH──▶ Server VM ──S3 API──▶ MinIO VM
                  (repos-mcp)           (S3 Storage)
```

**Usage**:
```bash
nix build .#checks.x86_64-linux.storage-s3
```

## Running Tests

### Run All VM Tests
```bash
nix flake check
```

### Run Individual Tests
```bash
# Basic SSH test
nix build .#checks.x86_64-linux.ssh-basic

# Git operations test
nix build .#checks.x86_64-linux.git-operations  

# Multi-client test
nix build .#checks.x86_64-linux.multi-client

# Local storage backend test
nix build .#checks.x86_64-linux.storage-local

# S3 storage backend test  
nix build .#checks.x86_64-linux.storage-s3
```

### Interactive Testing
For debugging and interactive exploration:

```bash
# Build the test driver
nix-build '<nixpkgs/nixos/tests/make-test-python.nix>' --arg testScript 'import ./.checks.ssh-basic.nix'

# Run interactively
./result/bin/nixos-test-driver
```

In interactive mode:
```python
>>> start_all()
>>> server.shell_interact()  # Get shell on server VM
>>> client.shell_interact()  # Get shell on client VM
>>> server.succeed("systemctl status repos-mcp")
```

## Test Configuration

### SSH Keys
Tests use Ed25519 SSH keys for security:
- **Private Key**: Embedded in test configuration
- **Public Key**: Added to authorized_keys on server
- **Format**: OpenSSH Ed25519 (modern, secure)

### Server Configuration
#### Local Storage Backend
```yaml
server:
  ssh:
    port: 2222
    bind_address: "0.0.0.0"
storage:
  type: "local"
  local:
    path: "/var/lib/repos-mcp"
auth:
  public_key_auth: true
```

#### S3 Storage Backend
```yaml
server:
  ssh:
    port: 2222
    bind_address: "0.0.0.0"
storage:
  type: "s3"
  s3:
    bucket: "repos-mcp-test"
    region: "us-east-1"
    endpoint: "http://minio:9000"
    access_key: "minioadmin"
    secret_key: "minioadmin"
    path_style: true
auth:
  public_key_auth: true
```

### VM Resources
- **Server VM**: 512-1024MB RAM, 2 CPU cores
- **Client VMs**: 256MB RAM, 1 CPU core
- **Network**: Isolated virtual network per test

## Test Scenarios Covered

### Authentication Testing
- ✅ SSH key-based authentication
- ✅ Public key verification
- ✅ Unauthorized access rejection

### Protocol Testing
- ✅ SSH protocol compliance (OpenSSH)
- ✅ Git Smart Protocol over SSH
- ✅ Command execution (`git-upload-pack`, `git-receive-pack`)
- ✅ Error handling and responses

### Concurrency Testing  
- ✅ Multiple simultaneous connections
- ✅ Parallel Git operations
- ✅ Session isolation
- ✅ Resource contention handling

### Service Integration
- ✅ systemd service management
- ✅ Proper startup and shutdown
- ✅ Service restart behavior
- ✅ Configuration file handling

### Storage Backend Testing
- ✅ Local filesystem storage operations
- ✅ S3-compatible storage with MinIO
- ✅ Storage backend configuration validation
- ✅ Network connectivity to storage services
- ✅ Data persistence across service restarts
- ✅ Storage backend error handling

## Debugging Test Failures

### View Test Logs
```bash
# Build test and check output
nix build .#checks.x86_64-linux.ssh-basic 2>&1 | less

# For more verbose output
nix build .#checks.x86_64-linux.ssh-basic --print-build-logs
```

### Common Issues

**Test Timeouts**:
- Increase VM memory in flake.nix
- Check service startup time
- Verify port availability

**SSH Connection Failures**:
- Check SSH key format
- Verify firewall rules
- Ensure SSH daemon is running

**Git Command Errors**:
- Check repos-mcp service logs
- Verify repository permissions
- Test SSH connectivity first

### Manual Testing
You can also test manually against a development instance:

```bash
# Start development shell
nix develop

# Build and run repos-mcp
cargo build --release
./target/release/repos-mcp --config config.yaml

# Test from another terminal
ssh -i ~/.ssh/id_ed25519 -p 2222 user@localhost 'git-upload-pack /test/repo'
```

## Integration with CI/CD

### GitHub Actions Example
```yaml
name: VM Tests
on: [push, pull_request]
jobs:
  vm-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: cachix/install-nix-action@v22
      - name: Run VM tests
        run: nix flake check
```

### Local Development
```bash
# Quick smoke test
nix build .#checks.x86_64-linux.ssh-basic

# Full test suite
nix flake check

# Watch for changes (requires entr)
find . -name "*.rs" -o -name "*.nix" | entr -c nix flake check
```

## Extending Tests

### Adding New Test Cases

1. **Add test to flake.nix**:
```nix
new-test = pkgs.testers.runNixOSTest {
  name = "repos-mcp-new-test";
  nodes = { /* VM configuration */ };
  testScript = '' /* Python test script */ '';
};
```

2. **Update checks output**:
```nix
checks = {
  # ... existing tests
  new-test = /* test definition */;
};
```

### Test Best Practices

- **Isolation**: Each test should be independent
- **Cleanup**: Tests clean up after themselves
- **Timeouts**: Set appropriate timeouts for operations
- **Logging**: Include clear success/failure messages
- **Resources**: Use minimal VM resources needed

## Performance Considerations

### VM Test Performance
- **Parallel Execution**: Tests run in parallel when possible
- **Resource Limits**: VMs use minimal required resources
- **Caching**: Nix caches VM images between runs
- **Build Time**: First run takes longer (building VM images)

### Optimization Tips
- Use `--max-jobs` to limit parallel VM tests
- Adjust VM memory based on available host resources
- Use `--keep-going` to run all tests even if some fail

## Troubleshooting

### Performance Issues
```bash
# Run single test to isolate issues
nix build .#checks.x86_64-linux.ssh-basic

# Monitor resource usage
nix build --show-trace --print-build-logs
```

### Network Issues
```bash
# Check VM networking in interactive mode
nix-shell '<nixpkgs>' -A nixosTests.networking.simple
```

### SSH Issues
```bash
# Test SSH key format
ssh-keygen -l -f /path/to/test_ssh_key.pub

# Debug SSH connection
ssh -vvv -i test_key -p 2222 localhost
```

This VM testing framework ensures repos-mcp works correctly in realistic deployment scenarios and catches integration issues that unit tests cannot detect.
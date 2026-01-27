# Testing Guide for repos-mcp

## Overview

This document provides guidance on implementing and running tests for repos-mcp, including unit tests, integration tests, and NixOS VM tests.

## Test Status

### Unit Tests
- **Location**: `tests/` directory
- **Status**: ✅ Fully implemented
- **Count**: 131 tests (112 passing, 19 requiring PostgreSQL)
- **Run**: `cargo test`

### VM Integration Tests  
- **Location**: `nix/vm-tests.nix`
- **Status**: ⚠️  **342 tests defined, currently STUBS requiring implementation**
- **Framework**: NixOS VM test framework
- **Run**: `nix build .#checks.x86_64-linux.<test-name>`

## Current VM Test Status

The VM test suite currently contains **238 stub implementations** that verify infrastructure (VM boots, service starts, network connects) but do not perform the actual test scenarios they describe.

### Stub Test Pattern

Current stub tests follow this pattern:

```nix
{
  id = "concurrent-clones-10";
  testScript = ''
    server.wait_for_unit("repos-mcp.service")
    server.succeed("mkdir -p /var/lib/repos-mcp/perf")
    server.succeed("git init --bare /var/lib/repos-mcp/perf")
    client.succeed("echo concurrent_clones_10_ok")  # ❌ STUB - doesn't test anything
  '';
}
```

### Required Implementation Pattern

Tests should be implemented following this pattern:

```nix
{
  id = "concurrent-clones-10";
  testScript = ''
    # Setup
    server.wait_for_unit("repos-mcp.service")
    server.wait_for_open_port(2222)
    
    # Create SSH keys and configure authentication
    client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"\"")
    client.copy_from_host("/path/to/key.pub", "/tmp/key.pub")
    server.copy_from_vm("/tmp/key.pub", "/etc/repos-mcp/authorized_keys")
    
    # Create test repository
    server.succeed("mkdir -p /var/lib/repos-mcp/tenant/test.git")
    server.succeed("git init --bare /var/lib/repos-mcp/tenant/test.git")
    
    # Perform actual test
    with subtest("Concurrent clones"):
        for i in range(10):
            client.succeed(f"git clone ssh://root@server:2222/tenant/test.git /tmp/clone{i} &")
        client.succeed("wait")  # Wait for all background jobs
        
        # Verify all clones succeeded
        for i in range(10):
            client.succeed(f"test -d /tmp/clone{i}/.git")
  '';
}
```

## VM Test Implementation Guide

### String Escaping in Nix

NixOS VM tests embed Python code inside Nix strings. This creates escaping challenges:

**Problem**: Nix `''` strings interpret single quotes and `${...}` specially

```nix
# ❌ WRONG - Nix will try to interpolate ${pubkey}
testScript = ''
  pubkey = client.succeed("cat /root/.ssh/id_ed25519.pub").strip()
  server.succeed("echo '${pubkey}' > /etc/repos-mcp/authorized_keys")
'';
```

**Solutions**:

1. **Use file-based key transfer** (recommended):
```nix
testScript = ''
  client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"\"")
  client.copy_from_vm("/root/.ssh/id_ed25519.pub", "/tmp/client_key.pub")
  server.copy_from_host("/tmp/client_key.pub", "/etc/repos-mcp/authorized_keys")
'';
```

2. **Use Python string methods**:
```nix
testScript = ''
  pubkey = client.succeed("cat /root/.ssh/id_ed25519.pub").strip()
  # Write to temp file then move
  client.succeed("cat /root/.ssh/id_ed25519.pub > /tmp/key.pub")
  server.copy_from_vm("/tmp/key.pub", "/etc/repos-mcp/authorized_keys")
'';
```

3. **Escape Nix interpolation**:
```nix
testScript = ''
  # Use ''${ to escape Nix interpolation
  pubkey = client.succeed("cat /root/.ssh/id_ed25519.pub").strip()
  server.succeed(f"echo ''${pubkey} > /etc/repos-mcp/authorized_keys")
'';
```

### Test Categories Requiring Implementation

| Category | Stub Count | Priority | Complexity |
|----------|------------|----------|------------|
| Authentication | 53 | High | Medium |
| Git Protocol | 80 | High | High |
| Performance | 25 | High | High |
| Storage | 40 | Medium | Medium |
| Multi-tenancy | 30 | Medium | Medium |
| Network | 25 | Medium | Low |
| LFS | 33 | Medium | High |
| Error Handling | 42 | Low | Low |
| Security | 33 | High | Medium |
| Edge Cases | 21 | Low | Low |

### Example: Implementing Authentication Tests

```nix
{
  id = "ssh-ed25519-key";
  testScript = ''
    import shlex
    
    # Wait for services
    server.wait_for_unit("repos-mcp.service")
    server.wait_for_open_port(2222)
    
    # Generate SSH keypair
    client.succeed("mkdir -p /root/.ssh")
    client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"\"")
    
    # Transfer public key to server (avoids string escaping issues)
    client.succeed("cat /root/.ssh/id_ed25519.pub > /tmp/pubkey")
    server.succeed("mkdir -p /etc/repos-mcp")
    # Copy key content via VM file sharing
    pubkey_content = client.succeed("cat /tmp/pubkey").strip()
    server.succeed(f"printf '%s' {shlex.quote(pubkey_content)} > /etc/repos-mcp/authorized_keys")
    server.succeed("systemctl restart repos-mcp.service")
    
    # Create test repository
    server.succeed("mkdir -p /var/lib/repos-mcp/tenant/test.git")
    server.succeed("cd /var/lib/repos-mcp/tenant && git init --bare test.git")
    
    # Configure SSH known_hosts
    client.succeed("ssh-keyscan -p 2222 server >> /root/.ssh/known_hosts")
    
    # Test: Clone with ed25519 authentication
    client.succeed("git clone ssh://root@server:2222/tenant/test.git /tmp/test-clone")
    client.succeed("test -d /tmp/test-clone/.git")
    
    # Test: Push commits
    client.succeed("""
      cd /tmp/test-clone &&
      git config user.name 'Test User' &&
      git config user.email 'test@example.com' &&
      echo 'test' > README.md &&
      git add README.md &&
      git commit -m 'Initial commit' &&
      git push origin main
    """)
    
    # Verify on server
    server.succeed("test -f /var/lib/repos-mcp/tenant/test.git/refs/heads/main")
  '';
}
```

### Example: Implementing Performance Tests

```nix
{
  id = "concurrent-clones-10";
  nodes = {
    server = { config, pkgs, ... }: {
      virtualisation.memorySize = 2048;
      virtualisation.cores = 4;
    };
  };
  testScript = ''
    import time
    import shlex
    
    # Setup (authentication, repository creation)
    server.wait_for_unit("repos-mcp.service")
    server.wait_for_open_port(2222)
    
    # ... SSH setup (see authentication example) ...
    
    # Create repository with initial content
    server.succeed("""
      mkdir -p /var/lib/repos-mcp/tenant/perf.git &&
      cd /var/lib/repos-mcp/tenant &&
      git init --bare perf.git
    """)
    
    # Add some commits to make clone non-trivial
    client.succeed("""
      git clone ssh://root@server:2222/tenant/perf.git /tmp/init &&
      cd /tmp/init &&
      git config user.name 'Test' &&
      git config user.email 'test@example.com'
    """)
    for i in range(100):
        client.succeed(f"""
          cd /tmp/init &&
          echo 'commit {i}' > file{i}.txt &&
          git add file{i}.txt &&
          git commit -m 'Commit {i}'
        """)
    client.succeed("cd /tmp/init && git push origin main")
    
    # Test: 10 concurrent clones
    start_time = time.time()
    
    # Start clones in parallel
    for i in range(10):
        client.succeed(f"git clone ssh://root@server:2222/tenant/perf.git /tmp/clone{i} >/dev/null 2>&1 &")
    
    # Wait for all to complete
    client.succeed("wait")
    
    elapsed = time.time() - start_time
    
    # Verify all clones succeeded
    for i in range(10):
        client.succeed(f"test -d /tmp/clone{i}/.git")
        client.succeed(f"test -f /tmp/clone{i}/file99.txt")
    
    print(f"10 concurrent clones completed in {elapsed:.2f}s")
    
    # Performance assertion (adjust threshold as needed)
    assert elapsed < 60, f"Concurrent clones took too long: {elapsed:.2f}s"
  '';
}
```

### Example: Implementing Error Handling Tests

```nix
{
  id = "invalid-ssh-key-rejected";
  testScript = ''
    server.wait_for_unit("repos-mcp.service")
    server.wait_for_open_port(2222)
    
    # Create invalid SSH key file
    client.succeed("mkdir -p /root/.ssh")
    client.succeed("echo 'invalid-key-data' > /root/.ssh/id_ed25519")
    client.succeed("echo 'ssh-ed25519 INVALID_BASE64_DATA user@host' > /root/.ssh/id_ed25519.pub")
    
    # Try to add invalid key to server
    server.succeed("mkdir -p /etc/repos-mcp")
    server.succeed("echo 'ssh-ed25519 INVALID_BASE64_DATA user@host' > /etc/repos-mcp/authorized_keys")
    server.succeed("systemctl restart repos-mcp.service")
    
    # Test: Clone should fail with invalid key
    client.fail("git clone ssh://root@server:2222/tenant/test.git /tmp/test-clone")
    
    # Verify error is logged on server
    server.succeed("journalctl -u repos-mcp.service | grep -i 'invalid.*key\\|auth.*fail'")
  '';
}
```

## Running VM Tests

### Single Test
```bash
# Run specific test
nix build .#checks.x86_64-linux.auth-ssh-ed25519-key

# View test output
nix log .#checks.x86_64-linux.auth-ssh-ed25519-key
```

### All Tests in Category
```bash
# Build all authentication tests
nix build .#checks.x86_64-linux.auth-*

# Note: This will fail for stub tests - they need implementation first
```

### Test Development Workflow

1. **Edit test in `nix/vm-tests.nix`**
2. **Validate syntax**: `nix-instantiate --parse nix/vm-tests.nix`
3. **Run test**: `nix build .#checks.x86_64-linux.<test-name> -L`
   - `-L` flag shows live build output
4. **Debug failures**: `nix log <derivation-path>`
5. **Iterate** until test passes

### VM Test Debugging

#### Interactive VM Session
```bash
# Start interactive VM
nix build .#checks.x86_64-linux.<test-name>.driver
./result/bin/nixos-test-driver

# In Python REPL:
>>> start_all()
>>> server.wait_for_unit("repos-mcp.service")
>>> server.shell_interact()  # Get shell on server
>>> client.shell_interact()  # Get shell on client
```

#### Common Issues

**Issue**: Test hangs waiting for service
```python
server.wait_for_unit("repos-mcp.service")  # Times out
```
**Solution**: Check service status and logs
```python
server.succeed("systemctl status repos-mcp.service")
server.succeed("journalctl -u repos-mcp.service -n 50")
```

**Issue**: SSH connection fails
```bash
# On client
ssh -vvv -p 2222 root@server
```
**Solution**: Check authorized_keys and SSH config
```python
server.succeed("cat /etc/repos-mcp/authorized_keys")
server.succeed("ls -la /etc/repos-mcp/ssh_host_*")
```

**Issue**: Git clone fails
```bash
GIT_TRACE=1 GIT_SSH_COMMAND="ssh -vvv" git clone ssh://root@server:2222/tenant/test.git
```

## Test Implementation Priority

### Phase 1 - Core Functionality (High Priority)
1. **Authentication tests** (5 key tests):
   - `auth-ssh-ed25519-key` - Basic SSH key auth
   - `auth-multiple-keys-same-user` - Multiple keys per user
   - `auth-invalid-key-rejected` - Invalid key rejection
   - `auth-permission-read-only-user` - Permission checks
   - `auth-key-rotation-ed25519` - Key rotation workflow

2. **Git Protocol tests** (5 key tests):
   - `git-protocol-clone-empty-repo` - Clone empty repository
   - `git-protocol-clone-with-commits` - Clone with history
   - `git-protocol-push-new-branch` - Push new branches
   - `git-protocol-fetch-specific-branch` - Fetch operations
   - `git-protocol-shallow-clone-depth-1` - Shallow clone

3. **Security tests** (3 key tests):
   - `security-unauthorized-access-denied` - Reject unauthorized access
   - `security-command-injection-prevention` - Prevent command injection
   - `security-path-traversal-prevention` - Prevent directory traversal

### Phase 2 - Performance & Scale (Medium Priority)
4. **Performance tests** (3 key tests):
   - `performance-concurrent-clones-10` - 10 concurrent clones
   - `performance-large-file-100mb` - 100MB file handling
   - `performance-memory-pressure-low` - Low memory conditions

5. **Storage tests** (2 key tests):
   - `storage-local-storage-persistence` - Local storage
   - `storage-corruption-detect-bad-object` - Corruption detection

### Phase 3 - Advanced Features (Lower Priority)
6. **Multi-tenancy, Network, LFS, Error Handling, Edge Cases**

## Contributing Test Implementations

When implementing stub tests:

1. **Follow the patterns** shown in this document
2. **Test incrementally** - implement one test, verify it works, move to next
3. **Handle errors gracefully** - use `client.fail()` for expected failures
4. **Add comments** explaining complex test logic
5. **Use subtests** for multi-step tests: `with subtest("description"):`
6. **Verify state** - don't just check exit codes, verify actual outcomes

## Additional Resources

- [NixOS VM Test Documentation](https://nixos.org/manual/nixos/stable/index.html#sec-nixos-tests)
- [repos-mcp Architecture](docs/ARCHITECTURE.md)
- [Git Protocol Specification](https://git-scm.com/docs/pack-protocol)

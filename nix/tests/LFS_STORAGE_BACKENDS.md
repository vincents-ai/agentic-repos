# LFS Storage Backend Configuration

This document describes the storage backend configurations supported by the agentic-repos LFS integration tests.

## Overview

The `lfs-integration.nix` test suite validates LFS operations across multiple storage backends:

1. **Local Storage** - Filesystem-based storage (default)
2. **S3-Compatible Storage** - Object storage via MinIO for testing

## Local Storage Backend

### Configuration

```yaml
storage:
  type: "local"
  local:
    path: "/var/lib/agentic-repos"

lfs:
  enabled: true
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
```

### Path Pattern Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `{tenant}` | Tenant identifier | `tenant1` |
| `{repo}` | Repository name | `myrepo` |
| `{oid}` | Full SHA256 object ID | `abc123...` |
| `{oid[0:2]}` | First 2 chars of OID | `ab` |
| `{oid[2:4]}` | Chars 2-4 of OID | `c1` |

### Example Path

For tenant `acme`, repo `website`, OID `abcdef123456...`:

```
/var/lib/agentic-repos/acme/website.git/lfs/objects/ab/cd/abcdef123456...
```

### NixOS Test Configuration

```nix
server = { config, pkgs, ... }: {
  environment.etc."agentic-repos/config.yaml".text = ''
    storage:
      type: "local"
      local:
        path: "/var/lib/agentic-repos"

    lfs:
      enabled: true
      storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  '';
};
```

## S3-Compatible Storage Backend

### Configuration

```yaml
storage:
  type: "s3"
  s3:
    endpoint: "http://minio:9000"
    access_key: "minioadmin"
    secret_key: "minioadmin"
    region: "us-east-1"
    bucket: "lfs-objects"
    use_ssl: false

lfs:
  enabled: true
  base_url: "http://server:8090"
```

### MinIO Test Setup

The test suite includes a MinIO node for S3-compatible storage testing:

```nix
storage = { config, pkgs, ... }: {
  systemd.services.minio = {
    description = "MinIO S3-compatible storage";
    environment = {
      MINIO_ROOT_USER = "minioadmin";
      MINIO_ROOT_PASSWORD = "minioadmin";
    };
    serviceConfig = {
      ExecStart = "${pkgs.minio}/bin/minio server /var/lib/minio --console-address :9001";
    };
  };
};
```

### Garage Alternative

For production-like testing with Garage (used in agentic-repos integration):

```yaml
storage:
  type: "s3"
  s3:
    endpoint: "http://garage:3900"
    access_key: "GK..."
    secret_key: "GS..."
    region: "garage"
    bucket: "lfs-bucket"
```

## Quota Configuration

### Default Quota

```yaml
lfs:
  quota:
    default_quota_bytes: 10737418240  # 10 GB
```

### Per-Tenant Quotas

```yaml
lfs:
  quota:
    default_quota_bytes: 10737418240
    per_tenant_quotas:
      "enterprise-customer": 107374182400  # 100 GB
      "basic-customer": 1073741824          # 1 GB
```

### Per-Repository Quotas

```yaml
lfs:
  quota:
    per_repo_quotas:
      "enterprise-customer/media-repo": 53687091200  # 50 GB
      "enterprise-customer/docs-repo": 1073741824    # 1 GB
```

## Bandwidth Throttling

### Configuration

```yaml
lfs:
  throttle:
    default_bytes_per_second: 10485760  # 10 MB/s
    burst_size: 52428800                # 50 MB burst
    per_tenant_limits:
      "premium-customer": 52428800      # 50 MB/s
    per_repo_limits:
      "premium-customer/videos": 104857600  # 100 MB/s
```

### Token Bucket Algorithm

The throttler uses a token bucket algorithm:

- **Capacity**: `burst_size` bytes
- **Refill Rate**: `bytes_per_second`
- **Burst**: Allows temporary spikes up to `burst_size`
- **Sustained**: Limited to `bytes_per_second`

## Locking Configuration

### Enable Locking

```yaml
lfs:
  locking_enabled: true
  lock_expiration_secs: 86400  # 24 hours
```

### Lock Behavior

- **Creation**: Requires valid LFS token with upload scope
- **Deletion**: Owner or force flag required
- **Expiration**: Locks auto-expire after `lock_expiration_secs`
- **Cleanup**: Expired locks removed by background job

## Running the Tests

### Run All LFS Tests

```bash
nix build .#tests.lfs-integration
```

### Run Specific Test Category

```bash
# Object operations only
nix eval .#tests.lfs-integration --apply 't: t.testScript' | jq -r 'select(contains("LFS object"))'

# Locking tests only
nix eval .#tests.lfs-integration --apply 't: t.testScript' | jq -r 'select(contains("LFS Locking"))'
```

### Debug Failed Tests

```bash
# Interactive VM shell
nix build .#tests.lfs-integration.driver
./result/bin/nixos-test-driver

# Inside driver
> start_all()
> server.succeed("journalctl -u agentic-repos -f")
```

## Storage Backend Selection

### Environment Variable

```bash
export LFS_STORAGE_BACKEND=local   # or s3
```

### Dynamic Configuration

The test suite supports dynamic backend selection via the `nodes` parameter:

```nix
mkLfsTest = { backend ? "local", ... }:
  let
    storageConfig = if backend == "s3" then s3Config else localConfig;
  in pkgs.testers.runNixOSTest {
    nodes.server = { ... }: {
      environment.etc."agentic-repos/config.yaml".text = storageConfig;
    };
  };
```

## Troubleshooting

### Common Issues

1. **Object not found after upload**
   - Check path pattern matches storage layout
   - Verify tenant/repo isolation

2. **Lock creation fails**
   - Ensure `locking_enabled: true`
   - Check PostgreSQL connectivity for lock storage

3. **Quota exceeded unexpectedly**
   - Verify `quota.default_quota_bytes` is appropriate
   - Check per-tenant/per-repo overrides

4. **S3 connection refused**
   - Ensure MinIO/Garage is running
   - Verify endpoint URL and credentials

### Debug Commands

```bash
# Check LFS storage layout
server.succeed("find /var/lib/agentic-repos -type d -name 'lfs'")

# List LFS locks
server.succeed("psql -d agentic_repos -c 'SELECT * FROM lfs_locks'")

# Check quota usage
server.succeed("curl -s http://localhost:8090/api/v1/{tenant}/quota")

# MinIO health
storage.succeed("curl -s http://localhost:9000/minio/health/live")
```

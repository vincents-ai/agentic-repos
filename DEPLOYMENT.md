# repos-mcp Plugin Deployment Guide

## Complete deployment instructions for repos-mcp with garage-s3 storage backend

## Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Quick Deployment](#quick-deployment)
4. [Manual Deployment](#manual-deployment)
5. [Configuration](#configuration)
6. [Integration with garage-s3](#integration-with-garage-s3)
7. [Testing](#testing)
8. [Troubleshooting](#troubleshooting)
9. [Production Deployment](#production-deployment)

## Overview

The repos-mcp plugin provides multi-tenant Git server functionality for Skylet, including:
- Git repository management (create, clone, push, pull)
- Git LFS support for large files
- Multi-tenant isolation
- Multiple storage backends (local, S3)

When combined with the garage-s3 plugin, repos-mcp can use S3-compatible storage for both Git repositories and LFS objects.

### Architecture

```
┌─────────────────────────────────────────┐
│           Skylet Runtime                │
└────────────────┬────────────────────────┘
                 │
         ┌───────┴────────┐
         │                │
    ┌────▼─────┐    ┌────▼──────┐
    │ repos-mcp│    │ garage-s3 │
    │  (Git)   │────│  (S3)     │
    └──────────┘    └─────┬─────┘
                          │
                    ┌─────▼─────┐
                    │  Garage   │
                    │  Storage  │
                    └───────────┘
```

## Prerequisites

### System Requirements

- **OS**: Linux (x86_64 or aarch64)
- **Memory**: Minimum 512MB RAM
- **Disk**: 1GB for plugins + storage space
- **Skylet**: v0.5.0 or later

### Required Software

- Skylet runtime installed
- Git (for repository operations)
- Git LFS (for large file support)

### Files Needed

1. **repos-mcp plugin**: `librepos_mcp.so` (~10MB)
2. **garage-s3 plugin**: `libgarage_s3.so` (~11MB) - optional but recommended

## Quick Deployment

### Step 1: Build Plugins

```bash
# Clone repos-mcp
cd ~/code/vincents-ai/repos-mcp
nix build

# Build garage-s3 (optional, for S3 storage)
cd ~/code/vincents-ai/skynet
cargo build --release -p garage-s3
```

### Step 2: Create Deployment Package

```bash
cd ~/code/vincents-ai/repos-mcp
./package.sh
```

This creates: `dist/repos-mcp-plugin-0.1.0.tar.gz`

### Step 3: Deploy

```bash
# Extract package
tar -xzf dist/repos-mcp-plugin-0.1.0.tar.gz
cd repos-mcp-plugin-0.1.0

# Install to Skylet
export SKYLET_DIR=~/code/skylet
./install.sh
```

### Step 4: Start Skylet

```bash
cd ~/code/skylet
source repos-mcp.env
./target/release/skylet server
```

### Step 5: Verify

```bash
# Check health
curl http://localhost:8080/health

# Test git operations
cd ~/code/vincents-ai/repos-mcp
./test-simple.sh
```

## Manual Deployment

### Step 1: Install Plugin Files

```bash
# Create Skylet plugins directory
mkdir -p ~/code/skylet/plugins/repos-mcp
mkdir -p ~/code/skylet/plugins/garage-s3

# Copy repos-mcp plugin
cp ~/code/vincents-ai/repos-mcp/target/release/librepos_mcp.so \
   ~/code/skylet/plugins/repos-mcp/

# Copy garage-s3 plugin (optional)
cp ~/code/vincents-ai/skynet/target/release/libgarage_s3.so \
   ~/code/skylet/plugins/garage-s3/

# Make executable
chmod +x ~/code/skylet/plugins/*/*.so
```

### Step 2: Create Configuration

Create `~/code/skylet/config.yaml`:

```yaml
server:
  host: "0.0.0.0"
  port: 8080

logging:
  level: "info"
  format: "json"

plugins:
  # Storage backend - load first
  - name: garage-s3
    path: ./plugins/garage-s3/libgarage_s3.so
    enabled: true
    priority: 10
    config:
      auto_start_garage: true
      data_dir: "./data/garage"
      s3_port: 3900

  # Git operations - load second
  - name: git-operations
    path: ./plugins/repos-mcp/librepos_mcp.so
    enabled: true
    priority: 5
    config:
      storage.default_backend: "local"
      storage.local_path: "./data/repos"
      ssh.enabled: false
      lfs.enabled: true
      log.level: "info"

discovery:
  enabled: true
  scan_paths:
    - ./plugins
```

### Step 3: Set Environment Variables

Create `~/code/skylet/repos-mcp.env`:

```bash
# Storage Configuration
export REPOS_STORAGE_PATH="./data/repos"
export REPOS_STORAGE_BACKEND="local"

# S3 Configuration (when using garage-s3)
export GARAGE_ENDPOINT="http://localhost:3900"
export GARAGE_REGION="garage"
export GARAGE_ACCESS_KEY_ID="garage"
export GARAGE_SECRET_ACCESS_KEY="garage"
export GARAGE_BUCKET="repos"

# LFS Configuration
export REPOS_LFS_ENABLED=true
export REPOS_LFS_BASE_URL="http://localhost:8080"

# Logging
export REPOS_LOG_LEVEL="info"
```

Source it:

```bash
source ~/code/skylet/repos-mcp.env
```

### Step 4: Start Skylet

```bash
cd ~/code/skylet
./target/release/skylet server
```

Expected output:

```
INFO Starting Skylet server...
INFO Bootstrap plugins loaded successfully
INFO Discovered 7 plugins from 1 search paths
INFO Successfully initialized v2 plugin: garage_s3
INFO Successfully initialized v2 plugin: repos_mcp
INFO Server listening on 0.0.0.0:8080
```

## Configuration

### repos-mcp Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `storage.default_backend` | string | "local" | Storage backend: "local" or "s3" |
| `storage.local_path` | string | "./data/repos" | Local storage directory |
| `ssh.enabled` | bool | false | Enable SSH access |
| `ssh.port` | int | 2222 | SSH server port |
| `lfs.enabled` | bool | true | Enable Git LFS |
| `lfs.base_url` | string | "http://localhost:8080" | LFS server URL |
| `lfs.max_object_size` | int | 5368709120 | Max LFS object size (5GB) |
| `log.level` | string | "info" | Log level: debug, info, warn, error |

### garage-s3 Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `auto_start_garage` | bool | true | Automatically start Garage process |
| `data_dir` | string | "./data/garage" | Garage data directory |
| `s3_port` | int | 3900 | S3 API port |
| `rpc_port` | int | 3901 | RPC port |
| `admin_port` | int | 3903 | Admin API port |

### Environment Variables

repos-mcp respects these environment variables:

```bash
# Storage
REPOS_STORAGE_PATH         # Local storage path
REPOS_STORAGE_BACKEND      # "local" or "s3"
REPOS_SSH_ENABLED         # Enable SSH (true/false)

# S3 (when using garage-s3)
GARAGE_ENDPOINT           # S3 endpoint URL
GARAGE_REGION             # S3 region
GARAGE_ACCESS_KEY_ID      # Access key
GARAGE_SECRET_ACCESS_KEY  # Secret key
GARAGE_BUCKET             # Default bucket

# LFS
REPOS_LFS_ENABLED         # Enable LFS (true/false)
REPOS_LFS_BASE_URL        # LFS server URL
REPOS_LFS_MAX_OBJECT_SIZE # Max object size in bytes

# Logging
REPOS_LOG_LEVEL           # Log level
```

## Integration with garage-s3

### How It Works

1. **garage-s3** starts embedded Garage server as a subprocess
2. **garage-s3** registers S3 storage service in Skylet's global registry
3. **repos-mcp** queries global registry for available storage backends
4. **repos-mcp** uses S3 backend for Git repositories and LFS objects

### Enabling S3 Storage

Update `config.yaml`:

```yaml
plugins:
  - name: git-operations
    path: ./plugins/repos-mcp/librepos_mcp.so
    config:
      storage.default_backend: "s3"  # Changed from "local"
      storage.local_path: "./data/repos"
      lfs.enabled: true
```

Or set environment variable:

```bash
export REPOS_STORAGE_BACKEND="s3"
```

### S3 Storage Layout

When using garage-s3, repositories are stored as:

```
bucket/
├── {tenant_id}/
│   ├── repos/
│   │   ├── {repo_name}.git/
│   │   │   ├── HEAD
│   │   │   ├── objects/
│   │   │   └── refs/
│   └── lfs/
│       └── objects/
│           └── {oid}
```

### Multi-Tenant Isolation

Each tenant gets isolated storage:
- Separate S3 prefix per tenant
- Separate bucket or bucket prefix
- Access control via tenant ID

## Testing

### Basic Git Operations Test

```bash
cd ~/code/vincents-ai/repos-mcp
./test-simple.sh
```

Expected output:

```
=== repos-mcp Git Operations Test ===
✓ Plugin found
✓ Plugin validated
✓ Storage configured
✓ Repository created
✓ Repository listed
✓ Git repository initialized
✓ Created 3 commits
✓ Branch operations completed
✓ LFS configured
=== All tests passed! ===
```

### Integration Test with Skylet

```bash
cd ~/code/vincents-ai/repos-mcp
./test-integration.sh
```

This tests:
- Plugin loading in Skylet
- Health checks
- Repository operations
- Git operations
- LFS configuration

### Manual Testing

```bash
# Create repository
~/code/skylet/target/release/plugin-test-harness execute \
  -p ~/code/skylet/plugins/repos-mcp/librepos_mcp.so \
  -a repo.create \
  --args '{"id":"1","method":"repo.create","params":{"name":"test","tenant_id":"tenant1"}}'

# List repositories
~/code/skylet/target/release/plugin-test-harness execute \
  -p ~/code/skylet/plugins/repos-mcp/librepos_mcp.so \
  -a repo.list \
  --args '{"id":"2","method":"repo.list","params":{"tenant_id":"tenant1"}}'

# Health check
~/code/skylet/target/release/plugin-test-harness execute \
  -p ~/code/skylet/plugins/repos-mcp/librepos_mcp.so \
  -a health \
  --args '{}'
```

## Troubleshooting

### Plugin Not Loading

**Symptoms**: Plugin doesn't appear in loaded plugins list

**Solutions**:
1. Check file permissions: `chmod +x plugins/*/*.so`
2. Check dependencies: `ldd plugins/repos-mcp/librepos_mcp.so`
3. Check logs for errors
4. Verify ABI version matches Skylet version

### S3 Storage Not Working

**Symptoms**: Repository operations fail with S3 errors

**Solutions**:
1. Verify garage-s3 plugin loaded first (check priority)
2. Check Garage is running: `ps aux | grep garage`
3. Check S3 endpoint: `curl http://localhost:3900/health`
4. Verify bucket exists
5. Check environment variables are set

### LFS Uploads Failing

**Symptoms**: Git LFS objects not uploading

**Solutions**:
1. Verify LFS is enabled in config
2. Check LFS base URL is correct
3. Verify S3 backend is accessible
4. Check LFS tracking is configured: `git lfs track`
5. Verify `.gitattributes` file exists

### Permission Denied Errors

**Symptoms**: Repository operations fail with permission errors

**Solutions**:
1. Check data directory permissions
2. Verify tenant ID is correct
3. Check user has access to tenant
4. Review Skylet security policies

### Logs and Debugging

Enable debug logging:

```yaml
plugins:
  - name: git-operations
    config:
      log.level: "debug"
```

Or:

```bash
export REPOS_LOG_LEVEL="debug"
```

Check logs:

```bash
# Skylet logs
tail -f ~/code/skylet/skylet.log

# Plugin logs (if configured)
tail -f ~/code/skylet/logs/repos-mcp.log
```

## Production Deployment

### Security Checklist

- [ ] Change default credentials
- [ ] Enable TLS/SSL
- [ ] Configure firewall rules
- [ ] Set up authentication
- [ ] Enable audit logging
- [ ] Review tenant isolation
- [ ] Configure backup strategy

### Performance Tuning

```yaml
plugins:
  - name: git-operations
    config:
      # Increase for high load
      max_concurrent_ops: 100
      
      # Optimize for large repos
      pack.window_memory: "1g"
      pack.delta_cache_size: "512m"
      
      # LFS tuning
      lfs.max_object_size: 10737418240  # 10GB
```

### High Availability

1. **Load Balancer**: Use multiple Skylet instances behind load balancer
2. **Shared Storage**: Use S3 backend with redundant storage
3. **Database**: Use external database for metadata
4. **Monitoring**: Set up health checks and alerting

### Backup Strategy

```bash
# Backup repositories
tar -czf repos-backup-$(date +%Y%m%d).tar.gz ./data/repos/

# Backup S3 data (if using garage-s3)
aws s3 sync s3://repos s3://repos-backup/

# Backup configuration
cp config.yaml config.yaml.backup
```

### Monitoring

Enable metrics:

```yaml
metrics:
  enabled: true
  port: 9090
  path: "/metrics"
```

Monitor:
- Repository count per tenant
- LFS storage usage
- Operation latency
- Error rates
- Git operation throughput

### Scaling

For large deployments:

1. **Horizontal**: Add more Skylet instances
2. **Storage**: Use distributed S3 (Garage cluster)
3. **Caching**: Add Redis for metadata caching
4. **CDN**: Use CDN for LFS objects

## Support

### Documentation

- [Plugin Validation Guide](./docs/SKYLET_PLUGIN_VALIDATION.md)
- [Git Operations Validation](./docs/GIT_OPERATIONS_VALIDATION.md)
- [Integration Report](./docs/INTEGRATION_REPORT.md)
- [AGENTS.md](./AGENTS.md)

### Getting Help

- **GitHub**: https://github.com/vincents-ai/repos-mcp
- **Skylet**: https://github.com/vincents-ai/skylet
- **Issues**: Report bugs and request features on GitHub

### Contributing

1. Fork the repository
2. Create feature branch
3. Make changes
4. Run tests: `cargo test`
5. Submit pull request

## License

AGPL-3.0

---

**Package Version**: 0.1.0  
**Last Updated**: 2026-03-07  
**Maintained by**: Vincents AI

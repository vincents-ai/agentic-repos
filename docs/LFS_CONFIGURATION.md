# Git LFS Configuration Reference

This document provides complete configuration reference for Git LFS support in repos-mcp.

## Prerequisites

- `git_lfs` feature flag must be enabled
- `lfs.enabled` must be set to `true` in configuration
- HTTP API server must be running

## Configuration Structure

```yaml
features:
  git_lfs: true          # Enable LFS feature (required)
  lfs_locks: false       # Enable file locking (optional, future feature)

lfs:
  enabled: true          # Enable LFS functionality
  jwt_secret: "your-strong-secret-here"
  base_url: "https://your-server.com"
  max_object_size: 5368709120
  token_expiration_secs: 3600
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  locking_enabled: false
  lock_expiration_secs: 86400
```

## Configuration Parameters

### Feature Flags

#### `features.git_lfs`
- **Type**: Boolean
- **Default**: `false`
- **Required**: Yes (to use LFS)
- **Description**: Master switch for Git LFS functionality. Must be enabled for any LFS operations.

#### `features.lfs_locks`
- **Type**: Boolean
- **Default**: `false`
- **Required**: No
- **Description**: Enables file locking API endpoints (Phase 7 feature, not yet implemented).

### LFS Core Settings

#### `lfs.enabled`
- **Type**: Boolean
- **Default**: `false`
- **Required**: Yes
- **Description**: Enables LFS HTTP API endpoints. Both `features.git_lfs` and `lfs.enabled` must be `true` for LFS to function.

#### `lfs.jwt_secret`
- **Type**: String
- **Default**: `"CHANGE_THIS_SECRET_IN_PRODUCTION_USE_STRONG_RANDOM_STRING"`
- **Required**: Yes
- **Security**: **CRITICAL** - Must be changed in production
- **Description**: Secret key used for JWT token generation and validation. Used by SSH authentication to generate tokens for HTTP API access.
- **Recommendations**:
  - Use at least 32 characters
  - Use cryptographically random string
  - Store securely (environment variable or secrets manager)
  - Rotate periodically for security

**Example generation**:
```bash
# Generate strong random secret
openssl rand -base64 32
```

#### `lfs.base_url`
- **Type**: String
- **Default**: `"http://localhost:8090"`
- **Required**: Yes
- **Description**: Base URL for LFS HTTP API endpoints. Used in SSH authentication responses and object URLs.
- **Format**: `protocol://hostname:port` (no trailing slash)
- **Examples**:
  - Development: `http://localhost:8090`
  - Production: `https://lfs.example.com`
  - With port: `https://lfs.example.com:8443`

### Object Storage Settings

#### `lfs.max_object_size`
- **Type**: Integer (bytes)
- **Default**: `5368709120` (5 GB)
- **Required**: No
- **Description**: Maximum size allowed for a single LFS object
- **Recommendations**:
  - Set based on available storage and network bandwidth
  - Consider client upload timeouts
  - Balance between flexibility and resource constraints
- **Examples**:
  - 100 MB: `104857600`
  - 1 GB: `1073741824`
  - 5 GB: `5368709120` (default)
  - 10 GB: `10737418240`

#### `lfs.storage_path_pattern`
- **Type**: String
- **Default**: `"{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"`
- **Required**: No
- **Description**: Path pattern for storing LFS objects in the storage backend
- **Template Variables**:
  - `{tenant}`: Tenant name from repository path
  - `{repo}`: Repository name
  - `{oid}`: Full SHA256 object ID (64 hex characters)
  - `{oid[0:2]}`: First 2 characters of OID (for directory sharding)
  - `{oid[2:4]}`: Characters 3-4 of OID (for nested directory sharding)
- **Purpose**: Directory sharding prevents filesystem performance issues with many files in one directory
- **Examples**:
  - Default: `{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}`
    - Result: `acme/myapp.git/lfs/objects/2c/26/2c26b46b...`
  - Flat structure: `{tenant}/{repo}.git/lfs/{oid}`
    - Result: `acme/myapp.git/lfs/2c26b46b...`
  - Deep sharding: `lfs/{tenant}/{oid[0:2]}/{oid[2:4]}/{oid[4:6]}/{repo}/{oid}`
    - Result: `lfs/acme/2c/26/b4/myapp.git/2c26b46b...`

### Authentication & Security

#### `lfs.token_expiration_secs`
- **Type**: Integer (seconds)
- **Default**: `3600` (1 hour)
- **Required**: No
- **Description**: JWT token lifetime for LFS operations
- **Recommendations**:
  - Short enough to limit exposure (300-3600 seconds)
  - Long enough for large file transfers
  - Consider network speeds and typical file sizes
- **Examples**:
  - 5 minutes: `300`
  - 15 minutes: `900`
  - 1 hour: `3600` (default)
  - 2 hours: `7200`

### File Locking (Future Feature)

#### `lfs.locking_enabled`
- **Type**: Boolean
- **Default**: `false`
- **Required**: No
- **Description**: Enable LFS file locking API. Requires `features.lfs_locks` to be `true`.
- **Status**: Not yet implemented (Phase 7)

#### `lfs.lock_expiration_secs`
- **Type**: Integer (seconds)
- **Default**: `86400` (24 hours)
- **Required**: No
- **Description**: Automatic lock expiration time
- **Status**: Not yet implemented (Phase 7)

## Complete Example Configurations

### Development Configuration

```yaml
server:
  host: "127.0.0.1"
  port: 8090

features:
  git_lfs: true

lfs:
  enabled: true
  jwt_secret: "dev-secret-do-not-use-in-production"
  base_url: "http://localhost:8090"
  max_object_size: 1073741824  # 1 GB for local testing
  token_expiration_secs: 3600
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  locking_enabled: false
  lock_expiration_secs: 86400
```

### Production Configuration

```yaml
server:
  host: "0.0.0.0"
  port: 8090

features:
  git_lfs: true

lfs:
  enabled: true
  jwt_secret: "${LFS_JWT_SECRET}"  # Load from environment
  base_url: "https://lfs.example.com"
  max_object_size: 5368709120  # 5 GB
  token_expiration_secs: 1800  # 30 minutes for security
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  locking_enabled: false
  lock_expiration_secs: 86400
```

### High-Security Configuration

```yaml
server:
  host: "0.0.0.0"
  port: 8090

features:
  git_lfs: true

lfs:
  enabled: true
  jwt_secret: "${LFS_JWT_SECRET}"  # Rotate monthly
  base_url: "https://lfs.secure-example.com"
  max_object_size: 2147483648  # 2 GB limit
  token_expiration_secs: 600  # 10 minutes only
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  locking_enabled: false
  lock_expiration_secs: 3600  # 1 hour lock timeout
```

## Environment Variables

While the configuration file is the primary configuration method, sensitive values can be loaded from environment variables:

```bash
export LFS_JWT_SECRET="$(openssl rand -base64 32)"
export LFS_BASE_URL="https://lfs.example.com"
```

Then reference in config:
```yaml
lfs:
  jwt_secret: "${LFS_JWT_SECRET}"
  base_url: "${LFS_BASE_URL}"
```

## Security Best Practices

1. **JWT Secret**:
   - Never commit secrets to version control
   - Use strong random strings (32+ characters)
   - Rotate secrets periodically
   - Store in secure secret management system

2. **Base URL**:
   - Always use HTTPS in production
   - Ensure TLS certificates are valid
   - Use proper DNS with load balancing

3. **Token Expiration**:
   - Balance security vs usability
   - Shorter is more secure
   - Consider your typical file sizes and network speeds

4. **Object Size Limits**:
   - Set realistic limits based on your infrastructure
   - Monitor storage capacity
   - Consider bandwidth costs

## Troubleshooting

### LFS Not Working

**Check**:
1. Is `features.git_lfs` enabled?
2. Is `lfs.enabled` set to `true`?
3. Is HTTP server running on correct port?
4. Is `base_url` accessible from clients?

**Verify**:
```bash
# Check server logs for LFS initialization
# Should see: "LFS routes configured"

# Test batch endpoint
curl -X POST http://localhost:8090/lfs/tenant/repo/info/lfs/objects/batch \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -d '{"operation":"download","objects":[{"oid":"test","size":100}]}'
```

### Authentication Failures

**Check**:
1. Is `jwt_secret` configured?
2. Are tokens expired? (check `token_expiration_secs`)
3. Is system time synchronized?

### Upload Failures

**Check**:
1. File size vs `max_object_size`
2. Storage backend permissions
3. Available disk space

### Download Failures

**Check**:
1. Object exists in storage
2. Correct tenant/repo path
3. Storage backend accessibility

## Related Documentation

- [Git LFS User Guide](./LFS_USER_GUIDE.md) - Client configuration and usage
- [LFS API Reference](./LFS_API.md) - HTTP endpoint specifications  
- [Troubleshooting Guide](./LFS_TROUBLESHOOTING.md) - Common issues and solutions
- [Security Guide](./LFS_SECURITY.md) - Security best practices and audit

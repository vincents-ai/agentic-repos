Overview
--------

This directory contains the Skynet v2 Git Operations Plugin, a production-ready multi-tenant Git server
that integrates seamlessly with the Skynet v2 orchestration platform.

Key Features
------------

- **Multi-Tenant Architecture**: Complete tenant isolation at the namespace level
- **SSH Authentication**: Ed25519 key support with secure key management
- **Git LFS Support**: Large file storage with S3-compatible backends
- **Storage Flexibility**: Local filesystem or S3-compatible storage
- **Rate Limiting**: Configurable per-user and per-operation limits
- **Monitoring**: Prometheus metrics and distributed tracing support
- **Skynet v2 Compatible**: Full PluginApi implementation for seamless integration

Module Structure
----------------

```
skynet/
├── mod.rs                   # Plugin entry point and initialization
├── error.rs                 # Error types and handling
├── metadata.rs              # Plugin metadata for Skynet v2 registration
├── interface.rs             # Skynet v2 PluginApi implementation
├── config.rs                # Plugin configuration management
├── api.rs                   # API method descriptors
├── git_ops.rs               # Git operation service
├── health.rs                # Health checking
├── monitoring.rs            # Metrics collection
├── multi_tenant/            # Multi-tenancy support
│   ├── mod.rs              # Tenant context and isolation
│   ├── namespace.rs         # Tenant namespace paths
│   ├── permissions.rs       # RBAC - Roles and permissions
│   └── quota.rs             # Per-tenant quotas
├── rate_limit/              # Rate limiting implementation
│   ├── mod.rs              # Rate limiter service
│   ├── config.rs            # Rate limit configuration
│   └── token_bucket.rs      # Token bucket algorithm
└── storage/                 # Storage backends
    ├── mod.rs              # Storage trait abstraction
    ├── local.rs             # Local filesystem backend
    └── s3.rs                # S3-compatible storage backend
```

Key Components
--------------

### Plugin Interface (interface.rs)

Implements the Skynet v2 PluginApi trait with methods:
- `metadata()` - Returns plugin information and capabilities
- `init()` - Initialize the plugin
- `shutdown()` - Gracefully shutdown
- `health_check()` - Health status reporting
- `execute()` - Execute plugin commands

Request/response handling through PluginRequest/PluginResponse structures
supporting JSON-RPC style communication.

### Multi-Tenancy (multi_tenant/)

- **TenantContext**: Contains tenant_id, user_id, roles, and API token
- **TenantNamespace**: Maps repositories to isolated filesystem paths
- **Role**: Owner, Maintainer, Contributor, ReadOnly
- **Permission**: Fine-grained permission model for operations

Repository paths follow pattern:
```
/base_path/{tenant_id}/repos/{repo_name}.git
```

### Storage Backends (storage/)

Abstract StorageBackend trait with implementations:

**LocalStorageBackend**
- Filesystem-based storage
- Efficient for single-node deployments
- Direct file system access

**S3StorageBackend**
- S3-compatible object storage (MinIO, AWS S3, etc.)
- Distributed storage with high availability
- Structured prefixes for organization

Both support:
- Repository creation/deletion
- LFS object upload/download

### Rate Limiting (rate_limit/)

Token bucket algorithm implementation:
- Per-user limits: API calls/minute, concurrent connections
- Per-operation limits: Clone/push timeouts
- Global limits: Max concurrent users, total data transfer
- Configurable burst allowance

### Monitoring (monitoring.rs)

MetricsCollector tracks:
- Request counts per method
- Success rates
- Error rates and codes
- Detailed metrics export in JSON format

Health Status:
- Overall health status (healthy/degraded/unhealthy)
- Active repositories and sessions
- Storage usage
- Error details if not healthy

Usage Examples
--------------

### Initialize Plugin

```rust
use repos_mcp::skynet;
use repos_mcp::config::AppConfig;

let config = AppConfig::from_env()?;
let plugin = skynet::init_plugin(config).await?;
```

### Create Repository

```rust
let request = skynet::interface::PluginRequest {
    id: "req-1".to_string(),
    method: "repo.create".to_string(),
    params: json!({"name": "my-repo"}),
    tenant_id: Some("tenant-1".to_string()),
    user_id: Some("user-1".to_string()),
    token: None,
};

let response = plugin.handle_request(request).await;
```

### Check Health

```rust
let request = skynet::interface::PluginRequest {
    id: "req-2".to_string(),
    method: "health.check".to_string(),
    params: json!({}),
    tenant_id: None,
    user_id: None,
    token: None,
};

let response = plugin.handle_request(request).await;
```

API Methods
-----------

Repository Operations:
- `repo.create` - Create a new repository
- `repo.delete` - Delete a repository
- `repo.list` - List repositories for tenant
- `repo.info` - Get repository information

Git Operations:
- `git.push` - Push commits
- `git.pull` - Pull commits
- `git.branch` - Manage branches
- `git.tag` - Create/manage tags
- `git.log` - View commit history

Authentication:
- `auth.ssh_key_add` - Add SSH public key
- `auth.ssh_key_remove` - Remove SSH key
- `auth.ssh_key_list` - List SSH keys

LFS Operations:
- `lfs.upload` - Upload LFS object
- `lfs.download` - Download LFS object
- `lfs.lock` - Lock LFS file
- `lfs.unlock` - Unlock LFS file

Monitoring:
- `health.check` - Check plugin health
- `metrics.get` - Get plugin metrics

Configuration
--------------

Plugin configuration in AppConfig:

```yaml
skynet:
  min_version: "2.0.0"
  api_timeout_secs: 30
  health_check_interval_secs: 30

storage:
  backend: s3  # or local
  local:
    base_path: /var/repos-mcp/repositories
  s3:
    endpoint: https://s3.amazonaws.com
    bucket: git-repositories
    region: us-east-1

multi_tenant:
  enabled: true
  isolation_level: namespace
  default_quota_mb: 10000

rate_limiting:
  enabled: true
  per_user:
    api_calls_per_minute: 1000
    concurrent_connections: 10
  per_operation:
    clone_timeout_secs: 300

monitoring:
  metrics:
    enabled: true
    port: 9090
  health_check:
    enabled: true
    port: 8080
```

Error Handling
--------------

SkynetPluginError provides comprehensive error types:
- Configuration - Config loading/validation errors
- Storage - Storage backend errors
- Tenant - Tenant isolation/permission errors
- PermissionDenied - Access control violations
- RateLimitExceeded - Rate limit violations
- Git - Git operation errors
- Lfs - Git LFS errors
- Authentication - Auth failures
- RepositoryNotFound - Repo not found
- RepositoryExists - Repo already exists
- InvalidRequest - Invalid request parameters
- ResourceExhausted - Quota/resource limits
- Timeout - Operation timeout
- Io - I/O errors
- Json - JSON serialization errors

Each error includes:
- Error code for protocol
- HTTP status code
- Detailed error message
- Conversion to Skynet v2 error response

Testing
-------

Unit tests for all components:
- Error handling and conversions
- Plugin metadata generation
- Rate limiter functionality
- Token bucket algorithm
- Permission checking
- Configuration parsing

Run tests with:
```bash
cargo test --lib skynet::
```

Future Enhancements
-------------------

1. Advanced Authentication:
   - LDAP/SAML/OAuth integration
   - MFA support
   - Token management

2. Performance:
   - Redis caching layer
   - Multi-region replication
   - Load balancing

3. Compliance:
   - Audit logging
   - SOC2/GDPR/HIPAA support
   - Access tracking

4. Developer Experience:
   - Web UI for repo management
   - CI/CD integration
   - GraphQL API
   - Analytics dashboard

5. Storage:
   - Azure Blob Storage backend
   - Google Cloud Storage backend
   - Ceph support

References
----------

- Skynet v2 Documentation: See parent repository
- Git LFS Specification: https://github.com/git-lfs/git-lfs/wiki/Specification
- SSH Protocol: RFC 4251-4254
- S3 API: https://docs.aws.amazon.com/s3/

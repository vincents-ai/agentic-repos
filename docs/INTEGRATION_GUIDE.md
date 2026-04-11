Integration Guide - repos-mcp Skynet v2 Git Operations Plugin
==============================================================

This guide provides step-by-step instructions for integrating the Git Operations Plugin
with Skynet v2 orchestration platform.

Table of Contents
-----------------
1. Overview
2. Prerequisites
3. Plugin Initialization
4. Skynet v2 Registration
5. Configuration
6. API Integration Examples
7. Deployment Considerations
8. Troubleshooting
9. Performance Tuning

1. Overview
-----------

The Git Operations Plugin provides:
- Skynet v2 PluginApi compliant interface
- Multi-tenant Git server capabilities
- SSH authentication
- Git LFS support
- Rate limiting
- Comprehensive monitoring

Integration Points:
- Plugin lifecycle (init, health_check, shutdown)
- JSON-RPC style request/response
- Tenant context propagation
- Rate limit enforcement
- Metrics export

2. Prerequisites
----------------

Required:
- Skynet v2 version 2.0.0 or later
- Rust 1.70 or later
- repos-mcp binary or library

Optional:
- S3-compatible storage (MinIO, AWS S3, etc.)
- OpenTelemetry collector for distributed tracing
- Prometheus for metrics collection
- PostgreSQL/SQLite for user/key management

3. Plugin Initialization
------------------------

### In Your Skynet v2 Integration Code

```rust
use repos_mcp::skynet;
use repos_mcp::config::AppConfig;
use repos_mcp::skynet::interface::PluginApi;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = AppConfig::from_env()?;
    
    // Initialize plugin
    let plugin = skynet::init_plugin(config).await?;
    
    // Get metadata for registration
    let metadata = plugin.metadata().await;
    println!("Plugin: {} v{}", metadata.name, metadata.version);
    
    // Initialize plugin
    plugin.init().await?;
    
    // Check health
    let health = plugin.health_check().await?;
    println!("Health: {}", health.status);
    
    Ok(())
}
```

### Plugin Initialization Steps

1. Create AppConfig with storage backend settings
2. Call skynet::init_plugin(config)
3. Call plugin.init() to perform initialization
4. Register plugin metadata with Skynet v2 registry
5. Start health check loop
6. Begin accepting requests

4. Skynet v2 Registration
-------------------------

### Register Plugin with Skynet v2 Manager

The plugin implements the standard PluginApi interface:

```rust
#[async_trait]
pub trait PluginApi: Send + Sync {
    async fn metadata(&self) -> PluginMetadata;
    async fn init(&self) -> SkynetPluginResult<()>;
    async fn shutdown(&self) -> SkynetPluginResult<()>;
    async fn health_check(&self) -> SkynetPluginResult<HealthStatus>;
    async fn execute(&self, request: PluginRequest) -> SkynetPluginResult<PluginResponse>;
}
```

### Metadata Structure

```json
{
  "name": "git-operations",
  "version": "0.1.0",
  "author": "Vincents.ai Team",
  "description": "Multi-tenant Git server with SSH authentication, Git LFS support...",
  "capabilities": [
    "repo.create",
    "repo.delete",
    "repo.list",
    "git.push",
    "git.pull",
    "lfs.upload",
    "lfs.download",
    "health.check",
    "metrics.get",
    ...
  ],
  "min_skynet_version": "2.0.0",
  "endpoints": {
    "health": "/health",
    "metrics": "/metrics",
    "api": "/api/v1"
  }
}
```

5. Configuration
----------------

### Environment Variables

```bash
# Storage backend
REPOS_STORAGE_BACKEND=s3  # or local
REPOS_STORAGE_LOCAL_PATH=/var/repos
REPOS_STORAGE_S3_ENDPOINT=https://s3.amazonaws.com
REPOS_STORAGE_S3_BUCKET=git-repositories
REPOS_STORAGE_S3_REGION=us-east-1

# Multi-tenancy
REPOS_MULTITENANT_ENABLED=true
REPOS_MULTITENANT_ISOLATION_LEVEL=namespace

# Rate limiting
REPOS_RATELIMIT_ENABLED=true
REPOS_RATELIMIT_PER_USER_API_CALLS=1000
REPOS_RATELIMIT_PER_USER_CONNECTIONS=10

# Monitoring
REPOS_MONITORING_METRICS_ENABLED=true
REPOS_MONITORING_METRICS_PORT=9090
REPOS_MONITORING_TRACING_ENABLED=true
REPOS_MONITORING_HEALTH_PORT=8080
```

### Configuration File (YAML)

```yaml
# repos-mcp configuration
skynet:
  min_version: "2.0.0"
  api_timeout_secs: 30
  health_check_interval_secs: 30

storage:
  backend: s3
  s3:
    endpoint: https://s3.amazonaws.com
    access_key: ${AWS_ACCESS_KEY_ID}
    secret_key: ${AWS_SECRET_ACCESS_KEY}
    bucket: git-repositories
    region: us-east-1
    use_path_style: false

multi_tenant:
  enabled: true
  isolation_level: namespace
  default_quota_mb: 10000
  per_tenant_quotas:
    tenant-1: 50000
    tenant-2: 25000

rate_limiting:
  enabled: true
  per_user:
    api_calls_per_minute: 1000
    concurrent_connections: 10
    data_transfer_mb_per_hour: 1000
  per_operation:
    clone_timeout_secs: 300
    push_timeout_secs: 300
  global:
    max_concurrent_users: 1000
    max_data_transfer_mb_per_hour: 100000

monitoring:
  metrics:
    enabled: true
    port: 9090
    path: /metrics
  tracing:
    enabled: true
    endpoint: http://localhost:4317
    service_name: repos-mcp
  health_check:
    enabled: true
    port: 8080
    path: /health
```

6. API Integration Examples
---------------------------

### Request Format

All requests follow the PluginRequest structure:

```json
{
  "id": "unique-request-id",
  "method": "repo.create",
  "params": {
    "name": "my-repository",
    "description": "My awesome repo",
    "private": false
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456",
  "token": "api-token-xyz"
}
```

### Response Format

Successful response:

```json
{
  "id": "unique-request-id",
  "result": {
    "repository": {
      "tenant_id": "tenant-123",
      "name": "my-repository",
      "created_at": "2024-01-28T12:00:00Z"
    }
  }
}
```

Error response:

```json
{
  "id": "unique-request-id",
  "error": {
    "code": "PERMISSION_DENIED",
    "message": "User does not have permission to create repositories",
    "details": {
      "http_status": 403
    }
  }
}
```

### Example API Calls

#### Create Repository

```rust
let request = PluginRequest {
    id: "req-1".to_string(),
    method: "repo.create".to_string(),
    params: json!({
        "name": "user-repo",
        "description": "User's personal repository",
        "private": true
    }),
    tenant_id: Some("tenant-123".to_string()),
    user_id: Some("user-456".to_string()),
    token: Some("api-token".to_string()),
};

let response = plugin.execute(request).await?;
```

#### Delete Repository

```rust
let request = PluginRequest {
    id: "req-2".to_string(),
    method: "repo.delete".to_string(),
    params: json!({"name": "user-repo"}),
    tenant_id: Some("tenant-123".to_string()),
    user_id: Some("user-456".to_string()),
    token: Some("api-token".to_string()),
};

let response = plugin.execute(request).await?;
```

#### List Repositories

```rust
let request = PluginRequest {
    id: "req-3".to_string(),
    method: "repo.list".to_string(),
    params: json!({}),
    tenant_id: Some("tenant-123".to_string()),
    user_id: Some("user-456".to_string()),
    token: Some("api-token".to_string()),
};

let response = plugin.execute(request).await?;
```

#### Health Check

```rust
let request = PluginRequest {
    id: "req-4".to_string(),
    method: "health.check".to_string(),
    params: json!({}),
    tenant_id: None,
    user_id: None,
    token: None,
};

let response = plugin.execute(request).await?;
```

#### Get Metrics

```rust
let request = PluginRequest {
    id: "req-5".to_string(),
    method: "metrics.get".to_string(),
    params: json!({}),
    tenant_id: None,
    user_id: None,
    token: None,
};

let response = plugin.execute(request).await?;
```

7. Deployment Considerations
-----------------------------

### Storage Backend Selection

**Local Filesystem**
- Use for single-node deployments
- High performance for local access
- No external dependencies
- Requires shared storage for distributed setup

**S3-Compatible**
- Use for distributed deployments
- High availability and scalability
- Works with AWS S3, MinIO, etc.
- Requires network access to S3

### Multi-Tenancy Setup

1. Define tenant namespaces in configuration
2. Create dedicated S3 prefixes per tenant
3. Configure per-tenant quotas
4. Set up tenant-specific rate limits
5. Implement tenant context propagation

### Rate Limiting Strategy

1. Set per-user limits based on tier:
   - Free: 100 API calls/min, 1GB/hour
   - Pro: 1000 API calls/min, 10GB/hour
   - Enterprise: 10000 API calls/min, 100GB/hour

2. Configure burst allowance:
   - Allow 2x capacity for short durations
   - Reset on minute boundaries

3. Implement circuit breakers:
   - Monitor rate limit violations
   - Alert on abuse patterns
   - Implement exponential backoff

### Monitoring Setup

1. Export metrics to Prometheus:
   - Hook /metrics endpoint
   - Scrape every 15-30 seconds
   - Set retention for 15 days

2. Enable distributed tracing:
   - Export to Jaeger or OpenTelemetry
   - Sample 1-5% of requests in production
   - Track latency for critical operations

3. Set up alerts:
   - Health check failures
   - High error rates (>5%)
   - Rate limit violations
   - Storage capacity warnings

### Security Hardening

1. SSH Configuration:
   - Use Ed25519 keys only
   - Rotate keys regularly
   - Validate authorized_keys format

2. API Security:
   - Require HTTPS in production
   - Implement API rate limiting
   - Add request signing/verification
   - Use short-lived tokens (1 hour)

3. Storage Security:
   - Encrypt data at rest
   - Use IAM roles for S3 access
   - Audit all operations
   - Enable versioning for recovery

8. Troubleshooting
------------------

### Plugin Won't Initialize

Check:
1. Configuration file syntax (YAML valid?)
2. Storage backend connectivity
3. Required directories exist and writable
4. Environment variables set correctly

```bash
# Check config
cargo test --lib skynet::config::

# Test storage backend
cargo test --lib skynet::storage::
```

### High Error Rates

Check:
1. Rate limiting configuration
2. Storage backend availability
3. Tenant quotas not exceeded
4. Network connectivity to S3

```bash
# Check rate limiter
curl http://localhost:8080/health

# Check metrics
curl http://localhost:9090/metrics
```

### Performance Issues

Check:
1. Storage backend latency
2. Network bandwidth
3. CPU/memory utilization
4. Concurrent connections

Options:
1. Increase rate limit burst
2. Optimize storage backend
3. Enable caching layer
4. Distribute load across replicas

### Tenant Isolation Issues

Check:
1. TenantContext properly propagated
2. Namespace paths correctly configured
3. S3 prefix structure correct
4. Permission checks working

9. Performance Tuning
---------------------

### Connection Pooling

```yaml
# Increase connection limits
rate_limiting:
  per_user:
    concurrent_connections: 50

  global:
    max_concurrent_users: 5000
```

### Storage Optimization

**Local Filesystem**:
- Use SSD for optimal performance
- Enable filesystem caching
- Monitor disk I/O saturation

**S3**:
- Enable S3 Transfer Acceleration
- Use multipart uploads for large files
- Implement connection pooling
- Configure retry logic

### Caching Strategy

1. Client-side caching of repository info
2. In-memory cache for hot repositories
3. Redis cache for user/key lookups
4. CDN for frequently accessed objects

### Scaling Strategies

1. **Horizontal Scaling**:
   - Deploy multiple plugin instances
   - Use S3 as shared storage
   - Load balance requests

2. **Vertical Scaling**:
   - Increase memory allocation
   - Use faster CPU
   - Add more disk capacity

3. **Geographic Distribution**:
   - Deploy in multiple regions
   - Use S3 cross-region replication
   - Route based on locality

References
----------

- Skynet v2 Documentation: ../
- Plugin Configuration: ./SKYNET_PLUGIN.md
- API Reference: ../src/skynet/interface.rs
- Examples: ../examples/

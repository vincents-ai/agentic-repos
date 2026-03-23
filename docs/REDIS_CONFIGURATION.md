# Redis Cache Configuration

This document describes the Redis cache configuration for repos-mcp.

## Configuration Options

### Basic Settings

```yaml
cache:
  enabled: true
  url: "redis://127.0.0.1:6379"
  key_prefix: "repos-mcp"
  default_ttl_secs: 3600
  max_connections: 10
  connection_timeout_secs: 5
  response_timeout_secs: 3
  retry_attempts: 3
  retry_delay_ms: 100
```

### Configuration Reference

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | bool | `false` | Enable/disable Redis caching |
| `url` | string | `redis://127.0.0.1:6379` | Redis connection URL |
| `key_prefix` | string | `repos-mcp` | Prefix for all cache keys |
| `default_ttl_secs` | u64 | `3600` | Default TTL for cached items (1 hour) |
| `max_connections` | u32 | `10` | Maximum connection pool size |
| `connection_timeout_secs` | u64 | `5` | Connection timeout in seconds |
| `response_timeout_secs` | u64 | `3` | Response timeout in seconds |
| `retry_attempts` | u32 | `3` | Number of retry attempts on failure |
| `retry_delay_ms` | u64 | `100` | Delay between retries in milliseconds |

### TTL Configuration

```yaml
cache:
  cache_ttls:
    repository_metadata_secs: 300    # 5 minutes
    session_secs: 86400              # 24 hours
    api_response_secs: 60            # 1 minute
    lfs_object_secs: 600             # 10 minutes
    permission_secs: 300             # 5 minutes
    user_info_secs: 3600             # 1 hour
```

| TTL Option | Default | Description |
|------------|---------|-------------|
| `repository_metadata_secs` | 300 | Repository metadata cache TTL |
| `session_secs` | 86400 | User session cache TTL |
| `api_response_secs` | 60 | API response cache TTL |
| `lfs_object_secs` | 600 | LFS object existence cache TTL |
| `permission_secs` | 300 | Permission cache TTL |
| `user_info_secs` | 3600 | User information cache TTL |

### TLS Configuration

```yaml
cache:
  tls:
    enabled: false
    ca_cert_path: "/path/to/ca.crt"
    cert_path: "/path/to/client.crt"
    key_path: "/path/to/client.key"
    skip_verify: false
```

| TLS Option | Type | Default | Description |
|------------|------|---------|-------------|
| `enabled` | bool | `false` | Enable TLS for Redis connection |
| `ca_cert_path` | string | - | Path to CA certificate |
| `cert_path` | string | - | Path to client certificate |
| `key_path` | string | - | Path to client key |
| `skip_verify` | bool | `false` | Skip certificate verification |

## Cache Key Namespacing

All cache keys follow a hierarchical namespace pattern:

```
{prefix}:{category}:{identifier}:{...components}
```

### Key Patterns

| Category | Pattern | Example |
|----------|---------|---------|
| Repository | `repos-mcp:repo:{tenant}:{name}:{type}` | `repos-mcp:repo:tenant1:myrepo:metadata` |
| Session | `repos-mcp:session:{session_id}` | `repos-mcp:session:abc123-def456` |
| User | `repos-mcp:user:{user_id}:{type}` | `repos-mcp:user:123:sessions` |
| API | `repos-mcp:api:{endpoint}:{hash}` | `repos-mcp:api:repos:a1b2c3d4` |
| LFS | `repos-mcp:lfs:{tenant}:{oid}:{type}` | `repos-mcp:lfs:tenant1:sha256:exists` |
| Permission | `repos-mcp:perm:{user}:{resource}` | `repos-mcp:perm:user1:repo:tenant:repo` |
| Branch List | `repos-mcp:branches:{tenant}:{repo}` | `repos-mcp:branches:tenant1:myrepo` |
| Repo List | `repos-mcp:repos:{tenant}:page:{n}` | `repos-mcp:repos:tenant1:page:1` |

## Connection Pool

The cache uses a connection pool (deadpool-redis) with the following characteristics:

- **Max Size**: Configurable via `max_connections` (default: 10)
- **Timeout Handling**: Connection and response timeouts are enforced
- **Retry Logic**: Automatic retries with configurable delay
- **Health Checks**: PING command used for health verification

### Pool Status

```rust
pub struct PoolStatus {
    pub max_size: u32,    // Maximum pool size
    pub size: u32,        // Current pool size
    pub available: u32,   // Available connections
}
```

## Cache Operations

### Basic Operations

```rust
// Set with TTL
conn.set("key", "value", Duration::from_secs(60)).await?;

// Get
let value: Option<String> = conn.get("key").await?;

// Delete
let deleted: bool = conn.delete("key").await?;

// Check existence
let exists: bool = conn.exists("key").await?;

// Get TTL
let ttl: i64 = conn.ttl("key").await?;

// Set TTL
conn.expire("key", Duration::from_secs(120)).await?;
```

### Pattern Operations

```rust
// Delete by pattern
let count: u64 = conn.delete_pattern("repos-mcp:repo:tenant1:*").await?;

// List keys
let keys: Vec<String> = conn.keys("repos-mcp:session:*").await?;
```

### Set Operations (for session tracking)

```rust
// Add to set
conn.sadd("user:sessions", "session-id").await?;

// Check membership
let is_member: bool = conn.sismember("user:sessions", "session-id").await?;

// Remove from set
conn.srem("user:sessions", "session-id").await?;

// Get all members
let members: Vec<String> = conn.smembers("user:sessions").await?;
```

### Hash Operations

```rust
// Set hash field
conn.hset("repo:info", "size", "1024").await?;

// Get hash field
let value: Option<String> = conn.hget("repo:info", "size").await?;

// Get all fields
let all: HashMap<String, String> = conn.hgetall("repo:info").await?;
```

## Cache Invalidation

The `CacheInvalidator` provides pattern-based invalidation:

```rust
let invalidator = CacheInvalidator::new(pool);

// Invalidate by pattern
invalidator.invalidate(InvalidationPattern::Tenant("tenant-1")).await?;
invalidator.invalidate(InvalidationPattern::Repository {
    tenant_id: "tenant-1".to_string(),
    repo_name: "myrepo".to_string(),
}).await?;

// Batch invalidation
let patterns = InvalidationBuilder::new()
    .tenant("tenant-1")
    .api_endpoint("repos")
    .build();
invalidator.invalidate_batch(patterns).await?;

// Event-based invalidation
invalidator.on_repository_deleted("tenant-1", "myrepo").await?;
invalidator.on_branch_changed("tenant-1", "myrepo").await?;
```

## Graceful Degradation

When Redis is unavailable, the system gracefully degrades:

1. **Cache Misses**: All cache operations return misses
2. **Direct Computation**: Values are computed directly without caching
3. **No Errors**: Cache failures don't propagate to the application layer
4. **Automatic Reconnection**: Pool attempts to reconnect automatically

```rust
// CacheLayer handles graceful degradation internally
pub async fn health_check(&self) -> CacheResult<bool> {
    match &self.pool {
        Some(pool) => pool.health_check().await,
        None => Ok(false),  // Returns false, not error
    }
}
```

## Redis Server Requirements

### Minimum Version

- Redis 6.0 or later recommended
- Redis 5.0 supported with reduced functionality

### Recommended Configuration

```redis
# /etc/redis/redis.conf

# Memory
maxmemory 256mb
maxmemory-policy allkeys-lru

# Persistence (optional)
appendonly yes
appendfsync everysec

# Network
bind 0.0.0.0
port 6379
timeout 0
tcp-keepalive 300

# Performance
tcp-backlog 511
```

### Docker Compose Example

```yaml
version: '3.8'
services:
  redis:
    image: redis:7-alpine
    command: redis-server --maxmemory 256mb --maxmemory-policy allkeys-lru
    ports:
      - "6379:6379"
    volumes:
      - redis-data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  redis-data:
```

## Monitoring

### Key Metrics

Monitor these Redis metrics:

- `keyspace_hits` / `keyspace_misses`: Cache hit ratio
- `used_memory`: Memory usage
- `connected_clients`: Active connections
- `total_commands_processed`: Command throughput

### Health Check Endpoint

The cache provides a health check:

```rust
// Returns true if Redis is healthy
let healthy: bool = cache_layer.health_check().await?;
```

### Statistics

```rust
let status = pool.status();
println!("Pool: {}/{} connections available", 
    status.available, status.max_size);
```

## Troubleshooting

### Common Issues

1. **Connection Refused**
   - Check Redis is running: `redis-cli ping`
   - Verify connection URL
   - Check firewall rules

2. **High Memory Usage**
   - Reduce TTLs
   - Enable LRU eviction
   - Check for memory leaks

3. **Slow Operations**
   - Avoid KEYS in production (use SCAN)
   - Check network latency
   - Monitor slow log

4. **Pool Exhaustion**
   - Increase `max_connections`
   - Check for connection leaks
   - Review timeout settings

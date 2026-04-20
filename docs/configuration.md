# Configuration Reference

Complete reference for all agentic-repos configuration options.

The primary configuration file is `config.yaml` (YAML format). A fully annotated example is at [`config-example.yaml`](../config-example.yaml).

## Configuration Sources

Priority (highest to lowest):

1. **Environment variables** — override specific settings (e.g., `BACKUP_ENCRYPTION_KEY`)
2. **Config file** — `config.yaml` or path specified via `--config` flag
3. **Defaults** — hardcoded sensible defaults

## Server Configuration

### `server.ssh`

SSH server settings for Git operations.

```yaml
server:
  ssh:
    port: 2222                    # SSH listen port
    host_key_path: "./ssh_host_key"  # Ed25519 host key (generated if missing)
    max_connections: 100          # Maximum concurrent SSH sessions
    connection_timeout_secs: 30   # Idle connection timeout
```

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `port` | integer | 2222 | SSH server listen port |
| `host_key_path` | string | `./ssh_host_key` | Path to SSH host key file. If missing, a key is generated on startup. **Set this to a persistent path in production.** |
| `max_connections` | integer | 100 | Maximum concurrent SSH connections |
| `connection_timeout_secs` | integer | 30 | Seconds before idle connections are closed |

### `server.http`

HTTP server settings for the REST API and web UI.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `port` | integer | 8090 | HTTP listen port |

## Authentication

### `auth`

```yaml
auth:
  authorized_keys_path: "./authorized_keys"
```

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `authorized_keys_path` | string | `./authorized_keys` | Path to SSH authorized_keys file. Format: `username:ssh-key-type key comment` |

## RBAC

### `rbac`

```yaml
rbac:
  enabled: true
  storage_type: "memory"
  admin_user:
    username: "admin"
    email: "admin@localhost"
    ssh_key: ""
```

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `enabled` | boolean | false | Enable RBAC authorization. Requires `features.rbac: true` |
| `storage_type` | string | `"memory"` | Storage backend: `"memory"` (restart loses data) or `"database"` (PostgreSQL) |
| `admin_user.username` | string | — | Initial admin username (created on first startup) |
| `admin_user.email` | string | — | Admin email address |
| `admin_user.ssh_key` | string | — | Admin SSH public key |

## Storage

### `storage`

```yaml
storage:
  type: "local"
  local:
    path: "./repositories"
  # s3:
  #   bucket: "my-git-repositories"
  #   region: "us-east-1"
  #   endpoint: "https://s3.amazonaws.com"
```

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `type` | string | `"local"` | Storage backend: `"local"` (filesystem) or `"s3"` (S3-compatible) |
| `local.path` | string | `./repositories` | Base directory for repository storage |
| `s3.bucket` | string | — | S3 bucket name |
| `s3.region` | string | — | S3 region |
| `s3.endpoint` | string | — | Custom S3-compatible endpoint (for Garage, MinIO, etc.) |

## Rate Limiting

### `rate_limit`

```yaml
rate_limit:
  enabled: true
  proxy_headers:
    - "X-Forwarded-For"
    - "X-Real-IP"
    - "CF-Connecting-IP"
  global_limits:
    max_requests_per_second: 100
    max_connections_per_ip: 10
    burst_size: 20
  tiers:
    free:
      requests_per_minute: 60
      requests_per_hour: 1000
      burst_size: 10
      concurrent_connections: 5
    pro:
      requests_per_minute: 300
      requests_per_hour: 10000
      burst_size: 50
      concurrent_connections: 20
    enterprise:
      requests_per_minute: 1000
      requests_per_hour: 100000
      burst_size: 200
      concurrent_connections: 100
```

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `enabled` | boolean | true | Enable rate limiting middleware |
| `proxy_headers` | string[] | `[X-Forwarded-For, X-Real-IP, CF-Connecting-IP]` | Headers to check for client IP (when behind reverse proxy) |
| `global_limits.max_requests_per_second` | integer | 100 | Global requests per second across all clients |
| `global_limits.max_connections_per_ip` | integer | 10 | Maximum concurrent connections per IP |
| `global_limits.burst_size` | integer | 20 | Token bucket burst size |
| `tiers.<name>.requests_per_minute` | integer | varies | Per-minute request limit for this tier |
| `tiers.<name>.requests_per_hour` | integer | varies | Per-hour request limit for this tier |
| `tiers.<name>.burst_size` | integer | varies | Token bucket burst size for this tier |
| `tiers.<name>.concurrent_connections` | integer | varies | Max concurrent connections for this tier |

## Logging

### `logging`

```yaml
logging:
  level: "info"
  json: false
  timestamp: true
  ansi: true
```

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `level` | string | `"info"` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `json` | boolean | false | Output structured JSON logs |
| `timestamp` | boolean | true | Include timestamps in log output |
| `ansi` | boolean | true | Enable ANSI colors in terminal output |

Can also be controlled via `RUST_LOG` environment variable (takes precedence).

## Git LFS

### `lfs`

```yaml
lfs:
  enabled: true
  max_object_size: 5368709120
  token_expiration_secs: 3600
  jwt_secret: "CHANGE_THIS_SECRET"
  base_url: "http://localhost:8090"
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  locking_enabled: false
  lock_expiration_secs: 86400
```

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `enabled` | boolean | false | Enable Git LFS support. Requires `features.git_lfs: true` |
| `max_object_size` | integer | 5368709120 (5GB) | Maximum LFS object size in bytes |
| `token_expiration_secs` | integer | 3600 (1 hour) | LFS JWT token lifetime |
| `jwt_secret` | string | **required** | Secret for signing LFS JWT tokens. **No default — must be set in production.** |
| `base_url` | string | `http://localhost:8090` | Base URL for LFS HTTP endpoints (must be reachable by clients) |
| `storage_path_pattern` | string | (see example) | Path template for LFS object storage |
| `locking_enabled` | boolean | false | Enable LFS file locking. Requires `features.lfs_locks: true` |
| `lock_expiration_secs` | integer | 86400 (24 hours) | Lock expiration time |

## Feature Flags

### `features`

Feature flags control which subsystems are active. All default to `false`.

```yaml
features:
  # Core
  git_lfs: false             # Git LFS support
  lfs_locks: false           # LFS file locking (requires git_lfs)
  rbac: false                # Role-based access control
  web_ui: false              # Embedded web UI

  # Authentication
  ldap_auth: false           # LDAP/Active Directory
  saml_auth: false           # SAML SSO
  oauth_auth: false          # OAuth2/OIDC

  # Audit & Governance
  audit_logging: false       # Comprehensive audit logging
  audit_tracking: false      # Tamper-proof access tracking
  repo_templates: false      # Repository templates with policies
  webhooks: false            # Outbound webhooks

  # Performance & Scale
  multi_region: false        # Multi-region replication
  horizontal_scaling: false  # Horizontal scaling support
  redis_cache: false         # Redis caching layer
  load_balancing: false      # Intelligent load balancing
  monitoring: false          # Prometheus/Grafana monitoring

  # Security & Compliance
  security_scanning: false   # Repository vulnerability scanning
  soc2_compliance: false     # SOC2 compliance reports
  gdpr_compliance: false     # GDPR data protection
  hipaa_compliance: false    # HIPAA healthcare compliance
  backup_recovery: false     # Automated backup & disaster recovery

  # CI/CD Integration
  github_actions: false      # GitHub Actions integration
  gitlab_ci: false           # GitLab CI integration
  jenkins: false             # Jenkins pipeline integration

  # Advanced
  graphql_api: false         # GraphQL API
  advanced_webhooks: false   # Advanced webhook filtering
  analytics: false           # Repository analytics dashboard
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `RUST_LOG` | No | Log level override. Values: `trace`, `debug`, `info`, `warn`, `error`. Can target modules: `agentic_repos::api=debug,info` |
| `BACKUP_ENCRYPTION_KEY` | No* | 32-byte hex key for AES-256-GCM backup encryption. Required if using encrypted backups. |
| `SESSION_ENCRYPTION_KEY` | No* | 32-byte hex key for AES-256-GCM session data encryption. Required if using Redis sessions. |
| `LFS_JWT_SECRET` | No | Override for `lfs.jwt_secret` config value. Takes precedence. |

\* Required when the corresponding feature is enabled.

## Redis Configuration

When `features.redis_cache: true`, Redis is used for:
- Session storage
- Permission caching (with TTL)
- Distributed locks (horizontal scaling)
- Rate limiting state

Connection is configured via `REDIS_URL` environment variable or defaults to `redis://127.0.0.1:6379`.

See [`docs/REDIS_CONFIGURATION.md`](REDIS_CONFIGURATION.md) for detailed Redis setup.

## Skylet Plugin Configuration

When running as a Skylet v2 plugin (`libagentic_repos.so`), configuration is passed through the Skylet runtime:

```yaml
plugins:
  - name: git-operations
    path: ./target/release/libagentic_repos.so
    config:
      storage.default_backend: "local"
      log.level: "info"
```

### Load Order

When using the garage-s3 storage plugin, load order matters:

1. `garage-s3` (priority 10) — starts embedded Garage, registers S3 service
2. `agentic-repos` (priority 5) — discovers garage-s3 via `global_registry()`

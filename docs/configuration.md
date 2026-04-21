# Configuration Reference

Configuration for agentic-repos is stored in **git refs** in the system repository (`_adp/system`). This is the canonical config store for audit compliance — all changes are tracked in git history.

## Configuration Sources

Priority (highest to lowest):

1. **Git refs** — `refs/adp/config/*` in `_adp/system` — **canonical, always used at runtime**
2. **YAML bootstrap seed** — `config.yaml` passed to `agentic-repos init` — **one-time seed only, never read at runtime**
3. **Defaults** — hardcoded sensible defaults

> **Compliance**: The server reads from git refs exclusively at runtime.
> YAML files are only accepted during `agentic-repos init` to seed initial values.

## Managing Configuration

### During initialization

```bash
# Seed config from YAML during init
agentic-repos init --config config.yaml

# Or use defaults (stored in git refs)
agentic-repos init
```

See [`config-example.yaml`](../config-example.yaml) for the full annotated bootstrap reference.

### After initialization

```bash
# View current config
agentic-repos config get server.ssh.port

# Update a config value (writes to git refs)
agentic-repos config set server.ssh.port 2223

# List all config
agentic-repos config list
```

All changes are written to git refs and appear in the system repo's history.

## Configuration Sections

### Server

#### SSH Server (`refs/adp/config/server`)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `ssh_port` | integer | 2222 | SSH server listen port |
| `ssh_host_key_path` | string | `./ssh_host_key` | Path to SSH host key file. If missing, generated on startup. **Set to persistent path in production.** |
| `ssh_max_connections` | integer | 100 | Maximum concurrent SSH connections |
| `ssh_connection_timeout_secs` | integer | 30 | Idle connection timeout |
| `hostname` | string | `localhost` | Public-facing hostname used in SSH clone URLs |
| `http_base_url` | string | `http://localhost:8080` | Public-facing HTTP base URL used in HTTP clone URLs |

#### HTTP Server (`refs/adp/config/server`)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `http_port` | integer | 8090 | HTTP listen port |
| `http_host` | string | `127.0.0.1` | HTTP bind address |

### Storage (`refs/adp/config/server`)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `storage_backend` | string | `"local"` | Storage backend: `"local"` (filesystem) or `"s3"` |
| `storage_path` | string | `./repositories` | Base directory for local storage |

### SSH Key Management

SSH keys are stored in git refs, NOT in a flat file:

- User keys: `refs/adp/users/{username}/ssh_keys` (JSON array of public key strings)
- Key-to-user mapping: `refs/adp/keys/{fingerprint}` → `{"username": "..."}`
- User identity: `refs/adp/users/{username}/identity`

Register keys during init:
```bash
agentic-repos init --admin-key "ssh-ed25519 AAAA... admin@host" --admin-username admin
```

Or via API:
```bash
curl -X POST http://localhost:8090/api/v1/rbac/users \
  -d '{"username": "alice", "ssh_key": "ssh-ed25519 AAAA... alice@host"}'
```

### Feature Flags (`refs/adp/config/features`)

Feature flags control which subsystems are active. Set during init via `--config` or via `agentic-repos config set`.

All feature flags default to `false`. Key flags:

```yaml
features:
  # Core
  git_lfs: false             # Git LFS support
  lfs_locks: false           # LFS file locking (requires git_lfs)
  rbac: false                # Role-based access control
  web_ui: false              # Embedded web UI (adp-web-ui binary)

  # Performance & Scale
  redis_cache: false         # Redis caching layer
  multi_region: false        # Multi-region replication
  horizontal_scaling: false  # Horizontal scaling

  # Security & Compliance
  security_scanning: false   # Repository vulnerability scanning
  gdpr_compliance: false     # GDPR data protection
  hipaa_compliance: false    # HIPAA healthcare compliance
  backup_recovery: false     # Automated backup & recovery

  # CI/CD
  github_actions: false      # GitHub Actions integration
  gitlab_ci: false           # GitLab CI integration
  jenkins: false             # Jenkins integration

  # Monitoring
  monitoring: false          # Prometheus/Grafana monitoring
  webhooks: false            # Outbound webhooks
  analytics: false           # Repository analytics
```

### LFS Configuration (`refs/adp/config/lfs`)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `enabled` | boolean | false | Enable Git LFS. Requires `features.git_lfs` |
| `max_object_size` | integer | 5368709120 (5GB) | Maximum LFS object size in bytes |
| `token_expiration_secs` | integer | 3600 | LFS JWT token lifetime |
| `jwt_secret` | string | **required** | Secret for LFS JWT tokens. **No default — must be set.** |
| `base_url` | string | `http://localhost:8090` | Base URL for LFS HTTP endpoints |
| `locking_enabled` | boolean | false | Enable LFS file locking |

### Rate Limiting (`refs/adp/config/rate_limit`)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `enabled` | boolean | true | Enable rate limiting |
| `proxy_headers` | string[] | X-Forwarded-For, X-Real-IP, CF-Connecting-IP | Headers for client IP |
| `global_limits.max_requests_per_second` | integer | 100 | Global requests per second |
| `global_limits.max_connections_per_ip` | integer | 10 | Max concurrent connections per IP |

### Logging

Controlled via `RUST_LOG` environment variable, not git refs:

```bash
RUST_LOG=info                # Default
RUST_LOG=debug               # Verbose
RUST_LOG=agentic_repos::api=trace  # Per-module
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `RUST_LOG` | Log level (trace/debug/info/warn/error). Per-module targeting supported. |
| `AGENTIC_REPOS_STORAGE_ROOT` | Storage root path (if not default `./repositories`) |
| `BACKUP_ENCRYPTION_KEY` | 32-byte hex key for AES-256-GCM backup encryption |
| `SESSION_ENCRYPTION_KEY` | 32-byte hex key for AES-256-GCM session encryption |

## Redis Configuration

When `features.redis_cache: true`, Redis is used for sessions, permission caching, distributed locks, and rate limiting. Connection via `REDIS_URL` env var (default: `redis://127.0.0.1:6379`).

See [`docs/REDIS_CONFIGURATION.md`](REDIS_CONFIGURATION.md) for detailed setup.

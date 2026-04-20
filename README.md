# agentic-repos

Multi-tenant Git server with SSH access, HTTP API, Git LFS, RBAC, webhooks, and horizontal scaling. Part of the **ADP (Agentic Development Platform)** — a vertically integrated platform for hosting and managing Git repositories, designed for both human and AI agent workflows.

**License**: AGPL-3.0-or-later OR LicenseRef-Commercial (see [Licensing](#licensing))

---

## Features

- **Git hosting** — create, clone, push, pull over SSH (port 2222) and HTTP
- **Git LFS** — full LFS server with quotas, bandwidth throttling, file locking, and storage backends (local + S3)
- **SSH authentication** — public key auth with authorized keys file, configurable host keys
- **HTTP REST API** — 150+ endpoints for repositories, users, RBAC, backups, webhooks, compliance, CI/CD, and more
- **RBAC** — role-based access control with granular permissions per resource type
- **Web UI** — embedded Svelte 5 frontend, compiled into the binary at build time (no external static files)
- **Webhooks** — outbound delivery with HMAC-SHA256 signature verification (GitHub, GitLab, Jenkins)
- **Backup & restore** — encrypted backups (AES-256-GCM), concurrent backup limiter, tenant-scoped restore
- **Security scanning** — repository vulnerability scanning with audit logging
- **Compliance** — GDPR, HIPAA, DORA, NIS2 frameworks with retention policies and data export
- **Horizontal scaling** — cluster management, leader election with fencing tokens, distributed locking
- **Multi-region replication** — conflict resolution (last-write-wins), audit logging, RBAC-gated
- **CI/CD integration** — webhook receivers for GitHub Actions, GitLab CI, Jenkins; build tracking
- **Monitoring** — Prometheus metrics, Grafana dashboards, health checks
- **Secrets infrastructure** — AGE-encrypted secrets with two-layer key wrapping (ADR-024)

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                agentic-repos binary                      │
│                                                         │
│  ┌──────────────────┐    ┌────────────────────────────┐  │
│  │  Rust backend    │    │  adp-web-ui (rust-embed)   │  │
│  │  actix-web :8090 │◄───│  Svelte 5 + Tailwind v4    │  │
│  │  russh     :2222 │    │  shadcn-svelte + Vite       │  │
│  │  Git / LFS       │    └────────────────────────────┘  │
│  └────────┬─────────┘                                    │
│           │ REST API (:8090/api/...)                      │
└───────────┼──────────────────────────────────────────────┘
            │
     ┌──────┴──────┐
     │  Optional   │
     ├─────────────┤
     │ Redis       │  caching, sessions, distributed locks
     │ S3 / Garage │  LFS object storage, backup storage
     └─────────────┘
```

The server compiles to a single self-contained binary — no external static files, no runtime dependency on Node.js.

### Deployment Modes

1. **Standalone binary** (primary) — `agentic-repos` runs as a standalone server
2. **Skylet v2 plugin** — compiles as `libagentic_repos.so`, loaded by the Skylet orchestration runtime via FFI

## Quick Start

### Build

```bash
# Nix (reproducible — preferred)
nix build
# Binary: result/bin/agentic-repos

# Cargo (fallback)
nix develop --command cargo build --release
# Binary: target/release/agentic-repos
```

### Initialize

```bash
# Bootstrap the system repository and AGE root key
./result/bin/agentic-repos init

# With custom storage path
./result/bin/agentic-repos init --storage /data/git

# Register an admin SSH key during init
./result/bin/agentic-repos init --admin-key "ssh-ed25519 AAAA... user@host" --admin-username admin
```

### Run

```bash
# Start the server (default: HTTP :8090, SSH :2222)
./result/bin/agentic-repos

# Or with a config file
./result/bin/agentic-repos --config config.yaml
```

### First Repository

```bash
# Add your SSH key to authorized_keys
echo "admin:ssh-ed25519 AAAA... user@host" >> authorized_keys

# Clone over SSH
git clone ssh://admin@localhost:2222/repos/my-project.git

# Or create via API
curl -X POST http://localhost:8090/api/v1/repositories/admin \
  -H "Content-Type: application/json" \
  -d '{"name": "my-project"}'

# Push some code
cd my-project
echo "# Hello" > README.md
git add . && git commit -m "initial" && git push
```

### Git LFS

```bash
# Enable LFS in config
# features: { git_lfs: true, lfs_locks: true }
# lfs: { enabled: true, jwt_secret: "<strong-random-secret>" }

# Client-side setup
git lfs install
git lfs track "*.psd"
git add .gitattributes && git commit -m "track LFS" && git push
```

## Configuration

Copy `config-example.yaml` to `config.yaml` and modify. Key sections:

```yaml
server:
  ssh:
    port: 2222                # SSH server port
  http:
    port: 8090                # HTTP API port

storage:
  type: "local"              # "local" or "s3"
  local:
    path: "./repositories"

rbac:
  enabled: true              # Enable role-based access control

lfs:
  enabled: true
  jwt_secret: "<random>"     # Required for LFS — no default
  max_object_size: 5368709120  # 5GB

features:
  git_lfs: true
  web_ui: true
  rbac: true
  redis_cache: false         # Enable for horizontal scaling
  webhooks: true
  security_scanning: true
  backup_recovery: true
```

Full reference: [`config-example.yaml`](config-example.yaml) | [`docs/configuration.md`](docs/configuration.md)

## API

The HTTP API serves the web UI and external integrations on port 8090.

| Scope | Prefix | Description |
|-------|--------|-------------|
| Health | `/health`, `/info` | Health checks, version info |
| Repositories | `/api/v1/repositories` | CRUD, branches, commits, tree |
| Auth | `/api/v1/auth` | Login, callback, refresh, logout |
| RBAC | `/api/v1/rbac` | Users, roles, permissions |
| LFS | `/api/v1/lfs/*` | Batch, upload, download, verify |
| LFS Locks | `/api/v1/lfs-locks` | File locking |
| Webhooks | `/api/v1/webhooks` | CRUD, test, delivery, retry |
| Backups | `/api/v1/backups` | Create, restore, schedule, encrypt |
| Cluster | `/api/v1/cluster` | Status, instances, leader, sessions, locks |
| ADP | `/api/v1/adp` | Schema access, sidecar refs |
| Secrets | `/api/v1/secrets` | Encrypted secrets management |
| Security | `/api/v1/security` | Vulnerability scanning, rules |
| Compliance | `/api/v1/compliance` | GDPR, DORA, NIS2, retention |
| CI/CD | `/api/v1/cicd` | Webhook receivers, builds |
| SDLC | `/api/v1/sdlc` | SDLC monitoring |
| Analytics | `/api/v1/analytics` | Repository analytics |
| Replication | `/api/v1/replication` | Multi-region replication |
| Metrics | `/metrics` | Prometheus exposition format |

Full endpoint reference: [`docs/API_REFERENCE.md`](docs/API_REFERENCE.md)

## Testing

```bash
# Unit tests (934 tests)
nix develop --command cargo test

# Integration tests
nix develop --command cargo test --test git_protocol_integration
nix develop --command cargo test --test lfs_integration_test
nix develop --command cargo test --test lfs_locks_integration

# Skylet plugin BDD validation
plugin-test-harness bdd --feature-path ./features --plugin-path ./target/release/libagentic_repos.so
```

Full testing guide: [`TESTING.md`](TESTING.md)

## Deployment

### Docker

```bash
docker compose up
# Or Nix-based image
docker compose -f compose-nix.yml up
```

### Kubernetes

```bash
kubectl apply -f k8s/
```

### systemd

```bash
# After building
cp result/bin/agentic-repos /usr/local/bin/
cp config-example.yaml /etc/agentic-repos/config.yaml

# Create systemd unit
cat > /etc/systemd/system/agentic-repos.service << 'EOF'
[Unit]
Description=agentic-repos Git Server
After=network.target

[Service]
ExecStart=/usr/local/bin/agentic-repos --config /etc/agentic-repos/config.yaml
Restart=always
User=agentic-repos

[Install]
WantedBy=multi-user.target
EOF

systemctl enable --now agentic-repos
```

Full deployment guide: [`DEPLOYMENT.md`](DEPLOYMENT.md)

## Documentation

### For Users
- [Getting Started](docs/getting-started.md) — create repos, push code, LFS setup
- [SSH Setup](docs/SSH_SETUP.md) — SSH key configuration
- [LFS Client Setup](docs/LFS_CLIENT_SETUP.md) — client-side LFS configuration
- [LFS Migration](docs/LFS_MIGRATION.md) — migrating existing repos to LFS

### For Administrators
- [Deployment Guide](DEPLOYMENT.md) — production deployment instructions
- [Configuration Reference](docs/configuration.md) — all config options
- [Admin Guide](docs/admin-guide.md) — RBAC, users, secrets, backup, compliance
- [LFS Admin Guide](docs/LFS_ADMIN_GUIDE.md) — LFS server management
- [Security Model](SECURITY.md) — authentication, authorization, encryption
- [Redis Configuration](docs/REDIS_CONFIGURATION.md) — caching and session storage

### For Developers
- [Developer Guide](docs/developer-guide.md) — build, code structure, contributing
- [API Reference](docs/API_REFERENCE.md) — full REST API documentation
- [Testing Guide](TESTING.md) — testing strategies and coverage
- [LFS Testing](docs/LFS_TESTING.md) — LFS-specific test suite

### Reference
- [Quick Reference Card](QUICK_REFERENCE.md) — common commands and endpoints
- [Config Example](config-example.yaml) — fully documented configuration file
- [Monitoring Setup](monitoring/README.md) — Prometheus + Grafana
- [Kubernetes](k8s/README.md) — K8s manifests

### LFS Deep Dives
- [LFS API](docs/LFS_API.md) — protocol implementation
- [LFS Storage Backends](docs/LFS_STORAGE.md) — local and S3 configuration
- [LFS Quota Management](docs/LFS_QUOTA_MANAGEMENT.md) — storage quotas
- [LFS Bandwidth Throttling](docs/LFS_BANDWIDTH_THROTTLING.md) — transfer limits
- [LFS File Locking](docs/LFS_FILE_LOCKING.md) — concurrent editing protection
- [LFS Security](docs/LFS_SECURITY.md) — authentication and authorization
- [LFS Monitoring](docs/LFS_MONITORING.md) — metrics and alerting
- [LFS Troubleshooting](docs/LFS_TROUBLESHOOTING.md) — common issues
- [LFS Audit Logging](docs/LFS_AUDIT_LOGGING.md) — audit trail
- [LFS Production Deployment](docs/LFS_PRODUCTION_DEPLOYMENT.md) — production checklist

## Licensing

Dual-licensed under:

- **AGPL-3.0-or-later** — for open source use
- **LicenseRef-Commercial** — for commercial/proprietary use (contact for licensing)

Dependencies with incompatible licences (GPL-only, SSPL, Commons Clause) must not be added. Run `cargo license` to audit.

## Related Projects

| Project | Repository | Role |
|---------|-----------|------|
| **engram** | [vincents-ai/engram](https://github.com/vincents-ai/engram) | Distributed AI memory layer — persistent tasks, reasoning, knowledge |
| **adp-web-ui** | [vincents-ai/adp-web-ui](https://github.com/vincents-ai/adp-web-ui) | Svelte 5 frontend — compiled into this binary |

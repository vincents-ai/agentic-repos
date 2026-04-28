# agentic-repos Deployment Guide

Complete deployment instructions for the agentic-repos Git server — a single-binary multi-tenant Git hosting platform with SSH access, HTTP API, Git LFS, RBAC, and embedded web UI.

## Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Quick Start](#quick-start)
4. [Building](#building)
5. [Initialization](#initialization)
6. [Configuration](#configuration)
7. [Running](#running)
8. [Production Deployment](#production-deployment)
9. [Docker Deployment](#docker-deployment)
10. [Kubernetes Deployment](#kubernetes-deployment)
11. [Post-Deployment Setup](#post-deployment-setup)
12. [Upgrading](#upgrading)
13. [Troubleshooting](#troubleshooting)

## Overview

agentic-repos is a self-contained binary with no runtime dependency on Node.js or external static files. The web UI is compiled into the binary via `rust-embed`. It supports two deployment modes:

| Mode | Output | Use Case |
|------|--------|----------|
| **Standalone binary** | `agentic-repos` | Self-hosted Git server |
| **Skylet plugin** | `libagentic_repos.so` | Embedded in Skylet orchestration runtime |

### Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| 8090 | HTTP | REST API + web UI |
| 2222 | SSH | Git operations (clone, push, pull) |

### External Dependencies (Optional)

| Service | Purpose | When needed |
|---------|---------|-------------|
| Redis | Caching, sessions, distributed locks | Horizontal scaling, multi-instance |
| S3 / Garage | LFS object storage, backup storage | Large file storage, S3 backends |
| PostgreSQL | Persistent RBAC data | Production RBAC with `storage_type: "database"` |

## Prerequisites

- **OS**: Linux (x86_64 or aarch64)
- **Memory**: Minimum 512MB RAM (2GB+ recommended for production)
- **Disk**: 1GB for binary + storage space for repositories
- **Git**: Required on client machines (not on server)
- **Nix** (recommended) or Rust toolchain for building

## Quick Start

```bash
# Build
nix build

# Initialize system repository and AGE root key
./result/bin/agentic-repos init

# Start server
./result/bin/agentic-repos

# Create your first repository via API
curl -X POST http://localhost:8090/api/v1/repositories/admin \
  -H "Content-Type: application/json" \
  -d '{"name": "my-project"}'

# Clone over SSH
# Register admin key (stored in git refs)
./result/bin/agentic-repos init --admin-key "ssh-ed25519 AAAA... user@host" --admin-username admin
git clone ssh://admin@localhost:2222/repos/my-project.git
```

## Building

### Nix (recommended — reproducible)

```bash
# Build the binary
nix build
# Output: result/bin/agentic-repos

# Build the Skylet plugin (.so)
nix build
# The crate-type "cdylib" produces libagentic_repos.so

# Enter development shell
nix develop
```

### Cargo (fallback)

```bash
# Requires Rust toolchain + OpenSSL dev headers
cargo build --release

# Binary:  target/release/agentic-repos
# Plugin:  target/release/libagentic_repos.so  (Linux)
#          target/release/libagentic_repos.dylib (macOS)
```

### Cross-compilation

```bash
# aarch64 via Nix
nix build --system aarch64-linux
```

## Initialization

Before first run, initialize the system repository and secrets infrastructure:

```bash
# Basic initialization (creates system repo at ./repositories)
agentic-repos init

# Custom storage path
agentic-repos init --storage /data/git

# Register admin SSH key during init
agentic-repos init \
  --storage /data/git \
  --admin-key "ssh-ed25519 AAAA... admin@host" \
  --admin-username admin

# Custom AGE root key path (for secrets infrastructure)
agentic-repos init \
  --storage /data/git \
  --root-key-path /etc/adp/wrapping-key.age
```

### What `init` does

1. **Creates the system repository** (`_adp/system`) — stores platform config, schemas, and operational data
2. **Generates AGE root key** — used for encrypting per-repo secrets (ADR-024)
3. **Bootstraps engram entity schemas** — task, session, ADR, knowledge, reasoning refs
4. **Bootstraps operational schemas** — webhook-config, deploy-target, CI pipeline, team member
5. **Registers admin key** (if provided) — creates admin user with full RBAC permissions

### Idempotency

`init` is safe to run multiple times — existing data is preserved.

## Configuration

Copy the example config and modify:

```bash
# Option 1: Seed from YAML bootstrap file
./result/bin/agentic-repos init --config config.yaml

# Option 2: Use defaults (config stored in git refs)
./result/bin/agentic-repos init

See [`config-example.yaml`](config-example.yaml) for the full annotated bootstrap reference, or [Configuration Reference](docs/configuration.md) for detailed documentation.

### Required Settings

```yaml
lfs:
  jwt_secret: "<strong-random-string>"  # NO default — must set for LFS
```

### Minimal Production Bootstrap

```yaml
# config.yaml — ONE-TIME SEED for init, NOT read at runtime
server:
  ssh:
    port: 2222
    host_key_path: "/data/ssh/ssh_host_ed25519_key"
    max_connections: 100
  http:
    port: 8090

auth:
# No authorized_keys_path — SSH keys stored in git refs

storage:
  type: "local"
  local:
    path: "/data/repositories"

rbac:
  enabled: true
  storage_type: "database"

lfs:
  enabled: true
  jwt_secret: "${LFS_JWT_SECRET}"
  max_object_size: 5368709120  # 5GB
  base_url: "https://git.example.com"

features:
  git_lfs: true
  web_ui: true
  rbac: true
  webhooks: true
  security_scanning: true
  backup_recovery: true
```

## Running

### Foreground

```bash
agentic-repos                    # Uses default config
agentic-repos --config path.yaml # Custom config path
```

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `RUST_LOG` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `LFS_JWT_SECRET` | Override LFS JWT secret (takes precedence over config) |
| `BACKUP_ENCRYPTION_KEY` | AES-256 key for encrypted backups |
| `SESSION_ENCRYPTION_KEY` | AES-256 key for session data encryption |

## Production Deployment

### systemd

```bash
# Install binary
cp result/bin/agentic-repos /usr/local/bin/

# Create config
mkdir -p /etc/agentic-repos
# Create bootstrap config (optional)
cp config-example.yaml /etc/agentic-repos/config.yaml

# Create data directories
mkdir -p /data/git /data/ssh /data/backups
useradd -r -s /bin/false agentic-repos
chown -R agentic-repos:agentic-repos /data/git /data/ssh /data/backups

# Generate SSH host key
ssh-keygen -t ed25519 -f /data/ssh/ssh_host_ed25519_key -N ""

# Create systemd unit
cat > /etc/systemd/system/agentic-repos.service << 'EOF'
[Unit]
Description=agentic-repos Git Server
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=agentic-repos
Group=agentic-repos
ExecStart=/usr/local/bin/agentic-repos
# Config read from git refs. Use AGENTIC_REPOS_STORAGE_ROOT env var for custom path.
Restart=always
RestartSec=5
WorkingDirectory=/data/git

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/data/git /data/ssh /data/backups
PrivateTmp=true

# Resource limits
LimitNOFILE=65536
Environment=RUST_LOG=info
EnvironmentFile=/etc/agentic-repos/env

[Install]
WantedBy=multi-user.target
EOF

# Enable and start
systemctl daemon-reload
systemctl enable --now agentic-repos
systemctl status agentic-repos
```

### Nginx Reverse Proxy

```nginx
server {
    listen 443 ssl http2;
    server_name git.example.com;

    ssl_certificate /etc/ssl/certs/git.example.com.pem;
    ssl_certificate_key /etc/ssl/private/git.example.com.key;

    # HTTP API + web UI
    location / {
        proxy_pass http://127.0.0.1:8090;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # LFS upload support (up to 5GB)
        client_max_body_size 5g;
    }

    # Metrics (restrict access)
    location /metrics {
        proxy_pass http://127.0.0.1:8090;
        allow 10.0.0.0/8;
        deny all;
    }
}

# SSH remains on port 2222 (not proxied)
```

### Checklist

- [ ] Generate persistent SSH host key (`ssh-keygen -t ed25519`)
- [ ] Set strong `lfs.jwt_secret` (32+ random bytes)
- [ ] Enable RBAC and create admin user
- [ ] Configure `storage.local.path` to persistent volume
- [ ] Set up TLS termination (Nginx, Caddy, or cloud LB)
- [ ] Configure Redis for horizontal scaling (optional)
- [ ] Set `BACKUP_ENCRYPTION_KEY` for encrypted backups (optional)
- [ ] Set up monitoring (Prometheus scrapes `/metrics`)
- [ ] Configure log aggregation
- [ ] Set up backup schedule

## Docker Deployment

### Build and run

```bash
# Build image
docker build -t agentic-repos .

# Run with Docker Compose
docker compose up -d

# Or Nix-based image
docker compose -f compose-nix.yml up -d
```

### compose.yml

The included `compose.yml` exposes:
- Port 2222 (SSH)
- Port 8090 (HTTP)
- Volume mount for data persistence (`./data:/app/data`)

### Custom image

```bash
docker run -d \
  --name agentic-repos \
  -p 2222:2222 \
  -p 8090:8090 \
  -v /data/git:/app/data \
  -v /data/git:/app/data \
  # No config file mount — config is in git refs
  -e RUST_LOG=info \
  agentic-repos
```

## Kubernetes Deployment

See [`k8s/README.md`](k8s/README.md) for full Kubernetes deployment with:
- Base manifests (namespace, configmap, deployment, service, ingress, HPA)
- Overlay environments: local (Garage), staging, production (SeaweedFS + S3)
- Gateway API support (Envoy Gateway or Cilium)
- Horizontal pod autoscaler (2–10 replicas)

```bash
# Deploy
kubectl apply -k k8s/overlays/production/

# Port forward for testing
kubectl port-forward -n agentic-repos svc/agentic-repos 8090:8090
```

## Post-Deployment Setup

### 1. Create admin user

```bash
# Via init (recommended)
agentic-repos init --admin-key "ssh-ed25519 AAAA... admin@host" --admin-username admin

# Via init (recommended)
agentic-repos init --admin-key "ssh-ed25519 AAAA... admin@host" --admin-username admin
```

### 2. Configure RBAC roles

```bash
# List default roles
curl http://localhost:8090/api/v1/rbac/roles

# Assign role to user
curl -X POST http://localhost:8090/api/v1/rbac/users/admin/roles \
  -H "Content-Type: application/json" \
  -d '{"role": "admin"}'
```

### 3. Set up webhooks

```bash
curl -X POST http://localhost:8090/api/v1/webhooks \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://ci.example.com/hook",
    "events": ["push"],
    "secret": "hmac-shared-secret"
  }'
```

### 4. Configure monitoring

The `/metrics` endpoint exposes Prometheus metrics. See [`monitoring/README.md`](monitoring/README.md) for Grafana dashboard setup.

### 5. Backups

```bash
# Create encrypted backup
curl -X POST http://localhost:8090/api/v1/backups \
  -H "Content-Type: application/json" \
  -d '{"repository": "*", "encrypt": true}'
```

See [Admin Guide](docs/admin-guide.md) for full backup/restore procedures.

## Upgrading

agentic-repos uses schema bootstrapping for migrations — the server auto-creates any missing refs on startup.

```bash
# 1. Build new version
nix build

# 2. Stop server
systemctl stop agentic-repos

# 3. Backup (recommended)
curl -X POST http://localhost:8090/api/v1/backups \
  -d '{"repository": "*", "encrypt": true}'

# 4. Replace binary
cp result/bin/agentic-repos /usr/local/bin/

# 5. Start server (schema bootstrapping is automatic)
systemctl start agentic-repos
```

## Troubleshooting

| Symptom | Check |
|---------|-------|
| Build fails: missing deps | Use `nix build` for reproducible builds |
| SSH host key changes on restart | Set `server.ssh.host_key_path` to a persistent file |
| SSH auth failure | Register key via `agentic-repos init --admin-key` |
| LFS 401 errors | Set `lfs.jwt_secret` in config — required, no default |
| LFS upload fails (>body size) | Increase `client_max_body_size` in reverse proxy |
| Web UI not loading | Enable `features.web_ui: true` in config |
| RBAC not enforcing | Set `rbac.enabled: true` and `features.rbac: true` |
| Plugin not loading (Skylet) | Check `ldd libagentic_repos.so` for missing symbols |
| Schema bootstrap warnings | Non-fatal — check logs for specific ref conflicts |
| Redis connection refused | Verify Redis URL and `features.redis_cache: true` |

### Logs

```bash
# systemd
journalctl -u agentic-repos -f

# Docker
docker compose logs -f agentic-repos

# Adjust log level
RUST_LOG=debug agentic-repos
```

### Health Check

```bash
curl http://localhost:8090/api/v1/health
```

Returns `200 OK` when the server is ready to accept connections.

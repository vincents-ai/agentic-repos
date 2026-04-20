# agentic-repos Administrator Guide

Operations guide for managing agentic-repos in production — user management, RBAC, secrets, backups, replication, and compliance.

## Table of Contents

1. [User Management](#user-management)
2. [RBAC Configuration](#rbac-configuration)
3. [Secrets Management](#secrets-management)
4. [Backup & Restore](#backup--restore)
5. [Replication](#replication)
6. [Compliance Configuration](#compliance-configuration)
7. [Monitoring & Alerting](#monitoring--alerting)
8. [Operational Runbooks](#operational-runbooks)

## User Management

### Creating Users

Users are identified by their SSH public key. The authorized_keys file maps keys to usernames:

```
# Format: username:key-type key comment
alice:ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... alice@laptop
bob:ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABg... bob@workstation
```

#### Via init command

```bash
agentic-repos init --admin-key "ssh-ed25519 AAAA... admin@host" --admin-username admin
```

#### Via API

```bash
# Create user (requires RBAC admin)
curl -X POST http://localhost:8090/api/v1/rbac/users \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "username": "alice",
    "email": "alice@example.com",
    "ssh_key": "ssh-ed25519 AAAA... alice@laptop"
  }'
```

#### Via authorized_keys file

```bash
echo "alice:ssh-ed25519 AAAA... alice@laptop" >> /data/authorized_keys
```

Changes to authorized_keys are picked up automatically.

### Key Rotation

Users can have multiple SSH keys registered. To rotate:

```bash
# Add new key
echo "alice:ssh-ed25519 NEW_KEY... alice@new-laptop" >> /data/authorized_keys

# Remove old key (edit authorized_keys, remove the old line)
```

### Removing Users

```bash
# Via API
curl -X DELETE http://localhost:8090/api/v1/rbac/users/alice \
  -H "Authorization: Bearer $TOKEN"

# Or remove from authorized_keys
# Delete the line containing the user's key
```

## RBAC Configuration

### Enabling RBAC

```yaml
# config.yaml
features:
  rbac: true
rbac:
  enabled: true
  storage_type: "memory"    # or "database" for PostgreSQL persistence
  admin_user:
    username: "admin"
    email: "admin@example.com"
    ssh_key: "ssh-ed25519 AAAA..."
```

### Default Roles

| Role | Permissions |
|------|-------------|
| `admin` | All permissions — full system access |
| `maintainer` | Repository CRUD, LFS, webhooks, compliance read |
| `developer` | Repository read/write, LFS upload/download |
| `viewer` | Repository read-only, LFS download |

### Managing Roles

```bash
# List all roles
curl http://localhost:8090/api/v1/rbac/roles

# Get role details
curl http://localhost:8090/api/v1/rbac/roles/admin

# Create custom role
curl -X POST http://localhost:8090/api/v1/rbac/roles \
  -H "Content-Type: application/json" \
  -d '{
    "name": "release-manager",
    "permissions": ["RepositoryRead", "RepositoryWrite", "RepositoryAdmin"]
  }'

# Assign role to user
curl -X POST http://localhost:8090/api/v1/rbac/users/alice/roles \
  -H "Content-Type: application/json" \
  -d '{"role": "release-manager"}'

# Remove role from user
curl -X DELETE http://localhost:8090/api/v1/rbac/users/alice/roles/release-manager
```

### Permission Reference

| Permission | Description | Required For |
|-----------|-------------|-------------|
| `SystemAdmin` | Full system access | Cluster management, ADP config |
| `SettingsManage` | System settings | Backups, security scanning, SDLC, analytics |
| `RepositoryRead` | Read repositories | List/view repos, branches, commits, tree |
| `RepositoryCreate` | Create repositories | Create new repositories |
| `RepositoryWrite` | Write repositories | Push, update repo settings |
| `RepositoryAdmin` | Admin repositories | Delete repos, manage secrets |
| `ComplianceManage` | Manage compliance | GDPR, DORA, NIS2, retention |
| `ReplicationManage` | Manage replication | Configure and run replication |

### RBAC Scope Enforcement

Permissions are enforced at the route scope level:

- `/api/v1/backups/*` → `SettingsManage`
- `/api/v1/cluster/*` → `SystemAdmin`
- `/api/v1/adp/*` → `SystemAdmin`
- `/api/v1/secrets/*` → `RepositoryAdmin`
- `/api/v1/security/*` → `SettingsManage`
- `/api/v1/compliance/*` → `ComplianceManage`
- `/api/v1/replication/*` → `ReplicationManage`

## Secrets Management

### Architecture (ADR-024)

Secrets use a two-layer AGE key wrapping scheme:

```
Root AGE key (/etc/adp/wrapping-key.age)
  └── Per-repo AGE keypair (encrypted with root key)
       └── SOPS-encrypted secrets (refs/secrets/*)
```

### Setting Up Secrets

#### 1. Bootstrap the root key

```bash
agentic-repos init --root-key-path /etc/adp/wrapping-key.age
```

This generates the AGE root key at the specified path. **Back up this key — it cannot be recovered if lost.**

#### 2. Create a secret

```bash
curl -X POST http://localhost:8090/api/v1/secrets/myorg/myrepo \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "key": "DATABASE_URL",
    "value": "postgresql://user:pass@db:5432/mydb"
  }'
```

#### 3. List secrets (metadata only — values never exposed)

```bash
curl http://localhost:8090/api/v1/secrets/myorg/myrepo \
  -H "Authorization: Bearer $TOKEN"
```

Response contains only metadata (key names, creation dates) — **plaintext values are never returned by the API**.

### Egress Control

Secrets can only leave the server via outbound webhooks with an explicit per-key allowlist:

```yaml
# In webhook configuration
secret_keys: ["DATABASE_URL"]  # Only these keys available to the webhook
```

- No wildcards allowed
- Default: empty list (no secret access)
- Plaintext secrets never appear in API responses, web UI, or git protocol

### Key Security

- AGE private keys are zeroized in memory on Drop using `volatile_write`
- `refs/secrets/*` is a private namespace stripped from git wire protocol
- Public keys are available at `refs/meta/public-key` for zero-knowledge secret submission

## Backup & Restore

### Encryption Setup

Set the `BACKUP_ENCRYPTION_KEY` environment variable before starting the server:

```bash
# Generate a strong key
export BACKUP_ENCRYPTION_KEY=$(openssl rand -hex 32)

# Or in env file
echo "BACKUP_ENCRYPTION_KEY=$(openssl rand -hex 32)" >> /etc/agentic-repos/env
```

Backups use AES-256-GCM with the `"ADPENC"` magic header for backwards-compatible encryption detection.

### Creating Backups

```bash
# Backup all repositories
curl -X POST http://localhost:8090/api/v1/backups \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"repository": "*", "encrypt": true}'

# Backup specific repository
curl -X POST http://localhost:8090/api/v1/backups \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"repository": "myorg/myrepo", "encrypt": true}'

# List backups
curl http://localhost:8090/api/v1/backups \
  -H "Authorization: Bearer $TOKEN"
```

### Restoring

```bash
# Restore from backup
curl -X POST http://localhost:8090/api/v1/backups/{backup_id}/restore \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"target_repository": "myorg/myrepo-restored"}'
```

Tenant scoping is enforced — restore operations reject cross-tenant backup files.

### Concurrency

Maximum 4 concurrent backups (configurable via `MAX_CONCURRENT_BACKUPS` semaphore). Additional backup requests are queued.

### Scheduled Backups

```bash
# Add to crontab for daily backups at 2am
0 2 * * * curl -X POST http://localhost:8090/api/v1/backups -H "Authorization: Bearer $TOKEN" -d '{"repository": "*", "encrypt": true}'
```

## Replication

### Enabling Replication

```yaml
features:
  multi_region: true
  horizontal_scaling: true
```

### Configuration

```bash
# Register a replication target
curl -X POST http://localhost:8090/api/v1/replication/targets \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "us-west-2",
    "endpoint": "https://git-west.example.com",
    "auth_tag": "hmac-shared-secret"
  }'
```

### Conflict Resolution

Replication uses Last-Write-Wins (LWW) with timestamp comparison. Conflicts are:
1. Logged to the audit trail with full details
2. Resolved by selecting the version with the latest timestamp
3. The loser version is preserved as a conflict artifact

### Monitoring Replication

```bash
# Check replication status
curl http://localhost:8090/api/v1/replication/status \
  -H "Authorization: Bearer $TOKEN"

# List replication operations
curl http://localhost:8090/api/v1/replication/operations \
  -H "Authorization: Bearer $TOKEN"
```

## Compliance Configuration

### GDPR

```yaml
features:
  gdpr_compliance: true
```

```bash
# Export user data (GDPR Art. 20 — data portability)
curl -X POST http://localhost:8090/api/v1/compliance/gdpr/data-export \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"user_id": "alice"}'

# Delete user data (GDPR Art. 17 — right to erasure)
curl -X DELETE http://localhost:8090/api/v1/compliance/gdpr/data \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"user_id": "alice"}'
```

### Retention Policies

```bash
# Create retention policy
curl -X POST http://localhost:8090/api/v1/compliance/retention/policies \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "standard-retention",
    "description": "Standard 7-year retention",
    "rules": [{"type": "age", "max_age_days": 2555}],
    "action": "archive"
  }'

# Enforce policies
curl -X POST http://localhost:8090/api/v1/compliance/retention/enforce-all \
  -H "Authorization: Bearer $TOKEN"
```

### DORA (ICT Risk Management)

```bash
# View ICT risk framework
curl http://localhost:8090/api/v1/compliance/dora/framework \
  -H "Authorization: Bearer $TOKEN"

# Register an ICT risk
curl -X POST http://localhost:8090/api/v1/compliance/dora/risks \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"description": "...", "severity": "high", "category": "operational"}'
```

### NIS2 (Incident Notification)

```bash
# Report an incident
curl -X POST http://localhost:8090/api/v1/compliance/nis2/incidents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "description": "Security incident...",
    "severity": "significant",
    "affected_systems": ["git-server"],
    "detection_time": "2026-04-20T10:00:00Z"
  }'
```

### Compliance Reports

```bash
# Generate compliance report
curl -X POST http://localhost:8090/api/v1/compliance/reports \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"framework": "GDPR", "scope": "full"}'

# Download report
curl http://localhost:8090/api/v1/compliance/reports/{id}/download \
  -H "Authorization: Bearer $TOKEN" \
  -o report.json
```

### Compliance Tagging

Tag repositories with compliance metadata for audit tracking:

```bash
curl -X POST http://localhost:8090/api/v1/compliance/tags \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "repository": "myorg/myrepo",
    "tags": ["GDPR", "HIPAA", "PCI-DSS"]
  }'
```

## Monitoring & Alerting

### Prometheus Metrics

The `/metrics` endpoint exposes:

- `lfs_uploads_total`, `lfs_downloads_total`
- `lfs_upload_bytes`, `lfs_download_bytes`
- `git_operations_total` (by type: clone, push, pull)
- `http_requests_total` (by endpoint, status)
- `rbac_permission_checks_total` (by permission, result)
- `backup_operations_total` (by type, status)
- `webhook_deliveries_total` (by status)
- `replication_lag_seconds`

### Grafana Dashboard

See [`monitoring/README.md`](monitoring/README.md) for pre-built Grafana dashboards.

### Recommended Alerts

```yaml
# Example Prometheus alerts
- alert: HighErrorRate
  expr: rate(http_requests_total{status=~"5.."}[5m]) > 0.1
  for: 5m

- alert: BackupFailure
  expr: increase(backup_operations_total{status="failed"}[1h]) > 0

- alert: ReplicationLag
  expr: replication_lag_seconds > 300
  for: 10m

- alert: HighLFSUploadLatency
  expr: histogram_quantile(0.95, rate(lfs_upload_duration_seconds_bucket[5m])) > 60
```

## Operational Runbooks

### Rotate SSH Host Key

```bash
# 1. Generate new key
ssh-keygen -t ed25519 -f /data/ssh/ssh_host_ed25519_key.new -N ""

# 2. Update config to point to new key
# server.ssh.host_key_path: "/data/ssh/ssh_host_ed25519_key.new"

# 3. Restart server
systemctl restart agentic-repos

# 4. Notify users — they will see a changed host key warning
# 5. After confirmation, remove old key
rm /data/ssh/ssh_host_ed25519_key.old
```

### Rotate LFS JWT Secret

```bash
# 1. Generate new secret
NEW_SECRET=$(openssl rand -hex 32)

# 2. Update config.yaml lfs.jwt_secret
# 3. Restart server
systemctl restart agentic-repos

# 4. All existing LFS tokens become invalid immediately
#    Users will need to re-authenticate
```

### Emergency: Disable All Access

```bash
# Stop the server
systemctl stop agentic-repos

# Or via iptables (if you need HTTP for investigation)
iptables -A INPUT -p tcp --dport 2222 -j DROP  # Block SSH
iptables -A INPUT -p tcp --dport 8090 -s !10.0.0.0/8 -j DROP  # Block external HTTP
```

### Recover from Backup

```bash
# 1. Stop the server
systemctl stop agentic-repos

# 2. List available backups
curl http://localhost:8090/api/v1/backups -H "Authorization: Bearer $TOKEN"

# 3. Restore (requires running server)
systemctl start agentic-repos
curl -X POST http://localhost:8090/api/v1/backups/{id}/restore \
  -H "Authorization: Bearer $TOKEN"

# 4. Verify repository integrity
curl http://localhost:8090/api/v1/repositories
```

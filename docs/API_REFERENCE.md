# API Reference — repos-mcp (agentic-repos)

**Base URL:** `http://localhost:8090/api/v1`  
**Server:** actix-web · Port configurable via `config.yaml`  
**Format:** JSON request/response bodies

---

## Table of Contents

- [Health & Info](#health--info)
- [Metrics](#metrics)
- [Repositories](#repositories)
- [Users](#users)
- [Authentication](#authentication)
- [RBAC](#rbac)
- [ADP](#adp)
- [Secrets](#secrets)
- [LFS](#lfs)
- [LFS Locks](#lfs-locks)
- [Backups](#backups)
- [Webhooks](#webhooks)
- [Cluster & Scaling](#cluster--scaling)
- [Security Scanning](#security-scanning)
- [SDLC Monitoring](#sdlc-monitoring)
- [Analytics](#analytics)
- [Replication](#replication)
- [CI/CD](#cicd)
- [Compliance](#compliance)
  - [GDPR & Retention](#gdpr--retention)
  - [DORA (ICT Risk)](#dora-ict-risk)
  - [NIS2 (Incident Notification)](#nis2-incident-notification)

---

## Health & Info

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/info` | API info and version |

---

## Metrics

| Method | Path | Description |
|--------|------|-------------|
| GET | `/metrics` | Prometheus metrics (LFS + SDLC gauges) |

Response: `text/plain` Prometheus exposition format.

---

## Repositories

| Method | Path | Description |
|--------|------|-------------|
| GET | `/repositories` | List repositories |
| POST | `/repositories` | Create repository |
| GET | `/repositories/{owner}/{repo}` | Get repository info |
| DELETE | `/repositories/{owner}/{repo}` | Delete repository |

---

## Users

| Method | Path | Description |
|--------|------|-------------|
| GET | `/users` | List users (with live repository counts) |
| POST | `/users` | Create user |
| GET | `/users/{username}` | Get user (with repository count) |
| PUT | `/users/{username}` | Update user |
| DELETE | `/users/{username}` | Delete user |
| GET | `/users/{username}/repositories` | List user's repositories |

**Create User Request:**
```json
{ "username": "alice", "email": "alice@example.com" }
```

**User Response:**
```json
{
  "username": "alice",
  "email": "alice@example.com",
  "created_at": "2026-04-12T12:00:00+00:00",
  "updated_at": "2026-04-12T12:00:00+00:00",
  "repository_count": 3
}
```

---

## Authentication

| Method | Path | Description |
|--------|------|-------------|
| POST | `/auth/login` | Login (SSH key / password) |
| POST | `/auth/logout` | Logout |
| POST | `/auth/refresh` | Refresh JWT token |
| POST | `/auth/saml/callback` | SAML SSO callback |
| POST | `/auth/oauth/callback` | OAuth2 callback |

---

## RBAC

| Method | Path | Description |
|--------|------|-------------|
| GET | `/rbac/users/me` | Get current user's roles/permissions |
| GET | `/rbac/roles` | List roles |
| POST | `/rbac/roles` | Create role |
| GET | `/rbac/roles/{id}` | Get role |
| PUT | `/rbac/roles/{id}` | Update role |
| DELETE | `/rbac/roles/{id}` | Delete role |
| POST | `/rbac/roles/{id}/permissions` | Add permission to role |
| DELETE | `/rbac/roles/{id}/permissions` | Remove permission |
| POST | `/rbac/users/{id}/roles` | Assign role to user |
| DELETE | `/rbac/users/{id}/roles` | Remove role from user |
| GET | `/rbac/permissions` | List all permissions |
| POST | `/rbac/check` | Check if user has permission |

---

## ADP

| Method | Path | Description |
|--------|------|-------------|
| GET | `/adp/schema` | List schemas |
| POST | `/adp/schema` | Create schema |
| GET | `/adp/schema/{id}` | Get schema |
| DELETE | `/adp/schema/{id}` | Delete schema |
| GET | `/adp/access/config` | Get access control config |
| POST | `/adp/access/config` | Update access control config |

---

## Secrets

| Method | Path | Description |
|--------|------|-------------|
| GET | `/{owner}/{repo}/secrets` | List secrets (names only, never values) |
| POST | `/{owner}/{repo}/secrets` | Create secret |
| GET | `/{owner}/{repo}/secrets/{name}` | Get decrypted secret |
| PUT | `/{owner}/{repo}/secrets/{name}` | Update secret |
| DELETE | `/{owner}/{repo}/secrets/{name}` | Delete secret |

**Create Secret Request:**
```json
{ "name": "db-password", "data": { "value": "s3cret123" } }
```

Secrets are AES-256-GCM encrypted at rest in git refs (`refs/secrets/{name}`).

---

## LFS

| Method | Path | Description |
|--------|------|-------------|
| POST | `/{tenant}/{repo}/info/lfs/objects/batch` | Batch API (upload/download) |
| PUT | `/{tenant}/{repo}/lfs/objects/{oid}` | Upload object |
| GET | `/{tenant}/{repo}/lfs/objects/{oid}` | Download object |
| GET | `/{tenant}/{repo}/quota` | Get quota info |
| POST | `/{tenant}/{repo}/locks/verify` | Verify locks before push |

---

## LFS Locks

| Method | Path | Description |
|--------|------|-------------|
| POST | `/lfs-locks/lock` | Create lock |
| POST | `/lfs-locks/unlock` | Delete lock |
| GET | `/lfs-locks/list` | List locks |

---

## Backups

| Method | Path | Description |
|--------|------|-------------|
| GET | `/backups` | List backups |
| POST | `/backups` | Create backup |
| GET | `/backups/{id}` | Get backup status |
| POST | `/backups/{id}/restore` | Restore from backup |
| DELETE | `/backups/{id}` | Delete backup |
| GET | `/backups/schedules` | List schedules |
| POST | `/backups/schedules` | Create schedule |
| PUT | `/backups/schedules/{id}` | Update schedule |
| DELETE | `/backups/schedules/{id}` | Delete schedule |
| POST | `/backups/{id}/verify` | Verify backup integrity |

---

## Webhooks

| Method | Path | Description |
|--------|------|-------------|
| GET | `/webhooks` | List webhooks |
| POST | `/webhooks` | Create webhook |
| GET | `/webhooks/{id}` | Get webhook |
| PUT | `/webhooks/{id}` | Update webhook |
| DELETE | `/webhooks/{id}` | Delete webhook |
| POST | `/webhooks/{id}/test` | Test webhook delivery |
| GET | `/webhooks/{id}/deliveries` | List deliveries |
| POST | `/webhooks/{id}/deliveries/{did}/retry` | Retry delivery |

---

## Cluster & Scaling

| Method | Path | Description |
|--------|------|-------------|
| GET | `/cluster/status` | Cluster status |
| GET | `/cluster/health` | Health check |
| POST | `/cluster/scale` | Scale instances |
| GET | `/cluster/sessions` | List sessions |
| DELETE | `/cluster/sessions/{id}` | Delete session |
| GET | `/cluster/load-balancer` | Load balancer status |
| POST | `/cluster/load-balancer/drain` | Drain node |
| POST | `/cluster/load-balancer/restore` | Restore node |

---

## Security Scanning

| Method | Path | Description |
|--------|------|-------------|
| POST | `/security/scan` | Submit content for security scan |
| GET | `/security/results/{scan_id}` | Get scan results |

---

## SDLC Monitoring

| Method | Path | Description |
|--------|------|-------------|
| GET | `/sdlc/metrics` | List quality metrics |
| POST | `/sdlc/metrics` | Record quality metric |
| GET | `/sdlc/gates` | List quality gates |
| POST | `/sdlc/gates` | Create quality gate |
| POST | `/sdlc/gates/{id}/evaluate` | Evaluate a gate |
| POST | `/sdlc/gates/evaluate-all` | Evaluate all gates |
| GET | `/sdlc/assessments` | List assessments |
| POST | `/sdlc/assessments` | Create assessment |
| GET | `/sdlc/dashboard` | SDLC dashboard (metrics + gates + DORA) |

**Record Metric Request:**
```json
{
  "name": "test_coverage",
  "category": "test_coverage",
  "value": 87.5,
  "target": 80.0,
  "unit": "%"
}
```

**Create Gate Request:**
```json
{
  "name": "merge-gate",
  "phase": "implementation",
  "conditions": [
    { "metric_name": "test_coverage", "operator": "greater_equal", "threshold": 80.0, "description": "Coverage >= 80%" }
  ],
  "enforcement": "blocking"
}
```

---

## Analytics

| Method | Path | Description |
|--------|------|-------------|
| GET | `/analytics/repos/{repo_id}/stats` | Repository statistics |
| GET | `/analytics/repos/{repo_id}/contributors` | Contributor stats |
| POST | `/analytics/trends` | Trend analysis |

---

## Replication

| Method | Path | Description |
|--------|------|-------------|
| GET | `/replication/targets` | List replication targets |
| POST | `/replication/targets` | Add target |
| GET | `/replication/targets/{id}` | Get target |
| DELETE | `/replication/targets/{id}` | Remove target |
| POST | `/replication/targets/{id}/enable` | Enable target |
| POST | `/replication/targets/{id}/disable` | Disable target |
| GET | `/replication/health` | List health status |
| GET | `/replication/health/{id}` | Get target health |
| GET | `/replication/dashboard` | Replication dashboard |
| GET | `/replication/conflicts` | List conflicts |
| POST | `/replication/conflicts/{id}/resolve` | Resolve conflict |
| GET | `/replication/alerts` | Check alerts |

---

## CI/CD

| Method | Path | Description |
|--------|------|-------------|
| POST | `/cicd/webhooks/{provider}` | Receive CI/CD webhook |
| GET | `/cicd/builds` | List builds |
| GET | `/cicd/builds/{id}` | Get build detail |
| POST | `/cicd/builds/{id}/artifacts` | Upload artifact |
| GET | `/cicd/builds/{id}/artifacts` | List artifacts |

---

## Compliance

### GDPR & Retention

| Method | Path | Description |
|--------|------|-------------|
| GET | `/compliance/reports` | List compliance reports |
| POST | `/compliance/reports` | Generate report |
| GET | `/compliance/reports/{id}` | Get report |
| GET | `/compliance/reports/{id}/download` | Download report (JSON/CSV) |
| GET | `/compliance/gdpr/data-export` | Export user data (GDPR Art. 20) |
| POST | `/compliance/gdpr/data-export` | Export user data (POST) |
| DELETE | `/compliance/gdpr/data` | Delete user data (GDPR Art. 17) |
| GET | `/compliance/retention/policies` | List retention policies |
| POST | `/compliance/retention/policies` | Create policy |
| GET | `/compliance/retention/policies/{id}` | Get policy |
| DELETE | `/compliance/retention/policies/{id}` | Delete policy |
| POST | `/compliance/retention/enforce` | Enforce a policy |
| POST | `/compliance/retention/enforce-all` | Enforce all policies |
| GET | `/compliance/retention/summary` | Retention summary |

### DORA (ICT Risk)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/compliance/dora/framework` | Get ICT risk framework |
| POST | `/compliance/dora/framework` | Save framework |
| GET | `/compliance/dora/incidents` | List incidents |
| POST | `/compliance/dora/incidents` | Report incident |
| POST | `/compliance/dora/incidents/{id}/notify` | Notify competent authority |
| GET | `/compliance/dora/incidents/overdue` | Overdue reports |
| GET | `/compliance/dora/testing/{year}` | Get testing programme |
| POST | `/compliance/dora/testing/{year}` | Save testing programme |
| GET | `/compliance/dora/providers` | List third-party providers |
| POST | `/compliance/dora/providers` | Register provider |
| GET | `/compliance/dora/providers/missing-exit` | Providers missing exit strategy |
| GET | `/compliance/dora/providers/needing-audit` | Critical providers needing audit |
| GET | `/compliance/dora/sharing` | List information sharing arrangements |
| POST | `/compliance/dora/sharing` | Create sharing arrangement |
| GET | `/compliance/dora/status` | DORA compliance status |

**Report Incident Request:**
```json
{
  "title": "Git server outage",
  "description": "SSH unavailable for 2 hours",
  "severity": "major",
  "classification": "availability"
}
```

### NIS2 (Incident Notification)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/compliance/nis2/entities` | List entities |
| POST | `/compliance/nis2/entities` | Register entity |
| GET | `/compliance/nis2/entities/{id}` | Get entity |
| GET | `/compliance/nis2/security-measures/{entity_id}` | Get security measures |
| POST | `/compliance/nis2/security-measures` | Save security measures |
| GET | `/compliance/nis2/security-measures/overdue` | Overdue assessments |
| GET | `/compliance/nis2/incidents` | List incidents |
| POST | `/compliance/nis2/incidents` | Report incident |
| POST | `/compliance/nis2/incidents/{id}/notify/initial` | Initial notification (24h) |
| POST | `/compliance/nis2/incidents/{id}/notify/detailed` | Detailed notification (72h) |
| POST | `/compliance/nis2/incidents/{id}/notify/final` | Final report (1 month) |
| GET | `/compliance/nis2/incidents/overdue` | Overdue notifications |
| GET | `/compliance/nis2/status` | NIS2 compliance status |

**Register Entity Request:**
```json
{
  "name": "GitCorp",
  "entity_type": "essential",
  "sector": "digital"
}
```

**Report Incident Request:**
```json
{
  "entity_id": "uuid-of-entity",
  "title": "Data breach",
  "description": "Customer data exposed",
  "severity": "significant",
  "category": "data_breach"
}
```

---

## Feature Flags

Endpoint availability depends on feature flags in `config.yaml` or environment variables:

| Flag | Enables |
|------|---------|
| `git_lfs` | LFS batch/upload/download/quota endpoints |
| `lfs_locks` | LFS lock management |
| `rbac` | RBAC role/permission management |
| `webhooks` | Webhook CRUD and delivery |
| `monitoring` | SDLC metrics and Prometheus export |
| `analytics` | Analytics endpoints |
| `backup_recovery` | Backup/schedule/restore endpoints |
| `web_ui` | Embedded web UI |

---

## Error Responses

All errors follow a consistent format:

```json
{
  "error": "error_code",
  "message": "Human-readable description"
}
```

Common HTTP status codes:
- `200` — Success
- `201` — Created
- `204` — No Content (successful deletion)
- `400` — Bad Request (validation error)
- `401` — Unauthorized
- `403` — Forbidden (RBAC denied)
- `404` — Not Found
- `500` — Internal Server Error

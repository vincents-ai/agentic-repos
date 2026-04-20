# agentic-repos Security Model

This document describes the security architecture and controls in agentic-repos.

## Table of Contents

1. [Authentication](#authentication)
2. [Authorization (RBAC)](#authorization-rbac)
3. [Transport Security](#transport-security)
4. [Secrets Infrastructure](#secrets-infrastructure)
5. [Data Protection](#data-protection)
6. [Input Validation & Injection Prevention](#input-validation--injection-prevention)
7. [Rate Limiting](#rate-limiting)
8. [Audit Logging](#audit-logging)
9. [Security Headers](#security-headers)
10. [Backup Security](#backup-security)
11. [Webhook Security](#webhook-security)
12. [LFS Security](#lfs-security)
13. [Cluster Security](#cluster-security)
14. [Compliance](#compliance)
15. [Security Configuration Checklist](#security-configuration-checklist)

## Authentication

### SSH Public Key Authentication

Git operations over SSH (port 2222) use public key authentication:

- Keys stored in `authorized_keys` file (format: `username:ssh-key-type key comment`)
- SSH host key persisted to disk (configurable via `server.ssh.host_key_path`)
- Ed25519 keys recommended; RSA keys accepted
- Key size limits enforced (max 8KB per key)

### HTTP Authentication

The HTTP API (port 8090) supports multiple authentication methods:

| Method | Endpoints | Mechanism |
|--------|-----------|-----------|
| OAuth2 session | Most API endpoints | Session cookie with `Secure` + `SameSite=Strict` flags |
| LFS JWT | LFS batch/upload/download | JWT with operation-specific claims (`download` or `upload`) |
| None | `/health`, `/info`, `/metrics` | Public endpoints |

#### OAuth2 Session Security

- Session cookies: `Secure` flag (HTTPS only), `SameSite=Strict` (CSRF protection)
- Session IDs validated against a strict format (rejects separators, control chars, path traversal)
- Session data encrypted at rest with AES-256-GCM via `SESSION_ENCRYPTION_KEY`
- X-ADP-User header: ignored by handlers — user identity comes ONLY from the validated session

#### LFS JWT Authentication

- JWT secret: `Option<String>` with no default — must be explicitly configured
- Tokens contain operation-specific claims (`download` or `upload`) — upload tokens cannot download and vice versa
- Token expiration configurable via `lfs.token_expiration_secs`

## Authorization (RBAC)

Role-Based Access Control gates all API operations:

### Permission Model

Permissions are scoped per resource type:

| Permission | Scope | Required For |
|-----------|-------|-------------|
| `SystemAdmin` | System-wide | Cluster status, ADP config, system settings |
| `SettingsManage` | Settings | Backups, security scanning, SDLC, analytics |
| `RepositoryRead` | Repositories | List/view repos, branches, commits |
| `RepositoryCreate` | Repositories | Create new repositories |
| `RepositoryWrite` | Repositories | Push, update repo settings |
| `RepositoryAdmin` | Repositories | Delete repos, manage secrets |
| `ComplianceManage` | Compliance | GDPR, DORA, NIS2, retention policies |
| `ReplicationManage` | Replication | Configure and manage replication |

### How RBAC is Applied

Auth middleware is applied at the scope level in the production route configurator:

```
/api/v1/backups      → SettingsManage
/api/v1/cluster      → SystemAdmin
/api/v1/adp          → SystemAdmin
/api/v1/security     → SettingsManage
/api/v1/sdlc         → SettingsManage
/api/v1/analytics    → SettingsManage
/api/v1/compliance   → ComplianceManage
```

Individual route handlers also check operation-specific permissions (e.g., repository owners get elevated access to their own repos).

### Disabling RBAC

RBAC is gated by the `features.rbac` feature flag. When disabled, all operations are permitted without authorization checks. **Do not disable RBAC in production.**

## Transport Security

### SSH

- Host key: Ed25519, persisted to configurable path
- Algorithm: russh (pure Rust SSH implementation — no libssh2 FFI)
- Connection limits: configurable `max_connections` and `connection_timeout_secs`

### HTTP

- TLS: Offloaded to reverse proxy (Nginx, Caddy, or cloud load balancer)
- Security headers: applied globally via actix DefaultHeaders middleware

### Recommended TLS Configuration

```nginx
ssl_protocols TLSv1.2 TLSv1.3;
ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
ssl_prefer_server_ciphers off;
```

## Secrets Infrastructure

Per ADR-024, secrets use a two-layer AGE key wrapping scheme:

| Layer | Key | Location |
|-------|-----|----------|
| Root wrapping key | AGE private key | `/etc/adp/wrapping-key.age` — NOT in git |
| Per-repo keypair | AGE keypair, private encrypted with root | `refs/adp/secrets/keys/{repo-id}/private` in system repo |
| Secret data | SOPS-encrypted JSON | `refs/secrets/*` in user repos |

### Egress Control

- Plaintext secrets **never** leave the server via API, UI, or git protocol
- Only outbound webhooks can access plaintext, via explicit per-key allowlist (`secret_keys: []` — no wildcards, default empty)
- `refs/secrets/*` is a private namespace stripped from the wire protocol

### AGE Key Zeroization

AGE private keys are zeroized on Drop using `volatile_write` to prevent compiler optimization from eliminating the memory clear.

## Data Protection

### Encryption at Rest

| Data | Algorithm | Key Source |
|------|-----------|------------|
| Session data | AES-256-GCM | `SESSION_ENCRYPTION_KEY` env var |
| Backup data | AES-256-GCM | `BACKUP_ENCRYPTION_KEY` env var |
| Secrets | AGE (X25519 + ChaCha20-Poly1305) | Per-repo keypair |
| S3 credentials | AES-256 via S3 SDK | AWS credentials chain |
| S3 secret keys | Zeroized on Drop | `volatile_write` |

### Cryptographic Design

- AES-256-GCM: random nonce per encryption (via `Aes256Gcm::generate_nonce(&mut OsRng)`)
- Auth tag supersedes plaintext hash — no plaintext hash stored (would leak information)
- `EncryptedData` struct does not include `plaintext_hash` field

## Input Validation & Injection Prevention

### Path Traversal Defense

All user-supplied path components are validated with `validate_path_component()`:

- Rejects empty strings, path separators (`/`, `\`), `..`, hidden files (`.`), null bytes
- Defense-in-depth: `starts_with()` check ensures resolved paths remain within the expected root
- Applied to: repository names, owner names, LFS OIDs, backup paths, namespace components, ref names

### Owner Name Validation

Whitelist approach — only alphanumeric, hyphen, and underscore characters. Rejects `/`, `\`, `.`, null bytes, and leading `-_`.

### Command Injection Prevention

SSH command execution validates against an allowlist of git commands (`git-upload-pack`, `git-receive-pack`). Raw input is never passed to a shell.

### ReDoS Protection

Three layers on all user-supplied regex patterns:
1. Length limit (max 256 characters)
2. Heuristic nested quantifier detection
3. Compile-time timeout

### Error Message Sanitization

`ApiError::internal_error` logs the full error server-side via `tracing::warn!` but returns a generic message to the client. Prevents CWE-209 (information exposure through error messages).

### SAML Assertion Verification

SAML SSO verifies signature presence on assertions (interim — full XML-DSig verification planned).

## Rate Limiting

Token-bucket rate limiter with tier-based configuration:

```yaml
rate_limit:
  enabled: true
  tiers:
    free:
      requests_per_minute: 60
      burst_size: 10
    pro:
      requests_per_minute: 300
      burst_size: 50
    enterprise:
      requests_per_minute: 1000
      burst_size: 200
```

- Applied globally via actix middleware
- Proxy header support: `X-Forwarded-For`, `X-Real-IP`, `CF-Connecting-IP`
- LFS throttle: separate bandwidth throttling for uploads/downloads

## Audit Logging

### Audit Trail Storage

Default: file-based JSONL (`FileBasedAuditStorage`) — append-only with `sync_all()` for crash durability.

### Audit Events

- Authentication attempts (success and failure)
- Repository mutations (create, delete, push)
- RBAC changes (role assignments, permission changes)
- Secret access
- Backup operations
- Security scan results
- Replication events
- Compliance reports

### Anonymization

Personally identifiable information in audit logs is cryptographically anonymized using SHA-256 (truncated to 8–12 hex chars) — irreversible and consistent.

## Security Headers

Applied globally via actix `DefaultHeaders` middleware:

| Header | Value | Purpose |
|--------|-------|---------|
| `X-Content-Type-Options` | `nosniff` | Prevent MIME type sniffing |
| `X-Frame-Options` | `DENY` | Prevent clickjacking |
| `X-XSS-Protection` | `1; mode=block` | XSS filtering |
| `Strict-Transport-Security` | `max-age=31536000` | Force HTTPS |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Limit referrer leakage |
| `Content-Security-Policy` | `default-src 'self'` | Restrict resource loading |

## Backup Security

- **Encryption**: AES-256-GCM at rest via `BACKUP_ENCRYPTION_KEY` (random nonce per backup)
- **Concurrency**: tokio::sync::Semaphore limits concurrent backups (`MAX_CONCURRENT_BACKUPS=4`)
- **Tenant scoping**: restore operations reject cross-tenant backup files
- **Path sanitization**: `sanitize_component()` + `safe_join()` prevent path traversal in backup paths
- **Sensitive paths**: backup storage paths use `skip_serializing` to prevent API exposure
- **Magic header**: `"ADPENC"` prefix for backwards-compatible encryption detection

## Webhook Security

- **SSRF protection**: DNS resolution at registration time, HTTPS-only URLs, private IP filtering, redirect policy `none`
- **HMAC signatures**: GitHub/Jenkins use HMAC-SHA256 with timing-safe comparison
- **GitLab tokens**: constant-time XOR-based comparison (not direct byte equality)
- **Wildcard restrictions**: webhook URLs cannot use wildcards
- **Payload limits**: 1MB max, 16-level JSON depth limit

## LFS Security

- **JWT**: operation-specific claims (`upload` vs `download`), no default secret
- **OID validation**: strict format check (64-char hex SHA-256)
- **Path traversal**: validated OID used for storage path construction
- **Quota enforcement**: per-repo and per-tenant storage quotas
- **Throttle**: bandwidth throttling for uploads and downloads
- **Lock re-entrancy**: `Mutex<HashSet>` prevents double-lock on same key

## Cluster Security

- **Instance authentication**: HMAC-SHA256 shared secret via `InstanceInfo.auth_tag`
- **Fencing tokens**: monotonically increasing `leadership_term` epoch prevents stale leaders
- **Distributed locks**: Redis `SET NX EX` + Lua compare-and-delete/expire scripts
- **Lock re-entrancy**: `std::sync::Mutex<HashSet>` for short-held operations in async context
- **Error sanitization**: `sanitize_error()` wraps internal errors before sending to other cluster members

## Compliance

| Framework | Features |
|-----------|----------|
| **GDPR** | Data export (Art. 20), right to deletion (Art. 17), retention policies, PII encryption |
| **HIPAA** | Audit logging, encryption at rest, access controls, compliance tagging |
| **DORA** | ICT risk framework, incident management, digital operational resilience |
| **NIS2** | Incident notification, risk management, supply chain security |
| **SOC2** | Audit trail, access control, encryption, monitoring |

### Compliance Tagging

Git refs can be tagged with compliance metadata (GDPR, HIPAA, SOC2, ISO27001, PCI-DSS). Tags are enforced at the repository level and auditable.

## Security Configuration Checklist

### Before Production

- [ ] Set strong `lfs.jwt_secret` (32+ random bytes, no default)
- [ ] Generate persistent SSH host key (`ssh-keygen -t ed25519`)
- [ ] Enable RBAC (`features.rbac: true`, `rbac.enabled: true`)
- [ ] Set `SESSION_ENCRYPTION_KEY` for session encryption
- [ ] Set `BACKUP_ENCRYPTION_KEY` for encrypted backups
- [ ] Configure TLS termination (Nginx, Caddy, or cloud LB)
- [ ] Enable rate limiting (`rate_limit.enabled: true`)
- [ ] Restrict `/metrics` endpoint to internal networks
- [ ] Review feature flags — disable unused features
- [ ] Configure audit logging destination

### Hardening

- [ ] Run as non-root user (systemd `User=agentic-repos`)
- [ ] Use systemd security directives (`NoNewPrivileges`, `ProtectSystem`, `PrivateTmp`)
- [ ] Limit file descriptors (`LimitNOFILE=65536`)
- [ ] Configure reverse proxy with `client_max_body_size` for LFS
- [ ] Set up log aggregation for security events
- [ ] Schedule regular backups with encryption
- [ ] Monitor Prometheus metrics for anomalies

## Reporting Security Issues

If you discover a security vulnerability:

1. Do NOT create public GitHub issues
2. Email security issues to the project maintainers
3. Provide detailed reproduction steps
4. Allow 90 days for fix development before public disclosure

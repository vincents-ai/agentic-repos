# Secrets Management — agentic-repos

Guide to the AGE-encrypted secrets infrastructure in agentic-repos. Based on ADR-024.

---

## Overview

agentic-repos provides zero-knowledge secret storage using AGE encryption (X25519). Secrets are encrypted client-side and stored in git refs. The server never sees plaintext secret values.

### Architecture

```
Client                Server (git refs)              Webhooks
  │                       │                            │
  │  GET /public-key      │                            │
  │◄──────────────────────┤                            │
  │  AGE public key       │                            │
  │                       │                            │
  │  Encrypt secret       │                            │
  │  locally with AGE     │                            │
  │                       │                            │
  │  PUSH to              │                            │
  │  refs/secrets/my-key  ├──── Webhook delivery ─────►│
  │                       │     (plaintext via         │
  │                       │      egress allowlist)     │
  │                       │                            │
  │  GET /secrets/my-key  │                            │
  │◄──────────────────────┤                            │
  │  Metadata only        │                            │
  │  (no plaintext)       │                            │
```

---

## Key Hierarchy (ADR-024)

### Layer 1: Root Wrapping Key

- **Location**: `/etc/adp/wrapping-key.age` on the server filesystem
- **NOT stored in git** — must be provisioned separately
- Used to encrypt per-repo private keys

### Layer 2: Per-Repo Key Pairs

- **Private key**: `refs/adp/secrets/keys/{repo-id}/private` (encrypted with root key)
- **Public key**: `refs/meta/public-key` (publicly accessible)
- AGE X25519 keypairs generated on repository creation

### Layer 3: Secret Blobs

- **Location**: `refs/secrets/{secret-name}`
- SOPS-encrypted JSON blobs using the repo's public key
- Encrypted client-side before pushing

---

## Workflow

### 1. Get the repository public key

```bash
curl https://git.example.com/api/v1/repositories/owner/repo/secrets/public-key
```

Response:
```json
{
  "public_key": "age1qyp85z5...",
  "algorithm": "age-x25519"
}
```

### 2. Encrypt the secret client-side

Using the `age` CLI:

```bash
echo "my-secret-value" | age -r age1qyp85z5... -o secret.enc
```

Or using SOPS for structured data:

```bash
sops --age age1qyp85z5... --encrypt --input-type json secret.json
```

### 3. Push the encrypted secret

```bash
# Write the encrypted blob to refs/secrets/my-secret
git hash-object -w secret.enc | xargs -I{} git update-ref refs/secrets/my-secret {}
git push origin refs/secrets/my-secret
```

Or via API:

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"data": {"API_KEY": "encrypted-value-here"}}' \
  https://git.example.com/api/v1/repositories/owner/repo/secrets/my-secret
```

### 4. Read secret metadata

```bash
curl -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories/owner/repo/secrets/my-secret
```

Response (metadata only, no plaintext):
```json
{
  "name": "my-secret",
  "version": 3,
  "keys": ["API_KEY"],
  "updated_at": "2026-04-21T10:00:00Z"
}
```

---

## Egress Control

Plaintext secrets **never** leave the server via the API or git protocol. The only egress path is through webhooks with an explicit per-key allowlist.

### Webhook secret allowlist

Webhooks that need access to secrets must declare which keys they consume:

```json
{
  "url": "https://deploy.example.com/hook",
  "events": ["push"],
  "secret": "whsec_...",
  "secret_keys": ["my-secret"]  // Explicit allowlist — no wildcards
}
```

- `secret_keys` defaults to `[]` (no access)
- Wildcards are not supported — each key must be listed explicitly
- The webhook payload includes decrypted secret values for the listed keys only

---

## Secret Name Validation

Secret names must pass validation:

- Non-empty
- No `/` or `\` characters
- No `..` (path traversal prevention)
- Cannot start with `.` (hidden files)
- Maximum 256 characters

---

## Security Properties

| Property | Implementation |
|----------|---------------|
| Zero-knowledge | Server never sees plaintext (AGE encryption) |
| At-rest encryption | SOPS-encrypted blobs in git refs |
| Key zeroization | AGE private keys zeroized on drop via volatile write |
| Egress control | Webhook allowlist only, no API plaintext exposure |
| Audit trail | All secret access logged |
| Path traversal protection | Secret name validation |
| Error sanitization | Generic error messages to clients, details server-side only |

---

## API Reference

### List secrets

```bash
GET /api/v1/repositories/{owner}/{repo}/secrets
```

### Get secret metadata

```bash
GET /api/v1/repositories/{owner}/{repo}/secrets/{name}
```

### Create/update secret

```bash
PUT /api/v1/repositories/{owner}/{repo}/secrets/{name}
```

### Delete secret

```bash
DELETE /api/v1/repositories/{owner}/{repo}/secrets/{name}
```

### Get public key

```bash
GET /api/v1/repositories/{owner}/{repo}/secrets/public-key
```

---

## Further Reading

- [Security Model](../SECURITY.md) — Detailed security architecture
- [Webhooks Guide](webhooks.md) — Webhook configuration
- [Configuration Reference](configuration.md) — Secrets configuration
- [Admin Guide](admin-guide.md) — AGE key provisioning

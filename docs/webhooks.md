# Webhooks — agentic-repos

Guide to creating, managing, and troubleshooting webhooks in agentic-repos.

---

## Overview

Webhooks are stored as git refs at `refs/webhooks/{name}` and fire HTTP requests when events occur in a repository.

### Key properties

- All webhooks require a secret for HMAC-SHA256 signature verification
- Outbound URLs must use HTTPS (SSRF prevention)
- Secrets are never exposed through the API — only metadata is returned
- Failed deliveries are retried up to 3 times with exponential backoff

---

## Creating a Webhook

### Via API

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "deploy-hook",
    "url": "https://deploy.example.com/hook",
    "events": ["push", "tag.create"],
    "secret": "whsec_my-webhook-secret"
  }' \
  https://git.example.com/api/v1/repositories/owner/repo/webhooks
```

### Via git ref

```bash
# Create the webhook config blob
cat > webhook.json <<EOF
{
  "name": "deploy-hook",
  "url": "https://deploy.example.com/hook",
  "events": ["push", "tag.create"],
  "secret": "whsec_my-webhook-secret"
}
EOF

# Write to ref
git hash-object -w webhook.json | xargs -I{} git update-ref refs/webhooks/deploy-hook {}
git push origin refs/webhooks/deploy-hook
```

---

## Event Types

| Event | Trigger | Payload includes |
|-------|---------|-----------------|
| `push` | Any ref update | Refs, commits, pusher |
| `tag.create` | New tag created | Tag name, commit |
| `tag.delete` | Tag deleted | Tag name |
| `branch.create` | New branch created | Branch name, head commit |
| `branch.delete` | Branch deleted | Branch name |

---

## Webhook Payload

### Headers

```
X-ADP-Signature-256: sha256=<hex-hmac>
X-ADP-Event: push
X-ADP-Delivery: <uuid>
Content-Type: application/json
```

### Signature verification

The `X-ADP-Signature-256` header contains an HMAC-SHA256 of the request body using the webhook secret:

```python
import hmac, hashlib

def verify_signature(body: bytes, secret: str, signature: str) -> bool:
    expected = "sha256=" + hmac.new(
        secret.encode(), body, hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, signature)
```

### Push event body

```json
{
  "event": "Push",
  "repository": "owner/repo",
  "pusher": "alice",
  "ref": "refs/heads/main",
  "before": "abc123...",
  "after": "def456...",
  "commits": [
    {
      "id": "def456...",
      "message": "Fix login bug",
      "author": "alice",
      "timestamp": "2026-04-21T10:00:00Z"
    }
  ]
}
```

---

## Secret Key Access

Webhooks that need to read secrets from the repository must declare an explicit allowlist:

```json
{
  "name": "deploy-hook",
  "url": "https://deploy.example.com/hook",
  "events": ["push"],
  "secret": "whsec_...",
  "secret_keys": ["deploy-key", "api-token"]
}
```

When the webhook fires, the listed secrets are decrypted and included in the payload. No wildcards are supported — each key must be listed explicitly.

See [Secrets Management](secrets.md) for details on the AGE encryption model.

---

## Delivery and Retry

### Delivery process

1. Event occurs (push, tag, branch)
2. All matching webhooks are identified
3. Payload is constructed with event data
4. HMAC-SHA256 signature is computed
5. POST request is sent to the webhook URL
6. Response is recorded (status, body, timing)

### Retry behavior

| Attempt | Delay |
|---------|-------|
| 1st | Immediate |
| 2nd | 5 seconds |
| 3rd | 30 seconds |

After 3 failed attempts, the delivery is marked as permanently failed. You can manually retry via the API.

### Checking delivery status

```bash
# List recent deliveries
curl -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories/owner/repo/webhooks/deploy-hook/deliveries

# Retry a failed delivery
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories/owner/repo/webhooks/deploy-hook/deliveries/{id}/retry
```

---

## Security

### URL validation

- Only HTTPS URLs are accepted (prevents SSRF attacks)
- URLs are validated at registration time

### Secret requirements

- All webhooks must have a non-empty secret
- Webhooks without secrets cannot be delivered (configuration error)
- Secrets are stored encrypted and never returned via API

### Timing-safe comparison

Signature verification uses constant-time comparison to prevent timing attacks.

---

## Troubleshooting

### Webhook not firing

- Verify the webhook has a secret set
- Check that the event type matches (case-sensitive: `Push`, `TagCreate`)
- Ensure the URL is HTTPS and reachable from the server

### Signature mismatch

- Ensure you're using the exact webhook secret (not the repository secret)
- Verify the HMAC is computed over the raw request body (not parsed JSON)
- Check the header format: `sha256=<hex>`

### Delivery timeouts

- Target server must respond within 10 seconds
- Check the delivery status for HTTP response codes

---

## Further Reading

- [Secrets Management](secrets.md) — AGE encryption and egress control
- [Getting Started](getting-started.md) — Basic webhook setup
- [API Reference](API_REFERENCE.md) — Full webhook API documentation
- [Security Model](../SECURITY.md) — Webhook security architecture

# Webhooks NixOS Integration Tests

This document describes the NixOS integration tests for the repos-mcp webhook system.

## Test File Location

```
nix/tests/webhooks-integration.nix
```

## Running the Tests

```bash
# Run the webhook integration tests
nix build .#tests.webhooks-integration

# Or using nix-shell
nix-shell -p nixos-rebuild --run "nixos-rebuild build-vm"
```

## Test Architecture

The test uses two NixOS virtual machines:

1. **server** - Runs repos-mcp with webhooks enabled
2. **receiver** - Runs webhook receiver services for testing delivery

### Server Configuration

```yaml
webhooks:
  enabled: true
  timeout_seconds: 5      # Delivery timeout
  max_retries: 3          # Maximum retry attempts
  retry_delay_seconds: 1  # Delay between retries
```

### Receiver Services

Three webhook receiver services run on the receiver VM:

| Service | Port | Purpose |
|---------|------|---------|
| `webhook-receiver` | 8888 | Normal webhook receiver |
| `slow-receiver` | 8889 | Simulates slow responses (15s delay) |
| `fail-receiver` | 8890 | Returns 500 errors for retry testing |

## Test Coverage

### Suite 1: Webhook Registration (CRUD Operations)

| Test | Description |
|------|-------------|
| 1.1 | Server startup and health check |
| 1.2 | Create webhook with events and secret |
| 1.3 | List all webhooks |
| 1.4 | List webhooks filtered by tenant |
| 1.5 | Get webhook by ID |
| 1.6 | Update webhook events |
| 1.7 | Deactivate webhook |
| 1.8 | Reactivate webhook |
| 1.9 | Reject invalid URL |
| 1.10 | Delete webhook |

### Suite 2: Event Delivery

| Test | Description |
|------|-------------|
| 2.1 | Setup for delivery tests |
| 2.2 | Push event delivery |
| 2.3 | Repository event delivery |
| 2.4 | LFS event delivery |
| 2.5 | Multiple webhooks for same event |

### Suite 3: Webhook Security

| Test | Description |
|------|-------------|
| 3.1 | HMAC-SHA256 signature computation |
| 3.2 | Signature verification on receiver |
| 3.3 | Webhook without secret |
| 3.4 | Secret key management/rotation |
| 3.5 | Payload validation |

### Suite 4: Delivery Tracking

| Test | Description |
|------|-------------|
| 4.1 | Delivery status tracking |
| 4.2 | Delivery history with pagination |
| 4.3 | Failed delivery handling |
| 4.4 | Custom delivery history limit |

### Suite 5: Delivery Retry and Timeout

| Test | Description |
|------|-------------|
| 5.1 | Delivery timeout handling (5s timeout) |
| 5.2 | Delivery retry on failure |
| 5.3 | Maximum retries exceeded (3 retries) |

### Suite 6: Event Types and Filtering

| Test | Description |
|------|-------------|
| 6.1 | All 16 supported event types |
| 6.2 | Event filtering by subscription |
| 6.3 | Inactive webhook event filtering |

### Suite 7: Multi-tenant Isolation

| Test | Description |
|------|-------------|
| 7.1 | Tenant isolation in webhook creation |
| 7.2 | List webhooks per tenant only |

### Suite 8: Error Handling

| Test | Description |
|------|-------------|
| 8.1 | Get non-existent webhook (404) |
| 8.2 | Update non-existent webhook |
| 8.3 | Delete non-existent webhook |
| 8.4 | Invalid UUID format |
| 8.5 | Malformed JSON request |

### Suite 9: Concurrent Operations

| Test | Description |
|------|-------------|
| 9.1 | Concurrent webhook creation (10 webhooks) |
| 9.2 | Concurrent read operations |

## Supported Event Types

The webhook system supports the following event types:

### Git Events
- `push` - Push to repository
- `branch.create` - Branch created
- `branch.delete` - Branch deleted
- `tag.create` - Tag created
- `tag.delete` - Tag deleted
- `fork` - Repository forked

### Repository Events
- `repository.create` - Repository created
- `repository.delete` - Repository deleted
- `repository.update` - Repository updated

### LFS Events
- `lfs.upload` - LFS file uploaded
- `lfs.download` - LFS file downloaded
- `lfs.lock.create` - LFS lock created
- `lfs.lock.delete` - LFS lock deleted

### System Events
- `system.health` - Health check event
- `system.error` - System error event
- `system.config` - Configuration change event

## HMAC Signature Verification

Webhooks with secrets include HMAC-SHA256 signatures:

### Header Format
```
X-Webhook-Signature-256: sha256=<hex-encoded-signature>
```

### Signature Computation
```python
import hmac
import hashlib

def compute_signature(body: str, secret: str) -> str:
    return hmac.new(
        secret.encode('utf-8'),
        body.encode('utf-8'),
        hashlib.sha256
    ).hexdigest()
```

### Verification Example
```python
import hmac
import hashlib

def verify_signature(body: str, signature: str, secret: str) -> bool:
    if not signature.startswith('sha256='):
        return False
    
    expected = hmac.new(
        secret.encode('utf-8'),
        body.encode('utf-8'),
        hashlib.sha256
    ).hexdigest()
    
    provided = signature[7:]  # Remove 'sha256=' prefix
    
    return hmac.compare_digest(expected, provided)
```

## Webhook Delivery Headers

Every webhook delivery includes:

| Header | Description |
|--------|-------------|
| `Content-Type` | `application/json` |
| `X-Webhook-Event` | Event type (e.g., `push`) |
| `X-Webhook-Delivery` | Unique delivery UUID |
| `X-Webhook-Signature-256` | HMAC signature (if secret configured) |
| `X-Webhook-Secret` | Secret value (if configured) |

## Delivery Status

| Status | Description |
|--------|-------------|
| `success` | Delivered successfully (2xx response) |
| `failed` | Delivery failed (non-2xx or network error) |
| `timeout` | Delivery timed out |

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/webhooks` | Create webhook |
| GET | `/webhooks` | List all webhooks |
| GET | `/webhooks?tenant_id=<id>` | List webhooks by tenant |
| GET | `/webhooks/{id}` | Get webhook by ID |
| PUT | `/webhooks/{id}` | Update webhook |
| DELETE | `/webhooks/{id}` | Delete webhook |
| GET | `/webhooks/{id}/deliveries` | Get delivery history |
| GET | `/webhooks/{id}/deliveries?limit=N` | Get last N deliveries |

## Troubleshooting

### Common Issues

1. **Webhook not receiving events**
   - Verify webhook is active (`active: true`)
   - Check event subscription matches triggered events
   - Verify URL is accessible from server

2. **Signature verification failing**
   - Ensure same secret is used on both ends
   - Verify signature format includes `sha256=` prefix
   - Check body is exactly the same (no extra whitespace)

3. **Delivery timeouts**
   - Check receiver responds within `timeout_seconds`
   - Verify network connectivity between server and receiver
   - Review receiver logs for processing delays

### Debug Logging

Enable debug logging in config:
```yaml
logging:
  level: "debug"
```

Check server logs:
```bash
journalctl -u repos-mcp -f
```

## Test Implementation Notes

### Receiver Implementation

The test receivers use simple netcat-based HTTP servers:

1. **webhook-receiver** - Logs requests and verifies signatures
2. **slow-receiver** - Sleeps 15s before responding (triggers timeout)
3. **fail-receiver** - Always returns 500 (triggers retries)

### Signature Verification

The receiver verifies HMAC signatures using OpenSSL:
```bash
echo -n "$body" | openssl dgst -sha256 -hmac "$secret"
```

### Test Data Isolation

All test webhooks use tenant IDs starting with:
- `test-`
- `delivery-`
- `tenant-`
- `verify-`
- `secure-`
- `tracking-`
- `timeout-`
- `retry-`
- `filter-`
- `inactive-`
- `concurrent-`

This allows easy cleanup after tests.

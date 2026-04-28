# Authentication — agentic-repos

Guide to the authentication methods supported by agentic-repos: SSH keys, OAuth2, LDAP, and SAML.

---

## Table of Contents

- [Authentication Overview](#authentication-overview)
- [SSH Key Authentication](#ssh-key-authentication)
- [OAuth2 Providers](#oauth2-providers)
- [LDAP Authentication](#ldap-authentication)
- [SAML 2.0 Authentication](#saml-20-authentication)
- [Session Management](#session-management)
- [Security Considerations](#security-considerations)

---

## Authentication Overview

agentic-repos supports multiple authentication methods, which can be used together:

| Method | Protocol | Use case |
|--------|----------|----------|
| SSH keys | Git over SSH | Primary method for git operations |
| OAuth2 | HTTP API | Web UI and API access (GitHub, Google, Okta) |
| LDAP | HTTP API | Corporate directory integration |
| SAML 2.0 | HTTP API | Enterprise SSO integration |

### Authentication flow

```
SSH git operations → SSH key lookup → refs/adp/keys/{fingerprint} → username
HTTP API calls     → OAuth2/LDAP/SAML → session token → Bearer header
```

---

## SSH Key Authentication

SSH key authentication is the primary method for git operations (clone, push, pull).

### How it works

1. Client connects via SSH on port 2222 (configurable)
2. Server presents its host key (ED25519 recommended)
3. Client offers public key
4. Server looks up the key fingerprint in `refs/adp/keys/{fingerprint}` in the system repo
5. The ref maps the fingerprint to a username
6. Server verifies the key is still listed in the user's `refs/adp/users/{username}/ssh_keys`
7. Access is granted or denied based on RBAC rules

### Key storage

Keys are stored in git refs (never in flat files):

- **Identity mapping**: `refs/adp/keys/{sha256-fingerprint-hex}` → `{"username": "alice"}`
- **User keys list**: `refs/adp/users/{username}/ssh_keys` → `["ssh-ed25519 AAAA...", ...]`
- **Key comparison**: Uses constant-time comparison to prevent timing attacks

### Registering a key

```bash
# During first-run bootstrap
agentic-repos init --admin-key "$(cat ~/.ssh/id_ed25519.pub)"

# Via API (admin only)
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"ssh_public_key": "ssh-ed25519 AAAA... alice@example.com"}' \
  https://git.example.com/api/v1/users/alice
```

### Supported key types

| Type | Recommended |
|------|------------|
| Ed25519 | ✅ Yes — preferred |
| RSA (2048+ bit) | ✅ Yes |
| ECDSA (P-256, P-384) | ✅ Yes |
| DSA | ❌ No — insecure |

### Rate limiting

SSH authentication attempts are rate-limited per IP and per username. After too many failed attempts, the connection is rejected.

---

## OAuth2 Providers

OAuth2 is used for HTTP API authentication (web UI, API calls).

### Supported providers

| Provider | Config key |
|----------|-----------|
| GitHub | `github` |
| Google | `google` |
| Okta | `okta` |
| Generic OIDC | `generic` |

### Configuration

OAuth2 providers are configured via git refs in the system repository, typically during `agentic-repos init`:

```yaml
oauth:
  provider: "github"
  client_id: "your-client-id"
  client_secret: "your-client-secret"
  redirect_uri: "https://git.example.com/auth/callback"
```

### GitHub setup

1. Go to GitHub Settings → Developer settings → OAuth Apps → New OAuth App
2. Set Authorization callback URL to `https://git.example.com/auth/callback`
3. Copy the Client ID and Client Secret into your config

### Google setup

1. Go to Google Cloud Console → APIs & Services → Credentials
2. Create an OAuth 2.0 Client ID
3. Add redirect URI: `https://git.example.com/auth/callback`
4. Copy credentials into your config

### Okta setup

1. In Okta admin, create a new Web application
2. Set Login redirect URIs to `https://git.example.com/auth/callback`
3. Note the Okta domain, client ID, and client secret

### CSRF protection

All OAuth2 flows use state parameters stored server-side and verified on callback. States are single-use and expire after 10 minutes.

---

## LDAP Authentication

LDAP is used for corporate directory integration, authenticating users against an existing directory service.

### Configuration

LDAP settings are configured in the system repository:

```yaml
ldap:
  url: "ldaps://dc.corp.example.com:636"
  base_dn: "dc=corp,dc=example,dc=com"
  bind_dn: "cn=service_account,ou=services,dc=corp,dc=example,dc=com"
  bind_password: "..."  # Store as environment variable in production
  user_search_filter: "(uid={username})"
  group_search_filter: "(memberUid={username})"
  tls_mode: "ldaps"  # Required in production
```

### TLS modes

| Mode | Description | Production use |
|------|-------------|---------------|
| `ldaps` | TLS from the start (port 636) | ✅ Yes — recommended |
| `starttls` | STARTTLS upgrade on port 389 | ✅ Yes |
| `none` | Plaintext | ❌ Rejected in production builds |

> **Plaintext LDAP connections are rejected in production builds.** The `none` TLS mode only works in test builds.

### TLS certificate verification

TLS certificate verification is always enforced in production. The `tls_insecure` option (which disables verification) is rejected in non-test builds.

---

## SAML 2.0 Authentication

SAML 2.0 is used for enterprise SSO integration.

### Configuration

```yaml
saml:
  entity_id: "https://git.example.com"
  sso_url: "https://idp.example.com/sso"
  slo_url: "https://idp.example.com/slo"
  certificate: "..."  # IdP signing certificate (PEM)
```

### Security features

- **NotBefore/NotOnOrAfter validation**: Assertion timestamps are verified
- **InResponseTo validation**: Responses are matched to outstanding requests
- **Destination validation**: Assertion Consumer Service URL is verified
- **XML signature verification**: Responses are validated against the IdP certificate

---

## Session Management

### Session tokens

After OAuth2/LDAP/SAML authentication, a session token (UUID v4) is issued:

- Tokens are random 128-bit values (UUID v4)
- Sessions expire after a configurable duration (default: 24 hours)
- Sessions track last activity time for idle timeout

### Session storage

Sessions are stored server-side in an in-memory store by default. For multi-node deployments, Redis-backed session storage is supported via the `SessionStore` trait.

### Session API

```bash
# Authenticate and get a session token
curl -X POST \
  https://git.example.com/auth/callback \
  -d "code=...&state=..."

# Use session token for API calls
curl -H "Authorization: Bearer $SESSION_TOKEN" \
  https://git.example.com/api/v1/repositories
```

---

## Security Considerations

### Audit logging

All authentication events are logged with:
- Username
- Source IP address
- Key fingerprint (SSH) / provider (OAuth)
- Success or failure
- Timestamp

### Failed authentication handling

- Failed SSH attempts are logged and rate-limited
- The server does not distinguish between "user not found" and "wrong key" (prevents enumeration)
- Rate limiting is per-IP and per-username

### Credential storage

- SSH keys are stored in git refs, not flat files
- OAuth2 client secrets are stored in git refs (encrypted at rest via AGE)
- Session tokens are stored server-side only, never sent to third parties
- LFS JWT tokens are short-lived and operation-scoped

### Error messages

Authentication error messages are generic to prevent information leakage:
- "Authentication required" (not "user not found" or "wrong password")
- "Internal error" for server-side errors (details logged server-side only)

---

## Further Reading

- [Getting Started](getting-started.md) — SSH key setup walkthrough
- [User Guide](user-guide.md) — Daily usage guide
- [Security Model](../SECURITY.md) — Detailed security architecture
- [Configuration Reference](configuration.md) — All configuration options
- [Admin Guide](admin-guide.md) — User and access management

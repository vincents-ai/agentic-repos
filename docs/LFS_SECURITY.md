# Git LFS Security Model and Authentication Flow

Complete security documentation for Git LFS implementation.

## Table of Contents

1. [Security Overview](#security-overview)
2. [Authentication Flow](#authentication-flow)
3. [JWT Token Security](#jwt-token-security)
4. [Transport Security](#transport-security)
5. [Authorization Model](#authorization-model)
6. [Threat Model](#threat-model)
7. [Security Best Practices](#security-best-practices)
8. [Hardening Checklist](#hardening-checklist)
9. [Security Monitoring](#security-monitoring)
10. [Incident Response](#incident-response)

## Security Overview

### Architecture

The Git LFS implementation uses a two-phase authentication model:

```
┌─────────────┐
│ Git Client  │
└──────┬──────┘
       │ SSH (Public Key Auth)
       ├─────────────────────────────┐
       │                             │
       ▼                             ▼
┌─────────────────┐          ┌─────────────────┐
│  SSH Server     │          │  HTTP API       │
│  Port 2222      │          │  Port 8090      │
│                 │          │                 │
│ • Git commands  │          │ • LFS Batch API │
│ • Repository    │          │ • Upload/Download│
│   access        │          │                 │
│ • LFS auth      │          │ JWT Auth        │
│   token gen     │          │ Required        │
└─────────────────┘          └─────────────────┘
       │                             │
       └─────────────┬───────────────┘
                     ▼
              ┌─────────────┐
              │   Storage   │
              │  Filesystem │
              └─────────────┘
```

### Security Layers

1. **Transport Layer**: TLS/SSH encryption
2. **Authentication Layer**: SSH public keys + JWT tokens
3. **Authorization Layer**: Repository access control (tenant/repo isolation)
4. **Object Integrity**: SHA256 verification
5. **Storage Layer**: Filesystem permissions

## Authentication Flow

### Phase 1: SSH Authentication for Token Generation

```mermaid
sequenceDiagram
    participant Client as Git LFS Client
    participant SSH as SSH Server
    participant Auth as LFS Auth Module
    
    Client->>SSH: SSH connection (public key)
    SSH->>SSH: Verify SSH key
    SSH-->>Client: Connection established
    
    Client->>SSH: git-lfs-authenticate org/repo upload
    SSH->>Auth: Generate JWT token
    Auth->>Auth: Create token claims:<br/>- tenant: org<br/>- repo: repo<br/>- operation: upload<br/>- expiry: now + 1h
    Auth->>Auth: Sign with JWT secret
    Auth-->>SSH: JWT token + endpoint URL
    SSH-->>Client: {<br/>  "header": {"Authorization": "Bearer TOKEN"},<br/>  "href": "http://server:8090/lfs/org/repo"<br/>}
```

**Command:**
```bash
ssh git@server.com git-lfs-authenticate org/repo upload
```

**Response:**
```json
{
  "header": {
    "Authorization": "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
  },
  "href": "https://server.com/lfs/org/repo",
  "expires_in": 3600
}
```

### Phase 2: HTTP API Access with JWT

```mermaid
sequenceDiagram
    participant Client as Git LFS Client
    participant API as HTTP API Server
    participant Auth as JWT Validator
    participant Storage as Object Storage
    
    Client->>API: POST /lfs/org/repo/info/lfs/objects/batch<br/>Authorization: Bearer TOKEN
    API->>Auth: Validate JWT token
    Auth->>Auth: Verify signature
    Auth->>Auth: Check expiration
    Auth->>Auth: Extract claims
    Auth-->>API: Valid (tenant: org, repo: repo, op: upload)
    API->>API: Verify tenant/repo match URL
    API->>Storage: Generate upload URLs
    Storage-->>API: Signed URLs
    API-->>Client: Batch response with actions
    
    Client->>API: PUT /lfs/org/repo/lfs/objects/OID<br/>Authorization: Bearer TOKEN<br/>[binary data]
    API->>Auth: Validate JWT token
    Auth-->>API: Valid
    API->>API: Verify SHA256
    API->>Storage: Store object
    Storage-->>API: Success
    API-->>Client: 200 OK
```

### Phase 3: Download Flow

```mermaid
sequenceDiagram
    participant Client as Git LFS Client
    participant SSH as SSH Server
    participant API as HTTP API Server
    participant Storage as Object Storage
    
    Client->>SSH: git-lfs-authenticate org/repo download
    SSH-->>Client: JWT token (download operation)
    
    Client->>API: POST /lfs/org/repo/info/lfs/objects/batch<br/>operation: download<br/>Authorization: Bearer TOKEN
    API-->>Client: Batch response with download URLs
    
    Client->>API: GET /lfs/org/repo/lfs/objects/OID<br/>Authorization: Bearer TOKEN
    API->>Storage: Retrieve object
    Storage-->>API: Object data
    API-->>Client: 200 OK [binary data]
```

## JWT Token Security

### Token Structure

```json
{
  "header": {
    "alg": "HS256",
    "typ": "JWT"
  },
  "payload": {
    "tenant": "org",
    "repo": "repo",
    "operation": "upload",
    "exp": 1737587200,
    "iat": 1737583600
  },
  "signature": "HMACSHA256(base64UrlEncode(header) + '.' + base64UrlEncode(payload), secret)"
}
```

### Claims

| Claim | Description | Validation |
|-------|-------------|------------|
| `tenant` | Organization/tenant identifier | Must match URL path |
| `repo` | Repository name | Must match URL path |
| `operation` | `upload` or `download` | Must match HTTP endpoint |
| `exp` | Expiration timestamp (Unix) | Must be in future |
| `iat` | Issued at timestamp (Unix) | Optional verification |

### Token Generation

**Location**: `src/lfs/auth.rs:15-45`

```rust
pub fn generate_token(
    tenant: &str,
    repo: &str,
    operation: LfsOperation,
    secret: &str,
    expiration_secs: u64,
) -> Result<String, AuthError> {
    let claims = Claims {
        tenant: tenant.to_string(),
        repo: repo.to_string(),
        operation,
        exp: (SystemTime::now() + Duration::from_secs(expiration_secs))
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize,
        iat: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|e| AuthError::TokenGenerationError(e.to_string()))
}
```

### Token Validation

**Location**: `src/lfs/auth.rs:47-100`

```rust
pub fn validate_token(
    token: &str,
    tenant: &str,
    repo: &str,
    expected_operation: LfsOperation,
    secret: &str,
) -> Result<Claims, AuthError> {
    // 1. Decode and verify signature
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::default(),
    )
    .map_err(|e| AuthError::TokenValidationError(e.to_string()))?;

    let claims = token_data.claims;

    // 2. Verify expiration (handled by jsonwebtoken crate)

    // 3. Verify claims match request
    if claims.tenant != tenant {
        return Err(AuthError::InvalidClaims(format!(
            "Tenant mismatch: expected {}, got {}",
            tenant, claims.tenant
        )));
    }

    if claims.repo != repo {
        return Err(AuthError::InvalidClaims(format!(
            "Repository mismatch: expected {}, got {}",
            repo, claims.repo
        )));
    }

    if claims.operation != expected_operation {
        return Err(AuthError::InvalidClaims(format!(
            "Operation mismatch: expected {:?}, got {:?}",
            expected_operation, claims.operation
        )));
    }

    Ok(claims)
}
```

### Token Lifetime

**Default**: 3600 seconds (1 hour)

**Configuration**:
```yaml
lfs:
  token_expiration_secs: 3600
```

**Rationale**:
- Long enough for large file transfers
- Short enough to limit exposure if compromised
- Clients automatically request new tokens

### Secret Management

**CRITICAL: JWT secret must be kept secure**

**Generation**:
```bash
# Generate cryptographically secure secret (32+ bytes recommended)
openssl rand -base64 32
```

**Storage**:
```yaml
# config.yaml - NEVER commit to version control
lfs:
  jwt_secret: "your-generated-secret-here"
```

**Environment variables** (preferred for production):
```bash
export LFS_JWT_SECRET="$(openssl rand -base64 32)"
```

**Rotation**:
1. Generate new secret
2. Update server configuration
3. Restart server
4. Old tokens immediately invalid
5. Clients will request new tokens automatically

## Transport Security

### SSH Transport

**Protocol**: SSH-2
**Port**: 2222 (configurable)
**Encryption**: AES256-CTR, ChaCha20-Poly1305
**Key Exchange**: Curve25519, ECDH

**Configuration**:
```yaml
ssh_server:
  host: "0.0.0.0"
  port: 2222
  host_key_path: "server_key"
```

**Security considerations**:
- Use strong SSH host keys (ED25519 or RSA 4096)
- Disable password authentication (key-only)
- Monitor failed authentication attempts

### HTTPS Transport

**Protocol**: TLS 1.2+ (TLS 1.3 recommended)
**Port**: 8090 (configurable, typically behind reverse proxy)

**Recommended reverse proxy configuration (Nginx)**:
```nginx
server {
    listen 443 ssl http2;
    server_name git.example.com;

    ssl_certificate /etc/ssl/certs/git.example.com.crt;
    ssl_certificate_key /etc/ssl/private/git.example.com.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers 'ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256';
    ssl_prefer_server_ciphers on;

    location /lfs/ {
        proxy_pass http://127.0.0.1:8090;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # Important for large file uploads
        client_max_body_size 0;
        proxy_request_buffering off;
    }
}
```

## Authorization Model

### Tenant Isolation

**Multi-tenancy enforcement**:
1. JWT claims include tenant/repo identifiers
2. URL path must match JWT claims
3. Storage paths segregated by tenant/repo

**URL format**: `/lfs/{tenant}/{repo}/...`

**Validation**:
```rust
// Extract from URL
let tenant = path_params.0;
let repo = path_params.1;

// Validate against JWT
if claims.tenant != tenant || claims.repo != repo {
    return Err(AuthError::InvalidClaims("Mismatch"));
}
```

### Repository Access

**Access control enforcement**:
- SSH key determines initial repository access
- JWT token scopes access to specific tenant/repo
- No cross-repository access possible with single token

**Example**:
```bash
# Get token for org1/repo1
ssh git@server.com git-lfs-authenticate org1/repo1 upload
# Token ONLY works for org1/repo1

# Cannot use token for org1/repo2
# Cannot use token for org2/repo1
```

### Operation Scoping

**Operations**:
- `upload`: Allows PUT operations (write)
- `download`: Allows GET operations (read)

**Enforcement**:
```rust
match expected_operation {
    LfsOperation::Upload => {
        // Only batch API with upload + PUT allowed
    }
    LfsOperation::Download => {
        // Only batch API with download + GET allowed
    }
}
```

**No privilege escalation**:
- Download token cannot be used for upload
- Upload token scoped to specific repository
- Token cannot be reused for different operations

## Threat Model

### Threats and Mitigations

| Threat | Risk Level | Mitigation |
|--------|-----------|------------|
| Token theft | High | Short token lifetime, HTTPS required |
| Replay attacks | Medium | Expiration enforcement, HTTPS |
| MITM attacks | High | TLS/SSH encryption, certificate validation |
| Unauthorized access | High | JWT validation, SSH key auth |
| Cross-tenant access | High | Tenant isolation in JWT claims |
| Object tampering | Medium | SHA256 verification |
| Storage exhaustion | Medium | File size limits, quotas (future) |
| Secret compromise | Critical | Secret rotation, secure storage |
| Brute force | Low | SSH key auth (no passwords) |

### Attack Scenarios

**Scenario 1: Stolen JWT Token**

**Attack**: Attacker obtains JWT token from network traffic or client.

**Mitigations**:
1. Token expires after 1 hour
2. Token scoped to specific tenant/repo
3. HTTPS required (prevents network sniffing)
4. Cannot be used beyond original scope

**Scenario 2: Repository Confusion**

**Attack**: Attacker tries to use token for different repository.

**Mitigations**:
1. JWT claims validated against URL path
2. Mismatch returns 403 Forbidden
3. No cross-repository access possible

**Scenario 3: Operation Escalation**

**Attack**: Attacker with download token tries to upload.

**Mitigations**:
1. Operation claim validated in JWT
2. Download token rejected for upload endpoints
3. Separate tokens required for each operation

**Scenario 4: JWT Secret Compromise**

**Attack**: Attacker gains access to JWT secret, can forge tokens.

**Impact**: **CRITICAL** - Attacker can access all repositories

**Mitigations**:
1. Secure secret storage (environment variables, secrets management)
2. Regular secret rotation
3. Monitor for unusual token patterns
4. Restrict server access

**Response**:
1. Immediately rotate JWT secret
2. Audit access logs for suspicious activity
3. Revoke all active tokens (automatic with rotation)

## Security Best Practices

### Server Configuration

**1. Use strong JWT secret:**
```bash
# 32+ bytes, cryptographically random
openssl rand -base64 32 > /secure/path/jwt_secret
chmod 600 /secure/path/jwt_secret
```

**2. Configure proper TLS:**
```yaml
# Use reverse proxy (Nginx/Caddy) for TLS termination
lfs:
  base_url: "https://git.example.com"  # HTTPS, not HTTP
```

**3. Restrict network access:**
```bash
# Firewall rules
sudo ufw allow 443/tcp   # HTTPS only
sudo ufw allow 2222/tcp  # SSH
sudo ufw deny 8090/tcp   # Block direct API access
```

**4. Set appropriate file size limits:**
```yaml
lfs:
  max_object_size: 5368709120  # 5GB - adjust based on needs
```

**5. Use dedicated storage with appropriate permissions:**
```bash
mkdir -p /var/git-lfs-storage
chown git-user:git-group /var/git-lfs-storage
chmod 750 /var/git-lfs-storage
```

### Client Configuration

**1. Always use HTTPS:**
```bash
git config lfs.url "https://server.com/lfs/org/repo"
# NOT: http://server.com/...
```

**2. Verify TLS certificates:**
```bash
# Do NOT disable verification in production
# git config http.sslVerify false  # DANGEROUS

# Instead, add custom CA if needed
git config http.sslCAInfo /path/to/ca-cert.pem
```

**3. Use SSH agent for key management:**
```bash
ssh-add ~/.ssh/id_ed25519
# Prevents key passphrase prompts
```

### Operational Security

**1. Regular secret rotation:**
```bash
# Quarterly or after any suspected compromise
NEW_SECRET=$(openssl rand -base64 32)
# Update config
# Restart server
```

**2. Monitor access logs:**
```bash
# Watch for:
# - Repeated 403 Forbidden (unauthorized attempts)
# - Unusual access patterns
# - Token validation failures
tail -f /var/log/agentic-repos/access.log | grep "403\|401"
```

**3. Audit SSH keys:**
```bash
# Regularly review authorized_keys
cat ~/.ssh/authorized_keys
# Remove old/unused keys
```

**4. Backup verification:**
```bash
# Ensure LFS objects included in backups
tar -czf backup.tar.gz /var/git-lfs-storage
# Verify backup integrity
```

## Hardening Checklist

### Initial Setup

- [ ] Generate strong JWT secret (32+ bytes)
- [ ] Store JWT secret securely (not in version control)
- [ ] Configure HTTPS with valid TLS certificate
- [ ] Set up reverse proxy (Nginx/Caddy)
- [ ] Configure firewall rules
- [ ] Set appropriate file size limits
- [ ] Create dedicated storage directory with proper permissions
- [ ] Use strong SSH host keys (ED25519 or RSA 4096)

### Configuration

- [ ] Disable HTTP (HTTPS only)
- [ ] Set reasonable token expiration (3600s recommended)
- [ ] Configure proper `base_url` (HTTPS)
- [ ] Enable logging
- [ ] Set storage path permissions
- [ ] Configure reverse proxy headers
- [ ] Enable rate limiting (future feature)

### Operational

- [ ] Rotate JWT secret quarterly
- [ ] Monitor access logs
- [ ] Audit SSH keys monthly
- [ ] Review storage quotas
- [ ] Test backup/restore procedures
- [ ] Update server regularly
- [ ] Review security advisories

### Network

- [ ] TLS 1.2+ configured
- [ ] Strong cipher suites only
- [ ] HSTS enabled (reverse proxy)
- [ ] Firewall configured
- [ ] Rate limiting configured (future)

## Security Monitoring

### Logging

**Enable comprehensive logging:**
```yaml
# Future feature
logging:
  level: info
  lfs_access: true
  lfs_auth: true
```

**Log entries to monitor:**
```
[INFO] LFS auth: Generated token for org/repo (operation: upload)
[WARN] LFS auth: Token validation failed - expired token
[ERROR] LFS auth: Token validation failed - invalid signature
[ERROR] LFS batch: Access denied for org/repo - claims mismatch
```

### Metrics

**Key metrics to track** (future feature):
- Authentication failures (401/403 responses)
- Token validation failures
- Upload/download bandwidth
- Storage utilization
- Request latency

### Alerts

**Configure alerts for**:
1. **High authentication failure rate**
   - Threshold: >10 failures/minute
   - Action: Possible brute force or token theft

2. **Token validation errors**
   - Threshold: >5 errors/minute
   - Action: Possible secret compromise or misconfiguration

3. **Storage approaching quota**
   - Threshold: >80% utilized
   - Action: Expand storage or implement cleanup

4. **Unusual access patterns**
   - Large number of requests from single IP
   - Access to many repositories from single token

## Incident Response

### Token Compromise Suspected

**Immediate actions:**
1. **Rotate JWT secret immediately**
   ```bash
   NEW_SECRET=$(openssl rand -base64 32)
   # Update configuration
   # Restart server
   ```

2. **All tokens invalidated automatically** (secret rotation invalidates all)

3. **Audit access logs**
   ```bash
   grep "LFS" /var/log/agentic-repos/access.log | grep -E "$(date +%Y-%m-%d)"
   ```

4. **Identify affected repositories**
   - Check for unusual uploads/downloads
   - Verify object integrity

5. **Notify affected users**
   - Inform of potential compromise
   - Request re-authentication

### SSH Key Compromise

**Immediate actions:**
1. **Remove compromised key from authorized_keys**
   ```bash
   vim ~/.ssh/authorized_keys
   # Remove suspect key
   ```

2. **Review access logs for key usage**
   ```bash
   grep "Accepted publickey" /var/log/auth.log
   ```

3. **Audit repository access**
   - Check which repositories were accessed
   - Verify no unauthorized changes

4. **User provides new key**

### Unauthorized Access Detected

**Investigation steps:**
1. **Identify access pattern**
   ```bash
   grep "403\|401" /var/log/agentic-repos/access.log
   ```

2. **Determine scope**
   - Which repositories targeted?
   - What operations attempted?
   - Source IP addresses?

3. **Block malicious IPs** (if pattern identified)
   ```bash
   sudo ufw deny from ATTACKER_IP
   ```

4. **Review authentication logs**

5. **Assess impact**
   - Was data accessed?
   - Were objects modified?

### Data Integrity Concerns

**Verification:**
1. **Verify object SHA256 hashes**
   ```bash
   # Check object integrity
   for obj in /var/git-lfs-storage/org/repo.git/lfs/objects/*/*/*/*; do
       oid=$(basename $obj)
       computed=$(sha256sum $obj | cut -d' ' -f1)
       if [ "$oid" != "$computed" ]; then
           echo "INTEGRITY FAILURE: $obj"
       fi
   done
   ```

2. **Restore from backup if tampering detected**

3. **Investigate how tampering occurred**

## Related Documentation

- [LFS Configuration](LFS_CONFIGURATION.md) - Server and client configuration
- [LFS API Reference](LFS_API.md) - API endpoint documentation
- [LFS Troubleshooting](LFS_TROUBLESHOOTING.md) - Common issues and solutions
- [LFS Admin Guide](LFS_ADMIN_GUIDE.md) - Administrative operations

## Security Contacts

**Report security vulnerabilities**:
- Email: security@your-domain.com
- PGP key: [link to public key]
- Response time: 24-48 hours

**For urgent security issues**: Contact server administrator directly.

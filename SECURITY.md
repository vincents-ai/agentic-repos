# Security Advisory

## Critical Security Vulnerabilities Found

### 🚨 CRITICAL: SSH Authentication Bypass

**File**: `src/git/ssh.rs:107`  
**Severity**: CRITICAL  
**CVE**: Pending  

**Issue**: The SSH authentication handler accepts ANY SSH public key without validation:

```rust
async fn auth_publickey(
    &mut self,
    _user: &str,
    _public_key: &PublicKey,
) -> std::result::Result<russh::server::Auth, Self::Error> {
    debug!("Public key authentication for user: {}", _user);
    Ok(russh::server::Auth::Accept)  // ← ACCEPTS ANY KEY
}
```

**Impact**: 
- Complete authentication bypass
- Any user with any SSH key can access the Git server
- No authorization controls whatsoever
- Full repository access for any attacker with SSH key generation capability

**Risk Assessment**: 
- **Confidentiality**: HIGH - Unauthorized access to all repositories
- **Integrity**: HIGH - Unauthorized push/commit access to all repositories  
- **Availability**: MEDIUM - Potential for malicious repository operations

**Immediate Mitigation**:
- Do NOT deploy to production with current SSH implementation
- Disable SSH module until proper authentication is implemented
- Use alternative Git access methods (if available)

### ⚠️ HIGH: Command Injection Vulnerability

**File**: `src/git/ssh.rs:127-146`  
**Severity**: HIGH  

**Issue**: SSH command execution accepts raw input without validation:

```rust
async fn exec_request(
    &mut self,
    channel: ChannelId,
    data: &[u8],
    session: &mut Session,
) -> std::result::Result<(), Self::Error> {
    let command = String::from_utf8_lossy(data);  // ← Raw command input
    debug!("Executing command: {}", command);
    
    match command.trim() {
        cmd if cmd.starts_with("git-upload-pack") => {
            // Direct string matching without validation
        }
        // ...
    }
}
```

**Impact**:
- Potential command injection through Git command parameters
- Malicious repository names could be crafted to inject commands
- Server-side command execution risks

**Risk Assessment**:
- **Confidentiality**: MEDIUM - File system access potential
- **Integrity**: HIGH - Potential system modification
- **Availability**: HIGH - Potential denial of service

### ⚠️ MEDIUM: Insecure Host Key Management

**File**: `src/git/ssh.rs:33-34`  
**Severity**: MEDIUM  

**Issue**: SSH host keys generated randomly on every startup:

```rust
let keys = vec![PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?];
```

**Impact**:
- No persistent server identity
- Man-in-the-middle attack vulnerability
- Client SSH fingerprint verification fails
- No host key continuity across restarts

### ⚠️ MEDIUM: Missing Rate Limiting

**Issue**: SSH connections have no rate limiting or connection controls

**Impact**:
- Denial of service vulnerability
- Brute force attack potential
- Resource exhaustion

## Required Security Fixes

### 1. Implement Authorized Keys Authentication

```rust
// Example secure implementation
async fn auth_publickey(
    &mut self,
    user: &str,
    public_key: &PublicKey,
) -> std::result::Result<russh::server::Auth, Self::Error> {
    if self.validate_authorized_key(user, public_key).await {
        Ok(russh::server::Auth::Accept)
    } else {
        warn!("SSH authentication failed for user: {}", user);
        Ok(russh::server::Auth::Reject)
    }
}
```

### 2. Add Command Input Validation

```rust
// Example secure command validation
fn validate_git_command(command: &str) -> Result<GitCommand, SecurityError> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    
    match parts.get(0) {
        Some("git-upload-pack") => {
            let repo = validate_repository_name(parts.get(1)?)?;
            Ok(GitCommand::Upload(repo))
        }
        Some("git-receive-pack") => {
            let repo = validate_repository_name(parts.get(1)?)?;
            Ok(GitCommand::Receive(repo))
        }
        _ => Err(SecurityError::InvalidCommand),
    }
}
```

### 3. Implement Persistent Host Keys

```rust
// Load persistent host key or generate and save new one
let host_key = match load_host_key(&config.host_key_path) {
    Ok(key) => key,
    Err(_) => {
        let new_key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)?;
        save_host_key(&config.host_key_path, &new_key)?;
        new_key
    }
};
```

### 4. Add Rate Limiting and Connection Controls

```rust
// Example rate limiting implementation
struct ConnectionLimiter {
    connections: Arc<Mutex<HashMap<IpAddr, ConnectionCount>>>,
    max_per_ip: usize,
    time_window: Duration,
}
```

## Security Testing

Current VM test framework validates functionality but does NOT test security:

- ✅ VM tests validate SSH connectivity  
- ❌ VM tests do NOT validate authentication rejection
- ❌ VM tests do NOT test authorization controls
- ❌ VM tests do NOT test command injection resistance

**Required Security Tests**:
1. Authentication rejection tests
2. Invalid key rejection tests  
3. Command injection attack tests
4. Rate limiting tests
5. Host key persistence tests

## Development Guidelines

### Before Production Deployment

1. ✅ Implement authorized keys validation
2. ✅ Add command input sanitization  
3. ✅ Implement persistent host keys
4. ✅ Add rate limiting and connection controls
5. ✅ Add comprehensive security testing
6. ✅ Security audit by external party
7. ✅ Penetration testing

### Secure Development Practices

- **Never deploy** with `Auth::Accept` for any key
- **Always validate** user input in SSH commands
- **Use persistent** host keys for server identity
- **Implement logging** for all authentication attempts
- **Add monitoring** for suspicious activity patterns

## Responsible Disclosure

If you discover additional security vulnerabilities in this project:

1. Do NOT create public issues on GitHub
2. Email security issues to: [SECURITY_EMAIL_PLACEHOLDER]
3. Provide detailed reproduction steps
4. Allow 90 days for fix development before public disclosure

## Security Contact

For security-related questions or concerns:
- Email: [SECURITY_EMAIL_PLACEHOLDER]
- PGP Key: [PGP_KEY_PLACEHOLDER]

---

**Last Updated**: 2026-01-21  
**Next Review**: 2026-02-21
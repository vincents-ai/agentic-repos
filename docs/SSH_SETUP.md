# SSH Setup Guide for agentic-repos

## Overview

agentic-repos provides SSH-based Git access for secure repository operations. This guide covers SSH setup, key management, and testing.

## Prerequisites

- OpenSSH client installed
- SSH key pair generated
- Port 2222 available (or custom port)

## SSH Configuration

### 1. Enable SSH in agentic-repos

**Via Environment Variables:**
```bash
export REPOS_SSH_ENABLED=true
export REPOS_SSH_PORT=2222
export REPOS_SSH_HOST_KEY_PATH="./data/ssh/host_key"
export REPOS_SSH_AUTHORIZED_KEYS_PATH="./data/ssh/authorized_keys"
```

**Via config.yaml:**
```yaml
plugins:
  - name: git-operations
    path: ./plugins/libagentic_repos.so
    config:
      ssh.enabled: true
      ssh.port: 2222
      ssh.host_key_path: "./data/ssh/host_key"
      ssh.authorized_keys_path: "./data/ssh/authorized_keys"
```

### 2. Generate SSH Host Key

```bash
# Create SSH directory
mkdir -p ./data/ssh

# Generate host key
ssh-keygen -t rsa -b 4096 -f ./data/ssh/host_key -N ""
chmod 600 ./data/ssh/host_key
```

### 3. Generate User SSH Keys

```bash
# Generate key pair for each user
ssh-keygen -t rsa -b 2048 -f ~/.ssh/agentic_repos_user -C "user@example.com"

# This creates:
# ~/.ssh/agentic_repos_user       (private key)
# ~/.ssh/agentic_repos_user.pub   (public key)
```

### 4. Authorize User Keys

```bash
# Add public key to authorized_keys
cat ~/.ssh/agentic_repos_user.pub >> ./data/ssh/authorized_keys

# Set permissions
chmod 600 ./data/ssh/authorized_keys
```

### 5. Configure SSH Client

Create or update `~/.ssh/config`:

```
Host agentic-repos
    HostName localhost
    Port 2222
    User git
    IdentityFile ~/.ssh/agentic_repos_user
    StrictHostKeyChecking no
```

## Testing SSH Access

### Test 1: Verify SSH Server is Running

```bash
# Check if SSH server is listening
netstat -tuln | grep 2222

# Or
lsof -i :2222

# Expected output:
# tcp  0  0  0.0.0.0:2222  LISTEN
```

### Test 2: Test SSH Connection

```bash
# Test basic SSH connectivity
ssh -T git@localhost -p 2222

# Expected: Welcome message or prompt for repository
```

### Test 3: Clone Repository via SSH

```bash
# Create test repository first
~/code/skylet/target/release/plugin-test-harness execute \
  -p ./target/release/libagentic_repos.so \
  -a repo.create \
  --args '{"id":"1","method":"repo.create","params":{"name":"test-repo","tenant_id":"test-tenant"}}'

# Clone via SSH
git clone ssh://git@localhost:2222/test-tenant/test-repo.git

# Or with SSH config
git clone agentic-repos:test-tenant/test-repo.git
```

### Test 4: Push to Repository

```bash
cd test-repo

# Create test file
echo "# Test Repository" > README.md
git add README.md
git commit -m "Initial commit"

# Push to SSH server
git push origin master
```

### Test 5: Pull from Repository

```bash
# In another location
git clone ssh://git@localhost:2222/test-tenant/test-repo.git test-repo-clone
cd test-repo-clone

# Verify content
cat README.md

# Make changes and push
echo "Test content" >> README.md
git add README.md
git commit -m "Update README"
git push origin master

# Pull in original repo
cd ../test-repo
git pull origin master
```

## SSH URL Format

### Standard Format
```
ssh://git@host:port/tenant-id/repo-name.git
```

### Examples
```bash
# Clone
git clone ssh://git@localhost:2222/my-company/my-project.git

# Push
git push ssh://git@localhost:2222/my-company/my-project.git master

# Pull
git pull ssh://git@localhost:2222/my-company/my-project.git master
```

### With SSH Config
```bash
# After adding Host to ~/.ssh/config
git clone agentic-repos:my-company/my-project.git
```

## Multi-Tenant SSH Access

Each tenant has isolated repository access:

```bash
# Tenant 1
git clone ssh://git@localhost:2222/tenant-1/project.git

# Tenant 2
git clone ssh://git@localhost:2222/tenant-2/project.git
```

Tenants cannot access each other's repositories.

## SSH Key Management

### Adding Multiple Users

```bash
# User 1
ssh-keygen -t rsa -b 2048 -f ~/.ssh/user1_key -C "user1@example.com"
cat ~/.ssh/user1_key.pub >> ./data/ssh/authorized_keys

# User 2
ssh-keygen -t rsa -b 2048 -f ~/.ssh/user2_key -C "user2@example.com"
cat ~/.ssh/user2_key.pub >> ./data/ssh/authorized_keys
```

### Per-Tenant Keys (Advanced)

Create separate authorized_keys files per tenant:

```bash
# Tenant-specific authorization
mkdir -p ./data/ssh/tenants/tenant-1
cat user1_key.pub > ./data/ssh/tenants/tenant-1/authorized_keys
```

Configure in agentic-repos:
```yaml
ssh.authorized_keys_path: "./data/ssh/tenants/{tenant_id}/authorized_keys"
```

## Security Best Practices

### 1. Use Strong Keys
```bash
# RSA 4096-bit
ssh-keygen -t rsa -b 4096 -f ~/.ssh/agentic_repos_key

# Or Ed25519 (recommended)
ssh-keygen -t ed25519 -f ~/.ssh/agentic_repos_key
```

### 2. Protect Private Keys
```bash
chmod 600 ~/.ssh/agentic_repos_user
chmod 700 ~/.ssh/
```

### 3. Use Key Passphrases
```bash
# Add passphrase when generating key
ssh-keygen -t rsa -b 4096 -f ~/.ssh/agentic_repos_key
# Enter passphrase when prompted
```

### 4. Restrict Host Key Access
```bash
chmod 600 ./data/ssh/host_key
chown skylet:skylet ./data/ssh/host_key
```

### 5. Firewall Rules
```bash
# Allow SSH port
ufw allow 2222/tcp

# Or iptables
iptables -A INPUT -p tcp --dport 2222 -j ACCEPT
```

## Troubleshooting

### SSH Connection Refused

**Symptoms**: `ssh: connect to host localhost port 2222: Connection refused`

**Solutions**:
1. Verify SSH is enabled: `echo $REPOS_SSH_ENABLED` should be `true`
2. Check server is running: `ps aux | grep agentic_repos`
3. Verify port: `netstat -tuln | grep 2222`
4. Check logs: `tail -f skylet.log | grep SSH`

### Permission Denied (publickey)

**Symptoms**: `Permission denied (publickey)`

**Solutions**:
1. Verify public key in authorized_keys: `cat ./data/ssh/authorized_keys`
2. Check private key permissions: `chmod 600 ~/.ssh/agentic_repos_user`
3. Verify SSH config points to correct key
4. Test with verbose: `ssh -vvv git@localhost -p 2222`

### Repository Not Found

**Symptoms**: `fatal: repository 'tenant/repo' not found`

**Solutions**:
1. Verify repository exists via plugin
2. Check tenant ID matches
3. Verify repository name is correct
4. Check storage path permissions

### Git Push Fails

**Symptoms**: `remote: error: insufficient permission`

**Solutions**:
1. Check user has write access
2. Verify repository is not in read-only mode
3. Check storage backend permissions
4. Review SSH key authorization

## Advanced Configuration

### Custom SSH Port

```bash
export REPOS_SSH_PORT=22022
```

### SSH Connection Limits

```yaml
ssh:
  enabled: true
  port: 2222
  max_connections: 100
  connection_timeout_secs: 30
```

### SSH Banner

Create custom welcome message:
```bash
echo "Welcome to agentic-repos Git Server" > ./data/ssh/banner.txt
```

Configure:
```yaml
ssh:
  banner_path: "./data/ssh/banner.txt"
```

### SSH Logging

Enable verbose SSH logging:
```bash
export REPOS_LOG_LEVEL="debug"
```

View SSH logs:
```bash
tail -f /var/log/auth.log | grep ssh
```

## Integration Test Script

Use the included test script:

```bash
./test-ssh-integration.sh
```

This tests:
- SSH server startup
- Key generation
- Repository creation
- Clone via SSH
- Push via SSH
- Pull via SSH

## Next Steps

1. ✅ SSH enabled in packaged version
2. ✅ SSH integration test created
3. Configure for production:
   - Generate production SSH keys
   - Set up user authentication
   - Configure firewall
   - Enable logging
4. Test with actual Skylet runtime

---

**Note**: SSH access is **REQUIRED** for git operations. The plugin will not function correctly without SSH enabled.

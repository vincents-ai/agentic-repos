# Git LFS Troubleshooting Guide

Comprehensive troubleshooting guide for common Git LFS issues.

## Table of Contents

1. [Installation Issues](#installation-issues)
2. [Authentication Problems](#authentication-problems)
3. [Upload/Download Errors](#uploaddownload-errors)
4. [Performance Issues](#performance-issues)
5. [Configuration Problems](#configuration-problems)
6. [Migration Issues](#migration-issues)
7. [Storage Issues](#storage-issues)
8. [Network and Connectivity](#network-and-connectivity)
9. [Diagnostic Commands](#diagnostic-commands)
10. [Getting Help](#getting-help)

## Installation Issues

### Problem: `git: 'lfs' is not a git command`

**Symptoms:**
```bash
$ git lfs version
git: 'lfs' is not a git command. See 'git --help'.
```

**Cause:** Git LFS is not installed or not in PATH.

**Solutions:**

**Linux (Debian/Ubuntu):**
```bash
sudo apt-get install git-lfs
git lfs install
```

**macOS:**
```bash
brew install git-lfs
git lfs install
```

**Verify installation:**
```bash
git lfs version
# Should output: git-lfs/3.x.x (GitHub; linux amd64; go 1.x.x)
```

### Problem: `git lfs install` fails with permission error

**Symptoms:**
```bash
$ git lfs install
Git LFS initialized.
Error: could not write to ~/.gitconfig: permission denied
```

**Solutions:**

**Fix file permissions:**
```bash
chmod 644 ~/.gitconfig
git lfs install
```

**Or install per-repository instead:**
```bash
git lfs install --local
```

### Problem: Old Git LFS version causing issues

**Check version:**
```bash
git lfs version
```

**Update to latest:**
```bash
# macOS
brew upgrade git-lfs

# Linux
sudo apt-get update
sudo apt-get install --only-upgrade git-lfs

# Manual install
curl -s https://packagecloud.io/install/repositories/github/git-lfs/script.deb.sh | sudo bash
sudo apt-get install git-lfs
```

## Authentication Problems

### Problem: `Access denied` or `401 Unauthorized`

**Symptoms:**
```bash
$ git push
Uploading LFS objects:   0% (0/1), 0 B | 0 B/s
batch response: Access denied
error: failed to push some refs
```

**Diagnosis:**
```bash
# Check authentication
git lfs logs last

# Test SSH connection
ssh -T git@your-server.com

# Verify credentials
git config credential.helper
```

**Solutions:**

**1. Verify SSH key:**
```bash
ssh-add -l  # List keys
ssh -vT git@your-server.com  # Verbose connection test
```

**2. Check Git credentials:**
```bash
# View stored credentials
git config --list | grep credential

# Re-authenticate
git credential reject
# Next git command will prompt for credentials
```

**3. Verify LFS endpoint authentication:**
```bash
# Enable debug logging
GIT_TRACE=1 GIT_CURL_VERBOSE=1 git lfs push origin main --all
```

**4. Test LFS authentication directly:**
```bash
# Get auth token via SSH
ssh git@your-server.com git-lfs-authenticate org/repo upload

# Should return:
{
  "header": {"Authorization": "Bearer TOKEN"},
  "href": "http://server:8090/lfs/org/repo"
}
```

### Problem: `Token expired` errors

**Symptoms:**
```bash
batch response: Unauthorized - Token expired
```

**Solution:**

Tokens have limited lifetime (default 1 hour). Simply retry:
```bash
git lfs push origin main --all
```

Git will request a new token automatically.

**If problem persists, check server configuration:**
```yaml
lfs:
  token_expiration_secs: 3600  # Increase if needed
```

### Problem: Wrong LFS URL being used

**Symptoms:**
```bash
Error: Repository or object not found
```

**Diagnosis:**
```bash
# Check LFS URL
git lfs env | grep -i endpoint

# Should show:
Endpoint=https://your-server.com/lfs/org/repo
```

**Solutions:**

**1. Configure correct endpoint:**
```bash
# Set custom LFS URL
git config lfs.url "https://your-server.com/lfs/org/repo"
```

**2. Check server configuration:**
```yaml
lfs:
  base_url: "https://your-server.com"  # Must be accessible from clients
```

**3. Verify SSH command returns correct URL:**
```bash
ssh git@your-server.com git-lfs-authenticate org/repo upload
# Check "href" field matches expected URL
```

## Upload/Download Errors

### Problem: `batch response: Repository or object not found`

**Symptoms:**
```bash
batch response: Repository or object not found: org/repo
```

**Solutions:**

**1. Verify repository exists:**
```bash
git ls-remote git@your-server.com:org/repo.git
```

**2. Check repository path format:**
```bash
# Correct format in LFS URL
git config lfs.url "https://server.com/lfs/org/repo"

# NOT:
# https://server.com/lfs/org/repo.git
```

**3. Verify LFS is enabled on server:**
```yaml
features:
  git_lfs: true

lfs:
  enabled: true
```

### Problem: Large file upload fails or times out

**Symptoms:**
```bash
Uploading LFS objects:  50% (1/2), 500 MB | 1.2 MB/s
error: failed to push some refs
```

**Solutions:**

**1. Check file size limits:**
```bash
# Server configuration
lfs:
  max_object_size: 5368709120  # 5GB default

# Client-side check
ls -lh large-file.bin
```

**2. Increase timeout:**
```bash
# Set longer timeout
git config lfs.activitytimeout 300  # 5 minutes
git config lfs.tlstimeout 300

# Retry upload
git lfs push origin main --all
```

**3. Use concurrent transfers:**
```bash
# Increase concurrent uploads (default: 8)
git config lfs.concurrenttransfers 16
```

**4. Check network stability:**
```bash
# Test with smaller files first
git lfs push origin main

# Monitor progress with verbose logging
GIT_CURL_VERBOSE=1 git lfs push origin main --all
```

### Problem: `Object not found` when cloning

**Symptoms:**
```bash
$ git clone git@server.com:org/repo.git
Downloading LFS objects:  50% (1/2), 500 MB | 1.2 MB/s
Error downloading object: large-file.bin
Smudge error: Error downloading
```

**Solutions:**

**1. Verify objects exist on server:**
```bash
# Server-side check (on server machine)
ls -la ./test_repositories/org/repo.git/lfs/objects/

# Or test API directly
curl -H "Authorization: Bearer TOKEN" \
  https://server.com/lfs/org/repo/lfs/objects/OID
```

**2. Retry download:**
```bash
git lfs fetch --all
git lfs pull
```

**3. Clone without LFS, then fetch separately:**
```bash
GIT_LFS_SKIP_SMUDGE=1 git clone git@server.com:org/repo.git
cd repo
git lfs pull
```

**4. Check specific object:**
```bash
# Find OID of missing object
git lfs ls-files -l | grep large-file.bin

# Download specific object
git lfs fetch --include="large-file.bin"
```

## Performance Issues

### Problem: Clone/pull is very slow

**Solutions:**

**1. Use concurrent transfers:**
```bash
git config lfs.concurrenttransfers 16
```

**2. Skip LFS during clone, fetch later:**
```bash
# Clone without LFS objects
GIT_LFS_SKIP_SMUDGE=1 git clone git@server.com:org/repo.git

cd repo

# Fetch only needed LFS objects
git lfs pull --include="assets/**"
```

**3. Use shallow clone:**
```bash
git clone --depth 1 git@server.com:org/repo.git
```

**4. Fetch only recent LFS objects:**
```bash
git lfs fetch --recent
```

### Problem: High disk usage after operations

**Solution: Clean up LFS cache:**
```bash
# Clear local LFS cache
git lfs prune

# More aggressive cleanup
git lfs prune --verify-remote

# Free up space from old objects
git lfs prune --verify-remote --recent 30
```

### Problem: Slow push operations

**Solutions:**

**1. Increase concurrent transfers:**
```bash
git config lfs.concurrenttransfers 16
```

**2. Skip LFS pointer verification:**
```bash
git config lfs.skipdownloadverify true
```

**3. Push only new LFS objects:**
```bash
# Instead of --all
git lfs push origin main
```

## Configuration Problems

### Problem: `.gitattributes` not working

**Symptoms:**
Files not being tracked by LFS despite `.gitattributes` configuration.

**Diagnosis:**
```bash
# Check if file is tracked
git lfs ls-files | grep filename

# Check .gitattributes
cat .gitattributes
```

**Solutions:**

**1. Verify `.gitattributes` syntax:**
```bash
# Correct format
*.psd filter=lfs diff=lfs merge=lfs -text
data/** filter=lfs diff=lfs merge=lfs -text
```

**2. Re-add files after configuring tracking:**
```bash
git rm --cached *.psd
git add *.psd
git commit -m "Move to LFS"
```

**3. Verify tracking configuration:**
```bash
git check-attr filter *.psd
# Should output: *.psd: filter: lfs
```

### Problem: LFS tracking too many files

**Solutions:**

**1. Review `.gitattributes`:**
```bash
cat .gitattributes
```

**2. Use more specific patterns:**
```bash
# Too broad
* filter=lfs diff=lfs merge=lfs -text

# Better - specific patterns
*.psd filter=lfs diff=lfs merge=lfs -text
*.zip filter=lfs diff=lfs merge=lfs -text
```

**3. Untrack specific files:**
```bash
# Edit .gitattributes to remove pattern
git add .gitattributes
git commit -m "Update LFS tracking"

# Re-add files to Git
git rm --cached large-file.txt
git add large-file.txt
git commit -m "Move out of LFS"
```

### Problem: Mixed content (some files in LFS, some not)

**Symptoms:**
Files of same type split between LFS and regular Git.

**Diagnosis:**
```bash
# Find all .psd files in Git
git ls-tree -r HEAD --name-only | grep "\.psd$"

# Find .psd files in LFS
git lfs ls-files | grep "\.psd$"
```

**Solution: Standardize on LFS:**
```bash
# Ensure tracking configured
git lfs track "*.psd"

# Re-add all .psd files
find . -name "*.psd" -exec git rm --cached {} \;
find . -name "*.psd" -exec git add {} \;

git commit -m "Consolidate .psd files in LFS"
```

## Migration Issues

See [LFS_MIGRATION.md](LFS_MIGRATION.md) for detailed migration troubleshooting.

### Problem: `git lfs migrate` fails

**Common errors:**

**1. Ambiguous argument error:**
```bash
# Solution: Specify branch
git lfs migrate import --include="*.psd" --include-ref=refs/heads/main
```

**2. Migration creates duplicate objects:**
```bash
# Solution: Fetch all refs before migration
git fetch --all
git lfs migrate import --everything --include="*.psd"
```

**3. Cannot force push after migration:**
```bash
# Solution: Use force-with-lease
git push origin main --force-with-lease
```

## Storage Issues

### Problem: Server running out of disk space

**Diagnosis:**
```bash
# Check LFS storage size
du -sh ./test_repositories/*/lfs/objects/

# List largest LFS objects
find ./test_repositories -name "lfs" -type d -exec du -sh {} \;
```

**Solutions:**

**1. Configure storage limits:**
```yaml
lfs:
  max_object_size: 2147483648  # 2GB limit
```

**2. Implement storage quotas (future feature):**
```yaml
lfs:
  storage_quotas:
    per_repo: 10737418240  # 10GB per repo
```

**3. Clean up unreferenced objects:**
```bash
# This requires admin access to server
# Future feature: garbage collection
```

### Problem: Client local cache too large

**Solutions:**

**1. Check cache size:**
```bash
du -sh .git/lfs
```

**2. Prune old objects:**
```bash
git lfs prune
```

**3. Clear all cached objects:**
```bash
rm -rf .git/lfs/objects
git lfs fetch --all
```

## Network and Connectivity

### Problem: Connection timeouts

**Solutions:**

**1. Increase timeout values:**
```bash
git config lfs.activitytimeout 300
git config lfs.tlstimeout 300
git config http.lowSpeedLimit 1000
git config http.lowSpeedTime 300
```

**2. Check firewall rules:**
```bash
# Test LFS endpoint connectivity
curl -v https://your-server.com/lfs/org/repo/info/lfs

# Should return 405 Method Not Allowed (batch endpoint requires POST)
```

**3. Use HTTP proxy:**
```bash
git config http.proxy http://proxy.example.com:8080
git config https.proxy https://proxy.example.com:8080
```

### Problem: SSL/TLS certificate errors

**Symptoms:**
```bash
Error: SSL certificate problem: unable to get local issuer certificate
```

**Solutions:**

**1. Install proper CA certificates:**
```bash
# Linux
sudo apt-get install ca-certificates
sudo update-ca-certificates
```

**2. Configure Git to trust certificate:**
```bash
# Add CA cert
git config http.sslCAInfo /path/to/ca-cert.pem

# Or disable verification (NOT RECOMMENDED for production)
git config http.sslVerify false
```

## Diagnostic Commands

### Comprehensive diagnostics

**1. Check LFS environment:**
```bash
git lfs env
```

Expected output includes:
- Git LFS version
- Git version
- Endpoint URLs
- Configuration values
- Local repository paths

**2. View LFS logs:**
```bash
# Last operation
git lfs logs last

# Show all logs
git lfs logs show

# Clear logs
git lfs logs clear
```

**3. Verify LFS files:**
```bash
# List all LFS files
git lfs ls-files

# List with sizes
git lfs ls-files -l

# List with OIDs
git lfs ls-files --long --oid
```

**4. Test connection:**
```bash
# Verbose fetch
GIT_TRACE=1 GIT_CURL_VERBOSE=1 git lfs fetch

# Verbose push
GIT_TRACE=1 GIT_CURL_VERBOSE=1 git lfs push origin main
```

**5. Check server health:**
```bash
# Test SSH
ssh -vT git@your-server.com

# Test LFS authenticate
ssh git@your-server.com git-lfs-authenticate org/repo upload

# Test HTTP endpoint
curl -X POST https://server.com/lfs/org/repo/info/lfs/objects/batch \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -H "Authorization: Bearer TOKEN" \
  -d '{"operation":"download","transfers":["basic"],"objects":[]}'
```

### Debug environment variables

```bash
# Enable all debug output
export GIT_TRACE=1
export GIT_CURL_VERBOSE=1
export GIT_TRACE_PACKET=1
export GIT_TRACE_PERFORMANCE=1

# Run git command
git lfs push origin main --all

# Disable debugging
unset GIT_TRACE GIT_CURL_VERBOSE GIT_TRACE_PACKET GIT_TRACE_PERFORMANCE
```

## Getting Help

### Information to provide

When reporting issues, include:

**1. Environment:**
```bash
git lfs version
git --version
uname -a  # or: systeminfo (Windows)
```

**2. Configuration:**
```bash
git lfs env
git config --list | grep lfs
cat .gitattributes
```

**3. Error logs:**
```bash
git lfs logs last
```

**4. Diagnostic output:**
```bash
GIT_TRACE=1 git lfs [command] 2>&1 | tee lfs-debug.log
```

### Useful resources

- **Server documentation**: [LFS_ENABLING_SERVER.md](LFS_ENABLING_SERVER.md)
- **Client setup**: [LFS_CLIENT_SETUP.md](LFS_CLIENT_SETUP.md)
- **Migration guide**: [LFS_MIGRATION.md](LFS_MIGRATION.md)
- **API reference**: [LFS_API.md](LFS_API.md)
- **Git LFS wiki**: https://github.com/git-lfs/git-lfs/wiki
- **Git LFS spec**: https://github.com/git-lfs/git-lfs/tree/main/docs/spec

## Common Error Messages Reference

| Error | Cause | Solution |
|-------|-------|----------|
| `batch response: Access denied` | Authentication failure | Check SSH keys, verify credentials |
| `Repository or object not found` | Wrong URL or repo doesn't exist | Verify LFS URL configuration |
| `Token expired` | JWT token expired | Retry operation (auto-generates new token) |
| `Object too large` | Exceeds max_object_size | Increase server limit or reduce file size |
| `Error downloading object` | Object missing from server | Verify object exists, retry download |
| `filter error` | LFS not installed | Run `git lfs install` |
| `SSL certificate problem` | Certificate validation failed | Install CA certs or configure trust |
| `connection timeout` | Network issues | Increase timeouts, check connectivity |
| `permission denied` | File permissions | Check file/directory permissions |
| `cannot lock ref` | Concurrent operations | Wait and retry |

## Quick Fixes Summary

**Installation:**
```bash
# Install
sudo apt-get install git-lfs  # Linux
brew install git-lfs           # macOS

# Initialize
git lfs install
```

**Authentication:**
```bash
# Test SSH
ssh -T git@server.com

# Get new token
git lfs push origin main  # Auto-requests new token
```

**Performance:**
```bash
# Speed up transfers
git config lfs.concurrenttransfers 16

# Skip smudge during clone
GIT_LFS_SKIP_SMUDGE=1 git clone ...

# Clean up cache
git lfs prune
```

**Debugging:**
```bash
# Enable verbose logging
GIT_TRACE=1 GIT_CURL_VERBOSE=1 git lfs push origin main --all

# View logs
git lfs logs last

# Check environment
git lfs env
```

**Configuration:**
```bash
# Check tracking
git lfs ls-files

# Re-add files to LFS
git rm --cached file.bin
git add file.bin

# Verify .gitattributes
git check-attr filter file.bin
```

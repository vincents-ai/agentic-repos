# User Guide: Enabling Git LFS on repos-mcp Server

This guide walks you through enabling Git LFS support on your repos-mcp server.

## Prerequisites

- repos-mcp server installed and running
- Access to server configuration files
- Basic understanding of YAML configuration
- SSH access to server (for testing)

## Step 1: Check Current Configuration

First, locate your configuration file. By default, repos-mcp looks for `config.yaml` in the working directory.

```bash
# Check if config.yaml exists
ls -l config.yaml

# View current configuration
cat config.yaml
```

If you don't have a configuration file, copy the example:

```bash
cp config-example.yaml config.yaml
```

## Step 2: Enable LFS Feature Flag

Open `config.yaml` in your preferred editor and add or modify the `features` section:

```yaml
features:
  git_lfs: true    # Enable LFS functionality
```

This is the master switch for Git LFS. Without this, LFS will not be available even if other settings are configured.

## Step 3: Configure LFS Settings

Add the `lfs` configuration section:

```yaml
lfs:
  enabled: true
  jwt_secret: "CHANGE_THIS_TO_A_STRONG_SECRET"
  base_url: "http://your-server.com:8090"
  max_object_size: 5368709120
  token_expiration_secs: 3600
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  locking_enabled: false
  lock_expiration_secs: 86400
```

## Step 4: Generate Secure JWT Secret

**CRITICAL**: The `jwt_secret` is used for authentication. Generate a strong random secret:

```bash
# Generate a 32-character base64 secret
openssl rand -base64 32
```

Example output:
```
Kx7vB2wN9pQ5tR8mL3nC6dF1gH4jK0sA
```

Update your config with this secret:

```yaml
lfs:
  jwt_secret: "Kx7vB2wN9pQ5tR8mL3nC6dF1gH4jK0sA"
```

**Security Note**: Never commit this secret to version control. Consider using environment variables for production:

```yaml
lfs:
  jwt_secret: "${LFS_JWT_SECRET}"
```

Then export the variable:
```bash
export LFS_JWT_SECRET="Kx7vB2wN9pQ5tR8mL3nC6dF1gH4jK0sA"
```

## Step 5: Configure Base URL

Set the `base_url` to match your server's external address:

### Development (Local)
```yaml
lfs:
  base_url: "http://localhost:8090"
```

### Production (Public Domain)
```yaml
lfs:
  base_url: "https://git.example.com"
```

### Production (with Custom Port)
```yaml
lfs:
  base_url: "https://git.example.com:8443"
```

**Important**: 
- Use `https://` in production for security
- Do NOT include trailing slash
- URL must be accessible from client machines

## Step 6: Adjust Object Size Limit (Optional)

The default maximum object size is 5 GB. Adjust based on your needs:

```yaml
lfs:
  max_object_size: 2147483648  # 2 GB
```

Common sizes:
- 100 MB: `104857600`
- 500 MB: `524288000`
- 1 GB: `1073741824`
- 5 GB: `5368709120` (default)
- 10 GB: `10737418240`

## Step 7: Complete Configuration Example

Here's a complete production-ready configuration:

```yaml
server:
  host: "0.0.0.0"
  port: 8090

features:
  git_lfs: true

lfs:
  enabled: true
  jwt_secret: "${LFS_JWT_SECRET}"  # Load from environment
  base_url: "https://git.example.com"
  max_object_size: 5368709120
  token_expiration_secs: 3600
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  locking_enabled: false
  lock_expiration_secs: 86400
```

## Step 8: Restart Server

After updating configuration, restart repos-mcp:

```bash
# Stop current server (if running as service)
sudo systemctl restart repos-mcp

# Or if running directly
killall repos-mcp
./repos-mcp
```

Check logs for successful LFS initialization:

```bash
# Logs should show:
# "LFS feature enabled"
# "LFS routes configured"
```

## Step 9: Verify LFS is Enabled

Test that LFS endpoints are accessible:

```bash
# Test batch endpoint (should return 200 or 401, not 404)
curl -I http://localhost:8090/lfs/test/repo/info/lfs/objects/batch

# Expected: HTTP 401 or 200 (not 404 Not Found)
```

## Step 10: Test End-to-End (Optional)

Complete verification with a test upload:

```bash
# 1. Create test file
echo "test content" > /tmp/test.txt
OID=$(sha256sum /tmp/test.txt | awk '{print $1}')

# 2. Request upload URL
curl -X POST http://localhost:8090/lfs/test/repo/info/lfs/objects/batch \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -d "{\"operation\":\"upload\",\"objects\":[{\"oid\":\"$OID\",\"size\":13}]}" | jq .

# 3. Upload object
curl -X PUT "http://localhost:8090/lfs/test/repo/lfs/objects/$OID" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @/tmp/test.txt

# 4. Verify file exists
find ./repositories -name "$OID"
```

## Troubleshooting

### LFS Endpoints Return 404

**Cause**: `git_lfs` feature flag not enabled or `lfs.enabled = false`

**Solution**:
```yaml
features:
  git_lfs: true  # Must be true

lfs:
  enabled: true   # Must also be true
```

Restart server after changes.

### Authentication Errors

**Cause**: Invalid or missing `jwt_secret`

**Solution**:
1. Verify `jwt_secret` is set and not empty
2. Generate new secret: `openssl rand -base64 32`
3. Update config and restart

### "Address already in use" Error

**Cause**: Port 8090 is already bound

**Solution**:
```bash
# Find process using port
lsof -i :8090

# Kill old process
kill <PID>

# Or change port in config
server:
  port: 8091
```

### Objects Not Found After Upload

**Cause**: Storage path or permissions issue

**Solution**:
```bash
# Check storage directory exists and is writable
ls -ld ./repositories
# Should show: drwxr-xr-x

# Create if missing
mkdir -p ./repositories
chmod 755 ./repositories
```

### JWT Token Expired Quickly

**Cause**: `token_expiration_secs` too short for large files

**Solution**:
```yaml
lfs:
  token_expiration_secs: 7200  # Increase to 2 hours
```

Consider your network speed and typical file sizes.

## Security Checklist

Before enabling in production:

- [ ] Strong `jwt_secret` generated (32+ characters)
- [ ] Secret stored securely (environment variable, not in config file)
- [ ] `base_url` uses HTTPS (not HTTP)
- [ ] TLS certificates valid and not expired
- [ ] `max_object_size` set to reasonable limit
- [ ] Firewall rules configured (allow port 8090 or configured port)
- [ ] Reverse proxy configured (nginx/HAProxy) with rate limiting
- [ ] Monitoring and logging enabled
- [ ] Backup strategy for LFS objects in place

## Next Steps

After enabling LFS on the server:

1. **Configure Git LFS clients**: See [Configuring git-lfs Client](./LFS_CLIENT_SETUP.md)
2. **Migrate existing repositories**: See [Migrating to LFS](./LFS_MIGRATION.md)
3. **Set up monitoring**: See [Admin Guide](./LFS_ADMIN_GUIDE.md)

## Additional Configuration Options

### Using Environment Variables

For better security, use environment variables for sensitive values:

```yaml
lfs:
  jwt_secret: "${LFS_JWT_SECRET}"
  base_url: "${LFS_BASE_URL}"
```

Then create `.env` file or export:
```bash
export LFS_JWT_SECRET="your-secret-here"
export LFS_BASE_URL="https://git.example.com"
```

### Custom Storage Path Pattern

The default pattern creates a 2-level directory hierarchy for performance:

```yaml
lfs:
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
```

This creates paths like:
```
acme/myapp.git/lfs/objects/2c/26/2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae
```

For simpler structure (not recommended for production with many files):
```yaml
lfs:
  storage_path_pattern: "{tenant}/{repo}.git/lfs/{oid}"
```

### Multiple Server Instances

If running multiple repos-mcp instances:

1. All instances must share the same `jwt_secret`
2. All instances must use the same storage backend (or replicated storage)
3. Consider load balancer in front of instances

## Configuration File Location

repos-mcp searches for configuration in this order:

1. `--config` command line flag
2. `config.yaml` in current directory
3. Default configuration (LFS disabled)

Specify custom location:
```bash
./repos-mcp --config=/etc/repos-mcp/config.yaml
```

## Related Documentation

- [LFS Configuration Reference](./LFS_CONFIGURATION.md) - Detailed parameter documentation
- [LFS API Reference](./LFS_API.md) - HTTP endpoint specifications
- [LFS Security Guide](./LFS_SECURITY.md) - Security best practices

## Support

If you encounter issues not covered in this guide:

1. Check server logs for error messages
2. Review [Troubleshooting Guide](./LFS_TROUBLESHOOTING.md)
3. Verify configuration with [Configuration Reference](./LFS_CONFIGURATION.md)
4. Report bugs to repos-mcp GitHub issues

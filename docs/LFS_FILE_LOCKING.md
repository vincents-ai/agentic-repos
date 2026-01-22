# Git LFS File Locking

Git LFS File Locking prevents concurrent editing of binary files by allowing users to lock files before modification. This feature is essential for teams working with large binary assets like PSDs, videos, or CAD files where merge conflicts cannot be resolved.

## Table of Contents

- [Overview](#overview)
- [Configuration](#configuration)
- [API Reference](#api-reference)
- [Client Usage](#client-usage)
- [RBAC Permissions](#rbac-permissions)
- [Lock Expiration](#lock-expiration)
- [Troubleshooting](#troubleshooting)

## Overview

### What is File Locking?

File locking allows users to mark files as "in use" to prevent others from modifying them simultaneously. This is particularly important for binary files where Git's merge capabilities are limited.

### Key Features

- **Multi-tenant support** - Complete isolation between tenants and repositories
- **Lock expiration** - Automatic expiration of stale locks
- **RBAC integration** - Fine-grained permission control
- **Force unlock** - Administrators can override locks
- **Git ref support** - Associate locks with specific branches
- **Pagination** - Efficient listing of large lock sets
- **Scheduled cleanup** - Automatic removal of expired locks

### Specification Compliance

This implementation follows the [official Git LFS Locking API specification](https://github.com/git-lfs/git-lfs/blob/main/docs/api/locking.md) with additional server-side enhancements.

## Configuration

### Enable File Locking

Edit your `config.yaml`:

```yaml
features:
  git_lfs: true          # Master LFS toggle
  lfs_locks: true        # File locking toggle

lfs:
  enabled: true
  locking_enabled: true  # Must be true for locks to work
  lock_expiration_secs: 86400  # 24 hours (default)
  jwt_secret: "your-secret-key-here"
  base_url: "https://your-server.com"
  # ... other LFS config
```

### Environment Variables

Alternatively, use environment variables:

```bash
export FEATURE_GIT_LFS=true
export FEATURE_LFS_LOCKS=true
```

### Database Setup

File locking requires PostgreSQL. The migration will be applied automatically:

```sql
CREATE TABLE lfs_locks (
    id UUID PRIMARY KEY,
    tenant TEXT NOT NULL,
    repo TEXT NOT NULL,
    path TEXT NOT NULL,
    ref_name TEXT,
    owner_name TEXT NOT NULL,
    owner_id TEXT,
    locked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    UNIQUE(tenant, repo, path)
);
```

## API Reference

All lock API endpoints require JWT authentication via the `Authorization: Bearer <token>` header.

### Create Lock

**Endpoint**: `POST /lfs/{tenant}/{repo}/info/lfs/locks`

**Request**:
```json
{
  "path": "designs/banner.psd",
  "ref": {
    "name": "refs/heads/main"
  }
}
```

**Response** (201 Created):
```json
{
  "lock": {
    "id": "123e4567-e89b-12d3-a456-426614174000",
    "path": "designs/banner.psd",
    "locked_at": "2026-01-22T20:00:00Z",
    "owner": {
      "name": "alice"
    }
  }
}
```

**Errors**:
- `409 Conflict` - File is already locked
- `403 Forbidden` - Missing `LfsLockCreate` permission
- `503 Service Unavailable` - Locking not enabled

### List Locks

**Endpoint**: `GET /lfs/{tenant}/{repo}/info/lfs/locks`

**Query Parameters**:
- `path` (optional) - Filter by file path
- `id` (optional) - Filter by lock ID
- `refspec` (optional) - Filter by Git ref
- `cursor` (optional) - Pagination cursor
- `limit` (optional) - Results per page (default: 100)

**Response** (200 OK):
```json
{
  "locks": [
    {
      "id": "123e4567-e89b-12d3-a456-426614174000",
      "path": "designs/banner.psd",
      "locked_at": "2026-01-22T20:00:00Z",
      "owner": {
        "name": "alice"
      }
    }
  ],
  "next_cursor": "eyJpZCI6IjEyM2U0NTY3LWU4OWItMTJkMy1hNDU2LTQyNjYxNDE3NDAwMCJ9"
}
```

**Errors**:
- `403 Forbidden` - Missing `LfsLockRead` permission

### Delete Lock (Unlock)

**Endpoint**: `POST /lfs/{tenant}/{repo}/info/lfs/locks/{id}/unlock`

**Request**:
```json
{
  "force": false,
  "ref": {
    "name": "refs/heads/main"
  }
}
```

**Response** (200 OK):
```json
{
  "lock": {
    "id": "123e4567-e89b-12d3-a456-426614174000",
    "path": "designs/banner.psd",
    "locked_at": "2026-01-22T20:00:00Z",
    "owner": {
      "name": "alice"
    }
  }
}
```

**Errors**:
- `403 Forbidden` - Not the owner and no `LfsLockForce` permission
- `404 Not Found` - Lock does not exist

### Verify Locks

**Endpoint**: `POST /lfs/{tenant}/{repo}/info/lfs/locks/verify`

Used before pushing to check which files are locked by you vs others.

**Request**:
```json
{
  "ref": {
    "name": "refs/heads/main"
  },
  "cursor": null,
  "limit": 100
}
```

**Response** (200 OK):
```json
{
  "ours": [
    {
      "id": "123e4567-e89b-12d3-a456-426614174000",
      "path": "designs/banner.psd",
      "locked_at": "2026-01-22T20:00:00Z",
      "owner": {
        "name": "alice"
      }
    }
  ],
  "theirs": [
    {
      "id": "789e4567-e89b-12d3-a456-426614174111",
      "path": "video/intro.mp4",
      "locked_at": "2026-01-22T19:30:00Z",
      "owner": {
        "name": "bob"
      }
    }
  ],
  "next_cursor": null
}
```

**Errors**:
- `403 Forbidden` - Missing `LfsLockRead` permission

## Client Usage

### Git LFS Commands

Standard Git LFS commands work with repos-mcp:

#### Lock a File

```bash
# Lock a file before editing
git lfs lock designs/banner.psd

# Lock with reason
git lfs lock video/intro.mp4
```

Output:
```
Locked designs/banner.psd
```

#### List Locks

```bash
# List all locks in the repository
git lfs locks

# List locks for a specific path
git lfs locks --path designs/

# List locks you own
git lfs locks --local

# List locks others own
git lfs locks --remote
```

Output:
```
designs/banner.psd  alice  ID:123e4567-e89b
video/intro.mp4     bob    ID:789e4567-e89b
```

#### Unlock a File

```bash
# Unlock a file you own
git lfs unlock designs/banner.psd

# Force unlock (admin only)
git lfs unlock --force designs/banner.psd
```

Output:
```
Unlocked designs/banner.psd
```

#### Verify Locks Before Push

```bash
# Check for lock conflicts before pushing
git lfs locks --verify

# Git LFS automatically verifies during push
git push origin main
```

### Example Workflow

```bash
# 1. Check if file is locked
git lfs locks --path designs/banner.psd

# 2. Lock the file
git lfs lock designs/banner.psd

# 3. Make your changes
vim designs/banner.psd

# 4. Commit and push
git add designs/banner.psd
git commit -m "Update banner design"
git push origin main

# 5. Unlock after push
git lfs unlock designs/banner.psd
```

## RBAC Permissions

File locking integrates with the RBAC system using four granular permissions:

### Permission Types

| Permission | Description | Required For |
|------------|-------------|--------------|
| `LfsLockCreate` | Create new locks | Locking files |
| `LfsLockRead` | List and verify locks | Viewing lock status |
| `LfsLockDelete` | Delete own locks | Unlocking own files |
| `LfsLockForce` | Force delete any lock | Admin override |

### Default Role Permissions

| Role | Permissions | Use Case |
|------|-------------|----------|
| **Administrator** | All four permissions | Full lock management |
| **Developer** | Create, Read, Delete (no Force) | Normal development workflow |
| **ReadOnly** | Read only | View lock status |

### Permission Checks

The API enforces permissions at each endpoint:

```
POST /locks          → Requires LfsLockCreate
GET /locks           → Requires LfsLockRead
POST /locks/{id}/unlock → Requires LfsLockDelete (or LfsLockForce if force=true)
POST /locks/verify   → Requires LfsLockRead
```

### Unauthorized Access

Requests without proper permissions receive `403 Forbidden`:

```json
{
  "message": "Permission denied: LfsLockCreate required"
}
```

## Lock Expiration

### Automatic Expiration

Locks can be configured to expire automatically to prevent indefinite holds:

```yaml
lfs:
  lock_expiration_secs: 86400  # 24 hours
```

### Expiration Behavior

1. **At creation** - Lock receives `expires_at` timestamp
2. **In queries** - Expired locks are automatically filtered out
3. **Cleanup job** - Scheduled task removes expired locks from database

### Scheduled Cleanup

The cleanup scheduler runs periodically to remove expired locks:

```rust
use repos_mcp::lfs::LockCleanupScheduler;

let scheduler = LockCleanupScheduler::new(
    lock_manager,
    3600,  // Run every hour
    true   // Enabled
);
let handle = scheduler.spawn();
```

### Manual Cleanup

Force cleanup via the API:

```rust
let count = lock_manager.cleanup_expired_locks().await?;
println!("Removed {} expired locks", count);
```

## Troubleshooting

### Common Errors

#### "File is already locked"

**Symptom**: `409 Conflict` when trying to lock a file

**Cause**: Another user has locked the file

**Solution**:
```bash
# Check who has the lock
git lfs locks --path designs/banner.psd

# Contact the owner or wait for expiration
# Admins can force unlock:
git lfs unlock --force designs/banner.psd
```

#### "Permission denied: LfsLockCreate required"

**Symptom**: `403 Forbidden` when attempting lock operations

**Cause**: User lacks RBAC permissions

**Solution**:
- Contact administrator to assign appropriate role
- Verify RBAC is configured in `config.yaml`
- Check role assignments: Administrator > Developer > ReadOnly

#### "Git LFS file locking is not enabled"

**Symptom**: `503 Service Unavailable` for all lock operations

**Cause**: Locking feature disabled in configuration

**Solution**:
```yaml
features:
  git_lfs: true
  lfs_locks: true

lfs:
  enabled: true
  locking_enabled: true
```

#### "Missing or invalid authentication token"

**Symptom**: `401 Unauthorized` for lock API requests

**Cause**: Missing or expired JWT token

**Solution**:
```bash
# Git LFS automatically handles authentication
# If using curl, ensure token is valid:
TOKEN=$(git-lfs-authenticate tenant repo download)
curl -H "Authorization: Bearer $TOKEN" ...
```

#### Expired locks not cleaning up

**Symptom**: Expired locks still appear in database

**Cause**: Cleanup scheduler not running or disabled

**Solution**:
1. Verify cleanup is enabled in configuration
2. Check server logs for cleanup job execution
3. Manually trigger cleanup:
```bash
# Via API or service restart
systemctl restart repos-mcp
```

### Lock Conflicts During Push

**Symptom**: Push rejected due to locked files

**Error**:
```
error: failed to push some refs to 'ssh://server/tenant/repo'
hint: The following files are locked by another user:
hint:   designs/banner.psd (locked by bob)
```

**Resolution**:
1. **Option A** - Wait for owner to unlock
2. **Option B** - Work on different files
3. **Option C** - Admin force unlock (emergency only)
4. **Option D** - Create new branch and merge later

### Database Connection Issues

**Symptom**: Lock operations fail with database errors

**Cause**: PostgreSQL connection issues

**Solution**:
```bash
# Check database connectivity
psql $DATABASE_URL -c "SELECT 1;"

# Verify table exists
psql $DATABASE_URL -c "\d lfs_locks"

# Check for locks
psql $DATABASE_URL -c "SELECT COUNT(*) FROM lfs_locks;"
```

### Multi-Tenant Isolation Issues

**Symptom**: Seeing locks from other tenants

**Cause**: Bug in tenant filtering (should not happen)

**Solution**:
1. Verify tenant parameter in API requests
2. Check database constraint: `UNIQUE(tenant, repo, path)`
3. Report as bug if isolation is broken

### Performance Issues with Large Lock Sets

**Symptom**: Slow responses when listing locks

**Cause**: Large number of locks, missing indexes

**Solution**:
```sql
-- Verify indexes exist
\d lfs_locks

-- Expected indexes:
-- idx_lfs_locks_tenant_repo
-- idx_lfs_locks_path
-- idx_lfs_locks_owner
-- idx_lfs_locks_ref
-- idx_lfs_locks_expires

-- Use pagination
git lfs locks --limit 50
```

## Advanced Usage

### Integrating with CI/CD

Lock operations in automated workflows:

```yaml
# .gitlab-ci.yml example
before_script:
  - git lfs lock critical-config.yaml || true

after_script:
  - git lfs unlock critical-config.yaml || true
```

### Custom Lock Expiration

Per-lock expiration is not directly supported, but you can:
1. Use short global expiration (e.g., 1 hour)
2. Have users re-lock periodically
3. Monitor lock age via list endpoint

### Lock Notifications

Set up webhooks to notify teams about lock operations:

```yaml
# Future feature - webhooks for lock events
webhooks:
  - url: https://slack.com/webhook/...
    events: ["lock.created", "lock.deleted"]
```

## API Examples

### Create Lock with curl

```bash
# Generate token via git-lfs-authenticate
TOKEN=$(ssh git@server git-lfs-authenticate tenant/repo download)

# Create lock
curl -X POST https://server/lfs/tenant/repo/info/lfs/locks \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -d '{
    "path": "designs/banner.psd",
    "ref": {
      "name": "refs/heads/main"
    }
  }'
```

### List Locks with Filtering

```bash
# List locks for specific path
curl -X GET "https://server/lfs/tenant/repo/info/lfs/locks?path=designs/" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.git-lfs+json"

# Paginated list
curl -X GET "https://server/lfs/tenant/repo/info/lfs/locks?limit=10" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.git-lfs+json"
```

### Verify Locks

```bash
curl -X POST https://server/lfs/tenant/repo/info/lfs/locks/verify \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -d '{
    "ref": {
      "name": "refs/heads/main"
    }
  }'
```

### Force Unlock

```bash
curl -X POST https://server/lfs/tenant/repo/info/lfs/locks/{LOCK_ID}/unlock \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -d '{
    "force": true
  }'
```

## Security Considerations

### JWT Token Security

- Tokens are short-lived (default: 1 hour)
- Generated via SSH authentication
- Transmitted over HTTPS only

### RBAC Best Practices

1. **Principle of least privilege** - Grant minimum required permissions
2. **Regular audits** - Review role assignments periodically
3. **Force unlock sparingly** - Only administrators should have this power
4. **Monitor lock operations** - Enable audit logging for compliance

### Database Security

- Use strong PostgreSQL credentials
- Restrict database access to application server
- Enable SSL for database connections
- Regular backups of lock data

## Performance Tuning

### Database Indexes

All necessary indexes are created automatically:
- `(tenant, repo)` - Fast repository lookup
- `(tenant, repo, path)` - Unique constraint + fast path queries
- `(tenant, repo, owner_name)` - Fast owner queries
- `(tenant, repo, ref_name)` - Fast ref queries
- `(expires_at)` - Fast cleanup queries

### Pagination

For large lock sets, use pagination:

```bash
# First page
git lfs locks --limit 100

# Subsequent pages (handled automatically by client)
```

### Cleanup Frequency

Adjust cleanup interval based on lock volume:

```yaml
# High volume: cleanup every hour
lock_cleanup_interval_secs: 3600

# Low volume: cleanup daily
lock_cleanup_interval_secs: 86400
```

## Migration from Other Systems

### From Git LFS Server

repos-mcp is API-compatible. Update your `.lfsconfig`:

```
[lfs]
    url = https://your-repos-mcp-server.com/lfs/tenant/repo
```

### Lock Data Migration

If migrating from another system:

1. Export locks from old system
2. Insert into `lfs_locks` table
3. Ensure UUID format and timestamps are correct
4. Verify with `git lfs locks` command

## Related Documentation

- [Git LFS Configuration](LFS_CONFIGURATION.md)
- [Git LFS Batch API](LFS_API.md)
- [RBAC Configuration](RBAC.md)
- [Official Git LFS Docs](https://git-lfs.com/)
- [Git LFS Locking Spec](https://github.com/git-lfs/git-lfs/blob/main/docs/api/locking.md)

## Support

For issues or questions:
- GitHub Issues: https://github.com/your-org/repos-mcp/issues
- Email: support@your-domain.com
- Documentation: https://docs.your-domain.com

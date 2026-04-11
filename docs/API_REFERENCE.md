API Reference - repos-mcp Skynet v2 Git Operations Plugin
==========================================================

Complete API reference for all methods and data structures.

Request/Response Protocol
========================

All communication follows the JSON-RPC style protocol with Skynet v2 extensions.

Request Structure
-----------------

```typescript
interface PluginRequest {
  id: string;              // Unique request identifier for correlation
  method: string;          // RPC method name (e.g., "repo.create")
  params: object;          // Method parameters (varies by method)
  tenant_id?: string;      // Tenant context (required for most operations)
  user_id?: string;        // User context (required for most operations)
  token?: string;          // API token for authentication (optional)
}
```

Response Structure
------------------

```typescript
interface PluginResponse {
  id: string;              // Echo of request ID
  result?: object;         // Successful operation result
  error?: {
    code: string;          // Error code (e.g., "PERMISSION_DENIED")
    message: string;       // Human-readable error message
    details?: object;      // Additional error details
  };
}
```

Repository Operations
====================

repo.create
-----------

Create a new Git repository in a tenant.

Request:
```json
{
  "id": "req-1",
  "method": "repo.create",
  "params": {
    "name": "my-repo",
    "description": "My repository",
    "private": false,
    "default_branch": "main"
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-1",
  "result": {
    "repository": {
      "tenant_id": "tenant-123",
      "name": "my-repo",
      "description": "My repository",
      "private": false,
      "default_branch": "main",
      "created_at": "2024-01-28T12:00:00Z",
      "clone_url_ssh": "git@example.com:my-repo.git",
      "clone_url_http": "https://example.com/my-repo.git"
    }
  }
}
```

Parameters:
- `name` (string, required): Repository name (alphanumeric, hyphens, underscores)
- `description` (string, optional): Human-readable description
- `private` (boolean, optional): Default false
- `default_branch` (string, optional): Default main

Errors:
- PERMISSION_DENIED: User lacks repo creation permission
- REPO_EXISTS: Repository already exists
- INVALID_REQUEST: Invalid parameters
- RESOURCE_EXHAUSTED: Tenant quota exceeded

repo.delete
-----------

Delete a repository.

Request:
```json
{
  "id": "req-2",
  "method": "repo.delete",
  "params": {
    "name": "my-repo",
    "force": false
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-2",
  "result": {
    "status": "deleted",
    "repository": {
      "tenant_id": "tenant-123",
      "name": "my-repo"
    }
  }
}
```

Parameters:
- `name` (string, required): Repository name
- `force` (boolean, optional): Force deletion if repo is empty

Errors:
- PERMISSION_DENIED: User lacks delete permission
- REPO_NOT_FOUND: Repository does not exist
- INVALID_REQUEST: Invalid parameters

repo.list
---------

List all repositories for a tenant.

Request:
```json
{
  "id": "req-3",
  "method": "repo.list",
  "params": {
    "limit": 50,
    "offset": 0,
    "sort": "name",
    "order": "asc"
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-3",
  "result": {
    "repositories": [
      {
        "name": "repo-1",
        "description": "First repository",
        "private": false,
        "created_at": "2024-01-28T12:00:00Z",
        "updated_at": "2024-01-28T12:00:00Z",
        "size_bytes": 1024000
      },
      {
        "name": "repo-2",
        "description": "Second repository",
        "private": true,
        "created_at": "2024-01-28T12:00:00Z",
        "updated_at": "2024-01-28T12:00:00Z",
        "size_bytes": 2048000
      }
    ],
    "count": 2,
    "total": 2,
    "limit": 50,
    "offset": 0
  }
}
```

Parameters:
- `limit` (integer, optional): Max results (default 50, max 1000)
- `offset` (integer, optional): Pagination offset (default 0)
- `sort` (string, optional): Sort field (name, created_at, size)
- `order` (string, optional): asc or desc (default asc)

Errors:
- PERMISSION_DENIED: User lacks list permission
- INVALID_REQUEST: Invalid parameters

repo.info
---------

Get detailed information about a repository.

Request:
```json
{
  "id": "req-4",
  "method": "repo.info",
  "params": {
    "name": "my-repo"
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-4",
  "result": {
    "repository": {
      "name": "my-repo",
      "description": "My repository",
      "private": false,
      "default_branch": "main",
      "created_at": "2024-01-28T12:00:00Z",
      "updated_at": "2024-01-28T12:00:00Z",
      "size_bytes": 1024000,
      "clone_urls": {
        "ssh": "git@example.com:my-repo.git",
        "http": "https://example.com/my-repo.git"
      },
      "permissions": ["push", "pull"],
      "branches": [
        {
          "name": "main",
          "commit_sha": "abc123...",
          "commit_message": "Initial commit",
          "author": "user-456",
          "created_at": "2024-01-28T12:00:00Z"
        }
      ]
    }
  }
}
```

Parameters:
- `name` (string, required): Repository name

Errors:
- REPO_NOT_FOUND: Repository does not exist
- PERMISSION_DENIED: User lacks view permission

Git Operations
==============

git.push
--------

Push commits to repository (metadata update).

Request:
```json
{
  "id": "req-5",
  "method": "git.push",
  "params": {
    "name": "my-repo",
    "branch": "main",
    "force": false
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Parameters:
- `name` (string, required): Repository name
- `branch` (string, optional): Target branch
- `force` (boolean, optional): Allow force push

Errors:
- REPO_NOT_FOUND: Repository does not exist
- PERMISSION_DENIED: User lacks push permission
- RATE_LIMIT_EXCEEDED: Push rate limit exceeded

git.pull
--------

Pull commits from repository.

Request:
```json
{
  "id": "req-6",
  "method": "git.pull",
  "params": {
    "name": "my-repo",
    "branch": "main"
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Parameters:
- `name` (string, required): Repository name
- `branch` (string, optional): Source branch

Errors:
- REPO_NOT_FOUND: Repository does not exist
- PERMISSION_DENIED: User lacks pull permission

Authentication Operations
==========================

auth.ssh_key_add
----------------

Add SSH public key for user.

Request:
```json
{
  "id": "req-7",
  "method": "auth.ssh_key_add",
  "params": {
    "key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...",
    "name": "my-laptop",
    "expires_at": "2025-01-28T12:00:00Z"
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-7",
  "result": {
    "key": {
      "id": "key-789",
      "name": "my-laptop",
      "fingerprint": "SHA256:...",
      "type": "ssh-ed25519",
      "created_at": "2024-01-28T12:00:00Z",
      "expires_at": "2025-01-28T12:00:00Z",
      "last_used_at": null
    }
  }
}
```

Parameters:
- `key` (string, required): SSH public key in OpenSSH format
- `name` (string, optional): Key name for reference
- `expires_at` (string, optional): ISO8601 expiration timestamp

Errors:
- INVALID_REQUEST: Invalid SSH key format
- AUTHENTICATION_ERROR: Key already exists

auth.ssh_key_list
-----------------

List SSH keys for user.

Request:
```json
{
  "id": "req-8",
  "method": "auth.ssh_key_list",
  "params": {},
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-8",
  "result": {
    "keys": [
      {
        "id": "key-1",
        "name": "my-laptop",
        "fingerprint": "SHA256:...",
        "type": "ssh-ed25519",
        "created_at": "2024-01-28T12:00:00Z",
        "expires_at": "2025-01-28T12:00:00Z",
        "last_used_at": "2024-01-28T12:30:00Z"
      }
    ],
    "count": 1
  }
}
```

auth.ssh_key_remove
-------------------

Remove SSH key.

Request:
```json
{
  "id": "req-9",
  "method": "auth.ssh_key_remove",
  "params": {
    "key_id": "key-1"
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-9",
  "result": {
    "status": "deleted",
    "key_id": "key-1"
  }
}
```

Parameters:
- `key_id` (string, required): SSH key ID to remove

Git LFS Operations
==================

lfs.upload
----------

Initiate LFS object upload.

Request:
```json
{
  "id": "req-10",
  "method": "lfs.upload",
  "params": {
    "repo": "my-repo",
    "oid": "abc123def456...",
    "size": 1048576,
    "expires_in_hours": 24
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-10",
  "result": {
    "upload": {
      "oid": "abc123def456...",
      "size": 1048576,
      "expires_at": "2024-01-29T12:00:00Z",
      "upload_url": "https://s3.amazonaws.com/bucket/path",
      "headers": {
        "Authorization": "AWS4-HMAC-SHA256 ..."
      }
    }
  }
}
```

Parameters:
- `repo` (string, required): Repository name
- `oid` (string, required): Git LFS object ID (SHA256)
- `size` (integer, required): Object size in bytes
- `expires_in_hours` (integer, optional): Expiration time

Errors:
- REPO_NOT_FOUND: Repository does not exist
- INVALID_REQUEST: Invalid OID format
- RESOURCE_EXHAUSTED: LFS quota exceeded

lfs.download
------------

Get LFS object download URL.

Request:
```json
{
  "id": "req-11",
  "method": "lfs.download",
  "params": {
    "repo": "my-repo",
    "oid": "abc123def456...",
    "expires_in_hours": 24
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-11",
  "result": {
    "download": {
      "oid": "abc123def456...",
      "size": 1048576,
      "expires_at": "2024-01-29T12:00:00Z",
      "download_url": "https://s3.amazonaws.com/bucket/path"
    }
  }
}
```

lfs.lock
--------

Lock LFS file for editing.

Request:
```json
{
  "id": "req-12",
  "method": "lfs.lock",
  "params": {
    "repo": "my-repo",
    "path": "large-file.bin",
    "force": false
  },
  "tenant_id": "tenant-123",
  "user_id": "user-456"
}
```

Response:
```json
{
  "id": "req-12",
  "result": {
    "lock": {
      "id": "lock-xyz",
      "path": "large-file.bin",
      "locked_by": "user-456",
      "locked_at": "2024-01-28T12:00:00Z",
      "expires_at": "2024-01-29T12:00:00Z"
    }
  }
}
```

Monitoring Operations
====================

health.check
------------

Check plugin health status.

Request:
```json
{
  "id": "req-13",
  "method": "health.check",
  "params": {}
}
```

Response:
```json
{
  "id": "req-13",
  "result": {
    "health": {
      "status": "healthy",
      "version": "0.1.0",
      "uptime_secs": 3600,
      "active_repositories": 42,
      "active_sessions": 12,
      "storage_bytes": 10737418240,
      "checked_at": "2024-01-28T12:00:00Z",
      "errors": []
    }
  }
}
```

Status values:
- "healthy": All systems operational
- "degraded": Some systems impaired
- "unhealthy": Critical failures

metrics.get
-----------

Get plugin metrics.

Request:
```json
{
  "id": "req-14",
  "method": "metrics.get",
  "params": {
    "filter": "repo.create"
  }
}
```

Response:
```json
{
  "id": "req-14",
  "result": {
    "metrics": {
      "requests": {
        "repo.create": 145,
        "repo.delete": 32,
        "git.push": 892
      },
      "successes": {
        "repo.create": 142,
        "repo.delete": 32,
        "git.push": 890
      },
      "errors": {
        "repo.create:PERMISSION_DENIED": 2,
        "repo.create:INVALID_REQUEST": 1,
        "git.push:RATE_LIMIT_EXCEEDED": 2
      }
    }
  }
}
```

Error Codes
===========

HTTP-style error codes:

| Code | HTTP | Description |
|------|------|-------------|
| PLUGIN_CONFIG_ERROR | 400 | Plugin configuration error |
| INVALID_REQUEST | 400 | Invalid request parameters |
| AUTH_ERROR | 401 | Authentication failure |
| PERMISSION_DENIED | 403 | Insufficient permissions |
| REPO_NOT_FOUND | 404 | Repository does not exist |
| REPO_EXISTS | 409 | Repository already exists |
| RATE_LIMIT_EXCEEDED | 429 | Rate limit exceeded |
| RESOURCE_EXHAUSTED | 507 | Quota or resource limit exceeded |
| TIMEOUT | 504 | Operation timeout |
| STORAGE_ERROR | 500 | Storage backend error |
| TENANT_ERROR | 500 | Tenant isolation error |
| GIT_ERROR | 500 | Git operation error |
| LFS_ERROR | 500 | Git LFS error |
| MONITORING_ERROR | 500 | Health check error |
| INTERNAL_ERROR | 500 | Internal server error |
| IO_ERROR | 500 | I/O error |
| JSON_ERROR | 500 | JSON serialization error |

Data Types
==========

Repository
----------

```typescript
interface Repository {
  tenant_id: string;
  name: string;
  description?: string;
  private: boolean;
  default_branch: string;
  created_at: string;           // ISO8601
  updated_at: string;           // ISO8601
  size_bytes: number;
  clone_url_ssh: string;
  clone_url_http?: string;
}
```

Branch
------

```typescript
interface Branch {
  name: string;
  commit_sha: string;
  commit_message: string;
  author: string;
  created_at: string;           // ISO8601
}
```

SSHKey
------

```typescript
interface SSHKey {
  id: string;
  name: string;
  fingerprint: string;
  type: string;                 // ssh-ed25519, ssh-rsa, etc.
  created_at: string;           // ISO8601
  expires_at?: string;          // ISO8601
  last_used_at?: string;        // ISO8601
}
```

HealthStatus
------------

```typescript
interface HealthStatus {
  status: string;               // healthy, degraded, unhealthy
  version: string;
  uptime_secs: number;
  active_repositories: number;
  active_sessions: number;
  storage_bytes: number;
  checked_at: string;           // ISO8601
  errors: string[];
}
```

Rate Limits
===========

Default rate limits per user:

- API calls: 1000 per minute
- Concurrent connections: 10
- Data transfer: 1000 MB per hour

Per operation:
- Clone timeout: 300 seconds
- Push timeout: 300 seconds

Global:
- Max concurrent users: 1000
- Max data transfer: 100 GB per hour

Exceed limits returns 429 (Too Many Requests) with details.

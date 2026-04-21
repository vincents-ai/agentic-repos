# RBAC and Access Control — agentic-repos

Comprehensive guide to role-based access control, repository-level permissions, and the access config ref format.

---

## Table of Contents

- [Overview](#overview)
- [Default Access Policy](#default-access-policy)
- [Roles](#roles)
- [Repository-Level Access Control](#repository-level-access-control)
- [AccessConfig Ref Format](#accessconfig-ref-format)
- [Namespace Rules](#namespace-rules)
- [Global vs Per-Repo](#global-vs-per-repo)
- [RBAC Scope Enforcement](#rbac-scope-enforcement)
- [Managing Access](#managing-access)

---

## Overview

agentic-repos uses a two-layer access control system:

1. **RBAC** (Role-Based Access Control) — global roles and permissions for API access
2. **Repository AccessConfig** — per-repo path-level rules for git ref access

Both layers must grant access for an operation to succeed.

---

## Default Access Policy

New repositories use a **deny-by-default** access model:

- No users have push access unless explicitly granted
- The repository creator is automatically set as the owner
- Owners and admins always have full access

This prevents accidental exposure of new repositories.

---

## Roles

### Built-in roles

| Role | Scope | Description |
|------|-------|-------------|
| `admin` | Global | Full system access — manage users, config, all repos |
| `owner` | Per-repo | Full repository control — manage access, delete repo |
| `maintainer` | Per-repo | Push to any ref, manage webhooks |
| `developer` | Per-repo | Push to branches (not protected refs) |
| `reader` | Per-repo | Read-only — clone, fetch, browse |

### Role permissions

| Action | reader | developer | maintainer | owner | admin |
|--------|--------|-----------|------------|-------|-------|
| Clone/fetch | ✅ | ✅ | ✅ | ✅ | ✅ |
| Push to branches | ❌ | ✅ | ✅ | ✅ | ✅ |
| Push to protected refs | ❌ | ❌ | ✅ | ✅ | ✅ |
| Manage webhooks | ❌ | ❌ | ✅ | ✅ | ✅ |
| Manage access | ❌ | ❌ | ❌ | ✅ | ✅ |
| Delete repository | ❌ | ❌ | ❌ | ✅ | ✅ |
| System administration | ❌ | ❌ | ❌ | ❌ | ✅ |

---

## Repository-Level Access Control

Each repository has an access configuration stored at `refs/adp/config/access` in the repository itself.

### AccessConfig ref format

```json
{
  "version": "1",
  "owners": ["alice"],
  "admins": ["ops-team"],
  "rules": [
    {
      "user": "bob",
      "paths": ["refs/heads/*"],
      "permissions": ["pull", "push"]
    },
    {
      "user": "charlie",
      "paths": ["refs/heads/main"],
      "permissions": ["pull"]
    },
    {
      "user": "dev-team",
      "paths": ["refs/heads/*", "refs/tags/*"],
      "permissions": ["pull", "push"]
    }
  ]
}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Schema version (currently `"1"`) |
| `owners` | string[] | Usernames with full repo control |
| `admins` | string[] | Usernames with admin-level access |
| `rules` | PathRule[] | Path-level permission rules |

### PathRule format

| Field | Type | Description |
|-------|------|-------------|
| `user` | string | Username or `"*"` for all authenticated users |
| `paths` | string[] | Ref path patterns (supports `*` glob) |
| `permissions` | string[] | `"pull"`, `"push"`, or both |

### Path patterns

Patterns support glob-style matching:

- `refs/heads/*` — all branches
- `refs/heads/release/*` — release branches only
- `refs/tags/*` — all tags
- `refs/heads/feature/*` — feature branches
- `*` — all refs

---

## Namespace Rules

### Protected namespaces

The following namespaces are protected and cannot be pushed to by non-admins:

| Namespace | Purpose | Admin only |
|-----------|---------|------------|
| `refs/adp/*` | Platform configuration | ✅ |
| `refs/adp/keys/*` | SSH key mappings | ✅ |
| `refs/adp/users/*` | User identity | ✅ |
| `refs/secrets/*` | Encrypted secrets | ✅ |
| `refs/adp/config/*` | Repo configuration | ✅ |

### Public-readable namespaces

These namespaces are readable by anyone with pull access:

| Namespace | Purpose |
|-----------|---------|
| `refs/heads/*` | Branches |
| `refs/tags/*` | Tags |
| `refs/meta/*` | Public metadata (e.g., AGE public keys) |

---

## Global vs Per-Repo

### Global RBAC

Global roles are stored in the system repository (`_adp/system`):

```bash
# List global roles
curl -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/rbac/roles

# Assign a role
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"username": "alice", "role": "admin"}' \
  https://git.example.com/api/v1/rbac/roles
```

### Per-repo access

Per-repo access is stored in the repository's own refs:

```bash
# View access config
curl -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories/owner/repo/access

# Grant access
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"user": "bob", "paths": ["refs/heads/*"], "permissions": ["pull", "push"]}' \
  https://git.example.com/api/v1/repositories/owner/repo/access/rules
```

---

## RBAC Scope Enforcement

API routes enforce permissions at the scope level:

| Route scope | Required permission |
|-------------|-------------------|
| `/api/v1/repositories` | Authenticated user |
| `/api/v1/repositories/{id}` | Pull access on repo |
| `/api/v1/backups/*` | `SettingsManage` |
| `/api/v1/cluster/*` | `SystemAdmin` |
| `/api/v1/adp/*` | `SystemAdmin` |
| `/api/v1/secrets/*` | `RepositoryAdmin` |
| `/api/v1/rbac/*` | `SystemAdmin` |
| `/api/v1/security/*` | `SystemAdmin` |
| `/api/v1/compliance/*` | `RepositoryAdmin` |
| `/api/v1/analytics/*` | `RepositoryAdmin` |
| `/api/v1/replication/*` | `SettingsManage` |
| `/api/v1/cicd/*` | `RepositoryAdmin` |
| `/metrics` | Authenticated user |

---

## Managing Access

### Adding a collaborator

```bash
# Grant push access to all branches
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user": "bob", "paths": ["refs/heads/*"], "permissions": ["pull", "push"]}' \
  https://git.example.com/api/v1/repositories/alice/project/access/rules
```

### Removing access

```bash
# Remove a specific rule
curl -X DELETE \
  -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories/alice/project/access/rules/bob
```

### Setting read-only access

```bash
# Grant read-only (pull only)
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user": "charlie", "paths": ["refs/heads/*", "refs/tags/*"], "permissions": ["pull"]}' \
  https://git.example.com/api/v1/repositories/alice/project/access/rules
```

---

## Further Reading

- [Admin Guide](admin-guide.md) — RBAC setup and management
- [Security Model](../SECURITY.md) — Authorization architecture
- [Authentication](authentication.md) — Authentication methods
- [User Guide](user-guide.md) — Daily usage

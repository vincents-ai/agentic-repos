# User Guide — agentic-repos

In-depth guide for daily use of agentic-repos beyond the [Getting Started](getting-started.md) basics.

---

## Table of Contents

- [Repository Management](#repository-management)
- [Branch and Tag Operations](#branch-and-tag-operations)
- [File Browsing](#file-browsing)
- [Access Control (User Perspective)](#access-control-user-perspective)
- [Entity Refs and Schema-Typed Data](#entity-refs-and-schema-typed-data)
- [Webhooks (Advanced)](#webhooks-advanced)
- [Git LFS (Advanced)](#git-lfs-advanced)
- [SSH Key Management](#ssh-key-management)
- [Troubleshooting](#troubleshooting)

---

## Repository Management

### Listing repositories

```bash
# Via API (requires auth)
curl -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories

# Paginated
curl -H "Authorization: Bearer $TOKEN" \
  "https://git.example.com/api/v1/repositories?page=2&per_page=20"
```

The API returns paginated results (default 10 per page, max 100).

### Updating repository settings

```bash
curl -X PATCH \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"description": "Updated description", "is_private": true}' \
  https://git.example.com/api/v1/repositories/owner/repo
```

### Deleting a repository

Only the repository owner or an admin can delete a repository. This is irreversible.

```bash
curl -X DELETE \
  -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories/owner/repo
```

---

## Branch and Tag Operations

### Branches

```bash
# List branches
curl -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories/owner/repo/branches

# Create a branch via git push
git checkout -b feature/new-thing
git push -u origin feature/new-thing
```

**Branch limits**: Each repository has a configurable maximum branch count. If you hit this limit, contact your repository admin to increase it or delete unused branches.

### Tags

```bash
# List tags
git ls-remote --tags origin

# Create and push an annotated tag
git tag -a v1.0.0 -m "Release 1.0.0"
git push origin v1.0.0

# Delete a remote tag
git push origin --delete v1.0.0
```

### Protected refs

The following ref namespaces are protected and cannot be pushed to by non-admins:

| Namespace | Purpose |
|-----------|---------|
| `refs/adp/*` | Platform configuration |
| `refs/adp/keys/*` | SSH key mappings |
| `refs/adp/users/*` | User identity data |
| `refs/secrets/*` | Encrypted secrets |
| `refs/webhooks/*` | Webhook configurations |

---

## File Browsing

### Via Web UI

Navigate to `https://git.example.com/{owner}/{repo}` to browse files, branches, commits, and the ref tree.

### Via API

```bash
# View file tree at a ref
curl -H "Authorization: Bearer $TOKEN" \
  "https://git.example.com/api/v1/repositories/owner/repo/tree?ref=main"

# View a specific file (blob)
curl -H "Authorization: Bearer $TOKEN" \
  "https://git.example.com/api/v1/repositories/owner/repo/blob?path=README.md&ref=main"
```

### Via git

```bash
# Show file at specific commit
git show main:README.md

# List files in a directory
git ls-tree main src/

# Show file history
git log --follow -- path/to/file
```

---

## Access Control (User Perspective)

New repositories use a **deny-by-default** access model. The repository owner must explicitly grant permissions to collaborators.

### Permission levels

| Permission | Description |
|------------|-------------|
| `pull` | Read access — clone, fetch, browse |
| `push` | Write access — push to allowed refs |
| `admin` | Full control — manage access, delete repo |

### Checking your access

```bash
# View access configuration (if you have admin rights)
curl -H "Authorization: Bearer $TOKEN" \
  https://git.example.com/api/v1/repositories/owner/repo/access
```

### Granting access (owner/admin only)

Access is managed via the RBAC API or web UI. Typical flow:

1. Ask the collaborator for their username
2. Navigate to the repository settings in the web UI
3. Add the user with the appropriate permission level

### Path-level rules

Access can be scoped to specific ref patterns:

- `refs/heads/*` — all branches
- `refs/heads/release/*` — release branches only
- `refs/tags/*` — all tags

---

## Entity Refs and Schema-Typed Data

agentic-repos supports schema-typed data stored in git refs. These are validated on push to ensure data integrity.

### Built-in schemas

| Schema ID | Namespace | Description |
|-----------|-----------|-------------|
| `engram-task` | `refs/engram/task/*` | Task tracking |
| `engram-session` | `refs/engram/session/*` | Session records |
| `engram-adr` | `refs/engram/adr/*` | Architecture decisions |
| `engram-knowledge` | `refs/engram/knowledge/*` | Knowledge base |
| `webhook-config` | `refs/webhooks/*` | Webhook configurations |
| `deploy-target` | `refs/deploy/*` | Deployment targets |
| `ci-pipeline` | `refs/ci/*` | CI/CD pipelines |
| `team-member` | `refs/team/*` | Team membership |

### Writing entity refs

Entity data must be valid JSON conforming to the schema. Schema validation runs automatically on push:

```bash
# Example: write a webhook config
git hash-object -w <<EOF | xargs -I{} git update-ref refs/webhooks/my-hook {}
{
  "url": "https://example.com/hook",
  "events": ["push"],
  "secret": "whsec_..."
}
EOF
```

If the data doesn't match the schema, the push is rejected with a validation error.

---

## Webhooks (Advanced)

See [Getting Started](getting-started.md) for basic webhook setup. This section covers advanced topics.

### Webhook security

- All webhooks require a secret for HMAC-SHA256 signature verification
- Outbound webhooks are restricted to HTTPS URLs (SSRF prevention)
- Secrets are never exposed through the API — only metadata is returned
- Signature header: `X-ADP-Signature-256`

### Event types

| Event | Trigger |
|-------|---------|
| `push` | Any ref update |
| `tag.create` | New tag created |
| `tag.delete` | Tag deleted |
| `branch.create` | New branch created |
| `branch.delete` | Branch deleted |

### Retry behavior

Failed deliveries are retried up to 3 times with exponential backoff. Check delivery status via the API.

---

## Git LFS (Advanced)

See [Getting Started](getting-started.md) for basic LFS setup.

### Quotas

Each repository has an LFS storage quota. When the quota is exceeded, new LFS uploads are rejected until space is freed or the quota is increased.

```bash
# Check LFS status
git lfs ls-files --all
```

### File locking

Use file locking to prevent concurrent editing of binary files:

```bash
# Lock a file
git lfs lock path/to/file.psd

# List your locks
git lfs locks --path path/to/

# Unlock when done
git lfs unlock path/to/file.psd
```

### LFS authentication

LFS operations use JWT tokens issued during the git push/pull handshake. These tokens are scoped to the specific operation (upload or download) and expire after 1 hour.

---

## SSH Key Management

### Registering a new key

Keys are stored in the system repository at `refs/adp/users/{username}/ssh_keys`. To register a new key:

1. Generate a key: `ssh-keygen -t ed25519 -C "your-username"`
2. Provide the public key to an administrator
3. The admin registers it via the API or `agentic-repos init --admin-key`

### Multiple keys

You can register multiple SSH keys. Each key is stored by its SHA-256 fingerprint and mapped to your username.

### Key rotation

To replace a key:

1. Register the new key
2. Remove the old key via the API or admin interface
3. Update your SSH config if needed

### SSH config

```
Host git.example.com
    Port 2222
    User your-username
    IdentityFile ~/.ssh/id_ed25519
    IdentitiesOnly yes
```

---

## Troubleshooting

### Push rejected: "Too many ref updates"

Each push is limited to 10,000 ref updates. Split your push into smaller batches.

### Push rejected: "Schema resolution failed"

Schema-typed refs must contain valid JSON matching their schema. Check the ref data and fix any validation errors.

### Push rejected: "Protected ref"

You're trying to push to a protected namespace (refs starting with `refs/adp/`, `refs/secrets/`, etc.). Only admins can push to these refs.

### Push rejected: "access denied"

The default access policy is deny-all. Ask the repository owner to grant you push permission.

### SSH: "authentication rejected"

- Verify your key is registered: `ssh -T git@git.example.com`
- Check your SSH config matches the server's port
- Ensure you're using the correct username

### Clone fails: "repository not found"

- Check the owner/repo name is correct
- Verify the repository exists and you have pull access
- For private repos, ensure you're authenticated

### LFS: "quota exceeded"

Contact the repository admin to increase the LFS storage quota or remove unused LFS objects.

---

## Further Reading

- [Getting Started](getting-started.md) — Initial setup guide
- [Configuration Reference](configuration.md) — Server configuration
- [Admin Guide](admin-guide.md) — Administration guide
- [API Reference](API_REFERENCE.md) — Full REST API documentation
- [Developer Guide](developer-guide.md) — Contributing to agentic-repos

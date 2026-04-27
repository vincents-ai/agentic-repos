# Getting Started — agentic-repos for Users

This guide walks you through creating repositories, pushing code, using Git LFS, and setting up webhooks on your agentic-repos server.

> **Username rules**: Usernames must start with a lowercase letter and contain
> only lowercase letters, digits, hyphens, and underscores. Maximum 256 characters.
> Examples: `alice`, `bob-dev`, `charlie_123`.

> **New repos are access-restricted by default**: After creating a repository,
> the owner must grant explicit access to collaborators via the RBAC API or
> web UI. No one gets push access unless explicitly permitted.

## Prerequisites

- **Git** installed on your machine ([git-scm.com](https://git-scm.com))
- **SSH key** generated (for SSH access)
- **Git LFS** installed (for large file support — [git-lfs.github.com](https://git-lfs.github.com))
- Your agentic-repos server URL and credentials from your administrator

## SSH Setup

### 1. Generate an SSH key (if you don't have one)

```bash
ssh-keygen -t ed25519 -C "you@example.com"
```

### 2. Have the administrator register your key

```bash
cat ~/.ssh/id_ed25519.pub
# Copy the output and send it to your admin
```

Your admin will register it via `agentic-repos init --admin-key` or the RBAC API.
The key is stored in the system repo at `refs/adp/users/{username}/ssh_keys`.

### 3. Test the connection

```bash
ssh -p 2222 you@git.example.com
# Should connect and show server info (not a shell)
```

### 4. Configure SSH (optional)

Add to `~/.ssh/config` for convenience:

```
Host git.example.com
    Port 2222
    User your-username
    IdentityFile ~/.ssh/id_ed25519
```

## Creating a Repository

### Via Web UI

1. Open `https://git.example.com/ui/` in your browser
2. Click "New Repository"
3. Enter repository name and description
4. Click "Create"

### Via API

```bash
curl -X POST https://git.example.com/api/v1/repositories/your-username \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"name": "my-project", "description": "My new project"}'
```

### Via SSH (implicit)

Simply clone a non-existent repository path and push — the server creates it on first push:

```bash
mkdir my-project && cd my-project
git init
git remote add origin ssh://git.example.com/repos/your-username/my-project.git
echo "# Hello" > README.md
git add . && git commit -m "initial commit"
git push -u origin main
```

## Cloning a Repository

### Via SSH

```bash
git clone ssh://git.example.com/repos/owner/project.git
```

If you configured `~/.ssh/config`:

```bash
git clone ssh://git.example.com/repos/owner/project.git
```

### Via HTTP

```bash
git clone https://git.example.com/repos/owner/project.git
```

## Day-to-Day Workflow

### Pushing changes

```bash
cd my-project
# Make changes...
git add .
git commit -m "fix: resolve login bug"
git push
```

### Branching

```bash
# Create and switch to a new branch
git checkout -b feature/new-ui

# Push the branch
git push -u origin feature/new-ui

# After review, merge to main
git checkout main
git merge feature/new-ui
git push
```

### Pulling changes

```bash
git pull
# Or to rebase instead of merge
git pull --rebase
```

## Git LFS — Large File Support

Git LFS stores large files (images, videos, datasets, binaries) outside the Git repository while keeping track of them as pointers.

### Setup

```bash
# Install Git LFS (one-time)
git lfs install

# In your repository
cd my-project
git lfs install
```

### Track large files

```bash
# Track specific file types
git lfs track "*.psd"
git lfs track "*.zip"
git lfs track "*.mp4"
git lfs track "datasets/**"

# This creates/updates .gitattributes
cat .gitattributes
# *.psd filter=lfs diff=lfs merge=lfs -text
# *.zip filter=lfs diff=lfs merge=lfs -text
```

### Commit and push with LFS

```bash
git add .gitattributes
git commit -m "track large files with LFS"

# Add large files normally — LFS handles them automatically
cp ~/large-design.psd .
git add large-design.psd
git commit -m "add design file"
git push
```

### Check LFS status

```bash
git lfs ls-files
# Show tracked files and their sizes

git lfs status
# Show pending LFS uploads
```

### LFS file locking

Prevent concurrent editing of binary files:

```bash
# Lock a file
git lfs lock design.psd

# List locks
git lfs locks

# Unlock when done
git lfs unlock design.psd
```

### Migrating an existing repo to LFS

See [LFS Migration Guide](LFS_MIGRATION.md) for converting an existing repository with large files in Git history.

## Webhooks

Webhooks notify external services when events happen in your repository.

### Creating a webhook

```bash
curl -X POST https://git.example.com/api/v1/webhooks \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "url": "https://ci.example.com/hook",
    "events": ["push"],
    "secret": "your-hmac-secret"
  }'
```

### Supported events

| Event | Trigger |
|-------|---------|
| `push` | Any branch or tag push |
| `push:main` | Push to main branch only |
| `tag` | Tag creation |
| `delete` | Branch or tag deletion |

### Webhook payload

Push events deliver a JSON payload:

```json
{
  "event": "push",
  "repository": "owner/project",
  "ref": "refs/heads/main",
  "before": "abc123",
  "after": "def456",
  "commits": [
    {
      "id": "def456",
      "message": "fix: resolve login bug",
      "author": "you@example.com",
      "timestamp": "2026-04-20T10:00:00Z"
    }
  ]
}
```

### Verifying webhook signatures

Webhooks include an `X-Hub-Signature-256` header with an HMAC-SHA256 signature:

```python
import hmac, hashlib

def verify_signature(payload, secret, signature):
    expected = "sha256=" + hmac.new(
        secret.encode(), payload, hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, signature)
```

### Testing webhooks

```bash
# Send a test delivery
curl -X POST https://git.example.com/api/v1/webhooks/{id}/test \
  -H "Authorization: Bearer $TOKEN"

# List recent deliveries
curl https://git.example.com/api/v1/webhooks/{id}/deliveries \
  -H "Authorization: Bearer $TOKEN"

# Retry a failed delivery
curl -X POST https://git.example.com/api/v1/webhooks/{id}/deliveries/{did}/retry \
  -H "Authorization: Bearer $TOKEN"
```

## Browsing Repositories

### Web UI

Open `https://git.example.com/ui/` to browse repositories, view files, branches, and commits.

### API

```bash
# List your repositories
curl https://git.example.com/api/v1/repositories \
  -H "Authorization: Bearer $TOKEN"

# View repository details
curl https://git.example.com/api/v1/repositories/owner/project \
  -H "Authorization: Bearer $TOKEN"

# List branches
curl https://git.example.com/api/v1/repositories/owner/project/branches \
  -H "Authorization: Bearer $TOKEN"

# View file tree
curl https://git.example.com/api/v1/repositories/owner/project/tree?path=src \
  -H "Authorization: Bearer $TOKEN"
```

## Common Issues

### SSH: "Permission denied"

```bash
# Check your key is registered
ssh -p 2222 -v you@git.example.com

# Verify your SSH config
cat ~/.ssh/config

# Try specifying the key explicitly
git clone -c core.sshCommand="ssh -i ~/.ssh/id_ed25519" \
  ssh://git.example.com/repos/owner/project.git
```

### LFS: "401 Unauthorized"

LFS requires a valid JWT. Contact your admin to verify the server's LFS configuration is correct.

### LFS: "413 Request Entity Too Large"

Your reverse proxy may have a body size limit. Ask your admin to increase `client_max_body_size` in Nginx.

### Push rejected: "branch limit exceeded"

Your repository has a branch limit configured. Delete unused branches or ask your admin to increase the limit.

## Further Reading

- [SSH Setup Guide](SSH_SETUP.md) — detailed SSH configuration
- [LFS Client Setup](LFS_CLIENT_SETUP.md) — comprehensive LFS configuration
- [LFS Migration](LFS_MIGRATION.md) — converting existing repos
- [API Reference](API_REFERENCE.md) — full REST API documentation

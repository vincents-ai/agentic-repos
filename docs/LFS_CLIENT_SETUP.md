# User Guide: Configuring Git LFS Client

This guide shows you how to configure the Git LFS client to work with your repos-mcp server.

## Prerequisites

- Git LFS client installed on your machine
- Git LFS enabled on repos-mcp server
- SSH access to the repos-mcp server
- Git repository (new or existing)

## Step 1: Install Git LFS Client

### Linux (Debian/Ubuntu)
```bash
sudo apt-get install git-lfs
```

### Linux (Fedora/RHEL)
```bash
sudo dnf install git-lfs
```

### macOS (Homebrew)
```bash
brew install git-lfs
```

### Windows
Download from [git-lfs.com](https://git-lfs.com/) or use:
```powershell
choco install git-lfs
```

### Verify Installation
```bash
git lfs version
# Should output: git-lfs/3.x.x
```

## Step 2: Initialize Git LFS

Initialize Git LFS in your user account (one-time setup):

```bash
git lfs install
```

This configures Git to use LFS hooks globally.

**Output**:
```
Updated Git hooks.
Git LFS initialized.
```

## Step 3: Configure SSH Access

Ensure you can SSH to the repos-mcp server:

```bash
# Test SSH connection
ssh -T git@your-server.com

# If using custom port
ssh -T -p 2222 git@your-server.com
```

### SSH Configuration

Add to `~/.ssh/config` for convenience:

```
Host repos-mcp
    HostName your-server.com
    Port 2222
    User git
    IdentityFile ~/.ssh/id_ed25519
```

Now you can use `repos-mcp` as hostname:
```bash
git clone repos-mcp:tenant/repo.git
```

## Step 4: Clone or Initialize Repository

### For New Repository:

```bash
# Create new repository
mkdir my-project
cd my-project
git init

# Add remote (adjust URL for your server)
git remote add origin repos-mcp:tenant/myproject.git
```

### For Existing Repository:

```bash
# Clone from repos-mcp
git clone repos-mcp:tenant/existing-repo.git
cd existing-repo
```

## Step 5: Configure LFS Tracking

Tell Git LFS which files to track based on patterns:

```bash
# Track all PSD files
git lfs track "*.psd"

# Track all video files
git lfs track "*.mp4"
git lfs track "*.mov"

# Track large data files
git lfs track "*.bin"
git lfs track "data/*.csv"

# Track specific directories
git lfs track "assets/videos/**"
```

This creates/updates `.gitattributes`:

```bash
cat .gitattributes
```

Output:
```
*.psd filter=lfs diff=lfs merge=lfs -text
*.mp4 filter=lfs diff=lfs merge=lfs -text
*.mov filter=lfs diff=lfs merge=lfs -text
```

**Important**: Commit `.gitattributes` to share LFS configuration:
```bash
git add .gitattributes
git commit -m "Configure Git LFS tracking"
```

## Step 6: Add and Commit LFS Files

Now add files that match your tracking patterns:

```bash
# Add large files
cp /path/to/design.psd assets/
git add assets/design.psd

# Git LFS automatically handles it
git commit -m "Add design file"
```

Verify file is tracked by LFS:
```bash
git lfs ls-files
```

Output shows LFS-tracked files:
```
12ab34cd56 * assets/design.psd
```

## Step 7: Push to Server

Push your changes:

```bash
git push origin main
```

**What Happens**:
1. Git pushes regular files via SSH
2. Git LFS authenticates via SSH (`git-lfs-authenticate`)
3. Server returns JWT token and HTTP URL
4. Git LFS uploads large files via HTTP
5. Pointer files are committed to Git

**Expected Output**:
```
Uploading LFS objects: 100% (1/1), 15 MB | 2.1 MB/s, done.
Enumerating objects: 5, done.
...
To repos-mcp:tenant/myproject.git
   abc1234..def5678  main -> main
```

## Step 8: Clone and Pull with LFS

When others clone the repository:

```bash
git clone repos-mcp:tenant/myproject.git
cd myproject

# LFS files are automatically downloaded
```

To update and fetch LFS objects:

```bash
git pull
```

## Common Tracking Patterns

### Design Files
```bash
git lfs track "*.psd"
git lfs track "*.ai"
git lfs track "*.sketch"
git lfs track "*.fig"
git lfs track "*.xd"
```

### Video and Audio
```bash
git lfs track "*.mp4"
git lfs track "*.mov"
git lfs track "*.avi"
git lfs track "*.mp3"
git lfs track "*.wav"
```

### 3D Models and CAD
```bash
git lfs track "*.blend"
git lfs track "*.obj"
git lfs track "*.fbx"
git lfs track "*.dae"
git lfs track "*.stl"
```

### Archives and Binaries
```bash
git lfs track "*.zip"
git lfs track "*.tar.gz"
git lfs track "*.exe"
git lfs track "*.dll"
git lfs track "*.so"
```

### Data Files
```bash
git lfs track "*.csv"
git lfs track "*.parquet"
git lfs track "*.db"
git lfs track "*.sqlite"
```

### Documents
```bash
git lfs track "*.pdf"
git lfs track "*.docx"
git lfs track "*.pptx"
```

## Advanced Configuration

### Check LFS Configuration

```bash
# View LFS config
git lfs env

# View tracked patterns
git lfs track

# View LFS-managed files
git lfs ls-files
```

### Fetch LFS Files Separately

```bash
# Clone without LFS files
GIT_LFS_SKIP_SMUDGE=1 git clone repos-mcp:tenant/repo.git

# Fetch LFS files later
git lfs pull
```

### Fetch Specific Files Only

```bash
# Pull only specific files
git lfs pull --include="path/to/file.psd"

# Exclude certain paths
git lfs pull --exclude="videos/*"
```

### Prune Old LFS Objects

Free disk space by removing old LFS objects:

```bash
# Remove LFS files older than 7 days
git lfs prune --verify-remote --recent

# Dry run to see what would be deleted
git lfs prune --dry-run
```

## Verifying LFS Setup

### Check if File is in LFS

```bash
# Method 1: List LFS files
git lfs ls-files

# Method 2: Check file directly
git lfs ls-files | grep filename.psd

# Method 3: Inspect pointer
git show HEAD:path/to/file.psd
# LFS files show: "version https://git-lfs.github.com/spec/v1"
```

### Check LFS Status

```bash
# See pending LFS uploads
git lfs status

# See LFS storage used
git lfs ls-files --size
```

## Troubleshooting

### "batch response: Repository or object not found"

**Cause**: LFS not enabled on server or wrong remote URL

**Solution**:
```bash
# Verify remote URL
git remote -v

# Test SSH access
ssh repos-mcp git-lfs-authenticate tenant/repo.git download

# Should return JSON with token and URL
```

### "error: external filter 'git-lfs filter-process' failed"

**Cause**: Git LFS not installed or not initialized

**Solution**:
```bash
# Install git-lfs
# (see Step 1)

# Initialize
git lfs install

# Verify
git lfs version
```

### Files Not Being Tracked by LFS

**Cause**: Files added before LFS tracking was configured

**Solution**:
```bash
# Migrate existing files to LFS
git lfs migrate import --include="*.psd"

# Or manually re-add files
git rm --cached *.psd
git add *.psd
git commit -m "Migrate PSD files to LFS"
```

### "SSH: Permission denied"

**Cause**: SSH key not authorized or wrong username

**Solution**:
```bash
# Ensure public key is in authorized_keys on server
cat ~/.ssh/id_ed25519.pub
# Add this to server's authorized_keys

# Test SSH
ssh -T git@your-server.com
```

### Slow Uploads/Downloads

**Cause**: Network speed or server performance

**Mitigations**:
```bash
# Increase transfer concurrency
git config lfs.concurrenttransfers 8

# Use custom transfer agent (advanced)
git config lfs.transfer.maxretries 3
```

## Performance Tips

### Exclude Unnecessary Files from LFS

Not everything needs LFS. Use it for:
- Files > 1 MB
- Binary files that change frequently
- Media files (images, videos, audio)

Don't use LFS for:
- Source code files
- Small text files
- Files that compress well

### Partial Clone for Large Repositories

```bash
# Clone without LFS files initially
GIT_LFS_SKIP_SMUDGE=1 git clone repos-mcp:tenant/repo.git

# Download only files you need
cd repo
git lfs pull --include="assets/needed-file.psd"
```

### Limit LFS Bandwidth

```bash
# Limit upload/download speed (in KB/s)
git config lfs.transfer.maxretries 10
git config lfs.transfer.maxretrydelay 10
```

## Uninstalling/Disabling LFS

### Remove LFS from Repository

```bash
# Untrack all LFS patterns
git lfs untrack "*.psd"

# Or remove .gitattributes
rm .gitattributes
git add .gitattributes
git commit -m "Remove LFS tracking"
```

### Migrate LFS Files Back to Git

```bash
# Export LFS files to regular Git
git lfs migrate export --include="*.psd" --everything
```

### Uninstall LFS Client

```bash
# Uninstall hooks
git lfs uninstall

# Remove package (Linux)
sudo apt-get remove git-lfs
```

## Working in Teams

### Share LFS Configuration

Ensure `.gitattributes` is committed:

```bash
git add .gitattributes
git push origin main
```

Team members will automatically use the same LFS tracking patterns.

### Onboarding New Team Members

1. Install Git LFS client
2. Clone repository (LFS files download automatically)
3. No additional configuration needed

### CI/CD Integration

In CI/CD pipelines:

```bash
# Install git-lfs in CI environment
apt-get install git-lfs

# Initialize
git lfs install

# Clone (automatically fetches LFS)
git clone repos-mcp:tenant/repo.git
```

## Next Steps

- **Migrate Existing Repository**: See [Migrating to LFS](./LFS_MIGRATION.md)
- **Troubleshooting**: See [Troubleshooting Guide](./LFS_TROUBLESHOOTING.md)
- **Administration**: See [Admin Guide](./LFS_ADMIN_GUIDE.md)

## Additional Resources

- [Official Git LFS Documentation](https://git-lfs.com/)
- [Git LFS Tutorial](https://github.com/git-lfs/git-lfs/wiki/Tutorial)
- [repos-mcp LFS API Reference](./LFS_API.md)

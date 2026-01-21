# Migrating Existing Repositories to Git LFS

This guide walks through the process of migrating an existing Git repository to use Git LFS for large file management.

## Table of Contents

1. [Before You Begin](#before-you-begin)
2. [Migration Strategy](#migration-strategy)
3. [Step-by-Step Migration](#step-by-step-migration)
4. [Rewriting History (Optional)](#rewriting-history-optional)
5. [Team Coordination](#team-coordination)
6. [Verification](#verification)
7. [Troubleshooting](#troubleshooting)

## Before You Begin

### Prerequisites

- Git LFS client installed (see [LFS_CLIENT_SETUP.md](LFS_CLIENT_SETUP.md))
- Server-side LFS enabled (see [LFS_ENABLING_SERVER.md](LFS_ENABLING_SERVER.md))
- Repository cloned locally
- Team coordination plan (if working with others)
- **Backup of repository** (critical!)

### Important Considerations

**Migration Types:**
1. **Forward-only migration**: Only new large files use LFS (simplest, recommended)
2. **Full history rewrite**: Converts historical large files to LFS (complex, requires force push)

**Impacts:**
- Full history rewrites require all team members to re-clone
- Forward-only migration preserves existing history
- Storage savings depend on migration strategy

**Backup First:**
```bash
# Create backup
git clone --mirror your-repo.git backup-$(date +%Y%m%d).git

# Or tar existing clone
tar -czf repo-backup-$(date +%Y%m%d).tar.gz /path/to/repo
```

## Migration Strategy

### Strategy 1: Forward-Only Migration (Recommended)

**Best for:**
- Active repositories with ongoing development
- Teams that want minimal disruption
- Repositories where historical large files are rarely accessed

**Process:**
1. Configure `.gitattributes` with LFS tracking patterns
2. Commit configuration changes
3. Future commits automatically use LFS
4. Existing large files remain in Git history

**Advantages:**
- No history rewrite needed
- No team coordination required
- Zero disruption to existing clones
- Can be done incrementally

**Disadvantages:**
- Historical large files still consume Git storage
- Clone size includes old large files
- Storage savings only apply to new files

### Strategy 2: Full History Migration

**Best for:**
- Repositories with significant historical large files
- Projects starting fresh migration effort
- When maximum storage savings needed
- Smaller teams that can coordinate

**Process:**
1. Use `git lfs migrate` to rewrite history
2. Convert historical large files to LFS pointers
3. Force push rewritten history
4. All team members must re-clone

**Advantages:**
- Maximum storage savings
- Smaller repository clone size
- All large files in LFS

**Disadvantages:**
- Requires history rewrite (destructive)
- Requires force push
- All team members must re-clone
- Breaks existing clones and forks
- More complex coordination

## Step-by-Step Migration

### Strategy 1: Forward-Only Migration

#### Step 1: Identify Large Files

```bash
# Find large files in current working tree
find . -type f -size +50M

# Find largest files in repository history
git rev-list --objects --all |
  git cat-file --batch-check='%(objecttype) %(objectname) %(objectsize) %(rest)' |
  sed -n 's/^blob //p' |
  sort --numeric-sort --key=2 --reverse |
  head -20 |
  numfmt --field=2 --to=iec-i --suffix=B --padding=7

# Or use git-sizer
git-sizer --verbose
```

#### Step 2: Configure LFS Tracking

```bash
# Track specific file types
git lfs track "*.psd"
git lfs track "*.zip"
git lfs track "*.mp4"
git lfs track "*.bin"

# Track specific directories
git lfs track "data/**"
git lfs track "assets/models/**"

# Verify tracking configuration
cat .gitattributes
```

#### Step 3: Commit Configuration

```bash
# Add .gitattributes
git add .gitattributes

# Commit changes
git commit -m "Configure Git LFS tracking"

# Push to server
git push origin main
```

#### Step 4: Add Existing Large Files (Optional)

If you want to immediately convert some existing files:

```bash
# Remove from Git but keep in working directory
git rm --cached path/to/large-file.bin

# Re-add (will be tracked by LFS due to .gitattributes)
git add path/to/large-file.bin

# Commit
git commit -m "Convert large-file.bin to LFS"

# Push
git push origin main
```

#### Step 5: Team Notification

Notify team members to:
1. Pull latest changes: `git pull`
2. Ensure they have Git LFS installed
3. Run `git lfs install` if not already done

**That's it!** Forward migration is complete.

### Strategy 2: Full History Migration

**WARNING: This rewrites repository history. All team members must re-clone.**

#### Step 1: Create Clean Working Environment

```bash
# Clone fresh copy
git clone https://your-server.com/org/repo.git repo-migration
cd repo-migration

# Verify clean state
git status
```

#### Step 2: Install Git LFS

```bash
git lfs install
```

#### Step 3: Analyze Repository

```bash
# Find file types to migrate
git lfs migrate info --everything

# Example output shows file types and sizes:
# *.psd  	1.2 GB 	15 files
# *.zip  	850 MB 	42 files
# *.mp4  	2.3 GB 	8 files
```

#### Step 4: Migrate History

**Option A: Migrate specific file types**

```bash
# Migrate all branches
git lfs migrate import --include="*.psd,*.zip,*.mp4" --everything

# Or migrate specific branches
git lfs migrate import --include="*.psd,*.zip,*.mp4" \
  --include-ref=refs/heads/main \
  --include-ref=refs/heads/develop
```

**Option B: Migrate files above size threshold**

```bash
# Migrate all files larger than 100MB
git lfs migrate import --above=100MB --everything
```

**Option C: Migrate specific paths**

```bash
# Migrate entire directories
git lfs migrate import --include="data/**,assets/models/**" --everything
```

#### Step 5: Verify Migration

```bash
# Check LFS objects
git lfs ls-files

# Verify repository size
du -sh .git

# Check tracking configuration
cat .gitattributes
```

#### Step 6: Test Locally

```bash
# Test checkout
git checkout main
ls -lh path/to/migrated/files

# Verify files are accessible
file path/to/large-file.psd
```

#### Step 7: Force Push (DESTRUCTIVE)

```bash
# Push rewritten history (WARNING: Destructive!)
git push origin --force --all

# Push LFS objects
git lfs push origin --all
```

#### Step 8: Team Coordination

**Immediately notify all team members:**

1. **Delete existing clones**:
```bash
cd ~/projects
rm -rf old-repo
```

2. **Clone fresh repository**:
```bash
git clone https://your-server.com/org/repo.git
cd repo
```

3. **Verify LFS setup**:
```bash
git lfs install
git lfs pull
git lfs ls-files
```

## Rewriting History (Optional)

### Using `git filter-repo` with LFS

For complex history rewrites:

```bash
# Install git-filter-repo
pip install git-filter-repo

# Create analysis
git filter-repo --analyze

# Review analysis
cat .git/filter-repo/analysis/*

# Rewrite with custom rules
git filter-repo --path-rename OLD:NEW --replace-text expressions.txt

# Then migrate to LFS
git lfs migrate import --everything --above=50MB
```

### Pruning Old LFS Objects

After migration, clean up old LFS objects:

```bash
# Prune old LFS objects not referenced
git lfs prune

# Verify cleanup
git lfs prune --verify-remote

# More aggressive cleanup (30 days old)
git lfs prune --verify-remote --recent 30d
```

## Team Coordination

### Communication Plan

**Before Migration:**
1. **Announce migration** 1-2 weeks in advance
2. **Freeze active development** during migration window
3. **Ensure all work is pushed** before migration starts
4. **Verify everyone has LFS installed**

**During Migration:**
1. **Disable push access** temporarily (optional)
2. **Perform migration** on test repository first
3. **Verify migration** thoroughly
4. **Push to production** when verified
5. **Re-enable push access**

**After Migration:**
1. **Notify team to re-clone** (full history migration)
2. **Provide updated documentation**
3. **Monitor for issues** during first week
4. **Provide support** for team members

### Sample Team Message

```
Subject: Git LFS Migration - Action Required

Team,

We're migrating our repository to Git LFS on [DATE] at [TIME].

REQUIRED ACTIONS:
1. Push all uncommitted work before [DEADLINE]
2. Ensure git-lfs is installed: `git lfs version`
3. After migration: re-clone the repository

TIMELINE:
- [DATE 9:00 AM]: Freeze commits
- [DATE 10:00 AM]: Begin migration
- [DATE 11:00 AM]: Migration complete, re-clone available

STEPS TO RE-CLONE:
```bash
cd ~/projects
mv old-repo old-repo.backup
git clone https://server.com/org/repo.git
cd repo
git lfs install
```

Questions? Contact [ADMIN]
```

## Verification

### Post-Migration Checks

**1. Verify LFS tracking:**
```bash
# Check .gitattributes
cat .gitattributes

# List LFS files
git lfs ls-files

# Verify specific file is in LFS
git lfs ls-files | grep filename
```

**2. Verify repository size:**
```bash
# Check .git directory size (should be smaller after full migration)
du -sh .git

# Check working directory size
du -sh .
```

**3. Verify file access:**
```bash
# Clone fresh and verify files work
git clone https://server.com/org/repo.git test-clone
cd test-clone
git lfs pull
# Test opening large files in applications
```

**4. Verify LFS storage:**
```bash
# Check LFS objects stored
git lfs ls-files -l

# Verify server has objects
git lfs fetch --all
```

**5. Test workflow:**
```bash
# Make changes to LFS file
echo "test" >> large-file.psd
git add large-file.psd
git commit -m "Test LFS commit"
git push origin main

# Clone elsewhere and verify
cd /tmp
git clone https://server.com/org/repo.git verify-test
cd verify-test
git lfs ls-files
```

## Troubleshooting

### Migration Issues

**Problem: `git lfs migrate` fails with "ambiguous argument"**
```bash
# Solution: Specify branch explicitly
git lfs migrate import --include="*.psd" --include-ref=refs/heads/main
```

**Problem: Migration takes forever**
```bash
# Solution: Migrate in batches or specific branches
git lfs migrate import --include="*.psd" --include-ref=refs/heads/main

# Then migrate other branches separately
git lfs migrate import --include="*.psd" --include-ref=refs/heads/develop
```

**Problem: Force push rejected**
```bash
# Ensure branch protection allows force push
# Or temporarily disable branch protection

# Verify remote state
git fetch origin
git log origin/main

# Force push with lease for safety
git push origin main --force-with-lease
```

### Post-Migration Issues

**Problem: Team member gets "filter error" after re-cloning**
```bash
# Solution: Ensure LFS is installed
git lfs install

# Pull LFS objects
git lfs pull
```

**Problem: Large repository clone size after migration**
```bash
# Solution: Ensure full LFS migration completed
git lfs ls-files  # Should show migrated files

# If files missing from LFS, migration may have failed
# Re-run migration on specific files
git lfs migrate import --include="*.psd" --everything
```

**Problem: Missing LFS objects error**
```bash
# Solution: Push LFS objects to server
git lfs push origin --all

# Verify server has objects
git lfs fetch --all
```

**Problem: Old clones still work but don't use LFS**
```bash
# Solution: Old clones must be updated
cd old-repo
git pull
git lfs install
git lfs pull

# Or better: re-clone fresh
cd ..
mv old-repo old-repo.backup
git clone https://server.com/org/repo.git
```

### Rollback Procedures

**If migration goes wrong:**

**1. Restore from backup (full history migration):**
```bash
# Restore server repository from backup
cd /path/to/git-server
rm -rf repo.git
cp -r backup-20260122.git repo.git
```

**2. Revert forward-only migration:**
```bash
# Simply revert the .gitattributes commit
git revert <commit-hash>
git push origin main

# Remove LFS files
git lfs uninstall
```

**3. Re-clone from backup:**
```bash
# Team members can re-clone from backup
git clone https://backup-server.com/org/repo.git
```

## Best Practices

1. **Always backup before migration**
2. **Test migration on copy first**
3. **Prefer forward-only for active repos**
4. **Coordinate with team for full migration**
5. **Document specific patterns tracked**
6. **Monitor storage usage post-migration**
7. **Provide team training on LFS workflows**
8. **Have rollback plan ready**

## Related Documentation

- [LFS Client Setup](LFS_CLIENT_SETUP.md) - Installing and configuring git-lfs
- [LFS Server Setup](LFS_ENABLING_SERVER.md) - Enabling LFS on server
- [LFS API Reference](LFS_API.md) - API endpoints
- [LFS Troubleshooting](LFS_TROUBLESHOOTING.md) - Common issues

## Additional Resources

- [Git LFS Migration Documentation](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-migrate.adoc)
- [Atlassian Git LFS Tutorial](https://www.atlassian.com/git/tutorials/git-lfs)
- [GitHub LFS Migration Guide](https://docs.github.com/en/repositories/working-with-files/managing-large-files)

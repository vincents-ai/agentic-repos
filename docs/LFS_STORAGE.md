# Git LFS Storage Backend Configuration

Documentation for configuring and understanding the LFS storage backend.

## Table of Contents

1. [Storage Overview](#storage-overview)
2. [LocalStorage Backend](#localstorage-backend)
3. [Storage Path Patterns](#storage-path-patterns)
4. [Storage Operations](#storage-operations)
5. [Storage Management](#storage-management)
6. [Performance Considerations](#performance-considerations)
7. [Future Storage Backends](#future-storage-backends)

## Storage Overview

### Architecture

The LFS storage system uses a pluggable backend architecture:

```
┌──────────────────┐
│   LFS Handler    │
└────────┬─────────┘
         │ LfsStorage trait
         ▼
┌──────────────────┐
│ Storage Backend  │
│  (LocalStorage)  │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│   Filesystem     │
│  (disk storage)  │
└──────────────────┘
```

### Storage Trait

**Location**: `src/lfs/storage.rs:20-50`

```rust
#[async_trait]
pub trait LfsStorage: Send + Sync {
    /// Store an LFS object
    async fn store(&self, tenant: &str, repo: &str, oid: &str, data: Bytes) 
        -> Result<(), StorageError>;
    
    /// Retrieve an LFS object
    async fn retrieve(&self, tenant: &str, repo: &str, oid: &str) 
        -> Result<Bytes, StorageError>;
    
    /// Check if an LFS object exists
    async fn exists(&self, tenant: &str, repo: &str, oid: &str) 
        -> Result<bool, StorageError>;
    
    /// Get object size
    async fn size(&self, tenant: &str, repo: &str, oid: &str) 
        -> Result<u64, StorageError>;
}
```

This trait allows for different storage backends (filesystem, S3, etc.) to be implemented in the future.

## LocalStorage Backend

### Overview

**LocalStorage** stores LFS objects directly on the filesystem using a content-addressed structure.

**Location**: `src/lfs/storage.rs:100-250`

### Configuration

```yaml
lfs:
  enabled: true
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
  max_object_size: 5368709120  # 5GB
```

### Storage Structure

LFS objects are stored using a content-addressed approach with the SHA256 OID:

```
test_repositories/
└── {tenant}/
    └── {repo}.git/
        └── lfs/
            └── objects/
                └── {oid[0:2]}/      # First 2 chars of OID (fan-out)
                    └── {oid[2:4]}/  # Next 2 chars (sub-fan-out)
                        └── {oid}    # Full OID as filename
```

**Example**:
```
test_repositories/
└── myorg/
    └── myrepo.git/
        └── lfs/
            └── objects/
                └── ab/
                    └── cd/
                        └── abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890
```

### Path Pattern Syntax

**Pattern variables**:
- `{tenant}` - Organization/tenant identifier
- `{repo}` - Repository name
- `{oid}` - Full SHA256 object identifier (64 hex chars)
- `{oid[0:2]}` - Substring of OID (characters 0-2)
- `{oid[2:4]}` - Substring of OID (characters 2-4)

**Default pattern**:
```
{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}
```

This creates a two-level fan-out structure for efficient filesystem performance.

### Why Fan-Out Structure?

**Problem**: Storing millions of files in a single directory causes performance issues:
- Slow directory listings
- Inode exhaustion
- Poor filesystem performance

**Solution**: Fan-out distributes files across subdirectories:
- `{oid[0:2]}` creates 256 subdirectories (00-ff)
- `{oid[2:4]}` creates 256 more subdirectories per parent
- Total: 65,536 possible leaf directories
- Typical: ~1,000 files per directory (millions of objects)

**Example distribution**:
```
SHA256: abcdef1234567890...
        ^^    → ab/
          ^^  → cd/
            ^^^^^^^...  → full OID as filename
```

## Storage Path Patterns

### Custom Patterns

You can customize the storage path pattern based on your needs:

**Default (two-level fan-out)**:
```yaml
storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
```

**Single-level fan-out** (simpler, less scalable):
```yaml
storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid}"
```

**Three-level fan-out** (more scalable):
```yaml
storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid[4:6]}/{oid}"
```

**Flat structure** (NOT recommended):
```yaml
storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid}"
```

**By date** (for archival use cases):
```yaml
# Note: Currently {date} not implemented, but pattern shows intent
storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{date}/{oid[0:2]}/{oid}"
```

### Pattern Recommendations

| Repository Size | Objects Count | Recommended Pattern |
|-----------------|---------------|---------------------|
| Small (<1GB) | <1,000 | Single-level fan-out |
| Medium (1-50GB) | 1,000-100,000 | Two-level fan-out (default) |
| Large (50GB+) | 100,000+ | Two or three-level fan-out |
| Very Large (1TB+) | 1,000,000+ | Three-level fan-out |

## Storage Operations

### Store Operation

**Flow**:
1. Receive binary data from HTTP upload
2. Compute SHA256 hash of data
3. Verify hash matches OID from URL
4. Construct storage path from pattern
5. Create parent directories if needed
6. Write data atomically to disk
7. Verify file written successfully

**Implementation**: `src/lfs/storage.rs:120-180`

```rust
async fn store(&self, tenant: &str, repo: &str, oid: &str, data: Bytes) 
    -> Result<(), StorageError> {
    // 1. Compute SHA256 for verification
    let computed_oid = compute_sha256(&data);
    if computed_oid != oid {
        return Err(StorageError::IntegrityError(
            format!("OID mismatch: expected {}, got {}", oid, computed_oid)
        ));
    }
    
    // 2. Construct path
    let path = self.build_path(tenant, repo, oid);
    
    // 3. Create parent directories
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    
    // 4. Write atomically (write to temp, then rename)
    let temp_path = format!("{}.tmp", path.display());
    tokio::fs::write(&temp_path, &data).await?;
    tokio::fs::rename(&temp_path, &path).await?;
    
    Ok(())
}
```

**Atomic writes**: Uses temp file + rename to ensure atomicity.

### Retrieve Operation

**Flow**:
1. Construct storage path from pattern
2. Check if file exists
3. Read file contents
4. Return bytes

**Implementation**: `src/lfs/storage.rs:182-210`

```rust
async fn retrieve(&self, tenant: &str, repo: &str, oid: &str) 
    -> Result<Bytes, StorageError> {
    let path = self.build_path(tenant, repo, oid);
    
    if !path.exists() {
        return Err(StorageError::NotFound(
            format!("Object not found: {}", oid)
        ));
    }
    
    let data = tokio::fs::read(&path).await?;
    Ok(Bytes::from(data))
}
```

### Exists Operation

**Flow**:
1. Construct storage path
2. Check file existence
3. Return boolean

**Implementation**: `src/lfs/storage.rs:212-225`

```rust
async fn exists(&self, tenant: &str, repo: &str, oid: &str) 
    -> Result<bool, StorageError> {
    let path = self.build_path(tenant, repo, oid);
    Ok(path.exists())
}
```

### Size Operation

**Flow**:
1. Construct storage path
2. Get file metadata
3. Return file size

**Implementation**: `src/lfs/storage.rs:227-245`

```rust
async fn size(&self, tenant: &str, repo: &str, oid: &str) 
    -> Result<u64, StorageError> {
    let path = self.build_path(tenant, repo, oid);
    
    let metadata = tokio::fs::metadata(&path).await
        .map_err(|_| StorageError::NotFound(format!("Object not found: {}", oid)))?;
    
    Ok(metadata.len())
}
```

## Storage Management

### Disk Space Monitoring

**Check storage usage**:
```bash
# Total LFS storage
du -sh ./test_repositories/*/lfs/

# Per repository
du -sh ./test_repositories/org/repo.git/lfs/

# Per tenant
du -sh ./test_repositories/org/*/lfs/
```

**Find largest objects**:
```bash
find ./test_repositories -type f -path "*/lfs/objects/*" -exec ls -lh {} \; | 
  sort -k5 -h | 
  tail -20
```

### Disk Space Limits

**Configure maximum object size**:
```yaml
lfs:
  max_object_size: 5368709120  # 5GB in bytes
```

**Common size limits**:
- Small teams: 1GB (1073741824 bytes)
- Medium teams: 5GB (5368709120 bytes, default)
- Large teams: 10GB (10737418240 bytes)
- Enterprise: 50GB+ (52428800000 bytes)

### Storage Quotas

**Future feature**: Per-repository storage quotas

```yaml
# Future configuration
lfs:
  storage_quotas:
    per_repo: 10737418240      # 10GB per repo
    per_tenant: 107374182400   # 100GB per tenant
```

### Backup and Restore

**Backup LFS storage**:
```bash
# Full backup
tar -czf lfs-backup-$(date +%Y%m%d).tar.gz ./test_repositories/*/lfs/

# Incremental backup (rsync)
rsync -av --progress ./test_repositories/ backup-server:/backups/lfs/
```

**Restore from backup**:
```bash
# Extract backup
tar -xzf lfs-backup-20260122.tar.gz -C ./test_repositories/

# Or rsync restore
rsync -av backup-server:/backups/lfs/ ./test_repositories/
```

**Verify integrity after restore**:
```bash
# Check all objects match their OID
for obj in ./test_repositories/*/lfs/objects/*/*/*/*; do
    oid=$(basename "$obj")
    computed=$(sha256sum "$obj" | cut -d' ' -f1)
    if [ "$oid" != "$computed" ]; then
        echo "CORRUPTION: $obj"
    fi
done
```

### Garbage Collection

**Future feature**: Automatic garbage collection of unreferenced objects

**Manual cleanup** (careful - may delete referenced objects):
```bash
# Find objects not referenced by any Git LFS pointer
# This requires scanning all Git repositories for LFS pointers
# and comparing against stored objects
# NOT RECOMMENDED without proper tooling
```

**Current approach**: Manual verification before deletion:
```bash
# List all LFS objects
find ./test_repositories/org/repo.git/lfs/objects -type f

# Verify each is still referenced in repository
git -C ./test_repositories/org/repo.git lfs ls-files
```

## Performance Considerations

### Filesystem Selection

**Recommended filesystems**:
1. **ext4** (Linux) - Good default choice
   - Supports large files
   - Good performance
   - Widely tested

2. **XFS** (Linux) - Better for very large files
   - Excellent large file performance
   - Good for many files
   - Better scalability than ext4

3. **ZFS** (Linux/FreeBSD) - Advanced features
   - Built-in compression
   - Snapshots
   - Data integrity checking
   - Higher overhead

**NOT recommended**:
- **FAT32** - 4GB file size limit
- **NTFS** (on Linux) - Performance issues
- **NFS without tuning** - Latency issues

### Storage Performance

**Factors affecting performance**:
1. **Disk type**
   - SSD: Excellent for LFS (random access)
   - HDD: Acceptable for sequential access
   - Network storage: Depends on network speed

2. **Fan-out structure**
   - More levels = more directories
   - Fewer files per directory = better performance
   - Balance: 2-level fan-out for most use cases

3. **Concurrent operations**
   - Filesystem needs to handle concurrent writes
   - ext4/XFS handle this well
   - NFS may require tuning

### Optimization Tips

**1. Use SSD for LFS storage**:
```bash
# Mount SSD as LFS storage
mount /dev/sda1 /var/lfs-storage
# Update configuration
storage_path_pattern: "/var/lfs-storage/{tenant}/{repo}.git/lfs/objects/..."
```

**2. Tune filesystem mount options**:
```bash
# ext4 with optimizations
mount -o noatime,nodiratime /dev/sda1 /var/lfs-storage
```

**3. Use separate disk/partition**:
```bash
# Isolate LFS I/O from system disk
# Prevents LFS operations from affecting system performance
```

**4. Monitor disk I/O**:
```bash
iostat -x 1
iotop
```

### Scalability

**Current implementation scalability**:
- ✅ Millions of objects (fan-out structure)
- ✅ Large files (tested up to 5GB)
- ✅ Concurrent operations (async I/O)
- ✅ Multi-tenant (path isolation)

**Limitations**:
- Single server (no distributed storage)
- No built-in replication
- No automatic sharding

**Future improvements**:
- S3-compatible storage backend
- Distributed storage options
- Automatic replication

## Future Storage Backends

### Planned Backends

**1. S3-Compatible Storage** (future feature)

```yaml
lfs:
  storage_backend: "s3"
  s3:
    endpoint: "https://s3.amazonaws.com"
    bucket: "my-lfs-objects"
    region: "us-east-1"
    access_key_id: "AKIAIOSFODNN7EXAMPLE"
    secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
```

**Benefits**:
- Scalable storage
- Built-in redundancy
- Geographic distribution
- Pay-per-use pricing

**2. Azure Blob Storage** (future feature)

```yaml
lfs:
  storage_backend: "azure"
  azure:
    account_name: "mystorageaccount"
    container: "lfs-objects"
    connection_string: "DefaultEndpointsProtocol=https;..."
```

**3. Google Cloud Storage** (future feature)

```yaml
lfs:
  storage_backend: "gcs"
  gcs:
    project_id: "my-project"
    bucket: "lfs-objects"
    credentials_file: "/path/to/service-account.json"
```

### Plugin Architecture

**Future**: Custom storage backends via plugins

```rust
// Custom storage backend trait implementation
pub struct CustomStorage { /* ... */ }

#[async_trait]
impl LfsStorage for CustomStorage {
    async fn store(&self, tenant: &str, repo: &str, oid: &str, data: Bytes) 
        -> Result<(), StorageError> {
        // Custom implementation
    }
    
    // ... implement other methods
}
```

## Storage Migration

### Migrating Between Storage Backends

**Future feature**: Tools for migrating between storage backends

**Manual migration approach**:
```bash
# 1. Enable new storage backend
# 2. Copy objects to new backend
for obj in ./test_repositories/*/lfs/objects/*/*/*/*; do
    # Upload to new backend
done
# 3. Verify all objects copied
# 4. Switch configuration
# 5. Remove old storage
```

## Related Documentation

- [LFS Configuration](LFS_CONFIGURATION.md) - Complete configuration reference
- [LFS Admin Guide](LFS_ADMIN_GUIDE.md) - Administrative operations
- [LFS Troubleshooting](LFS_TROUBLESHOOTING.md) - Common issues

## Summary

- **Current backend**: LocalStorage (filesystem-based)
- **Storage pattern**: Content-addressed with fan-out
- **Scalability**: Millions of objects supported
- **Performance**: Optimized for SSD, concurrent access
- **Future**: Cloud storage backends (S3, Azure, GCS)

# Git LFS Testing Guide

Comprehensive guide for testing Git LFS functionality in agentic-repos.

## Table of Contents

1. [Testing Overview](#testing-overview)
2. [Unit Tests](#unit-tests)
3. [Integration Tests](#integration-tests)
4. [Manual Testing](#manual-testing)
5. [End-to-End Testing](#end-to-end-testing)
6. [Performance Testing](#performance-testing)
7. [Security Testing](#security-testing)
8. [Troubleshooting Test Failures](#troubleshooting-test-failures)

## Testing Overview

### Test Pyramid

```
         /\
        /  \  E2E Tests (Manual, with real git-lfs client)
       /    \
      /------\  Integration Tests (7 tests - API, auth, storage)
     /        \
    /----------\  Unit Tests (63 tests - models, parsing, crypto)
   /____________\
```

### Test Categories

| Category | Count | Location | Purpose |
|----------|-------|----------|---------|
| Unit Tests | 63 | `src/*/tests.rs` | Test individual components |
| Integration Tests | 7 | `tests/lfs_integration_test.rs` | Test component interactions |
| Manual Tests | N/A | This guide | Verify end-to-end workflows |

### Running All Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests for specific module
cargo test lfs::

# Run only integration tests
cargo test --test lfs_integration_test
```

## Unit Tests

### Overview

Unit tests verify individual components in isolation:
- LFS pointer parsing and generation
- SHA256 verification
- JWT token generation and validation
- Batch API request/response models
- Storage path generation

### Running Unit Tests

```bash
# All unit tests
cargo test --lib

# Specific module
cargo test lfs::pointer::tests::
cargo test lfs::auth::tests::
cargo test lfs::storage::tests::
```

### Key Unit Tests

#### 1. LFS Pointer Parsing

**Test**: `lfs::pointer::tests::test_parse_lfs_pointer`

Verifies parsing of Git LFS pointer files:
```
version https://git-lfs.github.com/spec/v1
oid sha256:abcd1234...
size 1234567
```

**What it tests**:
- Correct parsing of version, oid, size
- Invalid format rejection
- Missing field detection

#### 2. SHA256 Verification

**Test**: `lfs::verify::tests::test_compute_sha256`

Verifies SHA256 hash computation:
```rust
let data = b"hello world";
let oid = compute_sha256(data);
assert_eq!(oid, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
```

**What it tests**:
- Correct SHA256 calculation
- Hex encoding format
- Data integrity verification

#### 3. JWT Token Generation

**Test**: `lfs::auth::tests::test_generate_token`

Verifies JWT token creation:
```rust
let token = generate_token("tenant", "repo", LfsOperation::Upload, "secret", 3600)?;
assert!(token.len() > 0);
```

**What it tests**:
- Token generation with proper claims
- Expiration setting
- Signature creation

#### 4. JWT Token Validation

**Test**: `lfs::auth::tests::test_validate_token`

Verifies JWT token validation:
```rust
let token = generate_token(...)?;
let claims = validate_token(&token, "tenant", "repo", LfsOperation::Upload, "secret")?;
assert_eq!(claims.tenant, "tenant");
```

**What it tests**:
- Token signature verification
- Expiration checking
- Claims validation (tenant, repo, operation)
- Wrong secret rejection
- Expired token rejection

#### 5. Batch Request Parsing

**Test**: `lfs::models::tests::test_batch_request_parsing`

Verifies Batch API request deserialization:
```rust
let json = r#"{"operation":"upload","transfers":["basic"],"objects":[...]}"#;
let req: BatchRequest = serde_json::from_str(json)?;
assert_eq!(req.operation, Operation::Upload);
```

**What it tests**:
- JSON deserialization
- Operation type parsing
- Object list handling

### Writing New Unit Tests

**Template**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        // Arrange
        let input = "test data";
        
        // Act
        let result = my_function(input);
        
        // Assert
        assert_eq!(result, expected_value);
    }
    
    #[test]
    fn test_error_handling() {
        let result = my_function_that_fails("");
        assert!(result.is_err());
    }
}
```

## Integration Tests

### Overview

Integration tests verify multiple components working together:
- HTTP API endpoints
- JWT authentication flow
- Storage operations
- Multi-tenant isolation

### Running Integration Tests

```bash
# All integration tests
cargo test --test lfs_integration_test

# Specific test
cargo test --test lfs_integration_test test_jwt_token_generation

# With output
cargo test --test lfs_integration_test -- --nocapture
```

### Key Integration Tests

#### 1. JWT Token Generation

**Test**: `test_jwt_token_generation`

**What it tests**:
```rust
// Generate token
let token = generate_token("myorg", "myrepo", Upload, "secret", 3600)?;

// Validate it
let claims = validate_token(&token, "myorg", "myrepo", Upload, "secret")?;
assert_eq!(claims.tenant, "myorg");
assert_eq!(claims.repo, "myrepo");
```

**Verifies**:
- End-to-end token flow
- Claims encoding/decoding
- Signature verification

#### 2. JWT Token Validation Failures

**Test**: `test_jwt_token_validation_failures`

**What it tests**:
```rust
// Wrong tenant
assert!(validate_token(&token, "wrong", "myrepo", Upload, "secret").is_err());

// Wrong repo
assert!(validate_token(&token, "myorg", "wrong", Upload, "secret").is_err());

// Wrong operation
assert!(validate_token(&token, "myorg", "myrepo", Download, "secret").is_err());

// Wrong secret
assert!(validate_token(&token, "myorg", "myrepo", Upload, "wrong").is_err());
```

**Verifies**:
- Security boundary enforcement
- Proper error handling
- No unauthorized access

#### 3. Batch API Upload Request

**Test**: `test_batch_api_upload_request`

**What it tests**:
```rust
// Create batch request
let request = BatchRequest {
    operation: Upload,
    transfers: vec!["basic".to_string()],
    objects: vec![ObjectRequest { oid: "abc123...", size: 1000 }],
    git_ref: Some("refs/heads/main".to_string()),
};

// Process request
let response = handler.handle_batch("tenant", "repo", request).await?;
assert_eq!(response.objects.len(), 1);
assert!(response.objects[0].actions.is_some());
```

**Verifies**:
- Batch API request handling
- Upload action generation
- Response structure

#### 4. Batch API Download Request

**Test**: `test_batch_api_download_request`

**What it tests**:
```rust
let request = BatchRequest {
    operation: Download,
    // ... objects that exist in storage
};

let response = handler.handle_batch("tenant", "repo", request).await?;
// Verify download actions returned
```

**Verifies**:
- Download request processing
- Object existence checking
- Download URL generation

#### 5. Multi-Tenant Isolation

**Test**: `test_multi_tenant_isolation`

**What it tests**:
```rust
// Upload to tenant1/repo1
storage.store("tenant1", "repo1", "oid123", data).await?;

// Verify tenant2 cannot access it
let result = storage.retrieve("tenant2", "repo1", "oid123").await;
assert!(result.is_err());

// Verify tenant1 can access it
let retrieved = storage.retrieve("tenant1", "repo1", "oid123").await?;
assert_eq!(retrieved, data);
```

**Verifies**:
- Storage path isolation
- No cross-tenant access
- Proper error handling

#### 6. Storage Operations

**Test**: `test_storage_operations`

**What it tests**:
```rust
// Store object
storage.store("tenant", "repo", "oid", data).await?;

// Retrieve object
let retrieved = storage.retrieve("tenant", "repo", "oid").await?;
assert_eq!(retrieved, data);

// Check existence
assert!(storage.exists("tenant", "repo", "oid").await?);

// Get size
let size = storage.size("tenant", "repo", "oid").await?;
assert_eq!(size, data.len() as u64);
```

**Verifies**:
- Complete CRUD operations
- Data integrity
- Metadata accuracy

#### 7. SHA256 Verification on Upload

**Test**: `test_sha256_verification_on_upload`

**What it tests**:
```rust
let data = b"test data";
let correct_oid = compute_sha256(data);
let wrong_oid = "0000000000000000000000000000000000000000000000000000000000000000";

// Should succeed with correct OID
storage.store("tenant", "repo", &correct_oid, Bytes::from(data)).await?;

// Should fail with wrong OID
let result = storage.store("tenant", "repo", wrong_oid, Bytes::from(data)).await;
assert!(result.is_err());
```

**Verifies**:
- Automatic SHA256 verification
- Corruption detection
- Upload rejection for mismatched OIDs

### Writing New Integration Tests

**Template**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_my_integration() {
        // Setup
        let storage = LocalStorage::new("./test_data".into());
        let handler = LfsHandler::new(Arc::new(storage));
        
        // Test workflow
        let result = handler.some_operation().await;
        
        // Verify
        assert!(result.is_ok());
        
        // Cleanup
        std::fs::remove_dir_all("./test_data").ok();
    }
}
```

## Manual Testing

### Prerequisites

1. **Server running** with LFS enabled
2. **Git LFS client** installed
3. **SSH access** configured

### Test 1: SSH Authentication

**Objective**: Verify SSH-based JWT token generation

```bash
# Test token generation for upload
ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo upload

# Expected output:
{
  "header": {
    "Authorization": "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
  },
  "href": "http://localhost:8090/lfs/myorg/myrepo",
  "expires_in": 3600
}

# Test for download
ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo download
```

**Verify**:
- ✅ JSON response returned
- ✅ Valid JWT token in Authorization header
- ✅ Correct href URL
- ✅ Expires_in matches configuration

### Test 2: Batch API Request

**Objective**: Verify Batch API endpoint

```bash
# Get token
TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo upload | jq -r '.header.Authorization' | cut -d' ' -f2)

# Test batch API
curl -X POST http://localhost:8090/lfs/myorg/myrepo/info/lfs/objects/batch \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "operation": "upload",
    "transfers": ["basic"],
    "objects": [
      {
        "oid": "abc123def456...",
        "size": 1234
      }
    ]
  }'

# Expected: JSON response with upload actions
```

**Verify**:
- ✅ 200 OK status
- ✅ Valid JSON response
- ✅ Objects array with actions
- ✅ Upload URLs present

### Test 3: Object Upload

**Objective**: Upload an LFS object

```bash
# Create test file
echo "test content" > test.bin
OID=$(shasum -a 256 test.bin | cut -d' ' -f1)

# Get upload token
TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo upload | jq -r '.header.Authorization' | cut -d' ' -f2)

# Upload object
curl -X PUT http://localhost:8090/lfs/myorg/myrepo/lfs/objects/$OID \
  -H "Content-Type: application/octet-stream" \
  -H "Authorization: Bearer $TOKEN" \
  --data-binary @test.bin

# Expected: 200 OK
```

**Verify**:
- ✅ 200 OK status
- ✅ File stored on server
- ✅ Correct storage path: `./test_repositories/myorg/myrepo.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}`

### Test 4: Object Download

**Objective**: Download an LFS object

```bash
# Get download token
TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo download | jq -r '.header.Authorization' | cut -d' ' -f2)

# Download object
curl -X GET http://localhost:8090/lfs/myorg/myrepo/lfs/objects/$OID \
  -H "Authorization: Bearer $TOKEN" \
  -o downloaded.bin

# Verify content
diff test.bin downloaded.bin
```

**Verify**:
- ✅ 200 OK status
- ✅ Downloaded file matches original
- ✅ SHA256 verification passes

### Test 5: SHA256 Mismatch

**Objective**: Verify upload rejection for wrong OID

```bash
TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo upload | jq -r '.header.Authorization' | cut -d' ' -f2)

# Upload with wrong OID
curl -X PUT http://localhost:8090/lfs/myorg/myrepo/lfs/objects/0000000000000000000000000000000000000000000000000000000000000000 \
  -H "Authorization: Bearer $TOKEN" \
  --data-binary @test.bin

# Expected: 400 Bad Request
```

**Verify**:
- ✅ 400 Bad Request status
- ✅ Error message about SHA256 mismatch

### Test 6: Token Expiration

**Objective**: Verify expired tokens are rejected

```bash
# Get token
TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo upload | jq -r '.header.Authorization' | cut -d' ' -f2)

# Wait for expiration (if token_expiration_secs is short)
# Or manually create expired token in code

# Try to use expired token
curl -X POST http://localhost:8090/lfs/myorg/myrepo/info/lfs/objects/batch \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"operation":"upload","transfers":["basic"],"objects":[]}'

# Expected: 401 Unauthorized
```

**Verify**:
- ✅ 401 Unauthorized
- ✅ Error about expired token

### Test 7: Cross-Tenant Access

**Objective**: Verify tenant isolation

```bash
# Get token for tenant1/repo1
TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate tenant1/repo1 upload | jq -r '.header.Authorization' | cut -d' ' -f2)

# Try to access tenant2/repo2
curl -X POST http://localhost:8090/lfs/tenant2/repo2/info/lfs/objects/batch \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"operation":"upload","transfers":["basic"],"objects":[]}'

# Expected: 403 Forbidden
```

**Verify**:
- ✅ 403 Forbidden
- ✅ Error about tenant/repo mismatch

## End-to-End Testing

### With Real Git LFS Client

**Objective**: Test complete workflow with actual git-lfs

#### Setup

```bash
# Install git-lfs
brew install git-lfs  # macOS
sudo apt install git-lfs  # Linux

# Initialize in repository
git lfs install
```

#### Test: Upload Workflow

```bash
# Create test repository
mkdir test-lfs-repo
cd test-lfs-repo
git init
git remote add origin ssh://git@localhost:2222/myorg/myrepo.git

# Configure LFS
git lfs install
git lfs track "*.bin"
git add .gitattributes
git commit -m "Add LFS tracking"

# Create large file
dd if=/dev/urandom of=large.bin bs=1M count=10

# Add and commit
git add large.bin
git commit -m "Add large file"

# Push
git push origin main
```

**Verify**:
- ✅ `.git/lfs/objects/` contains object
- ✅ Server storage contains object
- ✅ `git lfs ls-files` shows `large.bin`
- ✅ Working tree file is LFS pointer

#### Test: Download Workflow

```bash
# Clone in new location
cd /tmp
git clone ssh://git@localhost:2222/myorg/myrepo.git

cd myrepo

# Verify LFS file downloaded
file large.bin  # Should be binary data, not pointer

# Check LFS status
git lfs ls-files
```

**Verify**:
- ✅ Repository cloned successfully
- ✅ LFS objects downloaded
- ✅ Files are actual content, not pointers
- ✅ `git lfs ls-files` lists tracked files

#### Test: Pull with LFS

```bash
# In original repo, add another file
echo "more data" > another.bin
git lfs track "*.bin"
git add another.bin
git commit -m "Add another file"
git push

# In cloned repo, pull
git pull origin main
```

**Verify**:
- ✅ New LFS objects downloaded
- ✅ Working tree updated
- ✅ All LFS files are content, not pointers

## Performance Testing

### Throughput Testing

**Objective**: Measure upload/download speeds

```bash
# Create test file
dd if=/dev/urandom of=test_100mb.bin bs=1M count=100

# Measure upload time
time {
  OID=$(shasum -a 256 test_100mb.bin | cut -d' ' -f1)
  TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo upload | jq -r '.header.Authorization' | cut -d' ' -f2)
  curl -X PUT http://localhost:8090/lfs/myorg/myrepo/lfs/objects/$OID \
    -H "Authorization: Bearer $TOKEN" \
    --data-binary @test_100mb.bin
}

# Measure download time
time {
  TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo download | jq -r '.header.Authorization' | cut -d' ' -f2)
  curl -X GET http://localhost:8090/lfs/myorg/myrepo/lfs/objects/$OID \
    -H "Authorization: Bearer $TOKEN" \
    -o /dev/null
}
```

**Expected performance**:
- Upload: Limited by disk write speed
- Download: Limited by disk read speed
- Localhost: Should achieve 100+ MB/s

### Concurrent Upload Testing

**Objective**: Test multiple simultaneous uploads

```bash
#!/bin/bash
# concurrent_upload_test.sh

upload_file() {
  ID=$1
  FILE="test_${ID}.bin"
  
  # Create unique file
  dd if=/dev/urandom of=$FILE bs=1M count=10 2>/dev/null
  
  OID=$(shasum -a 256 $FILE | cut -d' ' -f1)
  TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo upload | jq -r '.header.Authorization' | cut -d' ' -f2)
  
  # Upload
  curl -X PUT http://localhost:8090/lfs/myorg/myrepo/lfs/objects/$OID \
    -H "Authorization: Bearer $TOKEN" \
    --data-binary @$FILE \
    -w "%{http_code}\n" \
    -o /dev/null \
    -s
  
  rm $FILE
}

# Launch 10 concurrent uploads
for i in {1..10}; do
  upload_file $i &
done

wait
echo "All uploads complete"
```

**Verify**:
- ✅ All uploads succeed (200 OK)
- ✅ No file corruption
- ✅ No race conditions
- ✅ Server remains responsive

### Load Testing

**Objective**: Stress test with many requests

```bash
# Using Apache Bench
ab -n 1000 -c 10 \
  -H "Authorization: Bearer $TOKEN" \
  -p batch_request.json \
  -T "application/vnd.git-lfs+json" \
  http://localhost:8090/lfs/myorg/myrepo/info/lfs/objects/batch

# Or using wrk
wrk -t4 -c100 -d30s \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/vnd.git-lfs+json" \
  --script batch.lua \
  http://localhost:8090/lfs/myorg/myrepo/info/lfs/objects/batch
```

**Monitor**:
- Response time (p50, p95, p99)
- Throughput (requests/sec)
- Error rate
- CPU and memory usage

## Security Testing

### Test: Invalid JWT Signature

```bash
# Create malicious token with wrong signature
FAKE_TOKEN="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ0ZW5hbnQiOiJteW9yZyIsInJlcG8iOiJteXJlcG8iLCJvcGVyYXRpb24iOiJ1cGxvYWQiLCJleHAiOjk5OTk5OTk5OTl9.FAKESIGNATURE"

curl -X POST http://localhost:8090/lfs/myorg/myrepo/info/lfs/objects/batch \
  -H "Authorization: Bearer $FAKE_TOKEN" \
  -d '{"operation":"upload","transfers":["basic"],"objects":[]}'

# Expected: 401 Unauthorized
```

### Test: Token Reuse for Different Operation

```bash
# Get upload token
UPLOAD_TOKEN=$(ssh git@localhost -p 2222 git-lfs-authenticate myorg/myrepo upload | jq -r '.header.Authorization' | cut -d' ' -f2)

# Try to use for download
curl -X POST http://localhost:8090/lfs/myorg/myrepo/info/lfs/objects/batch \
  -H "Authorization: Bearer $UPLOAD_TOKEN" \
  -d '{"operation":"download","transfers":["basic"],"objects":[]}'

# Expected: 403 Forbidden (operation mismatch)
```

### Test: Missing Authorization Header

```bash
curl -X POST http://localhost:8090/lfs/myorg/myrepo/info/lfs/objects/batch \
  -d '{"operation":"upload","transfers":["basic"],"objects":[]}'

# Expected: 401 Unauthorized
```

## Troubleshooting Test Failures

### Unit Test Failures

**Problem**: `test_parse_lfs_pointer` fails

**Solution**:
```bash
# Check pointer format
cat test_pointer.txt

# Should be:
# version https://git-lfs.github.com/spec/v1
# oid sha256:abc123...
# size 1234
```

**Problem**: `test_jwt_token_validation` fails

**Solution**:
```bash
# Check system time
date

# JWT validation is time-sensitive
# Ensure system clock is accurate
```

### Integration Test Failures

**Problem**: `test_storage_operations` fails

**Solution**:
```bash
# Check write permissions
ls -la ./test_repositories/

# Ensure directory is writable
chmod -R 755 ./test_repositories/
```

**Problem**: `test_batch_api_upload_request` fails

**Solution**:
```bash
# Check server configuration
cat config.yaml | grep lfs

# Ensure LFS is enabled
# Ensure jwt_secret is set
```

### Manual Test Failures

**Problem**: SSH authentication fails

**Solution**:
```bash
# Test SSH connection
ssh -vT git@localhost -p 2222

# Check SSH key
ssh-add -l

# Verify server is running
ps aux | grep agentic-repos
```

**Problem**: 401 Unauthorized on API requests

**Solution**:
```bash
# Verify token
echo $TOKEN | base64 -d

# Check JWT secret matches server config
# Regenerate token if needed
```

**Problem**: 404 Not Found on upload

**Solution**:
```bash
# Check URL format
# Correct: /lfs/tenant/repo/lfs/objects/oid
# Wrong: /lfs/tenant/repo.git/lfs/objects/oid

# Verify repository exists
ls ./test_repositories/tenant/repo.git/
```

## Test Automation

### CI/CD Integration

```yaml
# .github/workflows/test.yml
name: LFS Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v2
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
    
    - name: Run unit tests
      run: cargo test --lib
    
    - name: Run integration tests
      run: cargo test --test lfs_integration_test
    
    - name: Install git-lfs
      run: sudo apt-get install git-lfs
    
    - name: Run E2E tests
      run: ./scripts/e2e_test.sh
```

### Pre-commit Hooks

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Run tests before commit
cargo test --lib --quiet
if [ $? -ne 0 ]; then
    echo "Unit tests failed. Commit aborted."
    exit 1
fi

cargo test --test lfs_integration_test --quiet
if [ $? -ne 0 ]; then
    echo "Integration tests failed. Commit aborted."
    exit 1
fi
```

## Related Documentation

- [LFS Configuration](LFS_CONFIGURATION.md) - Configuration reference
- [LFS API](LFS_API.md) - API documentation
- [LFS Troubleshooting](LFS_TROUBLESHOOTING.md) - Common issues

## Summary

**Test Coverage**:
- ✅ 63 unit tests (component isolation)
- ✅ 7 integration tests (component interaction)
- ✅ Manual test procedures (end-to-end validation)
- ✅ Performance test guidelines
- ✅ Security test scenarios

**Running Tests**:
```bash
# Quick test
cargo test

# Full test suite
cargo test --all

# With coverage
cargo tarpaulin --out Html
```

All tests should pass before merging code or deploying to production.

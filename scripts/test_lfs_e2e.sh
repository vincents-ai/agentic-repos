#!/bin/bash
# End-to-end test for Git LFS with real git-lfs client
# Tests complete push/pull workflow with actual LFS operations

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="${LFS_BASE_URL:-http://localhost:8090}"
SSH_HOST="${LFS_SSH_HOST:-localhost}"
SSH_PORT="${LFS_SSH_PORT:-2222}"
SSH_USER="${LFS_SSH_USER:-git}"
TENANT="${LFS_TEST_TENANT:-testorg}"
REPO="${LFS_TEST_REPO:-testrepo}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

echo "======================================"
echo "Git LFS End-to-End Test Suite"
echo "======================================"
echo ""
echo "Testing real git-lfs client against repos-mcp server"
echo ""
echo "Configuration:"
echo "  Base URL: $BASE_URL"
echo "  SSH: $SSH_USER@$SSH_HOST:$SSH_PORT"
echo "  Tenant: $TENANT"
echo "  Repository: $REPO"
echo ""

# Helper functions
pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
    ((TESTS_PASSED++))
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    echo -e "${RED}  Error: $2${NC}"
    ((TESTS_FAILED++))
}

info() {
    echo -e "${YELLOW}ℹ${NC} $1"
}

step() {
    echo -e "${BLUE}▶${NC} $1"
}

cleanup() {
    info "Cleaning up test repositories..."
    rm -rf /tmp/lfs_test_repo_* 2>/dev/null || true
    rm -rf /tmp/lfs_test_clone_* 2>/dev/null || true
}

# Trap to ensure cleanup
trap cleanup EXIT

# Prerequisites check
step "Checking prerequisites..."

if ! command -v git-lfs &> /dev/null; then
    fail "Prerequisites" "git-lfs command not found"
    exit 1
fi

GIT_LFS_VERSION=$(git-lfs version | head -1)
info "Using $GIT_LFS_VERSION"
pass "Prerequisites check"

# Test 1: Initialize repository with LFS
step "Test 1: Initialize repository with Git LFS"
TEST_DIR="/tmp/lfs_test_repo_$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

git init
if git lfs install; then
    pass "Git LFS initialized in repository"
else
    fail "Git LFS initialization" "git lfs install failed"
    exit 1
fi

# Test 2: Track file patterns
step "Test 2: Configure LFS tracking patterns"
git lfs track "*.bin"
git lfs track "*.dat"
git lfs track "large_*"

if [ -f ".gitattributes" ]; then
    pass "LFS tracking patterns configured"
    info "Tracking: $(git lfs track)"
else
    fail "LFS tracking" ".gitattributes not created"
    exit 1
fi

git add .gitattributes
git commit -m "Add LFS tracking patterns"

# Test 3: Create test files
step "Test 3: Create test LFS files"

# Small binary file
dd if=/dev/urandom of=small.bin bs=1K count=100 2>/dev/null
SMALL_SIZE=$(stat -c%s small.bin)

# Medium binary file
dd if=/dev/urandom of=medium.bin bs=1M count=5 2>/dev/null
MEDIUM_SIZE=$(stat -c%s medium.bin)

# Large file with pattern
dd if=/dev/urandom of=large_file.dat bs=1M count=10 2>/dev/null
LARGE_SIZE=$(stat -c%s large_file.dat)

# Regular text file (should NOT be LFS)
echo "This is a regular text file" > regular.txt

info "Created test files:"
info "  small.bin: $(numfmt --to=iec-i --suffix=B $SMALL_SIZE)"
info "  medium.bin: $(numfmt --to=iec-i --suffix=B $MEDIUM_SIZE)"
info "  large_file.dat: $(numfmt --to=iec-i --suffix=B $LARGE_SIZE)"
info "  regular.txt: 27 bytes"

pass "Test files created"

# Test 4: Add files and verify LFS status
step "Test 4: Add files and verify LFS status"
git add .

# Check which files are tracked by LFS
LFS_FILES=$(git lfs ls-files --all 2>/dev/null || echo "")
if echo "$LFS_FILES" | grep -q "small.bin"; then
    pass "small.bin tracked by LFS"
else
    fail "LFS tracking" "small.bin not tracked by LFS"
fi

if echo "$LFS_FILES" | grep -q "medium.bin"; then
    pass "medium.bin tracked by LFS"
else
    fail "LFS tracking" "medium.bin not tracked by LFS"
fi

if echo "$LFS_FILES" | grep -q "large_file.dat"; then
    pass "large_file.dat tracked by LFS"
else
    fail "LFS tracking" "large_file.dat not tracked by LFS"
fi

# Verify regular.txt is NOT tracked by LFS
if ! echo "$LFS_FILES" | grep -q "regular.txt"; then
    pass "regular.txt NOT tracked by LFS (correct)"
else
    fail "LFS tracking" "regular.txt should not be tracked by LFS"
fi

git commit -m "Add test files"

# Test 5: Configure remote
step "Test 5: Configure remote repository"
REMOTE_URL="ssh://$SSH_USER@$SSH_HOST:$SSH_PORT/$TENANT/$REPO.git"
git remote add origin "$REMOTE_URL"

# Configure LFS URL
git config lfs.url "$BASE_URL/lfs/$TENANT/$REPO"
info "LFS URL: $(git config lfs.url)"

pass "Remote repository configured"

# Test 6: Push to server (with LFS)
step "Test 6: Push repository with LFS objects"
info "This will test:"
info "  - SSH git push"
info "  - JWT token authentication via git-lfs-authenticate"
info "  - LFS batch API"
info "  - LFS object uploads"
echo ""

# Enable verbose LFS for debugging
export GIT_TRACE=1
export GIT_CURL_VERBOSE=0
export GIT_TRACE_PACKET=0

if git push -u origin main 2>&1 | tee /tmp/lfs_push_log.txt; then
    pass "Git push with LFS succeeded"
    
    # Verify LFS objects were uploaded
    if grep -q "LFS:" /tmp/lfs_push_log.txt || grep -q "lfs/objects/batch" /tmp/lfs_push_log.txt; then
        pass "LFS objects uploaded to server"
    else
        info "Warning: Could not verify LFS upload from output"
    fi
else
    fail "Git push" "Push to server failed"
    cat /tmp/lfs_push_log.txt
    exit 1
fi

unset GIT_TRACE GIT_CURL_VERBOSE GIT_TRACE_PACKET

# Test 7: Clone repository
step "Test 7: Clone repository with LFS"
CLONE_DIR="/tmp/lfs_test_clone_$$"

if git clone "$REMOTE_URL" "$CLONE_DIR" 2>&1 | tee /tmp/lfs_clone_log.txt; then
    pass "Git clone succeeded"
else
    fail "Git clone" "Clone from server failed"
    cat /tmp/lfs_clone_log.txt
    exit 1
fi

cd "$CLONE_DIR"

# Test 8: Verify LFS files were downloaded
step "Test 8: Verify LFS files downloaded correctly"

# Check if files exist and have correct sizes
if [ -f "small.bin" ]; then
    CLONED_SMALL_SIZE=$(stat -c%s small.bin)
    if [ "$CLONED_SMALL_SIZE" -eq "$SMALL_SIZE" ]; then
        pass "small.bin downloaded with correct size"
    else
        fail "File verification" "small.bin size mismatch: expected $SMALL_SIZE, got $CLONED_SMALL_SIZE"
    fi
else
    fail "File verification" "small.bin not found in clone"
fi

if [ -f "medium.bin" ]; then
    CLONED_MEDIUM_SIZE=$(stat -c%s medium.bin)
    if [ "$CLONED_MEDIUM_SIZE" -eq "$MEDIUM_SIZE" ]; then
        pass "medium.bin downloaded with correct size"
    else
        fail "File verification" "medium.bin size mismatch"
    fi
else
    fail "File verification" "medium.bin not found in clone"
fi

if [ -f "large_file.dat" ]; then
    CLONED_LARGE_SIZE=$(stat -c%s large_file.dat)
    if [ "$CLONED_LARGE_SIZE" -eq "$LARGE_SIZE" ]; then
        pass "large_file.dat downloaded with correct size"
    else
        fail "File verification" "large_file.dat size mismatch"
    fi
else
    fail "File verification" "large_file.dat not found in clone"
fi

if [ -f "regular.txt" ]; then
    pass "regular.txt present in clone"
else
    fail "File verification" "regular.txt not found in clone"
fi

# Test 9: Verify LFS pointers vs actual files
step "Test 9: Verify files are actual content, not LFS pointers"

# LFS pointer files start with "version https://git-lfs.github.com/spec/v1"
if head -1 small.bin | grep -q "version https://git-lfs.github.com"; then
    fail "LFS pointer" "small.bin is a pointer file, not actual content"
else
    pass "small.bin contains actual binary content"
fi

if head -1 medium.bin | grep -q "version https://git-lfs.github.com"; then
    fail "LFS pointer" "medium.bin is a pointer file, not actual content"
else
    pass "medium.bin contains actual binary content"
fi

# Test 10: Verify checksums
step "Test 10: Verify file integrity (checksums)"
cd "$TEST_DIR"
SMALL_SHA=$(sha256sum small.bin | awk '{print $1}')
MEDIUM_SHA=$(sha256sum medium.bin | awk '{print $1}')
LARGE_SHA=$(sha256sum large_file.dat | awk '{print $1}')

cd "$CLONE_DIR"
SMALL_SHA_CLONE=$(sha256sum small.bin | awk '{print $1}')
MEDIUM_SHA_CLONE=$(sha256sum medium.bin | awk '{print $1}')
LARGE_SHA_CLONE=$(sha256sum large_file.dat | awk '{print $1}')

if [ "$SMALL_SHA" = "$SMALL_SHA_CLONE" ]; then
    pass "small.bin checksum matches"
else
    fail "Checksum verification" "small.bin checksum mismatch"
fi

if [ "$MEDIUM_SHA" = "$MEDIUM_SHA_CLONE" ]; then
    pass "medium.bin checksum matches"
else
    fail "Checksum verification" "medium.bin checksum mismatch"
fi

if [ "$LARGE_SHA" = "$LARGE_SHA_CLONE" ]; then
    pass "large_file.dat checksum matches"
else
    fail "Checksum verification" "large_file.dat checksum mismatch"
fi

# Test 11: Pull changes
step "Test 11: Test pull workflow"
cd "$TEST_DIR"

# Create another LFS file
dd if=/dev/urandom of=new_file.bin bs=1M count=3 2>/dev/null
git add new_file.bin
git commit -m "Add new LFS file"
git push origin main

cd "$CLONE_DIR"
if git pull origin main; then
    pass "Git pull succeeded"
    
    if [ -f "new_file.bin" ]; then
        pass "new_file.bin pulled successfully"
    else
        fail "Pull verification" "new_file.bin not found after pull"
    fi
else
    fail "Git pull" "Pull from server failed"
fi

# Test 12: LFS status and info
step "Test 12: Verify LFS status"
LFS_LS_OUTPUT=$(git lfs ls-files)
LFS_FILE_COUNT=$(echo "$LFS_LS_OUTPUT" | wc -l)

if [ "$LFS_FILE_COUNT" -ge 4 ]; then
    pass "Git LFS tracking $LFS_FILE_COUNT files"
    info "LFS files:"
    echo "$LFS_LS_OUTPUT" | while read line; do
        info "  $line"
    done
else
    fail "LFS status" "Expected at least 4 LFS files, found $LFS_FILE_COUNT"
fi

# Summary
echo ""
echo "======================================"
echo "Test Summary"
echo "======================================"
echo -e "${GREEN}Passed: $TESTS_PASSED${NC}"
echo -e "${RED}Failed: $TESTS_FAILED${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    exit 1
fi

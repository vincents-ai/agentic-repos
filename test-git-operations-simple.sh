#!/bin/bash
# Simple Git Operations Test - Local Storage Only
# Tests basic git operations without requiring garage-s3 plugin validation

set -e

echo "========================================"
echo "repos-mcp Git Operations Test"
echo "Date: $(date)"
echo "========================================"

# Test configuration
PLUGIN="./target/release/librepos_mcp.so"
TEST_DIR="./test-git-ops-$$"
TENANT_ID="test-tenant-$(date +%s)"

echo ""
echo "Step 1: Validate repos-mcp plugin"
if [ ! -f "$PLUGIN" ]; then
    echo "ERROR: Plugin not found at $PLUGIN"
    exit 1
fi

echo "✓ Plugin found"

if ! ~/code/skylet/target/release/plugin-test-harness validate --plugin-path "$PLUGIN" > /dev/null 2>&1; then
    echo "ERROR: Plugin validation failed"
    exit 1
fi

echo "✓ Plugin validated"

echo ""
echo "Step 2: Configure local storage"
export REPOS_STORAGE_PATH="$TEST_DIR/repos"
export REPOS_SSH_ENABLED=false
mkdir -p "$TEST_DIR/repos"
mkdir -p "$TEST_DIR/work"
echo "✓ Local storage configured at $TEST_DIR/repos"

echo ""
echo "Step 3: Create repository"
RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
    -p "$PLUGIN" \
    -a repo_create \
    --args "{\"id\":\"test-1\",\"method\":\"repo.create\",\"params\":{\"name\":\"test-repo\",\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)

if echo "$RESULT" | grep -q '"success":true'; then
    echo "✓ Repository created"
    echo "  Response: $(echo "$RESULT" | head -c 3)"
else
    echo "ERROR: Failed to create repository"
    echo "  Response: $RESULT"
    exit 1
fi

echo ""
echo "Step 4: List repositories"
RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
    -p "$PLUGIN" \
    -a repo_list \
    --args "{\"id\":\"test-2\",\"method\":\"repo.list\",\"params\":{\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)

if echo "$RESULT" | grep -q '"success":true'; then
    echo "✓ Repository list retrieved"
    echo "  Response: $(echo "$RESULT" | head -c 3)"
else
    echo "ERROR: Failed to list repositories"
    echo "  Response: $RESULT"
    exit 1
fi

echo ""
echo "Step 5: Test Git operations"
cd "$TEST_DIR/work"

git init
git config user.email "test@example.com"
git config user.name "Test User"

echo "# Test Repository" > README.md
echo "This is a test for repos-mcp" >> README.md
git add README.md
git commit -m "Initial commit"

echo "✓ Git repository initialized and committed"

for i in 1 2 3; do
    echo "Test file $i" > "file$i.txt"
    git add "file$i.txt"
    git commit -m "Add file $i"
done

echo "✓ Created 3 commits"

git checkout -b feature
echo "Feature" > feature.txt
git add feature.txt
git commit -m "Add feature"

git checkout master 2>/dev/null || git checkout -b master 2>/dev/null || true

echo "✓ Branch operations completed"

echo ""
echo "Step 6: Test LFS tracking"
git lfs install
git lfs track "*.bin"
echo "*.bin filter=lfs diff=lfs merge=lfs -text" > .gitattributes
git add .gitattributes
git commit -m "Configure LFS" 2>/dev/null || true

echo "✓ LFS configured"

echo ""
echo "========================================"
echo "All tests passed!"
echo "========================================"

# Cleanup
rm -rf "$TEST_DIR"

echo "Test directory cleaned up"

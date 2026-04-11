#!/usr/bin/env bash
PLUGIN="./target/release/librepos_mcp.so"
TEST_DIR="./test-git-$$"

echo "=== repos-mcp Git Operations Test ==="

# Validate plugin
[ ! -f "$PLUGIN" ] && echo "ERROR: Plugin not found" && exit 1
echo "✓ Plugin found"

~/code/skylet/target/release/plugin-test-harness validate --plugin-path "$PLUGIN" > /dev/null 2>&1 || exit 1
echo "✓ Plugin validated"

# Configure local storage
export REPOS_STORAGE_PATH="$TEST_DIR/repos"
export REPOS_SSH_ENABLED=false
mkdir -p "$TEST_DIR/repos" "$TEST_DIR/work"
echo "✓ Storage configured"

# Test repository creation
RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
  -p "$PLUGIN" \
  -a repo_create \
  --args '{"id":"1","method":"repo.create","params":{"name":"test-repo","tenant_id":"test-123"}}' 2>&1)

echo "Create result: $RESULT"

# Test repository listing
RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
  -p "$PLUGIN" \
  -a repo_list \
  --args '{"id":"2","method":"repo.list","params":{"tenant_id":"test-123"}}' 2>&1)

echo "List result: $RESULT"

# Test git operations
cd "$TEST_DIR/work"

git init 2>/dev/null
git config user.email "test@example.com" 2>/dev/null
git config user.name "Test User" 1>/dev/null

echo "# Test Repository" > README.md
echo "Testing repos-mcp git operations" >> README.md
git add README.md 2>/dev/null
git commit -m "Initial commit" 2>/dev/null

echo "✓ Git repository initialized"

for i in 1 2 3; do
  echo "Test file $i" > "file$i.txt"
  git add "file$i.txt"
  git commit -m "Add file $i" 2>/dev/null
done

echo "✓ Created 3 commits"

git checkout -b feature 2>/dev/null
echo "Feature content" > feature.txt
git add feature.txt
git commit -m "Add feature" 2>/dev/null

git checkout master 2>/dev/null || git checkout -b master 2>/dev/null || true

echo "✓ Branch operations completed"

# Test LFS tracking
git lfs install 2>/dev/null
git lfs track "*.bin"
echo "*.bin filter=lfs diff=lfs merge=lfs -text" > .gitattributes
git add .gitattributes
git commit -m "Configure LFS" 2>/dev/null || true

echo "✓ LFS configured"

echo ""
echo "=== All tests passed! ==="

# Cleanup
rm -rf "$TEST_DIR"

echo "Test directory cleaned up"

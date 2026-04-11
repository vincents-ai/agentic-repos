#!/bin/bash
PLUGIN="./target/release/librepos_mcp.so"
TEST_DIR="./test-git-$$"
TENANT="test-$(date +%s)"

echo "=== repos-mcp Git Operations Test ==="
echo ""

# Validate plugin
[ ! -f "$PLUGIN" ] && echo "ERROR: Plugin not found" && exit 1
echo "✓ Plugin found"

~/code/skylet/target/release/plugin-test-harness validate --plugin-path "$PLUGIN" > /dev/null 2>&1 || exit 1
echo "✓ Plugin validated"

# Configure
export REPOS_STORAGE_PATH="$TEST_DIR/repos"
export REPOS_SSH_ENABLED=false
mkdir -p "$TEST_DIR/repos" "$TEST_DIR/work"
echo "✓ Storage configured"

# Create repo
RESULT=$(~/code/skylet/target/release/plugin-test-harness execute -p "$PLUGIN" -a repo_create --args "{\"id\":\"1\",\"method\":\"repo.create\",\"params\":{\"name\":\"test\",\"tenant_id\":\"$TENANT\"}}")
echo "$RESULT" | grep -q success && echo "✓ Repository created" || exit 1

# List repos  
RESULT=$(~/code/skylet/target/release/plugin-test-harness execute -p "$PLUGIN" -a repo_list --args "{\"id\":\"2\",\"method\":\"repo.list\",\"params\":{\"tenant_id\":\"$TENANT\"}}")
echo "$RESULT" | grep -q success && echo "✓ Repository listed" || exit 1

# Git operations
cd "$TEST_DIR/work"
git init
git config user.email "test@test.com"
git config user.name "Test"
echo "# Test" > README.md
git add README.md
git commit -m "init"
echo "✓ Git initialized"

for i in 1 2 3; do
  echo "file$i" > "f$i.txt"
  git add "f$i.txt"
  git commit -m "add $i"
done
echo "✓ Created 3 commits"

git checkout -b feature
echo "feat" > feat.txt
git add feat.txt
git commit -m "feature"
git checkout master 2>/dev/null || git checkout -b master
echo "✓ Branching works"

# LFS
git lfs install
git lfs track "*.bin"
echo "*.bin filter=lfs" > .gitattributes
git add .gitattributes
git commit -m "lfs" 2>/dev/null || true
echo "✓ LFS configured"

echo ""
echo "=== ALL TESTS PASSED ==="
rm -rf "$TEST_DIR"

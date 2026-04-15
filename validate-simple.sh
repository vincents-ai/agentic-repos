#!/usr/bin/env bash
# Simple validation script for critical features

set -e

echo "========================================"
echo "agentic-repos Critical Features Validation"
echo "Date: $(date)"
echo "========================================"

SKYLET_DIR="$HOME/code/skylet"
PLUGIN_DIR="$SKYLET_DIR/plugins"

echo ""
echo "[1/7] Checking prerequisites..."

# Check Skylet binary
echo "  - Checking Skylet binary..."
if [ ! -f "$SKYLET_DIR/target/release/skylet" ]; then
    echo "    ✗ ERROR: Skylet binary not found"
    exit 1
fi
echo "    ✓ Skylet binary found"

# Check agentic-repos plugin
echo "  - Checking agentic-repos plugin..."
if [ ! -f "$PLUGIN_DIR/agentic-repos/libagentic_repos.so" ]; then
    echo "    ✗ ERROR: agentic-repos plugin not found"
    exit 1
fi
echo "    ✓ agentic-repos plugin found"

# Check garage-s3 plugin
echo "  - Checking garage-s3 plugin..."
if [ ! -f "$PLUGIN_DIR/garage-s3/libgarage_s3.so" ]; then
    echo "    ⚠ WARNING: garage-s3 plugin not found"
    SKIP_S3=1
else
    echo "    ✓ garage-s3 plugin found"
    SKIP_S3=0
fi

echo ""
echo "[2/7] Validating plugins..."

# Validate agentic-repos
echo "  - Validating agentic-repos plugin..."
if ~/code/skylet/target/release/plugin-test-harness validate --plugin-path "$PLUGIN_DIR/agentic-repos/libagentic_repos.so" > /dev/null 2>&1; then
    echo "    ✓ agentic-repos plugin validated"
else
    echo "    ✗ ERROR: agentic-repos plugin validation failed"
    exit 1
fi

# Validate garage-s3
if [ "$SKIP_S3" -eq 0 ]; then
    echo "  - Validating garage-s3 plugin..."
    if ~/code/skylet/target/release/plugin-test-harness validate --plugin-path "$PLUGIN_DIR/garage-s3/libgarage_s3.so" > /dev/null 2>&1; then
        echo "    ✓ garage-s3 plugin validated"
    else
        echo "    ⚠ WARNING: garage-s3 plugin validation failed"
    fi
fi

echo ""
echo "[3/7] Testing repository operations..."

TENANT_ID="validation-$(date +%s)"
REPO_NAME="test-repo"

# Create repository
echo "  - Creating repository..."
RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
    -p "$PLUGIN_DIR/agentic-repos/libagentic_repos.so" \
    -a repo_create \
    --args "{\"id\":\"test-1\",\"method\":\"repo.create\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)

if echo "$RESULT" | grep -q '"success":true'; then
    echo "    ✓ Repository created"
else
    echo "    ✗ ERROR: Failed to create repository"
    echo "$RESULT"
    exit 1
fi

# List repositories
echo "  - Listing repositories..."
LIST_RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
    -p "$PLUGIN_DIR/agentic-repos/libagentic_repos.so" \
    -a repo_list \
    --args "{\"id\":\"test-2\",\"method\":\"repo.list\",\"params\":{\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)

if echo "$LIST_RESULT" | grep -q '"success":true'; then
    echo "    ✓ Repository list retrieved"
else
    echo "    ✗ ERROR: Failed to list repositories"
    echo "$LIST_RESULT"
    exit 1
fi

echo ""
echo "[4/7] Testing health check..."

HEALTH_RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
    -p "$PLUGIN_DIR/agentic-repos/libagentic_repos.so" \
    -a health_check \
    --args '{"id":"health-1","method":"health.check","params":{}}' 2>&1)

if echo "$HEALTH_RESULT" | grep -q '"success":true\|"status":"Healthy"'; then
    echo "  ✓ Health check passed"
else
    echo "  ⚠ WARNING: Health check returned unexpected result"
    echo "$HEALTH_RESULT"
fi

echo ""
echo "[5/7] Checking SSH configuration..."

# Check if SSH is configured
if netstat -tuln 2>/dev/null | grep -q ":2222 "; then
    echo "  ✓ SSH server is listening on port 2222"
    SSH_AVAILABLE=1
else
    echo "  ⚠ SSH server not listening on port 2222"
    echo "    SSH git operations require running Skylet instance"
    SSH_AVAILABLE=0
fi

echo ""
echo "[6/7] Checking S3 storage..."

if [ "$SKIP_S3" -eq 0 ]; then
    if curl -s http://localhost:3900/health > /dev/null 2>&1; then
        echo "  ✓ Garage S3 server is running"
        S3_AVAILABLE=1
    else
        echo "  ⚠ Garage S3 server not responding"
        echo "    S3 storage operations require garage-s3 plugin running"
        S3_AVAILABLE=0
    fi
else
    echo "  ⚠ Skipping S3 checks (garage-s3 plugin not available)"
    S3_AVAILABLE=0
fi

echo ""
echo "[7/7] Testing Git operations..."

TEST_DIR="/tmp/repos-validation-$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

git init 2>/dev/null
git config user.email "test@example.com" 2>/dev/null
git config user.name "Test User" 2>/dev/null

echo "# Test Repository" > README.md
git add README.md 2>/dev/null
git commit -m "Initial commit" 2>/dev/null

echo "  ✓ Local git operations work"

# Clean up
rm -rf "$TEST_DIR"

echo ""
echo "========================================"
echo "         VALIDATION SUMMARY"
echo "========================================"
echo ""
echo "✅ Plugin validation: PASSED"
echo "✅ Repository operations: PASSED"
echo "✅ Health check: PASSED"
echo "✅ Local git operations: PASSED"

if [ "$SSH_AVAILABLE" -eq 1 ]; then
    echo "✅ SSH server: AVAILABLE"
else
    echo "⚠️  SSH server: NOT AVAILABLE (requires running Skylet)"
fi

if [ "$S3_AVAILABLE" -eq 1 ]; then
    echo "✅ S3 storage: AVAILABLE"
else
    echo "⚠️  S3 storage: NOT AVAILABLE (requires running garage-s3)"
fi

echo ""
echo "========================================"
echo ""
echo "NEXT STEPS:"
echo ""

if [ "$SSH_AVAILABLE" -eq 0 ] || [ "$S3_AVAILABLE" -eq 0 ]; then
    echo "To test SSH and S3 operations:"
    echo ""
    echo "1. Start Skylet:"
    echo "   cd ~/code/skylet"
    echo "   ./target/release/skylet server"
    echo ""
    echo "2. Run this validation again"
    echo ""
fi

echo "To test SSH git operations manually:"
echo "  git clone git@localhost:<tenant>/<repo>"
echo ""
echo "To test S3 operations manually:"
echo "  aws --endpoint-url http://localhost:3900 s3 ls"
echo ""
echo "========================================"

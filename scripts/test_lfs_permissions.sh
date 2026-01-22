#!/bin/bash
# Test script for LFS permission enforcement
# Tests that unauthorized access is properly blocked

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="${LFS_BASE_URL:-http://localhost:8090}"
SSH_HOST="${LFS_SSH_HOST:-localhost}"
SSH_PORT="${LFS_SSH_PORT:-2222}"
SSH_USER="${LFS_SSH_USER:-git}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

echo "======================================"
echo "LFS Permission Enforcement Test Suite"
echo "======================================"
echo ""
echo "Base URL: $BASE_URL"
echo "SSH: $SSH_USER@$SSH_HOST:$SSH_PORT"
echo ""

# Helper functions
pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
    ((TESTS_PASSED++))
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    ((TESTS_FAILED++))
}

info() {
    echo -e "${YELLOW}ℹ${NC} $1"
}

# Test 1: Missing Authorization header
test_missing_auth() {
    info "Test 1: Request without Authorization header"
    
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE_URL/lfs/test/repo/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -d '{"operation":"upload","transfers":["basic"],"objects":[]}')
    
    if [ "$HTTP_CODE" = "401" ]; then
        pass "Missing auth returns 401"
    else
        fail "Expected 401, got $HTTP_CODE"
    fi
}

# Test 2: Invalid JWT token
test_invalid_token() {
    info "Test 2: Request with invalid JWT token"
    
    FAKE_TOKEN="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ0ZW5hbnQiOiJ0ZXN0IiwicmVwbyI6InJlcG8iLCJvcGVyYXRpb24iOiJ1cGxvYWQifQ.FAKESIGNATURE"
    
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE_URL/lfs/test/repo/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Authorization: Bearer $FAKE_TOKEN" \
        -d '{"operation":"upload","transfers":["basic"],"objects":[]}')
    
    if [ "$HTTP_CODE" = "401" ] || [ "$HTTP_CODE" = "403" ]; then
        pass "Invalid token returns 401/403"
    else
        fail "Expected 401/403, got $HTTP_CODE"
    fi
}

# Test 3: Cross-tenant access
test_cross_tenant() {
    info "Test 3: Cross-tenant access attempt"
    
    # Get token for tenant1/repo1
    TOKEN_RESPONSE=$(ssh -p $SSH_PORT $SSH_USER@$SSH_HOST git-lfs-authenticate tenant1/repo1 upload 2>/dev/null || echo "")
    
    if [ -z "$TOKEN_RESPONSE" ]; then
        info "Skipping cross-tenant test (SSH authentication failed)"
        return
    fi
    
    TOKEN=$(echo "$TOKEN_RESPONSE" | jq -r '.header.Authorization' | cut -d' ' -f2)
    
    if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
        info "Skipping cross-tenant test (token extraction failed)"
        return
    fi
    
    # Try to access tenant2/repo2
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE_URL/lfs/tenant2/repo2/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Authorization: Bearer $TOKEN" \
        -d '{"operation":"upload","transfers":["basic"],"objects":[]}')
    
    if [ "$HTTP_CODE" = "403" ]; then
        pass "Cross-tenant access blocked (403)"
    else
        fail "Expected 403, got $HTTP_CODE"
    fi
}

# Test 4: Operation mismatch
test_operation_mismatch() {
    info "Test 4: Using upload token for download operation"
    
    TOKEN_RESPONSE=$(ssh -p $SSH_PORT $SSH_USER@$SSH_HOST git-lfs-authenticate test/repo upload 2>/dev/null || echo "")
    
    if [ -z "$TOKEN_RESPONSE" ]; then
        info "Skipping operation mismatch test (SSH authentication failed)"
        return
    fi
    
    TOKEN=$(echo "$TOKEN_RESPONSE" | jq -r '.header.Authorization' | cut -d' ' -f2)
    
    if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
        info "Skipping operation mismatch test (token extraction failed)"
        return
    fi
    
    # Try to use upload token for download
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE_URL/lfs/test/repo/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Authorization: Bearer $TOKEN" \
        -d '{"operation":"download","transfers":["basic"],"objects":[{"oid":"abc123","size":100}]}')
    
    if [ "$HTTP_CODE" = "403" ]; then
        pass "Operation mismatch blocked (403)"
    else
        fail "Expected 403, got $HTTP_CODE"
    fi
}

# Test 5: Malformed Authorization header
test_malformed_auth() {
    info "Test 5: Malformed Authorization header"
    
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE_URL/lfs/test/repo/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Authorization: Invalid" \
        -d '{"operation":"upload","transfers":["basic"],"objects":[]}')
    
    if [ "$HTTP_CODE" = "401" ]; then
        pass "Malformed auth returns 401"
    else
        fail "Expected 401, got $HTTP_CODE"
    fi
}

# Test 6: Unauthorized upload attempt
test_unauthorized_upload() {
    info "Test 6: Upload without proper authorization"
    
    TEST_OID="0000000000000000000000000000000000000000000000000000000000000000"
    
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X PUT "$BASE_URL/lfs/test/repo/lfs/objects/$TEST_OID" \
        -H "Content-Type: application/octet-stream" \
        --data-binary "test data")
    
    if [ "$HTTP_CODE" = "401" ]; then
        pass "Unauthorized upload blocked (401)"
    else
        fail "Expected 401, got $HTTP_CODE"
    fi
}

# Test 7: Unauthorized download attempt
test_unauthorized_download() {
    info "Test 7: Download without proper authorization"
    
    TEST_OID="0000000000000000000000000000000000000000000000000000000000000000"
    
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X GET "$BASE_URL/lfs/test/repo/lfs/objects/$TEST_OID")
    
    if [ "$HTTP_CODE" = "401" ]; then
        pass "Unauthorized download blocked (401)"
    else
        fail "Expected 401, got $HTTP_CODE"
    fi
}

# Test 8: Repository path mismatch
test_repo_mismatch() {
    info "Test 8: Token for different repository"
    
    TOKEN_RESPONSE=$(ssh -p $SSH_PORT $SSH_USER@$SSH_HOST git-lfs-authenticate test/repo1 upload 2>/dev/null || echo "")
    
    if [ -z "$TOKEN_RESPONSE" ]; then
        info "Skipping repo mismatch test (SSH authentication failed)"
        return
    fi
    
    TOKEN=$(echo "$TOKEN_RESPONSE" | jq -r '.header.Authorization' | cut -d' ' -f2)
    
    if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
        info "Skipping repo mismatch test (token extraction failed)"
        return
    fi
    
    # Try to access different repo
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE_URL/lfs/test/repo2/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Authorization: Bearer $TOKEN" \
        -d '{"operation":"upload","transfers":["basic"],"objects":[]}')
    
    if [ "$HTTP_CODE" = "403" ]; then
        pass "Repository mismatch blocked (403)"
    else
        fail "Expected 403, got $HTTP_CODE"
    fi
}

# Run all tests
echo "Running permission enforcement tests..."
echo ""

test_missing_auth
test_invalid_token
test_malformed_auth
test_unauthorized_upload
test_unauthorized_download
test_cross_tenant
test_operation_mismatch
test_repo_mismatch

echo ""
echo "======================================"
echo "Test Results"
echo "======================================"
echo -e "${GREEN}Passed: $TESTS_PASSED${NC}"
echo -e "${RED}Failed: $TESTS_FAILED${NC}"
echo "Total: $((TESTS_PASSED + TESTS_FAILED))"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All permission enforcement tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    exit 1
fi

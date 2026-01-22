#!/bin/bash
# Test script for LFS batch API operations
# Tests multi-file batch requests and responses

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="${LFS_BASE_URL:-http://localhost:8090}"
TENANT="${LFS_TEST_TENANT:-testorg}"
REPO="${LFS_TEST_REPO:-testrepo}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

echo "======================================"
echo "LFS Batch API Test Suite"
echo "======================================"
echo ""
echo "Base URL: $BASE_URL"
echo "Testing: $TENANT/$REPO"
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

# Test 1: Single file batch upload
test_single_batch_upload() {
    info "Test 1: Single file batch upload request"
    
    OID="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    SIZE=1024
    
    RESPONSE=$(curl -s -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Accept: application/vnd.git-lfs+json" \
        -d "{
            \"operation\": \"upload\",
            \"transfers\": [\"basic\"],
            \"objects\": [
                {\"oid\": \"$OID\", \"size\": $SIZE}
            ]
        }")
    
    # Check if response contains expected fields
    if echo "$RESPONSE" | jq -e '.objects[0].oid' >/dev/null 2>&1; then
        RESPONSE_OID=$(echo "$RESPONSE" | jq -r '.objects[0].oid')
        if [ "$RESPONSE_OID" = "$OID" ]; then
            pass "Single file batch upload - OID matches"
        else
            fail "Single file batch upload" "OID mismatch: expected $OID, got $RESPONSE_OID"
            return
        fi
    else
        fail "Single file batch upload" "Invalid response format"
        info "Response: $RESPONSE"
        return
    fi
    
    # Check if upload action is present
    if echo "$RESPONSE" | jq -e '.objects[0].actions.upload' >/dev/null 2>&1; then
        pass "Single file batch upload - Upload action present"
    else
        fail "Single file batch upload" "Upload action missing"
        info "Response: $RESPONSE"
    fi
}

# Test 2: Multi-file batch upload
test_multi_batch_upload() {
    info "Test 2: Multi-file batch upload request"
    
    OID1="0000000000000000000000000000000000000000000000000000000000000001"
    OID2="0000000000000000000000000000000000000000000000000000000000000002"
    OID3="0000000000000000000000000000000000000000000000000000000000000003"
    
    RESPONSE=$(curl -s -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Accept: application/vnd.git-lfs+json" \
        -d "{
            \"operation\": \"upload\",
            \"transfers\": [\"basic\"],
            \"objects\": [
                {\"oid\": \"$OID1\", \"size\": 1024},
                {\"oid\": \"$OID2\", \"size\": 2048},
                {\"oid\": \"$OID3\", \"size\": 4096}
            ]
        }")
    
    # Check if all objects are in response
    OBJECT_COUNT=$(echo "$RESPONSE" | jq '.objects | length')
    if [ "$OBJECT_COUNT" -eq 3 ]; then
        pass "Multi-file batch upload - All 3 objects in response"
    else
        fail "Multi-file batch upload" "Expected 3 objects, got $OBJECT_COUNT"
        info "Response: $RESPONSE"
        return
    fi
    
    # Verify each OID is present
    for OID in "$OID1" "$OID2" "$OID3"; do
        if echo "$RESPONSE" | jq -e ".objects[] | select(.oid == \"$OID\")" >/dev/null 2>&1; then
            pass "Multi-file batch upload - OID $OID present"
        else
            fail "Multi-file batch upload" "OID $OID missing from response"
        fi
    done
}

# Test 3: Batch download request
test_batch_download() {
    info "Test 3: Batch download request"
    
    OID="1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
    
    RESPONSE=$(curl -s -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Accept: application/vnd.git-lfs+json" \
        -d "{
            \"operation\": \"download\",
            \"transfers\": [\"basic\"],
            \"objects\": [
                {\"oid\": \"$OID\", \"size\": 1024}
            ]
        }")
    
    # For non-existent objects, server should return error in object
    if echo "$RESPONSE" | jq -e '.objects[0]' >/dev/null 2>&1; then
        pass "Batch download - Response received"
        
        # Check if error is present (object doesn't exist)
        if echo "$RESPONSE" | jq -e '.objects[0].error' >/dev/null 2>&1; then
            ERROR_CODE=$(echo "$RESPONSE" | jq -r '.objects[0].error.code')
            if [ "$ERROR_CODE" = "404" ]; then
                pass "Batch download - Correct error for non-existent object"
            else
                info "Batch download - Error code: $ERROR_CODE (may be valid)"
                pass "Batch download - Error present for non-existent object"
            fi
        else
            # If no error, download action should be present
            if echo "$RESPONSE" | jq -e '.objects[0].actions.download' >/dev/null 2>&1; then
                pass "Batch download - Download action present"
            else
                fail "Batch download" "Neither error nor download action present"
            fi
        fi
    else
        fail "Batch download" "Invalid response format"
        info "Response: $RESPONSE"
    fi
}

# Test 4: Mixed sizes batch
test_mixed_sizes() {
    info "Test 4: Mixed file sizes batch"
    
    RESPONSE=$(curl -s -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Accept: application/vnd.git-lfs+json" \
        -d '{
            "operation": "upload",
            "transfers": ["basic"],
            "objects": [
                {"oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "size": 1024},
                {"oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "size": 1048576},
                {"oid": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc", "size": 104857600},
                {"oid": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd", "size": 1073741824}
            ]
        }')
    
    OBJECT_COUNT=$(echo "$RESPONSE" | jq '.objects | length')
    if [ "$OBJECT_COUNT" -eq 4 ]; then
        pass "Mixed sizes batch - All 4 objects in response"
    else
        fail "Mixed sizes batch" "Expected 4 objects, got $OBJECT_COUNT"
        info "Response: $RESPONSE"
        return
    fi
    
    # Verify sizes are preserved
    SIZE_1KB=$(echo "$RESPONSE" | jq -r '.objects[0].size')
    SIZE_1MB=$(echo "$RESPONSE" | jq -r '.objects[1].size')
    SIZE_100MB=$(echo "$RESPONSE" | jq -r '.objects[2].size')
    SIZE_1GB=$(echo "$RESPONSE" | jq -r '.objects[3].size')
    
    [ "$SIZE_1KB" = "1024" ] && pass "Mixed sizes - 1KB size preserved" || fail "Mixed sizes" "1KB size not preserved"
    [ "$SIZE_1MB" = "1048576" ] && pass "Mixed sizes - 1MB size preserved" || fail "Mixed sizes" "1MB size not preserved"
    [ "$SIZE_100MB" = "104857600" ] && pass "Mixed sizes - 100MB size preserved" || fail "Mixed sizes" "100MB size not preserved"
    [ "$SIZE_1GB" = "1073741824" ] && pass "Mixed sizes - 1GB size preserved" || fail "Mixed sizes" "1GB size not preserved"
}

# Test 5: Empty batch request
test_empty_batch() {
    info "Test 5: Empty batch request"
    
    RESPONSE=$(curl -s -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Accept: application/vnd.git-lfs+json" \
        -d '{
            "operation": "upload",
            "transfers": ["basic"],
            "objects": []
        }')
    
    OBJECT_COUNT=$(echo "$RESPONSE" | jq '.objects | length')
    if [ "$OBJECT_COUNT" -eq 0 ]; then
        pass "Empty batch - Returns empty objects array"
    else
        fail "Empty batch" "Expected 0 objects, got $OBJECT_COUNT"
    fi
}

# Test 6: Large batch (10+ files)
test_large_batch() {
    info "Test 6: Large batch request (10 files)"
    
    # Build JSON with 10 objects
    OBJECTS="["
    for i in {1..10}; do
        OID=$(printf "%064d" $i)
        OBJECTS="$OBJECTS{\"oid\":\"$OID\",\"size\":$((i * 1024))}"
        [ $i -lt 10 ] && OBJECTS="$OBJECTS,"
    done
    OBJECTS="$OBJECTS]"
    
    RESPONSE=$(curl -s -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Accept: application/vnd.git-lfs+json" \
        -d "{
            \"operation\": \"upload\",
            \"transfers\": [\"basic\"],
            \"objects\": $OBJECTS
        }")
    
    OBJECT_COUNT=$(echo "$RESPONSE" | jq '.objects | length')
    if [ "$OBJECT_COUNT" -eq 10 ]; then
        pass "Large batch - All 10 objects in response"
    else
        fail "Large batch" "Expected 10 objects, got $OBJECT_COUNT"
        info "Response: $RESPONSE"
    fi
}

# Test 7: Verify transfer type in response
test_transfer_type() {
    info "Test 7: Transfer type in response"
    
    RESPONSE=$(curl -s -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Accept: application/vnd.git-lfs+json" \
        -d '{
            "operation": "upload",
            "transfers": ["basic"],
            "objects": [
                {"oid": "1111111111111111111111111111111111111111111111111111111111111111", "size": 1024}
            ]
        }')
    
    TRANSFER=$(echo "$RESPONSE" | jq -r '.transfer')
    if [ "$TRANSFER" = "basic" ]; then
        pass "Transfer type - 'basic' returned"
    else
        fail "Transfer type" "Expected 'basic', got '$TRANSFER'"
    fi
}

# Test 8: Invalid operation
test_invalid_operation() {
    info "Test 8: Invalid operation type"
    
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -d '{
            "operation": "invalid_op",
            "transfers": ["basic"],
            "objects": [
                {"oid": "2222222222222222222222222222222222222222222222222222222222222222", "size": 1024}
            ]
        }')
    
    if [ "$HTTP_CODE" = "400" ] || [ "$HTTP_CODE" = "422" ]; then
        pass "Invalid operation - Rejected with HTTP $HTTP_CODE"
    else
        fail "Invalid operation" "Expected 400 or 422, got $HTTP_CODE"
    fi
}

# Test 9: Missing required fields
test_missing_fields() {
    info "Test 9: Missing required fields"
    
    # Missing operation
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -d '{
            "transfers": ["basic"],
            "objects": [{"oid": "abc", "size": 1024}]
        }')
    
    if [ "$HTTP_CODE" = "400" ] || [ "$HTTP_CODE" = "422" ]; then
        pass "Missing operation - Rejected with HTTP $HTTP_CODE"
    else
        fail "Missing operation" "Expected 400 or 422, got $HTTP_CODE"
    fi
}

# Test 10: Duplicate OIDs in batch
test_duplicate_oids() {
    info "Test 10: Duplicate OIDs in batch"
    
    OID="3333333333333333333333333333333333333333333333333333333333333333"
    
    RESPONSE=$(curl -s -X POST "$BASE_URL/lfs/$TENANT/$REPO/info/lfs/objects/batch" \
        -H "Content-Type: application/vnd.git-lfs+json" \
        -H "Accept: application/vnd.git-lfs+json" \
        -d "{
            \"operation\": \"upload\",
            \"transfers\": [\"basic\"],
            \"objects\": [
                {\"oid\": \"$OID\", \"size\": 1024},
                {\"oid\": \"$OID\", \"size\": 1024}
            ]
        }")
    
    # Server may deduplicate or return both - either is acceptable
    OBJECT_COUNT=$(echo "$RESPONSE" | jq '.objects | length')
    if [ "$OBJECT_COUNT" -ge 1 ] && [ "$OBJECT_COUNT" -le 2 ]; then
        pass "Duplicate OIDs - Handled correctly (returned $OBJECT_COUNT object(s))"
    else
        fail "Duplicate OIDs" "Unexpected object count: $OBJECT_COUNT"
    fi
}

# Run all tests
test_single_batch_upload
test_multi_batch_upload
test_batch_download
test_mixed_sizes
test_empty_batch
test_large_batch
test_transfer_type
test_invalid_operation
test_missing_fields
test_duplicate_oids

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

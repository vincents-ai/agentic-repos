#!/usr/bin/env bash
set -e

# Run 5 representative tests to validate infrastructure
# These tests cover: Git, LFS, Security, Storage, and Caching

echo "=== Running Sample Test Suite (5 tests) ==="
echo "This validates the test infrastructure before running full 519-test suite"
echo ""

TESTS=(
  "git-integration-ssh-authentication-ed25519"  # Basic Git + SSH
  "lfs-object-upload"                           # LFS functionality
  "security-auth-ssh-ed25519-key"               # Security/Auth
  "storage-local-repo-create"                   # Local storage
  "cache-set-get"                               # Redis caching
)

PASSED=0
FAILED=0
FAILED_TESTS=()

for test in "${TESTS[@]}"; do
  echo "----------------------------------------"
  echo "Running: $test"
  echo "----------------------------------------"
  
  if nix build ".#checks.x86_64-linux.$test" --no-link 2>&1 | tee "/tmp/test-$test.log"; then
    echo "✅ PASSED: $test"
    PASSED=$((PASSED + 1))
  else
    echo "❌ FAILED: $test"
    FAILED=$((FAILED + 1))
    FAILED_TESTS+=("$test")
    
    # Show last 30 lines of failure
    echo ""
    echo "Last 30 lines of error:"
    tail -30 "/tmp/test-$test.log"
    echo ""
  fi
  
  echo ""
done

echo "========================================"
echo "Sample Test Results:"
echo "  Passed: $PASSED/5"
echo "  Failed: $FAILED/5"
echo "========================================"

if [ $FAILED -gt 0 ]; then
  echo ""
  echo "Failed tests:"
  for test in "${FAILED_TESTS[@]}"; do
    echo "  - $test"
    echo "    Log: /tmp/test-$test.log"
  done
  echo ""
  echo "To debug a specific failure:"
  echo "  nix log /nix/store/...-vm-test-run-<test-name>.drv"
  exit 1
else
  echo ""
  echo "✅ All sample tests passed!"
  echo "Infrastructure is validated and ready for full test suite."
  echo ""
  echo "To run full test suite (519 tests - VERY SLOW):"
  echo "  nix flake check --keep-going"
  exit 0
fi

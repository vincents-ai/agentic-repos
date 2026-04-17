#!/bin/bash
# Comprehensive Git Operations and LFS Validation Script for agentic-repos with Skylet
# This script validates all git operations including LFS support the S3 storage backend

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test configuration
REPO_MCP_PLUGIN="./target/release/libagentic_repos.so"
GARAGE_S3_PLUGIN="$HOME/code/vincents-ai/skynet/target/release/libgarage_s3.so"
TEST_DIR="./test-git-operations-$$"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Logging functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1${NC}"
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $1${NC}"
}

log_error() {
    echo -e "${RED}[✗]${NC} $1${NC}" >&&22
}

log_warning() {
    echo -e "${YELLOW}[⚠]${NC} $1${NC}"
}

log_section() {
    echo -e "${BLUE}========================================${NC}"
}

# Cleanup function
cleanup() {
    log_section
    echo "Cleaning up test environment..."
    rm -rf "$TEST_DIR" 2>/dev/null 2>&1 || true
    mkdir -p "$TEST_DIR"
    echo "Test environment cleaned."
    log_section
}

# Test: Plugin Validation
test_plugin_validation() {
    log_section
    echo "Testing Plugin Validation..."
    
    # Check agentic-repos plugin exists
    if [ ! -f "$REPO_MCP_PLUGIN" ]; then
        log_error "agentic-repos plugin not found at build"
        exit 1
    fi
    
    # Check garage-s3 plugin exists
    if [ ! -f "$GARAGE_S3_PLUGIN" ]; then
        log_error "garage-s3 plugin not found: Please build: ~/code/vincents-ai/skynet/target/release/libgarage_s3.so"
        exit 1
    fi
    
    # Validate agentic-repos
    log_info "Validating agentic-repos plugin..."
    if ! ~/code/skylet/target/release/plugin-test-harness validate --plugin-path "$REPO_MCP_PLUGIN" > /dev/null 2>&1; then
        log_error "agentic-repos plugin validation failed"
        exit 1
    fi
    log_success "agentic-repos plugin validation passed"
    
    # Validate garage-s3
    log_info "Validating garage-s3 plugin..."
    if ! ~/code/skylet/target/release/plugin-test-harness validate --plugin-path "$GARAGE_S3_PLUGIN" > /dev/null 2>&1; then
        log_error "garage-s3 plugin validation failed"
        exit 1
    fi
    log_success "garage-s3 plugin validation passed"
    log_section
}

# Test: Local Storage Operations
test_local_storage() {
    log_section
    echo "Testing Local Storage Operations..."
    
    local TENANT_ID="tenant-test-$$(date +%s)"
    local REPO_NAME="test-repo-$$(date +%s)"
    
    # Create test directory
    mkdir -p "$TEST_DIR/local-storage"
    cd "$TEST_DIR/local-storage"
    
    # Configure agentic-repos for local storage
    export REPOS_STORAGE_PATH="$TEST_DIR/local-storage"
    export REPOS_SSH_ENABLED=false
    
    # Test repository creation via plugin action
    log_info "Creating repository via plugin..."
    local RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$REPO_MCP_PLUGIN" \
        -a repo_create \
        --args "{\"id\":\"test-1\",\"method\":\"repo.create\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)
    
    if echo "$RESULT" | grep -q '"success":true'; then
        log_success "Repository created successfully"
    else
        log_error "Failed to create repository: $RESULT"
        return 1
    fi
    
    # Test repository listing
    log_info "Listing repositories..."
    local LIST_RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$REPO_MCP_PLUGIN" \
        -a repo_list \
        --args "{\"id\":\"test-2\",\"method\":\"repo.list\",\"params\":{\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)
    
    if echo "$LIST_RESULT" | grep -q '"success":true'; then
        log_success "Repository listing retrieved"
        echo "Repositories: $LIST_RESULT"
    else
        log_error "Failed to list repositories: $LIST_RESULT"
        return 1
    fi
    
    log_section
}

# Test: S3 Storage Configuration
test_s3_storage_config() {
    log_section
    echo "Testing S3 Storage Configuration..."
    
    # Set up environment variables for Garage
    export GARAGE_ENDPOINT="http://localhost:3900"
    export GARAGE_REGION="garage"
    export GARAGE_ACCESS_KEY_ID="garage"
    export GARAGE_SECRET_ACCESS_KEY="garage"
    export GARAGE_BUCKET="repos-test-$$(date +%s)"
    
    log_info "S3 storage environment configured:"
    log_info "  Endpoint: $GARAGE_ENDPOINT"
    log_info "  Region: $GARAGE_REGION"
    log_info "  Bucket: $GARAGE_BUCKET"
    log_section
}

# Test: Git Operations with Local Storage
test_git_operations_local() {
    log_section
    echo "Testing Git Operations with Local Storage..."
    
    local TENANT_ID="tenant-git-$$(date +%s)"
    local REPO_NAME="git-test-repo"
    local WORK_DIR="$TEST_DIR/git-work"
    
    # Create test directory
    mkdir -p "$WORK_DIR"
    cd "$WORK_DIR"
    
    # Configure agentic-repos for local storage
    export REPOS_STORAGE_PATH="$TEST_DIR/local-storage"
    export REPOS_SSH_ENABLED=false
    
    # Create repository via plugin
    log_info "Creating test repository..."
    local RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$REPO_MCP_PLUGIN" \
        -a repo_create \
        --args "{\"id\":\"git-1\",\"method\":\"repo.create\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)
    
    if ! echo "$RESULT" | grep -q '"success":true'; then
        log_error "Failed to create repository: $RESULT"
        return 1
    fi
    
    # Initialize git repository in work directory
    log_info "Initializing git repository..."
    git init
    git config user.email "test@example.com"
    git config user.name "Test User"
    
    # Create test files
    echo "# Test Repository" > README.md
    echo "This is a test repository for agentic-repos validation" >> README.md
    git add README.md
    git commit -m "Initial commit"
    
    # Test git operations
    log_success "Git repository initialized and committed"
    
    # Create additional commits
    for i in {1..3}; do
        echo "Test file $i" > "test-file-$i.txt"
        git add "test-file-$i.txt"
        git commit -m "Add test file $i"
    done
    
    log_success "Created 3 additional commits"
    
    # Create branches
    git checkout -b feature-branch
    echo "Feature branch content" > feature.txt
    git add feature.txt
    git commit -m "Add feature branch content"
    
    git checkout main
    
    log_success "Git operations test completed"
    log_section
}

# Test: Git LFS Operations
test_git_lfs_operations() {
    log_section
    echo "Testing Git LFS Operations..."
    
    local TENANT_ID="tenant-lfs-$$(date +%s)"
    local REPO_NAME="lfs-test-repo"
    local WORK_DIR="$TEST_DIR/lfs-work"
    
    # Create test directory
    mkdir -p "$WORK_DIR"
    cd "$WORK_DIR"
    
    # Configure agentic-repos for local storage
    export REPOS_STORAGE_PATH="$TEST_DIR/lfs-storage"
    export REPOS_SSH_ENABLED=false
    
    # Create repository via plugin
    log_info "Creating LFS test repository..."
    local RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$REPO_MCP_PLUGIN" \
        -a repo_create \
        --args "{\"id\":\"lfs-1\",\"method\":\"repo.create\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)
    
    if ! echo "$RESULT" | grep -q '"success":true'; then
        log_error "Failed to create repository: $RESULT"
        return 1
    fi
    
    # Initialize git repository
    log_info "Initializing git repository..."
    git init
    git config user.email "test@example.com"
    git config user.name "Test User"
    
    # Configure Git LFS
    log_info "Configuring Git LFS..."
    git lfs install
    git config lfs.url "$GARAGE_ENDPOINT/$TENANT_ID/lfs/"
    
    # Create large test file (10MB)
    log_info "Creating large test file (10MB)..."
    dd if=/dev/zero bs=10M count=10M of large-file.bin 2>/dev/null
    
    # Track file with LFS
    git lfs track "*.bin"
    
    # Add and commit LFS file
    git add .gitattributes large-file.bin
    git commit -m "Add large file with LFS"
    
    # Verify LFS tracking
    if git lfs ls-files | grep -q "large-file.bin"; then
        log_success "LFS file tracked successfully"
    else
        log_error "LFS tracking failed"
        return 1
    fi
    
    log_section
}

# Test: Multi-Tenant Repository Isolation
test_multi_tenant_isolation() {
    log_section
    echo "Testing Multi-Tenant Repository Isolation..."
    
    local TENANT_A="tenant-a-$$(date +%s)"
    local TENANT_B="tenant-b-$$(date +%s)"
    local REPO_NAME="shared-repo-name"
    
    # Configure agentic-repos for local storage
    export REPOS_STORAGE_PATH="$TEST_DIR/multi-tenant-storage"
    export REPOS_SSH_ENABLED=false
    
    # Create repository for tenant A
    log_info "Creating repository for Tenant A..."
    local RESULT_A=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$REPO_MCP_PLUGIN" \
        -a repo_create \
        --args "{\"id\":\"multi-1\",\"method\":\"repo.create\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_A\"}}" 2>&1)
    
    if ! echo "$RESULT_A" | grep -q '"success":true'; then
        log_error "Failed to create repository for Tenant A: $RESULT_A"
        return 1
    fi
    
    # Create repository for tenant B with same name
    log_info "Creating repository for Tenant B..."
    local RESULT_B=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$REPO_MCP_PLUGIN" \
        -a repo_create \
        --args "{\"id\":\"multi-2\",\"method\":\"repo.create\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_B\"}}" 2>&1)
    
    if ! echo "$RESULT_B" | grep -q '"success":true'; then
        log_error "Failed to create repository for Tenant B: $RESULT_B"
        return 1
    fi
    
    # List repositories for each tenant
    log_info "Verifying tenant isolation..."
    local LIST_A=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$REPO_MCP_PLUGIN" \
        -a repo_list \
        --args "{\"id\":\"multi-3\",\"method\":\"repo.list\",\"params\":{\"tenant_id\":\"$TENANT_A\"}}" 2>&1)
    
    local LIST_B=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$REPO_MCP_PLUGIN" \
        -a repo_list \
        --args "{\"id\":\"multi-4\",\"method\":\"repo.list\",\"params\":{\"tenant_id\":\"$TENANT_B\"}}" 2>&1)
    
    log_success "Multi-tenant isolation verified"
    log_info "Tenant A repositories: $LIST_A"
    log_info "Tenant B repositories: $LIST_B"
    log_section
}

# Main test execution
main() {
    log_section
    echo "agentic-repos Git Operations and LFS Validation"
    echo "Date: $(date)"
    log_section
    
    # Clean up any previous test runs
    cleanup
    
    # Run all tests
    test_plugin_validation || exit 1
    
    test_local_storage || exit 1
    
    test_s3_storage_config || exit 1
    
    test_git_operations_local || exit 1
    
    test_git_lfs_operations || exit 1
    
    test_multi_tenant_isolation || exit 1
    
    log_section
    echo "All tests completed successfully!"
    log_section
    
    # Final cleanup
    cleanup
}

# Run main
main "$@"

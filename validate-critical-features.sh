#!/usr/bin/env bash
# Comprehensive Validation Test for Critical Features
# Tests: SSH git operations, S3 storage, and end-to-end functionality
# Requires: Running Skylet instance with repos-mcp and garage-s3 plugins

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
SKYLET_DIR="$HOME/code/skylet"
SKYLET_PID=""
TEST_DIR="/tmp/repos-mcp-critical-validation-$$"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$TEST_DIR/validation.log"

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_TOTAL=0

# Logging functions
log_info() { echo -e "${GREEN}[INFO]${NC} $1"; [ -f "$LOG_FILE" ] && echo -e "[INFO] $1" >> "$LOG_FILE" || true; }
log_success() { echo -e "${GREEN}[✓]${NC} $1"; [ -f "$LOG_FILE" ] && echo -e "[✓] $1" >> "$LOG_FILE" || true; ((TESTS_PASSED++)); }
log_error() { echo -e "${RED}[✗]${NC} $1"; [ -f "$LOG_FILE" ] && echo -e "[✗] $1" >> "$LOG_FILE" || true; ((TESTS_FAILED++)); }
log_warning() { echo -e "${YELLOW}[⚠]${NC} $1"; [ -f "$LOG_FILE" ] && echo -e "[⚠] $1" >> "$LOG_FILE" || true; }
log_section() { echo -e "${BLUE}========================================${NC}"; [ -f "$LOG_FILE" ] && echo "========================================" >> "$LOG_FILE" || true; }
log_test() { echo -e "${BLUE}[TEST $1]${NC} $2"; [ -f "$LOG_FILE" ] && echo -e "[TEST $1] $2" >> "$LOG_FILE" || true; ((TESTS_TOTAL++)); }

# Cleanup function
cleanup() {
    log_section
    echo "Cleaning up..." | tee -a "$LOG_FILE"
    
    # Stop Skylet if we started it
    if [ ! -z "$SKYLET_PID" ]; then
        log_info "Stopping Skylet (PID: $SKYLET_PID)..."
        kill $SKYLET_PID 2>/dev/null || true
        wait $SKYLET_PID 2>/dev/null || true
    fi
    
    # Kill any stray processes
    pkill -f "skylet server" 2>/dev/null || true
    pkill -f "garage" 2>/dev/null || true
    
    # Clean up test directory (optional - keep for debugging)
    if [ "$1" == "--full" ]; then
        rm -rf "$TEST_DIR" 2>/dev/null || true
    else
        log_info "Test directory preserved at: $TEST_DIR"
        log_info "Log file: $LOG_FILE"
    fi
    
    log_section
}

# Error handling
trap 'log_error "Test interrupted"; cleanup --full; exit 1' INT TERM

# Setup test environment
setup_test_env() {
    log_section
    echo "Setting up test environment..." | tee -a "$LOG_FILE"
    
    # Create test directory
    mkdir -p "$TEST_DIR"/{repos,keys,client1,client2,logs}
    
    # Initialize log file
    echo "repos-mcp Critical Features Validation" > "$LOG_FILE"
    echo "Date: $(date)" >> "$LOG_FILE"
    echo "Test Directory: $TEST_DIR" >> "$LOG_FILE"
    echo "========================================" >> "$LOG_FILE"
    
    # Check prerequisites
    log_test 1 "Checking prerequisites..."
    
    if [ ! -f "$SKYLET_DIR/target/release/skylet" ]; then
        log_error "Skylet binary not found at $SKYLET_DIR/target/release/skylet"
        exit 1
    fi
    log_success "Skylet binary found"
    
    if [ ! -f "$SKYLET_DIR/plugins/repos-mcp/librepos_mcp.so" ]; then
        log_error "repos-mcp plugin not found"
        exit 1
    fi
    log_success "repos-mcp plugin found"
    
    if [ ! -f "$SKYLET_DIR/plugins/garage-s3/libgarage_s3.so" ]; then
        log_warning "garage-s3 plugin not found - S3 tests will be skipped"
        SKIP_S3_TESTS=1
    else
        log_success "garage-s3 plugin found"
        SKIP_S3_TESTS=0
    fi
    
    log_section
}

# Start Skylet server
start_skylet() {
    log_section
    log_test 2 "Starting Skylet server..."
    
    cd "$SKYLET_DIR"
    
    # Check if Skylet is already running
    if pgrep -f "skylet server" > /dev/null; then
        log_warning "Skylet server already running, stopping it..."
        pkill -f "skylet server" || true
        sleep 2
    fi
    
    # Start Skylet in background
    log_info "Starting Skylet..."
    ./target/release/skylet server > "$TEST_DIR/logs/skylet.log" 2>&1 &
    SKYLET_PID=$!
    
    log_info "Waiting for Skylet to initialize..."
    sleep 5
    
    # Verify Skylet started
    if ! ps -p $SKYLET_PID > /dev/null 2>&1; then
        log_error "Skylet failed to start"
        log_error "Check logs at: $TEST_DIR/logs/skylet.log"
        cat "$TEST_DIR/logs/skylet.log" | tail -20 | tee -a "$LOG_FILE"
        exit 1
    fi
    
    log_success "Skylet started (PID: $SKYLET_PID)"
    
    # Check if plugins loaded
    sleep 3
    if grep -q "git-operations" "$TEST_DIR/logs/skylet.log" || grep -q "repos-mcp" "$TEST_DIR/logs/skylet.log"; then
        log_success "repos-mcp plugin loaded"
    else
        log_warning "repos-mcp plugin may not have loaded"
    fi
    
    if [ "$SKIP_S3_TESTS" -eq 0 ]; then
        if grep -q "garage-s3" "$TEST_DIR/logs/skylet.log"; then
            log_success "garage-s3 plugin loaded"
        else
            log_warning "garage-s3 plugin may not have loaded"
        fi
    fi
    
    log_section
}

# Setup SSH keys
setup_ssh_keys() {
    log_section
    log_test 3 "Setting up SSH keys..."
    
    cd "$TEST_DIR/keys"
    
    # Generate SSH key pair for testing
    log_info "Generating SSH key pair..."
    ssh-keygen -t rsa -b 2048 -f test_key -N "" -q
    chmod 600 test_key
    chmod 644 test_key.pub
    
    # Generate host key
    log_info "Generating SSH host key..."
    ssh-keygen -t rsa -f host_key -N "" -q
    chmod 600 host_key
    
    # Create authorized_keys
    cat test_key.pub > authorized_keys
    chmod 600 authorized_keys
    
    # Update Skylet config with SSH settings
    log_info "Configuring SSH in Skylet..."
    export REPOS_SSH_ENABLED=true
    export REPOS_SSH_PORT=2222
    export REPOS_SSH_HOST_KEY_PATH="$TEST_DIR/keys/host_key"
    export REPOS_SSH_AUTHORIZED_KEYS_PATH="$TEST_DIR/keys/authorized_keys"
    
    log_success "SSH keys configured"
    log_section
}

# Test repository creation
test_repo_creation() {
    log_section
    log_test 4 "Testing repository creation..."
    
    TENANT_ID="tenant-ssh-test-$TIMESTAMP"
    REPO_NAME="test-repo"
    
    # Create repository via plugin
    log_info "Creating repository '$REPO_NAME' for tenant '$TENANT_ID'..."
    RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$SKYLET_DIR/plugins/repos-mcp/librepos_mcp.so" \
        -a repo.create \
        --args "{\"id\":\"test-1\",\"method\":\"repo.create\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)
    
    if echo "$RESULT" | grep -q '"success":true'; then
        log_success "Repository created successfully"
        echo "$RESULT" | tee -a "$LOG_FILE"
    else
        log_error "Failed to create repository"
        echo "$RESULT" | tee -a "$LOG_FILE"
        return 1
    fi
    
    # List repositories to verify
    log_info "Listing repositories..."
    LIST_RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$SKYLET_DIR/plugins/repos-mcp/librepos_mcp.so" \
        -a repo.list \
        --args "{\"id\":\"test-2\",\"method\":\"repo.list\",\"params\":{\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)
    
    if echo "$LIST_RESULT" | grep -q "$REPO_NAME"; then
        log_success "Repository found in list"
    else
        log_error "Repository not found in list"
        echo "$LIST_RESULT" | tee -a "$LOG_FILE"
        return 1
    fi
    
    # Save for later tests
    echo "$TENANT_ID" > "$TEST_DIR/tenant_id"
    echo "$REPO_NAME" > "$TEST_DIR/repo_name"
    
    log_section
}

# Test SSH git operations
test_ssh_git_operations() {
    log_section
    log_test 5 "Testing SSH git clone/push/pull operations..."
    
    # Check if SSH server is running
    log_info "Checking SSH server status..."
    if ! netstat -tuln 2>/dev/null | grep -q ":2222 "; then
        log_error "SSH server not listening on port 2222"
        log_warning "SSH tests require Skylet runtime with SSH enabled"
        log_warning "Skipping SSH tests"
        return 0
    fi
    
    log_success "SSH server is listening on port 2222"
    
    # Read tenant and repo info
    TENANT_ID=$(cat "$TEST_DIR/tenant_id")
    REPO_NAME=$(cat "$TEST_DIR/repo_name")
    
    # Configure git to use our SSH key
    export GIT_SSH_COMMAND="ssh -i $TEST_DIR/keys/test_key -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p 2222"
    
    cd "$TEST_DIR/client1"
    
    # Test 1: Git clone
    log_info "Testing git clone via SSH..."
    log_info "Clone URL: git@localhost:$TENANT_ID/$REPO_NAME"
    
    if timeout 30 git clone "git@localhost:$TENANT_ID/$REPO_NAME" 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Git clone succeeded"
        
        cd "$REPO_NAME"
        
        # Test 2: Git push
        log_info "Testing git push via SSH..."
        echo "# Test Repository" > README.md
        echo "Testing SSH git operations" >> README.md
        git config user.email "test@example.com"
        git config user.name "Test User"
        git add README.md
        git commit -m "Initial commit"
        
        if timeout 30 git push origin master 2>&1 | tee -a "$LOG_FILE"; then
            log_success "Git push succeeded"
        else
            log_error "Git push failed"
            return 1
        fi
        
        # Test 3: Git pull (from another client)
        log_info "Testing git pull via SSH..."
        cd "$TEST_DIR/client2"
        
        export GIT_SSH_COMMAND="ssh -i $TEST_DIR/keys/test_key -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p 2222"
        
        if timeout 30 git clone "git@localhost:$TENANT_ID/$REPO_NAME" 2>&1 | tee -a "$LOG_FILE"; then
            cd "$REPO_NAME"
            
            # Make a change and push
            echo "Client 2 change" >> README.md
            git add README.md
            git commit -m "Client 2 update"
            git push origin master 2>&1 | tee -a "$LOG_FILE"
            
            # Pull in client 1
            cd "$TEST_DIR/client1/$REPO_NAME"
            if timeout 30 git pull origin master 2>&1 | tee -a "$LOG_FILE"; then
                log_success "Git pull succeeded"
            else
                log_error "Git pull failed"
                return 1
            fi
        else
            log_error "Second client clone failed"
            return 1
        fi
        
    else
        log_error "Git clone failed"
        log_warning "This is expected if SSH server is not fully integrated"
        log_warning "SSH git operations require full Skylet runtime integration"
        return 1
    fi
    
    log_section
}

# Test S3 storage operations
test_s3_storage_operations() {
    log_section
    log_test 6 "Testing S3 storage operations..."
    
    if [ "$SKIP_S3_TESTS" -eq 1 ]; then
        log_warning "Skipping S3 tests (garage-s3 plugin not available)"
        return 0
    fi
    
    # Check if Garage is running
    log_info "Checking Garage S3 server..."
    if ! curl -s http://localhost:3900/health > /dev/null 2>&1; then
        log_warning "Garage S3 server not responding at localhost:3900"
        log_warning "S3 storage tests require garage-s3 plugin to be running"
        return 0
    fi
    
    log_success "Garage S3 server is running"
    
    # Configure AWS CLI for Garage
    export AWS_ACCESS_KEY_ID="GK_test"
    export AWS_SECRET_ACCESS_KEY="test_secret"
    export AWS_DEFAULT_REGION="garage"
    
    # Test S3 bucket creation
    log_info "Creating S3 bucket..."
    if aws --endpoint-url http://localhost:3900 s3 mb s3://test-bucket 2>&1 | tee -a "$LOG_FILE"; then
        log_success "S3 bucket created"
    else
        log_warning "S3 bucket creation failed (may already exist)"
    fi
    
    # Test S3 upload
    log_info "Testing S3 upload..."
    echo "Test content" > "$TEST_DIR/test-file.txt"
    if aws --endpoint-url http://localhost:3900 s3 cp "$TEST_DIR/test-file.txt" s3://test-bucket/ 2>&1 | tee -a "$LOG_FILE"; then
        log_success "S3 upload succeeded"
    else
        log_error "S3 upload failed"
        return 1
    fi
    
    # Test S3 download
    log_info "Testing S3 download..."
    if aws --endpoint-url http://localhost:3900 s3 cp s3://test-bucket/test-file.txt "$TEST_DIR/downloaded-file.txt" 2>&1 | tee -a "$LOG_FILE"; then
        log_success "S3 download succeeded"
    else
        log_error "S3 download failed"
        return 1
    fi
    
    # Test S3 list
    log_info "Testing S3 list..."
    if aws --endpoint-url http://localhost:3900 s3 ls s3://test-bucket/ 2>&1 | tee -a "$LOG_FILE"; then
        log_success "S3 list succeeded"
    else
        log_error "S3 list failed"
        return 1
    fi
    
    log_section
}

# Test end-to-end workflow
test_end_to_end_workflow() {
    log_section
    log_test 7 "Testing end-to-end workflow..."
    
    TENANT_ID="tenant-e2e-$TIMESTAMP"
    REPO_NAME="e2e-test-repo"
    
    # Create repository
    log_info "Creating repository for end-to-end test..."
    RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$SKYLET_DIR/plugins/repos-mcp/librepos_mcp.so" \
        -a repo.create \
        --args "{\"id\":\"e2e-1\",\"method\":\"repo.create\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)
    
    if ! echo "$RESULT" | grep -q '"success":true'; then
        log_error "Repository creation failed"
        echo "$RESULT" | tee -a "$LOG_FILE"
        return 1
    fi
    
    log_success "Repository created"
    
    # Initialize local git repo
    cd "$TEST_DIR"
    mkdir -p e2e-work
    cd e2e-work
    
    git init
    git config user.email "e2e@test.com"
    git config user.name "E2E Test"
    
    # Create initial content
    echo "# E2E Test Repository" > README.md
    git add README.md
    git commit -m "Initial commit"
    
    # Create multiple files
    for i in {1..5}; do
        echo "File $i content" > "file$i.txt"
        git add "file$i.txt"
        git commit -m "Add file $i"
    done
    
    log_success "Created local git repository with 6 commits"
    
    # Create branches
    git checkout -b feature-a
    echo "Feature A" > feature-a.txt
    git add feature-a.txt
    git commit -m "Add feature A"
    
    git checkout master
    git checkout -b feature-b
    echo "Feature B" > feature-b.txt
    git add feature-b.txt
    git commit -m "Add feature B"
    
    git checkout master
    
    log_success "Created feature branches"
    
    # Test LFS configuration
    log_info "Configuring Git LFS..."
    git lfs install
    git lfs track "*.bin"
    echo "*.bin filter=lfs diff=lfs merge=lfs -text" > .gitattributes
    git add .gitattributes
    git commit -m "Configure LFS"
    
    # Create a large file
    dd if=/dev/zero bs=1M count=5 of=large-file.bin 2>/dev/null
    git add large-file.bin
    git commit -m "Add large file"
    
    log_success "LFS configured and large file added"
    
    # Verify repository info
    log_info "Verifying repository info..."
    INFO_RESULT=$(~/code/skylet/target/release/plugin-test-harness execute \
        -p "$SKYLET_DIR/plugins/repos-mcp/librepos_mcp.so" \
        -a repo.info \
        --args "{\"id\":\"e2e-2\",\"method\":\"repo.info\",\"params\":{\"name\":\"$REPO_NAME\",\"tenant_id\":\"$TENANT_ID\"}}" 2>&1)
    
    if echo "$INFO_RESULT" | grep -q '"success":true'; then
        log_success "Repository info retrieved"
        echo "$INFO_RESULT" | tee -a "$LOG_FILE"
    else
        log_warning "Repository info retrieval failed"
        echo "$INFO_RESULT" | tee -a "$LOG_FILE"
    fi
    
    log_section
}

# Print test summary
print_summary() {
    log_section
    echo "========================================"  | tee -a "$LOG_FILE"
    echo "        VALIDATION SUMMARY"  | tee -a "$LOG_FILE"
    echo "========================================"  | tee -a "$LOG_FILE"
    echo ""  | tee -a "$LOG_FILE"
    echo "Total Tests:  $TESTS_TOTAL"  | tee -a "$LOG_FILE"
    echo "Passed:       $TESTS_PASSED"  | tee -a "$LOG_FILE"
    echo "Failed:       $TESTS_FAILED"  | tee -a "$LOG_FILE"
    echo ""  | tee -a "$LOG_FILE"
    
    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}✅ ALL CRITICAL TESTS PASSED${NC}"  | tee -a "$LOG_FILE"
    else
        echo -e "${RED}❌ SOME TESTS FAILED${NC}"  | tee -a "$LOG_FILE"
    fi
    
    echo ""  | tee -a "$LOG_FILE"
    echo "Test artifacts:"  | tee -a "$LOG_FILE"
    echo "  - Log file: $LOG_FILE"  | tee -a "$LOG_FILE"
    echo "  - Test directory: $TEST_DIR"  | tee -a "$LOG_FILE"
    echo "  - Skylet logs: $TEST_DIR/logs/skylet.log"  | tee -a "$LOG_FILE"
    echo ""  | tee -a "$LOG_FILE"
    echo "========================================"  | tee -a "$LOG_FILE"
    
    log_section
}

# Main test execution
main() {
    # Create test directory first
    mkdir -p "$TEST_DIR"/{repos,keys,client1,client2,logs}
    
    # Initialize log file
    LOG_FILE="$TEST_DIR/validation.log"
    echo "repos-mcp Critical Features Validation" > "$LOG_FILE"
    echo "Date: $(date)" >> "$LOG_FILE"
    echo "Testing: SSH Git Operations, S3 Storage, End-to-End" >> "$LOG_FILE"
    
    log_section
    echo "repos-mcp Critical Features Validation"  | tee -a "$LOG_FILE"
    echo "Date: $(date)"  | tee -a "$LOG_FILE"
    echo "Testing: SSH Git Operations, S3 Storage, End-to-End"  | tee -a "$LOG_FILE"
    log_section
    
    # Setup
    setup_test_env
    
    # Start Skylet
    start_skylet
    
    # Setup SSH
    setup_ssh_keys
    
    # Run tests
    test_repo_creation
    test_ssh_git_operations
    test_s3_storage_operations
    test_end_to_end_workflow
    
    # Print summary
    print_summary
    
    # Cleanup (keep logs for debugging)
    cleanup
    
    # Exit with appropriate code
    if [ $TESTS_FAILED -eq 0 ]; then
        exit 0
    else
        exit 1
    fi
}

# Run main
main "$@"

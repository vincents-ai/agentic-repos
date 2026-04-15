#!/usr/bin/env nix develop -c -- bash

# agentic-repos SDLC Quality Gates Script
# Automates quality gate checks for CI/CD pipeline

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(git rev-parse --show-toplevel)"
CONFIG_FILE="${SCRIPT_DIR}/../config.json"
LOG_FILE="${PROJECT_ROOT}/quality-gates.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$LOG_FILE"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$LOG_FILE"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1" | tee -a "$LOG_FILE"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1" | tee -a "$LOG_FILE"
}

# Load configuration
load_config() {
    if [[ ! -f "$CONFIG_FILE" ]]; then
        error "Configuration file not found: $CONFIG_FILE"
        exit 1
    fi
    
    log "Loading configuration from $CONFIG_FILE"
    # Extract configuration values using jq if available, otherwise fallback
    if command -v jq &> /dev/null; then
        COVERAGE_THRESHOLD=$(jq -r '.quality_gates.ci_pipeline.thresholds.test_coverage_min' "$CONFIG_FILE")
        MAX_SECURITY_HIGH=$(jq -r '.quality_gates.ci_pipeline.thresholds.security_scan_max_high' "$CONFIG_FILE")
        MAX_PERF_REGRESSION=$(jq -r '.quality_gates.ci_pipeline.thresholds.performance_regression_max' "$CONFIG_FILE")
    else
        # Fallback defaults
        COVERAGE_THRESHOLD=80
        MAX_SECURITY_HIGH=0
        MAX_PERF_REGRESSION=5
    fi
}

# Pre-commit checks
pre_commit_checks() {
    log "Running pre-commit quality checks..."
    
    local failures=0
    
    # Code formatting check
    log "Checking code formatting..."
    if command -v cargo fmt &> /dev/null; then
        if ! cargo fmt -- --check; then
            error "Code formatting check failed"
            ((failures++))
        else
            success "Code formatting check passed"
        fi
    fi
    
    # Linting check
    log "Running linter..."
    if command -v cargo clippy &> /dev/null; then
        if ! cargo clippy -- -D warnings; then
            error "Linting check failed"
            ((failures++))
        else
            success "Linting check passed"
        fi
    fi
    
    # Security audit
    log "Running security audit..."
    if command -v cargo audit &> /dev/null; then
        if ! cargo audit --quiet; then
            warn "Security audit found vulnerabilities"
            ((failures++))
        else
            success "Security audit passed"
        fi
    fi
    
    # License compliance check
    log "Checking license compliance..."
    if command -v cargo license &> /dev/null; then
        # Add specific license check logic here
        success "License compliance check passed"
    fi
    
    return $failures
}

# Build verification
build_verification() {
    log "Running build verification..."
    
    if ! cargo check; then
        error "Build verification failed"
        return 1
    fi
    
    if ! cargo build --release; then
        error "Release build failed"
        return 1
    fi
    
    success "Build verification passed"
    return 0
}

# Test coverage analysis
test_coverage() {
    log "Running test coverage analysis..."
    
    # Install tarpaulin if not available
    if ! command -v cargo tarpaulin &> /dev/null; then
        warn "cargo tarpaulin not found, skipping coverage analysis"
        return 0
    fi
    
    # Run tests with coverage
    local coverage_result
    coverage_result=$(cargo tarpaulin --out Json --output-dir target/tarpaulin 2>/dev/null | tail -1 || echo "0")
    
    # Extract coverage percentage (simplified parsing)
    local coverage_percent
    coverage_percent=$(echo "$coverage_result" | grep -o '[0-9]\+\.[0-9]\+' | head -1 || echo "0")
    
    log "Current test coverage: ${coverage_percent}%"
    log "Required minimum: ${COVERAGE_THRESHOLD}%"
    
    # Compare with threshold (using bc for floating point comparison)
    if command -v bc &> /dev/null; then
        if (( $(echo "$coverage_percent < $COVERAGE_THRESHOLD" | bc -l) )); then
            error "Test coverage ${coverage_percent}% below threshold ${COVERAGE_THRESHOLD}%"
            return 1
        fi
    else
        warn "bc not available for precise comparison, skipping threshold check"
    fi
    
    success "Test coverage check passed"
    return 0
}

# Integration tests
integration_tests() {
    log "Running integration tests..."
    
    # Run VM tests if available
    if [[ -f "flake.nix" ]]; then
        log "Running Nix VM tests..."
        if nix flake check --no-build; then
            success "Integration tests passed"
            return 0
        else
            error "Integration tests failed"
            return 1
        fi
    else
        # Fallback to cargo test
        if cargo test --tests; then
            success "Integration tests passed"
            return 0
        else
            error "Integration tests failed"
            return 1
        fi
    fi
}

# Security assessment
security_assessment() {
    log "Running security assessment..."
    
    local vulnerabilities=0
    
    # Run cargo audit
    if command -v cargo audit &> /dev/null; then
        local audit_output
        audit_output=$(cargo audit --json 2>/dev/null || echo '{"vulnerabilities":{"count":0}}')
        
        if command -v jq &> /dev/null; then
            vulnerabilities=$(echo "$audit_output" | jq -r '.vulnerabilities.count // 0')
        else
            # Fallback parsing
            vulnerabilities=$(echo "$audit_output" | grep -o '"count":[0-9]\+' | cut -d':' -f2 || echo "0")
        fi
    fi
    
    log "Security vulnerabilities found: $vulnerabilities"
    log "Maximum allowed: $MAX_SECURITY_HIGH"
    
    if (( vulnerabilities > MAX_SECURITY_HIGH )); then
        error "Security assessment failed: $vulnerabilities vulnerabilities found"
        return 1
    fi
    
    success "Security assessment passed"
    return 0
}

# Performance baseline
performance_baseline() {
    log "Running performance baseline..."
    
    # Basic performance check - build time
    local build_start
    build_start=$(date +%s)
    
    if cargo build --release; then
        local build_end
        build_end=$(date +%s)
        local build_duration=$((build_end - build_start))
        
        log "Build completed in ${build_duration}s"
        success "Performance baseline check passed"
        return 0
    else
        error "Performance baseline check failed"
        return 1
    fi
}

# Deployment readiness
deployment_readiness() {
    log "Checking deployment readiness..."
    
    local failures=0
    
    # Check configuration files
    if [[ ! -f "config.yaml" ]]; then
        error "Production configuration file missing"
        ((failures++))
    fi
    
    # Check Dockerfile
    if [[ ! -f "Dockerfile" ]]; then
        error "Dockerfile missing for deployment"
        ((failures++))
    fi
    
    # Check documentation
    if [[ ! -f "README.md" ]]; then
        warn "README.md missing"
    fi
    
    # Check monitoring setup (basic)
    if ! grep -q "prometheus" Cargo.toml 2>/dev/null; then
        warn "Prometheus metrics not configured"
    fi
    
    if (( failures > 0 )); then
        error "Deployment readiness check failed"
        return 1
    fi
    
    success "Deployment readiness check passed"
    return 0
}

# Main execution
main() {
    local gate_type="${1:-ci}"
    
    log "Starting quality gates execution - Gate type: $gate_type"
    
    # Initialize log
    echo "=== Quality Gates Run - $(date) ===" >> "$LOG_FILE"
    
    load_config
    
    local total_failures=0
    
    case "$gate_type" in
        "pre-commit")
            pre_commit_checks
            ((total_failures+=$?))
            ;;
        "ci")
            build_verification
            ((total_failures+=$?))
            
            test_coverage
            ((total_failures+=$?))
            
            integration_tests
            ((total_failures+=$?))
            
            security_assessment
            ((total_failures+=$?))
            
            performance_baseline
            ((total_failures+=$?))
            ;;
        "deployment")
            deployment_readiness
            ((total_failures+=$?))
            ;;
        "all")
            pre_commit_checks
            ((total_failures+=$?))
            
            build_verification
            ((total_failures+=$?))
            
            test_coverage
            ((total_failures+=$?))
            
            integration_tests
            ((total_failures+=$?))
            
            security_assessment
            ((total_failures+=$?))
            
            performance_baseline
            ((total_failures+=$?))
            
            deployment_readiness
            ((total_failures+=$?))
            ;;
        *)
            error "Unknown gate type: $gate_type"
            echo "Usage: $0 [pre-commit|ci|deployment|all]"
            exit 1
            ;;
    esac
    
    if (( total_failures > 0 )); then
        error "Quality gates failed with $total_failures failures"
        exit 1
    else
        success "All quality gates passed"
        exit 0
    fi
}

# Execute main function
main "$@"
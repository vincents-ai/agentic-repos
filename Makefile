# Makefile for repos-mcp (Rust Git Server)

.PHONY: help test test-unit test-security test-coverage build run clean lint fmt check deps dev nix-build nix-test setup

# Variables
BINARY_NAME=repos-mcp
BUILD_DIR=target
CONFIG_FILE=config-example.yaml

# Help
help: ## Display this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

# Install dependencies
deps: ## Install Rust dependencies
	cargo fetch

# Setup configuration files
setup: ## Set up configuration files for first time use
	@if [ ! -f config.yaml ]; then \
		echo "📝 Creating config.yaml from example..."; \
		cp config-example.yaml config.yaml; \
		echo "✅ Created config.yaml - please review and modify as needed"; \
	else \
		echo "⚠️  config.yaml already exists, skipping"; \
	fi
	@if [ ! -f authorized_keys ]; then \
		echo "📝 Creating authorized_keys from example..."; \
		cp authorized_keys.example authorized_keys; \
		echo "✅ Created authorized_keys - please add your SSH public keys"; \
	else \
		echo "⚠️  authorized_keys already exists, skipping"; \
	fi
	@mkdir -p repositories
	@echo "✅ Created repositories directory"
	@echo ""
	@echo "🚀 Setup complete! Next steps:"
	@echo "   1. Edit config.yaml to configure your server settings"
	@echo "   2. Add SSH public keys to authorized_keys file"
	@echo "   3. Run 'make run' to start the server"

# Unit tests
test-unit: ## Run unit tests only (exclude security tests)
	cargo test --lib -- --skip security_tests

# Security tests  
test-security: ## Run security test suite
	cargo test security_tests

# All tests
test: ## Run all tests
	cargo test

# Tests with coverage (requires cargo-tarpaulin)
test-coverage: ## Run tests with coverage
	@if command -v cargo-tarpaulin >/dev/null 2>&1; then \
		cargo tarpaulin --out Html --output-dir coverage; \
		echo "Coverage report generated: coverage/tarpaulin-report.html"; \
	else \
		echo "⚠️  cargo-tarpaulin not installed. Installation:"; \
		echo "cargo install cargo-tarpaulin"; \
	fi

# Code formatting
fmt: ## Format code
	cargo fmt

# Clippy linting
lint: ## Lint code with clippy
	cargo clippy -- -D warnings

# Check code (fmt + clippy + test)
check: fmt lint test ## Check code formatting, linting, and run tests

# Build debug version
build: ## Build the server (debug mode)
	cargo build

# Build release version
build-release: ## Build for release
	cargo build --release

# Run the server (debug mode)
run: build ## Build and run the server
	cargo run -- --config $(CONFIG_FILE)

# Run in release mode
run-release: build-release ## Build and run the server (release mode)
	./target/release/$(BINARY_NAME) --config $(CONFIG_FILE)

# Development mode with cargo-watch (auto-reload on file changes)
dev: ## Run in development mode with auto-reload (requires cargo-watch)
	@if command -v cargo-watch >/dev/null 2>&1; then \
		cargo watch -x 'run -- --config $(CONFIG_FILE)'; \
	else \
		echo "⚠️  cargo-watch not installed. Installation:"; \
		echo "cargo install cargo-watch"; \
		echo "Or use: make run"; \
	fi

# Nix build (using flake.nix)
nix-build: ## Build using Nix
	nix build

# Nix test
nix-test: ## Test using Nix
	nix develop -c -- cargo test

# Nix shell
nix-shell: ## Enter Nix development shell
	nix develop

# Cleanup
clean: ## Clean generated files
	cargo clean
	rm -rf coverage/

# Check for security advisories
audit: ## Check for security vulnerabilities in dependencies
	@if command -v cargo-audit >/dev/null 2>&1; then \
		cargo audit; \
	else \
		echo "⚠️  cargo-audit not installed. Installation:"; \
		echo "cargo install cargo-audit"; \
	fi

# Performance benchmarks (if any exist)
bench: ## Run benchmarks
	cargo bench

# Generate documentation
docs: ## Generate documentation
	cargo doc --open

# Install binary locally
install: ## Install binary to cargo's bin directory
	cargo install --path .

# Update dependencies
update: ## Update dependencies
	cargo update

# Check that everything is ready for commit
pre-commit: check audit ## Run all checks before committing

# Show project status
status: ## Show project status
	@echo "=== Project Status ==="
	@echo "Build status:"
	@cargo build --quiet && echo "✅ Debug build: OK" || echo "❌ Debug build: FAILED"
	@cargo build --release --quiet && echo "✅ Release build: OK" || echo "❌ Release build: FAILED"
	@echo ""
	@echo "Test status:"
	@cargo test --quiet && echo "✅ Tests: PASSING" || echo "❌ Tests: FAILING"
	@echo ""
	@echo "Security status:"
	@cargo test security_tests --quiet && echo "✅ Security tests: PASSING" || echo "❌ Security tests: FAILING"
	@echo ""
	@echo "Dependencies:"
	@if command -v cargo-audit >/dev/null 2>&1; then \
		cargo audit --quiet && echo "✅ Security audit: CLEAN" || echo "⚠️  Security audit: ISSUES FOUND"; \
	else \
		echo "⚠️  Security audit: cargo-audit not installed"; \
	fi
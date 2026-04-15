#!/usr/bin/env bash
# Package agentic-repos plugin for deployment with garage-s3

set -e

VERSION=${VERSION:-"0.1.0"}
PACKAGE_NAME="agentic-repos-plugin-${VERSION}"
BUILD_DIR="./dist/${PACKAGE_NAME}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

echo "========================================"
echo "agentic-repos Plugin Packaging"
echo "Version: ${VERSION}"
echo "Date: $(date)"
echo "========================================"

# Clean previous builds
echo ""
echo "Cleaning previous builds..."
rm -rf ./dist/${PACKAGE_NAME}
mkdir -p ${BUILD_DIR}

# Build the plugin
echo ""
echo "Building plugin..."
if ! nix build; then
    echo "ERROR: Build failed"
    exit 1
fi

if [ ! -f "./target/release/libagentic_repos.so" ]; then
    echo "ERROR: Plugin binary not found after build"
    exit 1
fi

echo "✓ Plugin built successfully"

# Create package structure
echo ""
echo "Creating package structure..."
mkdir -p ${BUILD_DIR}/{plugins,config,docs,scripts}

# Copy plugin binary
echo "Copying plugin binary..."
cp ./target/release/libagentic_repos.so ${BUILD_DIR}/plugins/
chmod +x ${BUILD_DIR}/plugins/libagentic_repos.so

# Copy garage-s3 plugin if available
GARAGE_S3_PLUGIN="$HOME/code/vincents-ai/skynet/target/release/libgarage_s3.so"
if [ -f "$GARAGE_S3_PLUGIN" ]; then
    echo "Copying garage-s3 plugin..."
    cp "$GARAGE_S3_PLUGIN" ${BUILD_DIR}/plugins/
    chmod +x ${BUILD_DIR}/plugins/libgarage_s3.so
else
    echo "WARNING: garage-s3 plugin not found at $GARAGE_S3_PLUGIN"
    echo "You'll need to build it separately from ~/code/vincents-ai/skynet/plugins/garage-s3/"
fi

# Copy documentation
echo "Copying documentation..."
cp -r docs/*.md ${BUILD_DIR}/docs/
cp AGENTS.md ${BUILD_DIR}/docs/
cp VALIDATION_COMPLETE.md ${BUILD_DIR}/docs/
cp README.md ${BUILD_DIR}/ 2>/dev/null || echo "# agentic-repos Plugin" > ${BUILD_DIR}/README.md

# Copy test scripts
echo "Copying test scripts..."
cp test-simple.sh ${BUILD_DIR}/scripts/
cp test-integration.sh ${BUILD_DIR}/scripts/
chmod +x ${BUILD_DIR}/scripts/*.sh

# Create deployment configuration
echo "Creating deployment configurations..."
cat > ${BUILD_DIR}/config/skylet-config.yaml << 'EOF'
# Skylet Configuration for agentic-repos with garage-s3
# Copy this file to your Skylet config directory

server:
  host: "0.0.0.0"
  port: 9000

logging:
  level: "info"
  format: "json"

plugins:
  # Storage backend - Garage S3 (must load first)
  - name: garage-s3
    path: ./plugins/libgarage_s3.so
    enabled: true
    priority: 10  # Load before agentic-repos
    config:
      auto_start_garage: true
      data_dir: "./data/garage"
      s3_port: 3900
      rpc_port: 3901
      admin_port: 3903

  # Git operations - agentic-repos (depends on garage-s3)
  - name: git-operations
    path: ./plugins/libagentic_repos.so
    enabled: true
    priority: 5  # Load after garage-s3
    config:
      storage.default_backend: "local"
      storage.local_path: "./data/repos"
      ssh.enabled: true
      ssh.port: 2222
      ssh.host_key_path: "./data/ssh/host_key"
      ssh.authorized_keys_path: "./data/ssh/authorized_keys"
      lfs.enabled: true
      lfs.base_url: "http://localhost:3900"
      lfs.max_object_size: 5368709120
      log.level: "info"

# Plugin discovery
discovery:
  enabled: true
  scan_paths:
    - ./plugins

# Service registry
service_registry:
  enabled: true
  allow_plugin_services: true

# Health checks
health:
  enabled: true
  interval_secs: 30
  timeout_secs: 5

# Metrics
metrics:
  enabled: true
  port: 9090
  path: "/metrics"
EOF

# Create environment configuration
cat > ${BUILD_DIR}/config/agentic-repos.env << 'EOF'
# agentic-repos Environment Configuration
# Source this file before starting Skylet

# Storage Configuration
export REPOS_STORAGE_PATH="./data/repos"
export REPOS_STORAGE_BACKEND="local"  # or "s3" for garage-s3

# SSH Configuration (REQUIRED for git operations)
export REPOS_SSH_ENABLED=true
export REPOS_SSH_PORT=2222
export REPOS_SSH_HOST_KEY_PATH="./data/ssh/host_key"
export REPOS_SSH_AUTHORIZED_KEYS_PATH="./data/ssh/authorized_keys"

# S3 Configuration (when using garage-s3)
export GARAGE_ENDPOINT="http://localhost:3900"
export GARAGE_REGION="garage"
export GARAGE_ACCESS_KEY_ID="garage"
export GARAGE_SECRET_ACCESS_KEY="garage"
export GARAGE_BUCKET="repos"

# LFS Configuration
export REPOS_LFS_ENABLED=true
export REPOS_LFS_BASE_URL="http://localhost:2222"
export REPOS_LFS_MAX_OBJECT_SIZE=5368709120

# Logging
export REPOS_LOG_LEVEL="info"
EOF

# Create deployment guide
echo "Creating deployment guide..."
cat > ${BUILD_DIR}/DEPLOYMENT.md << 'EOF'
# agentic-repos Plugin Deployment Guide

## Package Contents

```
agentic-repos-plugin-0.1.0/
├── plugins/
│   ├── libagentic_repos.so         # agentic-repos plugin
│   └── libgarage_s3.so         # garage-s3 storage backend
├── config/
│   ├── skylet-config.yaml      # Skylet configuration
│   └── agentic-repos.env           # Environment variables
├── docs/                        # Documentation
│   ├── AGENTS.md
│   ├── SKYLET_PLUGIN_VALIDATION.md
│   ├── GIT_OPERATIONS_VALIDATION.md
│   ├── INTEGRATION_REPORT.md
│   └── VALIDATION_COMPLETE.md
├── scripts/
│   ├── test-simple.sh          # Basic git operations test
│   └── test-integration.sh     # Full integration test
└── README.md
```

## Quick Deployment

### 1. Extract Package

```bash
tar -xzf agentic-repos-plugin-0.1.0.tar.gz
cd agentic-repos-plugin-0.1.0
```

### 2. Deploy to Skylet

```bash
# Copy plugins to Skylet
cp plugins/*.so /path/to/skylet/plugins/

# Copy configuration
cp config/skylet-config.yaml /path/to/skylet/config.yaml

# Set environment variables
source config/agentic-repos.env
```

### 3. Start Skylet

```bash
cd /path/to/skylet
./skylet server
```

### 4. Verify Deployment

```bash
# Check plugin loaded
curl http://localhost:9000/health

# Run integration test
./scripts/test-integration.sh
```

## Detailed Deployment Instructions

See docs/INTEGRATION_REPORT.md for complete deployment guide.
EOF

# Create README
cat > ${BUILD_DIR}/README.md << EOF
# agentic-repos Plugin Package v${VERSION}

Multi-tenant Git server with LFS support for Skylet.

## What's Included

- **agentic-repos plugin** - Git operations and repository management
- **garage-s3 plugin** - S3-compatible storage backend
- **Configuration files** - Ready-to-use Skylet configuration
- **Documentation** - Complete validation and integration guides
- **Test scripts** - Validate your deployment

## Quick Start

1. Extract this package
2. Copy plugins to Skylet plugins directory
3. Copy config/skylet-config.yaml to Skylet config
4. Start Skylet: \`./skylet server\`
5. Test: \`./scripts/test-integration.sh\`

## Documentation

- DEPLOYMENT.md - Deployment instructions
- docs/AGENTS.md - AI agent guide
- docs/INTEGRATION_REPORT.md - Integration test results
- docs/SKYLET_PLUGIN_VALIDATION.md - Plugin validation guide

## Support

- GitHub: https://github.com/vincents-ai/agentic-repos
- Skylet: https://github.com/vincents-ai/skylet

## License

AGPL-3.0

Package built: ${TIMESTAMP}
EOF

# Create installation script
cat > ${BUILD_DIR}/install.sh << 'EOF'
#!/usr/bin/env bash
# Install agentic-repos plugin to Skylet

set -e

SKYLET_DIR="${SKYLET_DIR:-$HOME/code/skylet}"
PLUGIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "========================================"
echo "agentic-repos Plugin Installer"
echo "========================================"

echo ""
echo "Skylet directory: $SKYLET_DIR"
echo "Plugin package: $PLUGIN_DIR"

# Check Skylet exists
if [ ! -d "$SKYLET_DIR" ]; then
    echo "ERROR: Skylet directory not found: $SKYLET_DIR"
    echo "Set SKYLET_DIR environment variable to your Skylet installation"
    exit 1
fi

# Create plugins directory if needed
mkdir -p "$SKYLET_DIR/plugins"

# Install plugins
echo ""
echo "Installing plugins..."
cp -v "$PLUGIN_DIR/plugins/"*.so "$SKYLET_DIR/plugins/"

# Install configuration
echo ""
echo "Installing configuration..."
if [ -f "$SKYLET_DIR/config.yaml" ]; then
    echo "Backing up existing config.yaml..."
    cp "$SKYLET_DIR/config.yaml" "$SKYLET_DIR/config.yaml.backup"
fi
cp -v "$PLUGIN_DIR/config/skylet-config.yaml" "$SKYLET_DIR/config.yaml"

# Install environment file
if [ ! -f "$SKYLET_DIR/agentic-repos.env" ]; then
    cp -v "$PLUGIN_DIR/config/agentic-repos.env" "$SKYLET_DIR/"
fi

echo ""
echo "========================================"
echo "Installation Complete!"
echo "========================================"
echo ""
echo "Next steps:"
echo "1. Review and update $SKYLET_DIR/config.yaml"
echo "2. Source environment: source $SKYLET_DIR/agentic-repos.env"
echo "3. Start Skylet: cd $SKYLET_DIR && ./target/release/skylet server"
echo "4. Test: $PLUGIN_DIR/scripts/test-integration.sh"
echo ""
EOF

chmod +x ${BUILD_DIR}/install.sh

# Create tarball
echo ""
echo "Creating distribution tarball..."
cd ./dist
tar -czf ${PACKAGE_NAME}.tar.gz ${PACKAGE_NAME}

# Calculate checksum
sha256sum ${PACKAGE_NAME}.tar.gz > ${PACKAGE_NAME}.tar.gz.sha256

echo ""
echo "========================================"
echo "Package Created Successfully!"
echo "========================================"
echo ""
echo "Package: ./dist/${PACKAGE_NAME}.tar.gz"
echo "Checksum: ./dist/${PACKAGE_NAME}.tar.gz.sha256"
echo "Size: $(du -h ${PACKAGE_NAME}.tar.gz | cut -f1)"
echo ""
echo "Contents:"
tar -tzf ${PACKAGE_NAME}.tar.gz | head -20
echo "..."
echo ""
echo "Total files: $(tar -tzf ${PACKAGE_NAME}.tar.gz | wc -l)"
echo ""
echo "To deploy:"
echo "  1. Copy ./dist/${PACKAGE_NAME}.tar.gz to target system"
echo "  2. Extract: tar -xzf ${PACKAGE_NAME}.tar.gz"
echo "  3. Install: cd ${PACKAGE_NAME} && ./install.sh"
echo ""

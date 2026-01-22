# repos-mcp

A lightweight, multi-tenant Git server with SSH protocol support, written in Rust.

# repos-mcp

A lightweight, multi-tenant Git server with SSH protocol support, written in Rust.

## 🔒 Security Status

⚠️ **SECURITY WARNING: DO NOT DEPLOY TO PRODUCTION WITHOUT SECURITY REVIEW**

This project implements security features but requires thorough security review and hardening before production use.

**Implemented Security Features**:
- ✅ SSH public key authentication
- ✅ Command injection prevention with input validation
- ✅ Secure Ed25519 host key management
- ✅ Rate limiting and connection limits
- ✅ Comprehensive security test suite (10 test functions)

**Security Considerations**:
- Requires proper SSH key management and authorized_keys configuration
- JWT secrets must be securely generated and managed (for LFS)
- HTTPS strongly recommended for HTTP API endpoints
- Regular security audits recommended
- See [SECURITY.md](SECURITY.md) for security policy and vulnerability reporting

**Before Production Deployment**:
1. Complete security audit and penetration testing
2. Review and harden configuration
3. Implement proper secrets management
4. Set up monitoring and logging
5. Follow deployment best practices in documentation

## License

**AGPLv3 License**: This project is licensed under the GNU Affero General Public License v3.0. This means:

- **Free for use**: You can use, modify, and distribute this software freely
- **Copyleft**: If you distribute this software as a service, you must share your modifications under the same license
- **Commercial features**: Enterprise features (authentication, compliance, scaling, monitoring) are available under separate commercial licensing

See [LICENSE](LICENSE) file for details.

## Commercial Features

The following enterprise features are available under commercial licensing. See [COMMERCIAL_ROADMAP.md](COMMERCIAL_ROADMAP.md) for detailed feature specifications and implementation timeline.

**🔐 Enterprise Git Management:**
- Advanced Authentication (LDAP/SAML/OAuth)
- Audit Logging and Access Tracking
- Repository Templates with Policies
- Custom Webhook Integrations

**⚡ Performance & Scale:**
- Horizontal Scaling and Load Balancing
- Redis-based Caching Layer
- Multi-Region Replication
- Performance Monitoring (Prometheus/Grafana)

**🛡️ Security & Compliance:**
- Advanced Security Scanning
- SOC2, GDPR, HIPAA Compliance Reports
- Role-Based Access Control (RBAC)
- Automated Backup & Disaster Recovery

**🚀 Developer Experience:**
- Web UI for Repository Management
- CI/CD Integration (GitHub Actions, GitLab CI, Jenkins)
- GraphQL API with Advanced Querying
- Repository Analytics and Insights Dashboard

Contact shift@someone.section.me for commercial licensing and enterprise support.

## Core Features (AGPL)

- **SSH Git Server**: Custom SSH implementation for Git operations
  - Clone, push, pull operations via SSH
  - Public key authentication
  - Git protocol over SSH (git-upload-pack, git-receive-pack)

- **Storage Backends**:
  - **Local**: File system storage for repositories
  - **S3**: Amazon S3 compatible storage (tested and working)

- **Configuration**:
  - YAML configuration files
  - Command-line flags
  - Environment variable support

- **Authentication**:
  - SSH public key authentication
  - Extensible authentication framework

- **Logging**: Structured logging with tracing

- **Git LFS (Large File Storage)**: Built-in support for large files
  - Git LFS Batch API (upload/download)
  - JWT-based authentication for HTTP transfers
  - Multi-tenant object storage with path sharding
  - SHA256 content verification
  - Configurable object size limits

## Git LFS Support

repos-mcp includes full Git LFS support for managing large files efficiently.

### Features

- **Batch API**: RFC-compliant Git LFS Batch API implementation
- **Multi-Tenant**: Complete isolation between tenants and repositories  
- **Authentication**: JWT tokens generated via SSH for secure HTTP transfers
- **Storage**: Flexible storage with local filesystem (S3 support planned)
- **Verification**: Automatic SHA256 integrity checking on uploads

### Quick Setup

1. **Enable LFS in configuration** (`config.yaml`):

```yaml
features:
  git_lfs: true

lfs:
  enabled: true
  jwt_secret: "your-strong-secret-here"  # Generate with: openssl rand -base64 32
  base_url: "http://localhost:8090"
  max_object_size: 5368709120  # 5 GB
  token_expiration_secs: 3600
```

2. **Initialize Git LFS in your repository**:

```bash
git lfs install
git lfs track "*.psd"
git lfs track "*.mp4"
git add .gitattributes
```

3. **Use as normal**:

```bash
# Add large files
git add large-file.psd
git commit -m "Add design file"
git push origin main
```

### Testing LFS HTTP API

```bash
# Test batch endpoint
curl -X POST http://localhost:8090/lfs/tenant/repo/info/lfs/objects/batch \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -d '{
    "operation": "upload",
    "objects": [{"oid": "abc123...", "size": 1024}]
  }'

# Upload object
echo "test data" > /tmp/test.bin
OID=$(sha256sum /tmp/test.bin | awk '{print $1}')
curl -X PUT "http://localhost:8090/lfs/tenant/repo/lfs/objects/$OID" \
  --data-binary @/tmp/test.bin
```

### Documentation

- [LFS Configuration Reference](docs/LFS_CONFIGURATION.md) - Complete configuration guide
- [Git LFS Specification](https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md) - Official API spec

## Quick Start

### Configuration

Copy the example configuration:
```bash
cp config-example.yaml config.yaml
```

Edit `config.yaml` to configure your storage backend and server settings.

### Running the Server

**Using Podman (Production Recommended)**:
```bash
# Clone and setup
git clone https://vincents.ai/repos/repos-mcp
cd repos-mcp

# Configure the server
cp config/config.yaml config/config-prod.yaml
# Edit config-prod.yaml with your settings

# Build and start with podman-compose
podman-compose -f compose.yml up -d

# View logs
podman-compose -f compose.yml logs -f

# Stop the server
podman-compose -f compose.yml down
```

**Using Make (Development)**:
```bash
# Build and run with configuration
make run

# Build release version and run
make run-release

# Run in development mode (auto-reload with cargo-watch)
make dev

# Check project status
make status

# Run tests
make test

# Run security tests specifically  
make test-security
```

**Using Cargo directly**:
```bash
# Build the application
cargo build --release

# Run with default configuration
./target/release/repos-mcp --config config.yaml
```

**Using Nix**:
```bash
# Enter development shell
nix develop

# Build with Nix
nix build

# Test with Nix
nix develop -c -- cargo test
```

## Container Deployment

### Traditional Container Build (Docker/Podman)

Use the included Dockerfile for traditional container builds:

```bash
# Build with Podman
podman build -t repos-mcp .

# Run the container
podman run -d \
  --name repos-mcp \
  -p 2222:2222 \
  -p 8090:8090 \
  -v ./data:/app/data \
  -v ./config/config.yaml:/app/config/config.yaml:ro \
  -v ./ssh_keys:/app/.ssh \
  -e RUST_LOG=info \
  --restart unless-stopped \
  repos-mcp
```

### NixOS-based Container (Future)

The flake.nix includes configuration for NixOS-based containers:

```bash
# Build with Nix (requires network access)
nix build .#containerImage

# Load and run with Podman
podman load < result
podman run repos-mcp:latest
```

Note: The Nix build requires network access and may need additional configuration for your environment.

### Production Setup with Podman

1. **Create production directories**:
```bash
mkdir -p data ssh_keys config
```

2. **Configure the server**:
```bash
# Copy and edit the production config
cp config/config.yaml config/config-prod.yaml
# Edit config-prod.yaml with your production settings
```

3. **Setup SSH authentication**:
```bash
# Add authorized SSH keys for Git access
echo "ssh-rsa AAAA... user@host" >> ssh_keys/authorized_keys
chmod 600 ssh_keys/authorized_keys
```

4. **Deploy with Podman Compose**:
```bash
# Build and start the services
podman-compose up -d

# Check status
podman-compose ps

# View logs
podman-compose logs -f repos-mcp
```

### Alternative: Direct Podman Commands

```bash
# Build the image
podman build -t repos-mcp .

# Run the container
podman run -d \
  --name repos-mcp \
  -p 2222:2222 \
  -p 8090:8090 \
  -v ./data:/app/data \
  -v ./config/config.yaml:/app/config/config.yaml:ro \
  -v ./ssh_keys:/app/.ssh \
  -e RUST_LOG=info \
  --restart unless-stopped \
  repos-mcp

# Check container health
podman healthcheck run repos-mcp
```

### Container Configuration

The container setup includes:
- **Multi-stage build** for smaller production images
- **Health checks** for service monitoring
- **Volume mounts** for data persistence
- **Rootless operation** (Podman default)
- **Automatic restarts** unless stopped

**Environment Variables**:
- `RUST_LOG`: Control logging level (debug, info, warn, error)

**Ports**:
- `2222`: SSH Git access
- `8090`: HTTP REST API

**Volumes**:
- `./data:/app/data`: Repository storage
- `./config:/app/config`: Configuration files
- `./ssh_keys:/app/.ssh`: SSH keys and authorized_keys

### Git Operations

**SSH Access:**
```bash
# Clone via SSH
git clone ssh://user@localhost:2222/tenant/repo.git

# Push to SSH
git push origin main
```

## Configuration Options

### Server Configuration
- `server.ssh.port`: SSH server port (default: 2222)
- `server.ssh.hostkey`: Path to SSH host key file

### Storage Configuration
- `storage.type`: Storage backend ("local" or "s3")
- `storage.local.path`: Local storage directory
- `storage.s3.*`: S3 configuration options

## Development

### Prerequisites
- Rust 1.70+ with Cargo
- NixOS/Nix (for development shell)
- Optional: `cargo-watch`, `cargo-tarpaulin`, `cargo-audit`

### Building
```bash
# Debug build
make build

# Release build  
make build-release

# Using Nix
make nix-build
```

### Testing
```bash
# Run all tests
make test

# Run only unit tests (excluding security tests)
make test-unit

# Run security test suite specifically
make test-security

# Run with coverage (requires cargo-tarpaulin)
make test-coverage
```

### Code Quality
```bash
# Check everything (format, lint, test)
make check

# Format code
make fmt

# Lint with clippy
make lint

# Security audit (requires cargo-audit)
make audit

# Pre-commit checks
make pre-commit
```

### Development Workflow
```bash
# Auto-reload development server (requires cargo-watch)
make dev

# Check project status
make status

# Generate documentation
make docs
```

## Architecture

- **SSH Server**: Uses `russh` for SSH protocol handling
- **Git Protocol**: Uses `gix-protocol` and `gix-transport` for Git Smart Protocol
- **Storage**: Pluggable storage backend system with local and S3 support
- **Concurrency**: Async/await with tokio runtime

## Known Issues 🐛

### SSH Protocol
- **Client Warning**: Git clients may show "remote end hung up unexpectedly" message
  - **Impact**: Cosmetic only - all operations complete successfully
  - **Status**: Under investigation - server-side operations work perfectly
  - **Workaround**: Message can be safely ignored

### Performance
- **S3 Storage**: Slower than local storage due to network latency
  - **Expected**: Normal behavior for remote storage
  - **Optimization**: Consider using S3 transfer acceleration

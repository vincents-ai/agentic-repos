# repos-mcp

A lightweight, multi-tenant Git server with SSH protocol support, written in Rust.

## License

**AGPLv3 License**: This project is licensed under the GNU Affero General Public License v3.0. This means:

- **Free for use**: You can use, modify, and distribute this software freely
- **Copyleft**: If you distribute this software as a service, you must share your modifications under the same license
- **Commercial features**: Commercial tiers with additional features (rate limiting, retention policies, branch limits) are available under separate commercial licensing

See [LICENSE](LICENSE) file for details.

## Commercial Features

The following features are available under commercial licensing:

- Rate limiting with proxy awareness (Envoy Gateway integration)
- Configurable retention policies per repository
- Branch limits and quota management
- Multi-tier pricing (free, pro, enterprise)
- Priority support and SLA guarantees

Contact [contact email] for commercial licensing.

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

## Quick Start

### Configuration

Copy the example configuration:
```bash
cp config-example.yaml config.yaml
```

Edit `config.yaml` to configure your storage backend and server settings.

### Running the Server

```bash
# Build the application
make build

# Run with default configuration
./main

# Or run with custom config
./main --config ./config.yaml
```

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

### Building
```bash
make build
```

### Testing
```bash
make test
```

### Running Tests with Coverage
```bash
make test-coverage
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

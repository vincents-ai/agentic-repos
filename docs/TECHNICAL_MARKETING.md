# agentic-repos -- Technical Overview

**Multi-Tenant Git Server with Model Context Protocol Integration**

---

## What It Is

agentic-repos is a Rust-built Git server that does two things no other Git server does: it runs as a standalone multi-tenant Git server *and* as a first-class Skylet v2 plugin via C FFI, and it exposes native MCP (Model Context Protocol) tool interfaces so AI agents can manage Git repositories directly.

Single binary. Dual mode. ~34,000 lines of Rust across 137 source files.

---

## Architecture at a Glance

```
                    +-----------------------+
                    |   AI / LLM Agent      |
                    |   (MCP Client)        |
                    +-----------+-----------+
                                |
                        MCP Protocol (JSON-RPC)
                                |
                    +-----------v-----------+
                    |    agentic-repos          |
                    |                       |
   Mode A: Standalone    |    Mode B: Skylet v2 Plugin (.so)
   (binary)              |    (cdylib via FFI)
                         |                       |
              +----------+----------+    +-------v--------+
              |  SSH Server :2222    |    | Skylet Runtime |
              |  (russh)             |    | Service Registry|
              +----------+----------+    +----------------+
                         |
              +----------+----------+-----------+
              |          |          |           |
         +----v---+ +---v----+ +--v----+ +---v-------+
         | Git    | | LFS    | | REST  | | Web UI    |
         | Proto  | | Server | | API   | | (Tera)    |
         +----+---+ +---+----+ +--+----+ +-----------+
              |          |          |
         +----v-----------v----------v----+
         |   Storage Backend Layer         |
         |   (Pluggable: Local / S3)       |
         +----+---------------------------+
              |
     +--------v--------+
     |  S3-Compatible   |
     |  (AWS / MinIO /  |
     |   Garage /       |
     |   SeaweedFS)     |
     +------------------+
```

---

## Key Capabilities

### Git Server

| Capability | Details |
|---|---|
| **SSH Access** | Custom SSH server on port 2222 (russh 0.58). Ed25519 host keys. Public-key auth only. |
| **Git Protocol** | `git-upload-pack` (clone/fetch), `git-receive-pack` (push), `git-lfs-authenticate`. Full protocol implementation via gix-protocol + libgit2. |
| **HTTP API** | REST API on port 8090 (actix-web 4.4). Repository CRUD, branch listing, paginated results. |
| **Web UI** | Dashboard, repository listing, and repository detail pages via Tera templates. |
| **Security** | Path traversal protection, command allowlist, 4096-byte command limit, control character rejection, 3-second auth rejection timing to mitigate timing attacks. |
| **Rate Limiting** | Token bucket algorithm. Three tiers: Free (60 req/min), Pro (300 req/min), Enterprise (unlimited). Per-IP connection tracking. Proxy-aware IP extraction. |

### Git LFS (14 source files)

Full-featured LFS server with enterprise-grade controls:

- **JWT Authentication** -- HMAC-SHA256 signed tokens with configurable expiration.
- **Quota Management** -- Per-repository and per-tenant quotas (default 10 GB), with configurable overrides.
- **Bandwidth Throttling** -- Per-tenant and per-repo bandwidth limits with burst support.
- **Lock Management** -- Create, list, verify, delete locks. PostgreSQL-backed persistence. Scheduled cleanup of expired locks.
- **Garbage Collection** -- Cron-based GC with configurable grace period (default 7 days), max deletions per run.
- **Audit Logging** -- Full audit trail with configurable retention (default 90 days).
- **SHA256 Verification** -- Integrity checks on all LFS objects.
- **Max Object Size** -- Configurable (default 5 GB).
- **Multipart Upload** -- Files over 100 MB use S3 multipart with 64 MB parts, 4 concurrent uploads. Supports up to 625 GiB per object.

### Multi-Tenancy

- Namespace-based storage isolation (`{tenant}/{repo}.git/`).
- Per-tenant storage backend mapping -- each tenant can use a different S3/local backend.
- Per-tenant quotas, rate limits, and bandwidth throttling.
- Tenant context propagated via `x-tenant-id` header or request body field.

### RBAC

16 granular permissions across default roles:

| Role | Permissions |
|---|---|
| **Administrator** | All 16 permissions |
| **Developer** | 6 permissions (create/read/write repos, create/delete branches, LFS lock) |
| **ReadOnly** | 2 permissions (read repos, LFS lock read) |

Full permission list: `RepositoryCreate`, `RepositoryRead`, `RepositoryWrite`, `RepositoryDelete`, `RepositoryAdmin`, `BranchCreate`, `BranchDelete`, `BranchProtect`, `UserManage`, `RoleManage`, `SystemAdmin`, `WebhookManage`, `SettingsManage`, `LfsLockCreate`, `LfsLockRead`, `LfsLockDelete`, `LfsLockForce`.

### MCP Integration

14 MCP tools exposed to AI agents:

| Tool | Description |
|---|---|
| `repo_create` | Create a new Git repository for a tenant |
| `repo_delete` | Delete a Git repository |
| `repo_list` | List all repositories for a tenant |
| `repo_info` | Get repository information |
| `health_check` | Check plugin health status |
| `metrics_get` | Get plugin metrics |
| `storage_list_backends` | List all available storage backends |
| `storage_list_buckets` | List all buckets across backends |
| `storage_create_bucket` | Create a bucket in a specific backend |
| `storage_delete_bucket` | Delete a bucket from a backend |
| `storage_set_tenant_mapping` | Configure tenant-to-backend mapping |
| `storage_get_tenant_mapping` | Get tenant's backend mapping |
| `storage_remove_tenant_mapping` | Remove tenant's backend mapping |
| `storage_set_default_backend` | Set default storage backend |

This is what makes agentic-repos unique: AI assistants can programmatically create, manage, and inspect Git repositories through a standardized protocol -- no custom integrations needed.

### Skylet v2 Plugin

Full C FFI bridge with 7 exported symbols:

- `plugin_get_info_v2` -- Static plugin metadata
- `plugin_init_v2` -- Initialize with Skylet context, discover storage services, start SSH server
- `plugin_shutdown_v2` -- Graceful shutdown with full state reset for hot-reload
- `plugin_handle_request_v2` -- HTTP request handler (bridges C ABI to async Rust)
- `plugin_health_check_v2` -- Health check
- `plugin_get_tools` / `plugin_free_tools` -- MCP tool schemas as JSON

Runtime storage service discovery via Skylet's `ServiceRegistryV2`. Hot-reload support. Max concurrency: 64.

---

## Security Posture

### SSH

- **Post-quantum ready** -- Key exchange preferences include `MLKEM768X25519_SHA256` (Kyber), making it one of the first Git servers to prefer post-quantum key exchange.
- Ed25519 host keys with `OsRng` cryptographic randomness.
- Host key files stored with `0o600` permissions (owner-only).
- Auto-regeneration of corrupted host keys.
- Public key authentication only. No password auth.
- Full audit trail for all auth events (timestamp, username, IP, fingerprint).

### Command Validation

- Strict allowlist: `git-upload-pack`, `git-receive-pack`, `git-lfs-authenticate` only.
- Path traversal protection: rejects `..`, absolute paths, `/./`, `/../`.
- Control character detection and rejection.
- 4096-byte command length limit.

### Secret Scanning

Regex-based scanner detecting: AWS Access Keys, AWS Secret Keys, GitHub Tokens, GitLab Tokens, JWTs, Private Keys, Passwords, API Keys, Generic Secrets. Severity levels: Critical, High, Medium, Low.

### Compliance

Built-in tools for GDPR data export, SOC2 reports, HIPAA compliance, audit logging, data retention policies, and privacy controls/anonymization.

---

## Storage

### Backend Abstraction

```rust
trait StorageBackend {
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn read(&self, path: &str) -> Result<Vec<u8>>;
    async fn write(&self, path: &str, data: &[u8]) -> Result<()>;
    async fn delete(&self, path: &str) -> Result<()>;
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<String>>;
    async fn create_directory(&self, path: &str) -> Result<()>;
}
```

### Local Storage

Async file operations via `tokio::fs`. Directory auto-creation. Recursive listing.

### S3 Storage

AWS SDK v1 with configurable endpoint. Works with Amazon S3, MinIO, Garage, SeaweedFS, and any S3-compatible service.

- Multipart upload for files over 100 MB (64 MB parts, 4 concurrent uploads).
- Up to 625 GiB per file.
- Paginated object listing with continuation tokens.
- Prometheus metrics for all S3 operations.

### Runtime Backend Discovery

The `S3StorageRegistry` discovers S3-compatible storage services at runtime through the Skylet service registry. Each tenant can be mapped to a different backend with automatic fallback to the default.

---

## Horizontal Scaling

- Stateless design helpers for easy horizontal deployment.
- Redis-based distributed locks for coordination.
- Leader election for scheduled jobs (LFS GC, lock cleanup).
- Session affinity configuration.
- Cluster manager with instance health tracking.
- Readiness and liveness probes.

---

## Observability

- **Prometheus metrics** endpoint at `/metrics` (format 0.0.4).
- **LFS metrics**: upload/download/error counters, GC scan/deletion counts, quota usage, throttle bytes, S3 multipart upload timing.
- **Plugin metrics**: request counts, success/error counts per method.
- **Health checks**: HTTP `/api/v1/health` and plugin `health.check`.
- **Cluster health**: instance tracking, readiness/liveness probes, leader election status.
- **Audit logging**: auth events, LFS operations, with configurable retention.

---

## Deployment Options

### Docker

Multi-stage build: Rust 1.92 builder + Debian Bookworm slim runtime. Non-root user (UID 1000). Health check on port 8090. Exposes SSH (2222) and HTTP (8090).

### Kubernetes

Full Kustomize-based deployment with Namespace, Deployment, Service, ConfigMap, HPA, PDB, and Gateway configuration. Overlays for different environments. Garage and SeaweedFS S3 integration manifests included.

### Nix

Flake-based with crane for reproducible builds. UPX binary compression (LZMA). NixOS module available as `nixosModules.agentic-repos`. Docker image via `dockerTools.buildLayeredImage`. Comprehensive Nix checks: build, container, VM tests, integration tests.

### Standalone Binary

Single binary distribution. Works with local filesystem storage -- no external dependencies required.

---

## What Sets It Apart

### vs. Gitea, GitLab, Gitee

| | agentic-repos | Gitea | GitLab |
|---|---|---|---|
| **MCP Integration** | Native (14 tools) | None | None |
| **Skylet Plugin** | First-class FFI | Not applicable | Not applicable |
| **AI Agent Compatibility** | Built-in | Requires custom integration | Requires custom integration |
| **Language** | Rust (memory-safe, no GC) | Go (GC pauses) | Ruby + Go |
| **Post-Quantum SSH** | Yes (MLKEM768X25519) | No | No |
| **Multi-Backend S3** | Runtime discovery per tenant | Limited | Yes (config-time) |
| **LFS Quota + Throttling** | Per-tenant, per-repo | Basic | Advanced |
| **Binary Size** | Small (UPX-compressed) | ~100 MB | ~2 GB |
| **NixOS Module** | Official | Community | None |
| **Dual Binary/Plugin** | Yes | No | No |
| **Built-in Secret Scanning** | Yes | Plugin required | Plugin required |
| **Compliance Suite** | GDPR/SOC2/HIPAA built-in | Limited | Advanced |

### The Key Differentiator

agentic-repos is the only Git server designed from the ground up for AI-agent-driven workflows. The MCP integration is not an add-on or a plugin -- it is a core capability. Any MCP-compatible AI agent can create repositories, manage storage backends, and perform Git operations through a standardized protocol. This makes agentic-repos the natural choice for platforms where AI agents are first-class participants in the development workflow.

---

## Technology Stack

| Component | Technology |
|---|---|
| Language | Rust 2021 Edition |
| Web Framework | actix-web 4.4 |
| Async Runtime | tokio 1.35 (full features) |
| Git Operations | git2 0.19 (vendored libgit2), gix-protocol 0.55 |
| SSH Server | russh 0.58 |
| S3 Storage | aws-sdk-s3 1.14 |
| Database | sqlx 0.7 (PostgreSQL + SQLite) |
| Caching | deadpool-redis 0.18 |
| Metrics | prometheus 0.13 |
| Templates | tera 1.19 |
| Auth | jsonwebtoken 9.2, bcrypt 0.18, oauth2 4.4 |
| Serialization | serde + serde_json |
| Build | Nix flake + Cargo |

---

## License

Dual-licensed:

- **AGPL-3.0** -- Open source for non-commercial use.
- **Commercial** -- Feature-flag-gated enterprise features including RBAC, LDAP, SAML, OAuth, audit logging, webhooks, multi-region, horizontal scaling, Redis cache, monitoring, security scanning, SOC2/GDPR/HIPAA compliance, backup/recovery, CI/CD integration, GraphQL API, analytics, and Git LFS.

---

## Target Use Cases

1. **AI/ML Platforms** -- MCP integration lets LLMs manage Git repositories as part of automated workflows.
2. **Multi-Tenant SaaS** -- Per-tenant storage, quotas, rate limiting, and compliance in a single binary.
3. **Edge Deployments** -- Small binary, Nix reproducibility, works with local storage, no external dependencies.
4. **Skylet Ecosystem** -- First-class plugin for Vincents AI's orchestration platform with runtime service discovery.
5. **Regulated Industries** -- Built-in GDPR, SOC2, HIPAA compliance tools and audit logging.
6. **Air-Gapped Environments** -- Works with local filesystem storage. No cloud dependencies required.

---

## Getting Started

```bash
# Build (Nix -- recommended)
nix build

# Build (Cargo)
cargo build --release

# Run standalone
./target/release/agentic-repos

# Run as Skylet plugin
# Copy target/release/libagentic_repos.so to your Skylet plugins directory
```

Ports: SSH on `:2222`, HTTP API on `:8090`.

---

*agentic-repos -- Git infrastructure for the AI-native development stack.*

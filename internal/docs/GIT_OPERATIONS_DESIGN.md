# Git Operations Layer Design Document

## Overview

This document describes the design of the Git operations layer for agentic-repos, a multi-tenant Git server written in Rust. The layer implements SSH Git protocol access using mature Rust crates, with optional commercial features for rate limiting, retention policies, and compliance-aware storage.

## Architecture

### Core Components

1. **SSH Server** (`src/git/ssh.rs`): Uses `russh` for SSH protocol handling
2. **Git Protocol Handler** (`src/git/protocol.rs`): Uses `gix-protocol` and `gix-transport` for Git Smart Protocol
3. **Storage Integration** (`src/git/storage.rs`): Uses `git2` for repository operations
4. **Authentication** (`src/git/auth.rs`): SSH public key authentication

### Dependencies

```toml
# Core (AGPL)
russh = "0.56"          # SSH server/client
gix-protocol = "0.55"   # Git protocol implementation
gix-transport = "0.52"  # Git transport layer
git2 = "0.18"           # Git repository operations

# Commercial Features
redis = "0.25"          # Rate limiting and quota tracking
```

## SSH Git Server

### Port Configuration

- **Default SSH port**: 2222 (configurable via `AGENTIC_REPOS_SSH_PORT`)
- **Rationale**: Avoid requiring root privileges, simplifies container deployment

### SSH Connection Flow

1. Client connects to port 2222 → `russh` accepts TCP connection
2. SSH handshake (key exchange) → `russh` handles negotiation
3. Authentication phase → Client sends public key → Verify against stored keys
4. On successful auth → `russh` opens a channel (exec)
5. Client runs: `git-upload-pack /{tenant_id}/{repo_name}.git`
6. `russh` spawns an async task to handle the command

### Authentication

**SSH Public Key Authentication** (AGPL):

```rust
pub struct SshAuthHandler {
    key_store: Arc<dyn KeyStore>,
}

impl russh::server::Handler for SshAuthHandler {
    type AuthFuture = Pin<Box<dyn Future<Output = Result<Auth, Self::Error>> + Send>>;

    fn auth_publickey(
        &mut self,
        username: &str,
        public_key: &PublicKey,
    ) -> Self::AuthFuture {
        let key_store = self.key_store.clone();
        Box::pin(async move {
            let user = key_store.get_user_by_key(public_key).await?;
            Ok(Auth::Accept {
                username: user.username,
            })
        })
    }
}
```

## Git Protocol Handler

### Git Smart Protocol Implementation

Uses `gix-protocol` for implementing the Git Smart Protocol (upload-pack/receive-pack) directly in Rust:

```rust
pub struct GitProtocolHandler {
    storage: Arc<dyn StorageBackend>,
    quota_service: Arc<dyn QuotaService>,
}

impl GitProtocolHandler {
    pub async fn handle_upload_pack(
        &self,
        tenant_id: &str,
        repo_name: &str,
        writer: impl AsyncWrite + Unpin,
        reader: impl AsyncRead + Unpin,
    ) -> Result<(), GitError> {
        // Check quotas before operation
        self.quota_service.check_rate_limit(tenant_id).await?;

        // Resolve repository path
        let repo_path = self.storage.resolve_repo_path(tenant_id, repo_name).await?;

        // Lazy creation if repo doesn't exist
        if !self.storage.exists(&repo_path).await? {
            self.create_repo(tenant_id, repo_name).await?;
        }

        // Execute upload-pack using gix-transport
        let repo = git2::Repository::open(&repo_path)?;
        let (mut reader, mut writer) = tokio::io::duplex(64 * 1024);

        // Spawn git2 pack generation task
        tokio::spawn(async move {
            // Use gix-protocol to handle the smart protocol
            gix_trans::write(
                &mut writer,
                &mutport::client reader,
                gix_protocol::Command::UploadPack,
                &repo,
            ).await
        });

        Ok(())
    }
}
```

### Protocol Support

- **Git Protocol v2**: Supported via gix-protocol
- **Packfile Streaming**: Bidirectional streaming via tokio async I/O
- **Capability Negotiation**: Automatic handling of client capabilities

## Rate Limiting (Commercial Feature)

### Proxy-Aware Rate Limiting

Compatible with Envoy Gateway rate limiting and X-Forwarded-For headers:

```yaml
# rate-limiting configuration
rate_limiting:
  enabled: true
  proxy_headers:
    - "x-forwarded-for"
    - "x-real-ip"
  tiers:
    free:
      requests_per_minute: 60
      requests_per_hour: 1000
      burst_size: 10
    pro:
      requests_per_minute: 300
      requests_per_hour: 10000
      burst_size: 50
    enterprise:
      requests_per_minute: -1  # Unlimited
      burst_size: 100
```

### Envoy Gateway Integration

Envoy Gateway can handle rate limiting at the edge, with agentic-repos providing:

1. **X-Request-ID**: For distributed tracing
2. **X-Tenant-ID**: For tenant-aware rate limits
3. **Custom Headers**: For advanced rate limiting scenarios

```rust
pub struct RateLimitContext {
    pub tenant_id: String,
    pub ip_address: String,
    pub request_id: String,
    pub user_agent: String,
}

impl RateLimitContext {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            tenant_id: headers.get("x-tenant-id")
                .map(|v| v.to_str().unwrap_or("").to_string())
                .unwrap_or_default(),
            ip_address: headers.get("x-forwarded-for")
                .or(headers.get("x-real-ip"))
                .map(|v| v.to_str().unwrap_or("").to_string())
                .unwrap_or_default(),
            request_id: headers.get("x-request-id")
                .map(|v| v.to_str().unwrap_or("").to_string())
                .unwrap_or(uuid::Uuid::new_v4().to_string()),
            user_agent: headers.get("user-agent")
                .map(|v| v.to_str().unwrap_or("").to_string())
                .unwrap_or_default(),
        }
    }
}
```

## Tenant Quotas & Restrictions (Commercial Feature)

### Limits Configuration

```yaml
quotas:
  tiers:
    free:
      max_repos: 5
      max_repo_size_mb: 5
      max_object_mb: 10
      max_branches: 10
      max_packfile_mb: 50
    pro:
      max_repos: 50
      max_repo_size_mb: 1000
      max_object_mb: 100
      max_branches: 100
      max_packfile_mb: 500
    enterprise:
      max_repos: -1  # Unlimited
      max_repo_size_mb: -1
      max_object_mb: 500
      max_branches: -1
      max_packfile_mb: -1
```

### Enforcement Points

1. **Before push**: Check tenant's current repo count vs limit
2. **During push**: Monitor packfile size, reject if exceeds limit
3. **After push**: Verify total repo size in storage

## Retention Policies (Commercial Feature)

### Per-Repository Configurable Retention

Retention policies are stored as Git refs in the repository itself:

```bash
# Set retention policy via Git ref
git push origin refs/heads/policies/retention:refs/heads/policies/retention

# Policy format in .git/agentic-repos/retention.json
{
  "retention_policy": {
    "mode": "git-ref",
    "git_ref": "refs/policies/retention",
    "rules": [
      {
        "pattern": "refs/heads/.*",
        "max_age_days": 90,
        "min_branches_to_keep": 1
      },
      {
        "pattern": "refs/tags/.*",
        "max_age_days": 365,
        "min_tags_to_keep": 0
      }
    ]
  }
}
```

### Retention Modes

- **Git Ref**: Policy stored in a special Git ref (`.git/agentic-repos/policy`)
- **Database**: Policy stored in external database
- **API**: Policy managed via external API (webhook-based)

### Example Retention Policy

```json
{
  "version": "1.0",
  "rules": [
    {
      "name": "main-branch",
      "pattern": "refs/heads/main",
      "retention": "permanent"
    },
    {
      "name": "feature-branches",
      "pattern": "refs/heads/feature/**",
      "retention": {
        "max_age_days": 90,
        "auto_delete": true,
        "notify_days": 7_before
      }
    },
    {
      "name": "release-tags",
      "pattern": "refs/tags/release-*",
      "retention": {
        "max_age_days": 365,
        "min_versions_to_keep": 5
      }
    }
  ]
}
```

## Branch Limits (Commercial Feature)

### Branch Configuration

```yaml
branches:
  max_per_repo: 100  # Per-repository limit
  patterns:
    - name: "main"
      pattern: "refs/heads/main"
      required: true
    - name: "feature/*"
      pattern: "refs/heads/feature/**"
      max_age_days: 90
    - name: "bugfix/*"
      pattern: "refs/heads/bugfix/**"
      max_age_days: 30
```

### Branch Management

```rust
pub struct BranchLimitService {
    max_branches: usize,
    branch_patterns: Vec<BranchPattern>,
}

impl BranchLimitService {
    pub async fn check_branch_limit(
        &self,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<(), BranchLimitError> {
        let current_branches = self.storage.count_branches(tenant_id, repo_name).await?;

        if current_branches >= self.max_branches {
            return Err(BranchLimitError::Exceeded {
                current: current_branches,
                limit: self.max_branches,
            });
        }

        Ok(())
    }
}
```

## Compliance-Aware Storage

### Repository Compliance Tags

```yaml
# Repository metadata
compliance:
  - GDPR        # EU data protection
  - HIPAA       # Healthcare data
  - SOC2        # Security controls
  - ISO27001    # Information security
  - PUBLIC      # Publicly shareable
```

### Bucket Configuration

```yaml
storage:
  buckets:
    - name: "default"
      compliance: []  # Default bucket
    - name: "gdpr-eu-west1"
      region: "eu-west-1"
      compliance: ["GDPR"]
      encryption: "AES256"
      retention:
        mode: "GOVERNANCE"
        days: 2555  # 7 years for GDPR
    - name: "us-standard"
      compliance: ["PUBLIC", "SOC2"]
      replication:
        enabled: true
        target: "us-east-1"
```

### Routing Logic

```rust
pub struct ComplianceAwareStorage {
    bucket_map: HashMap<String, S3BucketConfig>,
    default_bucket: String,
}

impl ComplianceAwareStorage {
    pub async fn get_bucket_for_repository(
        &self,
        repo: &Repository,
    ) -> Result<&S3BucketConfig, StorageError> {
        for compliance_tag in &repo.compliance_tags {
            if let Some(bucket) = self.bucket_map.get(compliance_tag) {
                return Ok(bucket);
            }
        }

        // Fall back to default bucket
        Ok(self.bucket_map.get(&self.default_bucket).unwrap())
    }
}
```

## Data Flow

### Push Operation Flow

```
1. Client connects via SSH
2. Authenticate using public key
3. Extract tenant_id from key or username
4. Parse command: git-receive-pack /{tenant_id}/{repo_name}.git
5. Rate limiting check (commercial)
6. Quota check (commercial)
7. Get compliance-aware storage bucket
8. Lazy creation: if repo doesn't exist, create with git2::Repository::init_bare()
9. Execute receive-pack using gix-transport
10. Track bytes written, enforce limits
11. Update quota counters (commercial)
12. Return success/failure to client
```

### Clone Operation Flow

```
1. Client connects via SSH
2. Authenticate using public key
3. Extract tenant_id from key or username
4. Parse command: git-upload-pack /{tenant_id}/{repo_name}.git
5. Rate limiting check (commercial)
6. Get compliance-aware storage bucket
7. Resolve repository path in storage
8. Execute upload-pack using gix-transport
9. Stream packfile to client
10. Update usage metrics
```

## Error Handling

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Authentication failed: {0}")]
    AuthError(#[from] AuthError),

    #[error("Repository not found: {0}")]
    RepoNotFound(String),

    #[error("Quota exceeded: {0}")]
    QuotaExceeded(#[from] QuotaError),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Compliance violation: {0}")]
    ComplianceError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
}
```

## Pricing Tiers Architecture (Commercial Feature)

### Predefined Tiers

| Feature | Free | Pro | Enterprise |
|---------|------|-----|------------|
| Max Repos | 5 | 50 | Unlimited |
| Max Repo Size | 5MB | 1GB | Unlimited |
| Max Branches | 10 | 100 | Unlimited |
| Rate Limit | 60/min | 300/min | Unlimited |
| Retention Policies | No | Yes | Yes |
| Compliance Routing | No | Yes | Yes |
| Custom Branding | No | No | Yes |
| SLA | None | Best Effort | 99.99% |

### Tier Implementation

```rust
pub fn pricing_tiers() -> HashMap<String, PricingTier> {
    let mut tiers = HashMap::new();

    tiers.insert("free".to_string(), PricingTier {
        name: "free".to_string(),
        max_repos: 5,
        max_repo_size_mb: 5,
        max_branches: 10,
        max_tags: 0,
        rate_limit_rpm: 60,
        retention_enabled: false,
        compliance_enabled: false,
        custom_features: vec![],
    });

    tiers.insert("pro".to_string(), PricingTier {
        name: "pro".to_string(),
        max_repos: 50,
        max_repo_size_mb: 1000,
        max_branches: 100,
        max_tags: 0,
        rate_limit_rpm: 300,
        retention_enabled: true,
        compliance_enabled: true,
        custom_features: vec!["webhooks".to_string()],
    });

    tiers.insert("enterprise".to_string(), PricingTier {
        name: "enterprise".to_string(),
        max_repos: -1, // Unlimited
        max_repo_size_mb: -1,
        max_branches: 0, // Unlimited
        max_tags: 0,
        rate_limit_rpm: 0, // Unlimited
        retention_enabled: true,
        compliance_enabled: true,
        custom_features: vec!["webhooks".to_string(), "sla".to_string(), "audit".to_string()],
    });

    tiers
}
```

### Tenant Tier Assignment

```yaml
tenants:
  - id: "tenant-001"
    tier: "free"
    custom_domain: null
  - id: "tenant-002"
    tier: "pro"
    custom_domain: "git.company.com"
  - id: "tenant-003"
    tier: "enterprise"
    custom_domain: "git.enterprise.com"
    sla_level: "99.99"
```

## Installation

### Core (AGPL)

```bash
cd agentic-repos
cargo build --release
```

### With Commercial Features

```bash
cd agentic-repos
cargo build --release --features commercial
```

## License

**Core**: AGPLv3 - Free to use, modify, and distribute

**Commercial Features**: Available under separate commercial license

- Rate limiting with proxy awareness
- Configurable retention policies
- Branch limits and quota management
- Multi-tier pricing
- Compliance-aware storage routing
- Priority support and SLA guarantees

Contact [contact email] for commercial licensing.

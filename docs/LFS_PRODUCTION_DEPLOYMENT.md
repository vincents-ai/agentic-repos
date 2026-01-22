# Production Deployment Guide - Git LFS

This guide provides step-by-step instructions for deploying repos-mcp with Git LFS in a production environment.

## Table of Contents

1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Production Configuration](#production-configuration)
3. [Security Hardening](#security-hardening)
4. [Infrastructure Setup](#infrastructure-setup)
5. [Monitoring & Alerting](#monitoring--alerting)
6. [Backup & Recovery](#backup--recovery)
7. [Capacity Planning](#capacity-planning)
8. [Troubleshooting](#troubleshooting)

---

## Pre-Deployment Checklist

Before deploying to production, ensure:

- [ ] **Hardware Requirements Met**
  - CPU: 4+ cores recommended
  - RAM: 8GB minimum, 16GB recommended
  - Disk: SSD with sufficient space for LFS objects (see capacity planning)
  - Network: 1Gbps+ for high-throughput scenarios

- [ ] **Software Requirements**
  - Rust 1.70+ (for building from source)
  - PostgreSQL 13+ (for RBAC storage)
  - Redis (optional, for caching)
  - Prometheus + Grafana (for monitoring)

- [ ] **Security Requirements**
  - TLS certificates (Let's Encrypt or commercial CA)
  - Strong JWT secret generated
  - Firewall rules configured
  - SSH host keys generated

- [ ] **Review Security Audit**
  - Read [`docs/LFS_SECURITY_AUDIT.md`](LFS_SECURITY_AUDIT.md)
  - Implement all high-priority recommendations

---

## Production Configuration

### 1. Create Production Config

```bash
cp config-example.yaml config-production.yaml
```

### 2. Configure Server Settings

```yaml
server:
  host: "0.0.0.0"  # Listen on all interfaces
  port: 8090       # HTTP API port
  ssh:
    port: 2222
    host_key_path: "/etc/repos-mcp/ssh_host_key"
    max_connections: 500  # Increase for production
    connection_timeout_secs: 60
```

### 3. Configure Git LFS (CRITICAL)

```yaml
features:
  git_lfs: true
  lfs_locks: false  # Enable if needed

lfs:
  enabled: true
  
  # CRITICAL: Generate strong JWT secret
  # NEVER use the default value in production
  jwt_secret: "${LFS_JWT_SECRET}"  # Load from environment variable
  
  # CRITICAL: Use HTTPS in production
  base_url: "https://git.example.com"
  
  # Adjust based on your use case
  max_object_size: 5368709120  # 5GB (adjust as needed)
  
  # Token expiration (1 hour is reasonable)
  token_expiration_secs: 3600
  
  # Storage path pattern (default is secure)
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
```

#### Generate Strong JWT Secret

**CRITICAL**: Use a cryptographically strong secret:

```bash
# Generate 32-byte secret (256-bit)
openssl rand -base64 32 > /etc/repos-mcp/jwt-secret.txt
chmod 600 /etc/repos-mcp/jwt-secret.txt

# Set as environment variable
export LFS_JWT_SECRET=$(cat /etc/repos-mcp/jwt-secret.txt)
```

**Secret Management Best Practices**:
- Store in secure secret manager (HashiCorp Vault, AWS Secrets Manager, etc.)
- Never commit to version control
- Rotate every 90 days
- Use different secrets for dev/staging/production

### 4. Configure Storage Backend

#### Option A: Local Storage (Simple, for single-server deployments)

```yaml
storage:
  type: "local"
  local:
    path: "/var/lib/repos-mcp/storage"
```

**File Permissions** (CRITICAL):
```bash
# Create storage directory
mkdir -p /var/lib/repos-mcp/storage

# Set restrictive permissions (owner-only access)
chown repos-mcp:repos-mcp /var/lib/repos-mcp/storage
chmod 700 /var/lib/repos-mcp/storage
```

#### Option B: S3 Storage (Recommended for production, scalable)

```yaml
storage:
  type: "s3"
  s3:
    bucket: "my-company-lfs-storage"
    region: "us-east-1"
    # Optional: Custom endpoint for S3-compatible services
    # endpoint: "https://minio.example.com"
```

**S3 IAM Policy** (least privilege):
```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::my-company-lfs-storage",
        "arn:aws:s3:::my-company-lfs-storage/*"
      ]
    }
  ]
}
```

### 5. Configure Rate Limiting

```yaml
rate_limit:
  enabled: true
  global_limits:
    max_requests_per_second: 1000  # Adjust based on capacity
    max_connections_per_ip: 50
    burst_size: 100
```

### 6. Configure Logging

```yaml
logging:
  level: "info"  # Use "warn" in high-traffic scenarios
  json: true     # Enable for log aggregation (ELK, Splunk, etc.)
  timestamp: true
```

---

## Security Hardening

### 1. HTTPS/TLS Setup

**CRITICAL**: Git LFS MUST use HTTPS in production. JWT tokens in HTTP URLs = credential leak.

#### Option A: Nginx Reverse Proxy (Recommended)

```nginx
# /etc/nginx/sites-available/repos-mcp
server {
    listen 443 ssl http2;
    server_name git.example.com;

    # TLS Configuration
    ssl_certificate /etc/letsencrypt/live/git.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/git.example.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;
    ssl_prefer_server_ciphers on;

    # Security Headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Frame-Options "DENY" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;

    # Proxy to repos-mcp
    location / {
        proxy_pass http://localhost:8090;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # LFS-specific: Allow large uploads
        client_max_body_size 10G;
        proxy_request_buffering off;
    }

    # Metrics endpoint (restrict to monitoring IPs)
    location /metrics {
        allow 10.0.0.0/8;     # Internal network
        deny all;
        proxy_pass http://localhost:8090/metrics;
    }
}

# Redirect HTTP to HTTPS
server {
    listen 80;
    server_name git.example.com;
    return 301 https://$server_name$request_uri;
}
```

#### Option B: Let's Encrypt with Certbot

```bash
# Install certbot
apt-get install certbot python3-certbot-nginx

# Obtain certificate
certbot --nginx -d git.example.com

# Auto-renewal (runs twice daily)
systemctl enable certbot.timer
```

### 2. Firewall Configuration

```bash
# Allow SSH (Git over SSH)
ufw allow 2222/tcp

# Allow HTTPS (Git LFS HTTP)
ufw allow 443/tcp

# Allow HTTP (for Let's Encrypt validation only)
ufw allow 80/tcp

# Deny everything else
ufw default deny incoming
ufw default allow outgoing

# Enable firewall
ufw enable
```

### 3. Systemd Service (Run as non-root user)

```ini
# /etc/systemd/system/repos-mcp.service
[Unit]
Description=repos-mcp Git Server with LFS
After=network.target postgresql.service

[Service]
Type=simple
User=repos-mcp
Group=repos-mcp
WorkingDirectory=/opt/repos-mcp
EnvironmentFile=/etc/repos-mcp/environment

# Run the server
ExecStart=/opt/repos-mcp/repos-mcp --config /etc/repos-mcp/config-production.yaml

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/repos-mcp

# Restart policy
Restart=on-failure
RestartSec=10s

[Install]
WantedBy=multi-user.target
```

**Environment file** (`/etc/repos-mcp/environment`):
```bash
# JWT Secret (loaded from secure storage)
LFS_JWT_SECRET=<your-strong-secret-here>

# AWS credentials (if using S3)
AWS_ACCESS_KEY_ID=<key>
AWS_SECRET_ACCESS_KEY=<secret>

# Logging
RUST_LOG=info
```

**Create service user**:
```bash
useradd --system --no-create-home --shell /bin/false repos-mcp
```

**Start service**:
```bash
systemctl daemon-reload
systemctl enable repos-mcp
systemctl start repos-mcp
systemctl status repos-mcp
```

### 4. JWT Secret Rotation

**Procedure** (every 90 days):

1. Generate new secret:
   ```bash
   NEW_SECRET=$(openssl rand -base64 32)
   ```

2. Update configuration with both old and new secrets (dual-accept period):
   ```yaml
   # This requires code modification to support dual-accept
   # For now, plan maintenance window for rotation
   ```

3. Update environment and restart:
   ```bash
   echo "LFS_JWT_SECRET=$NEW_SECRET" > /etc/repos-mcp/environment
   systemctl restart repos-mcp
   ```

4. Monitor for authentication errors:
   ```bash
   journalctl -u repos-mcp -f | grep -i "jwt\|auth"
   ```

---

## Infrastructure Setup

### Deployment Architecture

#### Single-Server Deployment (Small/Medium scale)

```
┌─────────────────────────────────────┐
│         Load Balancer (Optional)    │
│         - nginx / HAProxy           │
└────────────┬────────────────────────┘
             │ HTTPS
┌────────────▼────────────────────────┐
│         repos-mcp Server            │
│  ┌──────────────┐  ┌──────────────┐│
│  │   SSH:2222   │  │  HTTP:8090   ││
│  │   (Git SSH)  │  │  (LFS API)   ││
│  └──────────────┘  └──────────────┘│
│  ┌─────────────────────────────────┤
│  │   Local Storage / S3            │
│  │   /var/lib/repos-mcp/storage    │
│  └─────────────────────────────────┤
└─────────────────────────────────────┘
```

#### High-Availability Deployment (Large scale)

```
                 ┌─────────────────┐
                 │  Load Balancer  │
                 │   (nginx/ALB)   │
                 └────────┬────────┘
                          │
         ┌────────────────┼────────────────┐
         │                │                │
    ┌────▼────┐      ┌────▼────┐     ┌────▼────┐
    │ repos-  │      │ repos-  │     │ repos-  │
    │ mcp-1   │      │ mcp-2   │     │ mcp-3   │
    └────┬────┘      └────┬────┘     └────┬────┘
         │                │                │
         └────────────────┼────────────────┘
                          │
                 ┌────────▼────────┐
                 │   S3 Storage    │
                 │  (Shared State) │
                 └─────────────────┘
                 
         ┌────────────────────────────┐
         │     Monitoring Stack       │
         │  - Prometheus (metrics)    │
         │  - Grafana (dashboards)    │
         │  - Alertmanager (alerts)   │
         └────────────────────────────┘
```

**Requirements for HA**:
- S3 storage (shared state)
- Load balancer with health checks
- Consistent JWT secret across instances
- Shared configuration management

---

## Monitoring & Alerting

### 1. Deploy Monitoring Stack

See [`monitoring/README.md`](../monitoring/README.md) for detailed setup.

**Quick Start**:

```bash
# 1. Deploy Prometheus
docker run -d \
  --name prometheus \
  -p 9090:9090 \
  -v ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml \
  -v ./monitoring/alerts.yml:/etc/prometheus/alerts.yml \
  prom/prometheus

# 2. Deploy Grafana
docker run -d \
  --name grafana \
  -p 3000:3000 \
  grafana/grafana

# 3. Import LFS dashboard
# Upload monitoring/grafana-lfs-dashboard.json via Grafana UI
```

### 2. Configure Alertmanager

```yaml
# alertmanager.yml
global:
  resolve_timeout: 5m
  slack_api_url: 'https://hooks.slack.com/services/YOUR/WEBHOOK/URL'

route:
  group_by: ['alertname', 'tenant', 'repo']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'default'
  
  routes:
    - match:
        severity: critical
      receiver: 'pagerduty'
      continue: true
    
    - match:
        severity: warning
      receiver: 'slack'

receivers:
  - name: 'default'
    slack_configs:
      - channel: '#alerts'
        title: 'LFS Alert: {{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.description }}{{ end }}'
  
  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: 'YOUR_PAGERDUTY_KEY'
```

### 3. Health Checks

**HTTP Health Endpoint** (add to repos-mcp):
```bash
# Check if service is responding
curl -f http://localhost:8090/metrics || exit 1
```

**Load Balancer Health Check**:
```nginx
# nginx health check
location /health {
    access_log off;
    return 200 "healthy\n";
    add_header Content-Type text/plain;
}
```

---

## Backup & Recovery

### 1. Backup Strategy

#### LFS Objects (Critical Data)

**S3 Versioning** (Recommended):
```bash
# Enable versioning on S3 bucket
aws s3api put-bucket-versioning \
  --bucket my-company-lfs-storage \
  --versioning-configuration Status=Enabled

# Enable lifecycle policy for old versions
aws s3api put-bucket-lifecycle-configuration \
  --bucket my-company-lfs-storage \
  --lifecycle-configuration file://lifecycle.json
```

**lifecycle.json**:
```json
{
  "Rules": [
    {
      "Id": "DeleteOldVersions",
      "Status": "Enabled",
      "NoncurrentVersionExpiration": {
        "NoncurrentDays": 90
      }
    }
  ]
}
```

**Local Storage Backup**:
```bash
# Daily backup to S3
#!/bin/bash
DATE=$(date +%Y%m%d)
tar -czf /tmp/lfs-backup-$DATE.tar.gz /var/lib/repos-mcp/storage
aws s3 cp /tmp/lfs-backup-$DATE.tar.gz s3://backups/lfs/$DATE/
rm /tmp/lfs-backup-$DATE.tar.gz
```

**Cron schedule**:
```cron
# Daily backup at 2 AM
0 2 * * * /opt/repos-mcp/scripts/backup-lfs.sh
```

#### Configuration Backup

```bash
# Backup configuration files
cp /etc/repos-mcp/config-production.yaml /backups/config-$(date +%Y%m%d).yaml
cp /etc/repos-mcp/authorized_keys /backups/authorized_keys-$(date +%Y%m%d)
```

### 2. Disaster Recovery

**Recovery Time Objective (RTO)**: < 1 hour  
**Recovery Point Objective (RPO)**: < 24 hours

**Recovery Procedure**:

1. **Provision new server** (infrastructure-as-code recommended)
2. **Restore configuration**:
   ```bash
   scp backups/config-latest.yaml new-server:/etc/repos-mcp/config-production.yaml
   ```
3. **Restore storage** (if local):
   ```bash
   aws s3 sync s3://backups/lfs/latest/ /var/lib/repos-mcp/storage/
   ```
4. **Start service**:
   ```bash
   systemctl start repos-mcp
   ```
5. **Verify**:
   ```bash
   curl https://git.example.com/metrics
   ```

### 3. Test Recovery Quarterly

```bash
# Test restore in staging environment
./scripts/test-disaster-recovery.sh
```

---

## Capacity Planning

### Storage Requirements

**Formula**:
```
Total Storage = (Number of Files × Average File Size) × (1 + Overhead)
Overhead = 20% (for metadata, logs, temp files)
```

**Example**:
- 10,000 LFS objects
- Average size: 50 MB
- Total: 10,000 × 50 MB × 1.2 = **600 GB**

**Growth Planning**:
- Monitor `lfs_upload_bytes_total` metric
- Estimate monthly growth rate
- Plan storage upgrades 3 months in advance

### Compute Requirements

| Scale | Users | Repos | CPU | RAM | Disk IOPS |
|-------|-------|-------|-----|-----|-----------|
| Small | <100 | <500 | 4 cores | 8 GB | 1,000 |
| Medium | 100-1,000 | 500-5,000 | 8 cores | 16 GB | 5,000 |
| Large | 1,000-10,000 | 5,000-50,000 | 16 cores | 32 GB | 10,000 |
| Enterprise | >10,000 | >50,000 | 32+ cores | 64+ GB | 20,000+ |

### Network Bandwidth

**Upload Bandwidth** = `lfs_upload_bytes_total` metric  
**Download Bandwidth** = `lfs_download_bytes_total` metric

**Recommendation**: 1 Gbps minimum, 10 Gbps for large deployments

### Monitoring Capacity Metrics

```promql
# Storage usage trend
increase(lfs_upload_bytes_total[30d])

# Upload rate trend
rate(lfs_uploads_total[1h])

# Average object size
sum(lfs_upload_bytes_total) / sum(lfs_uploads_total)
```

---

## Troubleshooting

### Common Issues

#### 1. JWT Token Validation Failures

**Symptoms**:
- 401 Unauthorized on LFS operations
- Error: "Invalid JWT token"

**Solutions**:
- Verify JWT secret matches configuration
- Check token expiration (`token_expiration_secs`)
- Ensure system clocks are synchronized (NTP)
- Review logs: `journalctl -u repos-mcp | grep -i jwt`

#### 2. SHA256 Hash Mismatches

**Symptoms**:
- 422 Unprocessable Entity on upload
- Error: "Object hash does not match OID"
- `lfs_errors_total{error_type="hash_mismatch"}` increasing

**Solutions**:
- Check for network corruption (MTU issues)
- Verify client Git LFS version (update to latest)
- Check proxy/load balancer buffering settings
- Review logs for specific OIDs

#### 3. Storage Backend Errors

**Symptoms**:
- 500 Internal Server Error
- Error: "Storage error: ..."
- `lfs_errors_total{error_type="storage_error"}` increasing

**Solutions**:
- **S3**: Check AWS credentials, bucket permissions, region
- **Local**: Check disk space (`df -h`), permissions (`ls -la`)
- Review storage backend logs

#### 4. Performance Issues

**Symptoms**:
- Slow uploads/downloads
- High `lfs_upload_duration_seconds` P95

**Solutions**:
- Check CPU/RAM usage (`top`, `htop`)
- Check disk I/O (`iostat -x 1`)
- Check network bandwidth (`iftop`)
- Review Grafana dashboards for bottlenecks
- Consider scaling horizontally (multiple instances)

#### 5. Out of Memory

**Symptoms**:
- Service crashes with OOM (Out of Memory)
- Kernel logs show OOM killer

**Solutions**:
- Reduce `max_object_size` (splits large uploads)
- Increase system RAM
- Add swap space (temporary)
- Enable S3 multipart uploads (future feature)

### Debug Mode

Enable detailed logging:

```yaml
logging:
  level: "debug"  # Very verbose
```

Restart service:
```bash
systemctl restart repos-mcp
```

View logs:
```bash
journalctl -u repos-mcp -f
```

### Useful Commands

```bash
# Check service status
systemctl status repos-mcp

# View recent logs
journalctl -u repos-mcp --since "10 minutes ago"

# Check metrics
curl http://localhost:8090/metrics | grep lfs_

# Test LFS upload
echo "test" > test.bin
OID=$(sha256sum test.bin | awk '{print $1}')
curl -X PUT http://localhost:8090/lfs/test-tenant/test-repo/lfs/objects/$OID --data-binary @test.bin

# Check disk space
df -h /var/lib/repos-mcp

# Check open file descriptors
lsof -p $(pgrep repos-mcp) | wc -l

# Network connectivity
ping git.example.com
telnet git.example.com 443
```

---

## Production Readiness Checklist

Before going live:

### Configuration
- [ ] Strong JWT secret generated and stored securely
- [ ] HTTPS configured with valid TLS certificate
- [ ] `base_url` points to HTTPS endpoint
- [ ] `max_object_size` set appropriately
- [ ] Rate limiting configured
- [ ] Logging configured (JSON mode for aggregation)

### Security
- [ ] Security audit recommendations implemented
- [ ] Firewall rules configured
- [ ] Service runs as non-root user
- [ ] File permissions set (700 for storage directory)
- [ ] HTTP security headers configured (nginx)
- [ ] Secret rotation procedure documented

### Monitoring
- [ ] Prometheus deployed and scraping metrics
- [ ] Grafana dashboards imported
- [ ] Alertmanager configured
- [ ] Critical alerts configured (storage errors, hash mismatches)
- [ ] On-call rotation established

### Backup & Recovery
- [ ] Backup strategy implemented
- [ ] S3 versioning enabled (if using S3)
- [ ] Disaster recovery plan documented
- [ ] Recovery tested in staging

### Capacity Planning
- [ ] Storage requirements estimated
- [ ] Compute resources allocated
- [ ] Network bandwidth provisioned
- [ ] Growth monitoring configured

### Operational Readiness
- [ ] Runbooks created for common issues
- [ ] Team trained on troubleshooting procedures
- [ ] Escalation process defined
- [ ] Maintenance windows scheduled

---

## Additional Resources

- [LFS Security Audit](LFS_SECURITY_AUDIT.md)
- [LFS Monitoring Guide](LFS_MONITORING.md)
- [LFS Configuration Reference](LFS_CONFIGURATION.md)
- [Monitoring Setup](../monitoring/README.md)
- [Git LFS Specification](https://github.com/git-lfs/git-lfs/tree/main/docs/api)

---

**Questions?** Contact support or file an issue on GitHub.

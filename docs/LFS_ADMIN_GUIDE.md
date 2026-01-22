# Git LFS Administration Guide

Complete guide for administrators managing a Git LFS server.

## Table of Contents

1. [Administrator Overview](#administrator-overview)
2. [Server Setup](#server-setup)
3. [User Management](#user-management)
4. [Monitoring and Logging](#monitoring-and-logging)
5. [Maintenance Tasks](#maintenance-tasks)
6. [Performance Tuning](#performance-tuning)
7. [Security Management](#security-management)
8. [Troubleshooting](#troubleshooting)
9. [Capacity Planning](#capacity-planning)
10. [Disaster Recovery](#disaster-recovery)

## Administrator Overview

### Responsibilities

As a Git LFS administrator, you are responsible for:

- **Server setup and configuration**
- **User SSH key management**
- **Storage management and monitoring**
- **Security (JWT secret rotation, access control)**
- **Performance optimization**
- **Backup and disaster recovery**
- **Troubleshooting user issues**
- **Capacity planning**

### Key Documentation

Before starting, familiarize yourself with:
- [LFS Configuration](LFS_CONFIGURATION.md) - Configuration reference
- [LFS Security Model](LFS_SECURITY.md) - Security architecture
- [LFS Storage Backend](LFS_STORAGE.md) - Storage configuration
- [LFS Troubleshooting](LFS_TROUBLESHOOTING.md) - Common issues

## Server Setup

### Initial Installation

**1. Build from source:**
```bash
git clone https://github.com/your-org/repos-mcp.git
cd repos-mcp

# Build with LFS feature
cargo build --release --features git_lfs

# Verify build
./target/release/repos-mcp --version
```

**2. Create system user:**
```bash
sudo useradd -r -s /bin/bash -d /var/lib/git-lfs -m git-lfs
sudo passwd -l git-lfs  # Lock password (SSH key only)
```

**3. Set up directories:**
```bash
# Create directories
sudo mkdir -p /etc/repos-mcp
sudo mkdir -p /var/lib/repos-mcp/repositories
sudo mkdir -p /var/log/repos-mcp

# Set ownership
sudo chown -R git-lfs:git-lfs /var/lib/repos-mcp
sudo chown -R git-lfs:git-lfs /var/log/repos-mcp

# Set permissions
sudo chmod 750 /var/lib/repos-mcp
sudo chmod 750 /var/log/repos-mcp
```

**4. Create configuration:**
```bash
sudo tee /etc/repos-mcp/config.yaml > /dev/null <<EOF
features:
  git_lfs: true

ssh_server:
  host: "0.0.0.0"
  port: 2222
  host_key_path: "/var/lib/repos-mcp/server_key"

api_server:
  host: "127.0.0.1"
  port: 8090

lfs:
  enabled: true
  jwt_secret: "$(openssl rand -base64 32)"
  base_url: "https://git.example.com"
  max_object_size: 5368709120
  token_expiration_secs: 3600
  storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"

repositories_path: "/var/lib/repos-mcp/repositories"
authorized_keys_path: "/var/lib/repos-mcp/.ssh/authorized_keys"
EOF

sudo chown root:git-lfs /etc/repos-mcp/config.yaml
sudo chmod 640 /etc/repos-mcp/config.yaml
```

**5. Generate SSH host key:**
```bash
sudo -u git-lfs ssh-keygen -t ed25519 -f /var/lib/repos-mcp/server_key -N ""
```

**6. Set up systemd service:**
```bash
sudo tee /etc/systemd/system/repos-mcp.service > /dev/null <<EOF
[Unit]
Description=Git LFS Repos Server
After=network.target

[Service]
Type=simple
User=git-lfs
Group=git-lfs
WorkingDirectory=/var/lib/repos-mcp
ExecStart=/usr/local/bin/repos-mcp --config /etc/repos-mcp/config.yaml
Restart=on-failure
RestartSec=10

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/repos-mcp /var/log/repos-mcp

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
```

**7. Set up reverse proxy (Nginx):**
```bash
sudo tee /etc/nginx/sites-available/git-lfs > /dev/null <<'EOF'
server {
    listen 443 ssl http2;
    server_name git.example.com;

    ssl_certificate /etc/ssl/certs/git.example.com.crt;
    ssl_certificate_key /etc/ssl/private/git.example.com.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;
    ssl_prefer_server_ciphers on;

    access_log /var/log/nginx/git-lfs-access.log;
    error_log /var/log/nginx/git-lfs-error.log;

    location /lfs/ {
        proxy_pass http://127.0.0.1:8090;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Important for large files
        client_max_body_size 0;
        proxy_request_buffering off;
        proxy_read_timeout 300s;
    }
}
EOF

sudo ln -s /etc/nginx/sites-available/git-lfs /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

**8. Start service:**
```bash
sudo systemctl start repos-mcp
sudo systemctl enable repos-mcp
sudo systemctl status repos-mcp
```

### Configuration Management

**View current configuration:**
```bash
sudo cat /etc/repos-mcp/config.yaml
```

**Reload after configuration changes:**
```bash
sudo systemctl restart repos-mcp
```

**Validate configuration:**
```bash
# Test SSH
ssh -T git@localhost -p 2222

# Test HTTP API (should return 405)
curl -v http://localhost:8090/lfs/test/repo/info/lfs
```

## User Management

### Adding Users

**1. Receive user's public SSH key** (user generates):
```bash
# User runs on their machine
ssh-keygen -t ed25519 -C "user@example.com"
cat ~/.ssh/id_ed25519.pub
```

**2. Add key to authorized_keys:**
```bash
# As admin, add to authorized_keys
echo "ssh-ed25519 AAAAC3Nz... user@example.com" | \
  sudo -u git-lfs tee -a /var/lib/repos-mcp/.ssh/authorized_keys

# Set permissions
sudo chmod 600 /var/lib/repos-mcp/.ssh/authorized_keys
sudo chown git-lfs:git-lfs /var/lib/repos-mcp/.ssh/authorized_keys
```

**3. Verify user access:**
```bash
# User tests connection
ssh -T git@git.example.com -p 2222
```

### Removing Users

**1. Remove SSH key from authorized_keys:**
```bash
# Edit file to remove user's key line
sudo -u git-lfs vim /var/lib/repos-mcp/.ssh/authorized_keys
```

**2. Verify removal:**
```bash
# User should no longer be able to connect
ssh -T git@git.example.com -p 2222
# Should fail with "Permission denied"
```

### Listing Users

**List all authorized keys:**
```bash
sudo cat /var/lib/repos-mcp/.ssh/authorized_keys | \
  grep -oP '(?<=# )[^ ]+@[^ ]+$' | \
  sort | \
  uniq
```

## Monitoring and Logging

### Log Files

**Systemd journal:**
```bash
# View recent logs
sudo journalctl -u repos-mcp -n 100 -f

# View logs for specific time period
sudo journalctl -u repos-mcp --since "1 hour ago"

# Export logs
sudo journalctl -u repos-mcp --since today > repos-mcp-$(date +%Y%m%d).log
```

**Nginx access logs:**
```bash
# LFS access log
sudo tail -f /var/log/nginx/git-lfs-access.log

# Filter for LFS operations
sudo grep "POST.*batch" /var/log/nginx/git-lfs-access.log
sudo grep "PUT.*lfs/objects" /var/log/nginx/git-lfs-access.log
```

### Monitoring Commands

**Check service status:**
```bash
sudo systemctl status repos-mcp
```

**Monitor disk space:**
```bash
# Overall usage
df -h /var/lib/repos-mcp

# LFS storage usage
du -sh /var/lib/repos-mcp/repositories/*/lfs/

# Per repository
du -sh /var/lib/repos-mcp/repositories/org/repo.git/lfs/
```

**Monitor active connections:**
```bash
# SSH connections
sudo netstat -tnp | grep :2222

# HTTP connections
sudo netstat -tnp | grep :8090
```

**Monitor system resources:**
```bash
# CPU and memory
top -p $(pgrep repos-mcp)

# Disk I/O
sudo iotop -p $(pgrep repos-mcp)
```

### Metrics to Track

**Daily monitoring:**
- Disk space utilization (alert at >80%)
- Service uptime
- Failed authentication attempts
- HTTP 5xx errors

**Weekly monitoring:**
- Storage growth trend
- Number of LFS objects
- Bandwidth usage
- User activity patterns

**Monthly monitoring:**
- Capacity planning metrics
- Security audit logs
- Backup verification
- Performance trends

### Alerting Setup

**Disk space alert:**
```bash
# Create cron job for disk space monitoring
sudo crontab -e

# Add:
*/30 * * * * [ $(df /var/lib/repos-mcp | awk 'NR==2 {print $5}' | sed 's/%//') -gt 80 ] && echo "LFS storage >80%" | mail -s "LFS Disk Alert" admin@example.com
```

**Service down alert:**
```bash
# Systemd OnFailure notification
sudo systemctl edit repos-mcp

# Add:
[Unit]
OnFailure=status-email@%n.service
```

## Maintenance Tasks

### Daily Tasks

**1. Check service health:**
```bash
sudo systemctl status repos-mcp
```

**2. Review error logs:**
```bash
sudo journalctl -u repos-mcp --since today | grep -i error
```

**3. Monitor disk space:**
```bash
df -h /var/lib/repos-mcp
```

### Weekly Tasks

**1. Review authentication failures:**
```bash
sudo journalctl -u repos-mcp --since "1 week ago" | grep -i "auth.*fail"
```

**2. Check storage growth:**
```bash
du -sh /var/lib/repos-mcp/repositories/*/lfs/ | sort -h
```

**3. Verify backups:**
```bash
# Ensure automated backups are running
ls -lh /backups/repos-mcp/
```

### Monthly Tasks

**1. Rotate JWT secret:**
```bash
# Generate new secret
NEW_SECRET=$(openssl rand -base64 32)

# Update config
sudo vim /etc/repos-mcp/config.yaml
# Update jwt_secret value

# Restart service
sudo systemctl restart repos-mcp
```

**2. Review SSH keys:**
```bash
# Audit authorized_keys
sudo cat /var/lib/repos-mcp/.ssh/authorized_keys
# Remove any old/unused keys
```

**3. Update software:**
```bash
cd /path/to/repos-mcp
git pull
cargo build --release --features git_lfs
sudo systemctl restart repos-mcp
```

**4. Verify object integrity:**
```bash
# Check sample of objects
for obj in $(find /var/lib/repos-mcp/repositories -name "*.git" -path "*/lfs/objects/*" | shuf -n 100); do
    oid=$(basename "$obj")
    computed=$(sha256sum "$obj" | cut -d' ' -f1)
    [ "$oid" != "$computed" ] && echo "CORRUPTION: $obj"
done
```

## Performance Tuning

### Server Configuration

**Increase concurrent connections:**
```yaml
# config.yaml (future feature)
server:
  max_connections: 1000
  connection_timeout_secs: 300
```

**Tune storage I/O:**
```bash
# Mount LFS storage with optimized options
sudo mount -o noatime,nodiratime /dev/sda1 /var/lib/repos-mcp

# Make permanent in /etc/fstab
echo "/dev/sda1 /var/lib/repos-mcp ext4 noatime,nodiratime 0 2" | sudo tee -a /etc/fstab
```

### Nginx Tuning

```nginx
# /etc/nginx/sites-available/git-lfs
location /lfs/ {
    # ... existing config ...
    
    # Performance tuning
    proxy_buffering off;
    proxy_request_buffering off;
    proxy_http_version 1.1;
    proxy_set_header Connection "";
    
    # Timeouts for large files
    proxy_connect_timeout 300s;
    proxy_send_timeout 300s;
    proxy_read_timeout 300s;
    
    # Buffer sizes
    proxy_buffer_size 128k;
    proxy_buffers 4 256k;
    proxy_busy_buffers_size 256k;
}
```

### System Limits

**Increase file descriptor limits:**
```bash
sudo tee -a /etc/security/limits.conf > /dev/null <<EOF
git-lfs soft nofile 65536
git-lfs hard nofile 65536
EOF
```

**Systemd service limits:**
```bash
sudo systemctl edit repos-mcp

# Add:
[Service]
LimitNOFILE=65536
```

## Security Management

### JWT Secret Rotation

**Rotate quarterly or after suspected compromise:**

```bash
# 1. Generate new secret
NEW_SECRET=$(openssl rand -base64 32)

# 2. Update configuration
sudo vim /etc/repos-mcp/config.yaml
# Change jwt_secret value

# 3. Restart service (invalidates all existing tokens)
sudo systemctl restart repos-mcp

# 4. Notify users (tokens auto-regenerate on next operation)
echo "JWT secret rotated on $(date)" | sudo tee -a /var/log/repos-mcp/security-audit.log
```

### SSH Key Audit

**Monthly audit procedure:**

```bash
# 1. List all keys with comments
sudo cat /var/lib/repos-mcp/.ssh/authorized_keys | grep "^ssh-"

# 2. Verify each key with user
# Contact users to confirm key ownership

# 3. Remove unauthorized keys
sudo vim /var/lib/repos-mcp/.ssh/authorized_keys

# 4. Log audit
echo "SSH key audit completed: $(date)" | sudo tee -a /var/log/repos-mcp/security-audit.log
```

### Access Logging

**Enable comprehensive access logging** (future feature):

```yaml
# config.yaml
logging:
  access_log: "/var/log/repos-mcp/access.log"
  auth_log: "/var/log/repos-mcp/auth.log"
  level: "info"
```

**Review access logs:**
```bash
# Failed authentication attempts
sudo grep "403\|401" /var/log/nginx/git-lfs-access.log

# Unusual access patterns
sudo awk '{print $1}' /var/log/nginx/git-lfs-access.log | sort | uniq -c | sort -rn | head
```

## Troubleshooting

See [LFS_TROUBLESHOOTING.md](LFS_TROUBLESHOOTING.md) for comprehensive troubleshooting.

### Quick Diagnostics

**Service not starting:**
```bash
# Check logs
sudo journalctl -u repos-mcp -n 50

# Check configuration syntax
sudo /usr/local/bin/repos-mcp --config /etc/repos-mcp/config.yaml --check-config

# Check file permissions
ls -la /var/lib/repos-mcp
ls -la /etc/repos-mcp/config.yaml
```

**Users cannot authenticate:**
```bash
# Check SSH service
sudo systemctl status repos-mcp

# Verify authorized_keys format
sudo cat /var/lib/repos-mcp/.ssh/authorized_keys

# Check permissions
ls -la /var/lib/repos-mcp/.ssh/authorized_keys
# Should be: -rw------- git-lfs git-lfs
```

**Upload/download failures:**
```bash
# Check disk space
df -h /var/lib/repos-mcp

# Verify JWT secret configured
sudo grep jwt_secret /etc/repos-mcp/config.yaml

# Test token generation
ssh git@localhost -p 2222 git-lfs-authenticate org/repo upload

# Check Nginx logs
sudo tail -f /var/log/nginx/git-lfs-error.log
```

## Capacity Planning

### Storage Estimation

**Calculate current usage:**
```bash
# Total LFS storage
du -sh /var/lib/repos-mcp/repositories/*/lfs/

# Number of LFS objects
find /var/lib/repos-mcp/repositories -path "*/lfs/objects/*" -type f | wc -l

# Average object size
total_size=$(du -sb /var/lib/repos-mcp/repositories/*/lfs/ | awk '{s+=$1} END {print s}')
object_count=$(find /var/lib/repos-mcp/repositories -path "*/lfs/objects/*" -type f | wc -l)
echo "scale=2; $total_size / $object_count / 1024 / 1024" | bc
# Result in MB
```

### Growth Projection

**Calculate growth rate:**
```bash
# Compare storage over 1 week
WEEK_AGO=$(du -sb /var/lib/repos-mcp/repositories/*/lfs/ | awk '{s+=$1} END {print s}')
# Wait 1 week
NOW=$(du -sb /var/lib/repos-mcp/repositories/*/lfs/ | awk '{s+=$1} END {print s}')

GROWTH=$(echo "scale=2; ($NOW - $WEEK_AGO) / $WEEK_AGO * 100" | bc)
echo "Weekly growth: ${GROWTH}%"
```

### Capacity Recommendations

| Usage Level | Recommendation |
|-------------|----------------|
| < 50GB | Single disk adequate |
| 50-500GB | Dedicated disk or partition |
| 500GB-2TB | RAID array or SSD |
| 2TB+ | Consider S3/cloud storage |

**Expand storage when:**
- Utilization > 70%
- Growth rate > 10% per week
- Expected to reach capacity in < 3 months

## Disaster Recovery

### Backup Strategy

**Full backup (weekly):**
```bash
#!/bin/bash
# /usr/local/bin/backup-lfs.sh

DATE=$(date +%Y%m%d)
BACKUP_DIR="/backups/repos-mcp"
SOURCE="/var/lib/repos-mcp"

# Create backup
tar -czf "${BACKUP_DIR}/lfs-full-${DATE}.tar.gz" \
  "${SOURCE}/repositories/*/lfs/" \
  /etc/repos-mcp/

# Verify backup
tar -tzf "${BACKUP_DIR}/lfs-full-${DATE}.tar.gz" > /dev/null
if [ $? -eq 0 ]; then
    echo "Backup successful: ${DATE}"
else
    echo "Backup FAILED: ${DATE}" | mail -s "LFS Backup Failure" admin@example.com
fi

# Remove backups older than 30 days
find "${BACKUP_DIR}" -name "lfs-full-*.tar.gz" -mtime +30 -delete
```

**Incremental backup (daily):**
```bash
#!/bin/bash
# /usr/local/bin/backup-lfs-incremental.sh

DATE=$(date +%Y%m%d)
BACKUP_DIR="/backups/repos-mcp/incremental"
SOURCE="/var/lib/repos-mcp/repositories"

# Rsync incremental backup
rsync -av --link-dest="${BACKUP_DIR}/latest" \
  "${SOURCE}/" \
  "${BACKUP_DIR}/${DATE}/"

# Update latest symlink
ln -sfn "${BACKUP_DIR}/${DATE}" "${BACKUP_DIR}/latest"
```

**Schedule backups:**
```bash
sudo crontab -e

# Add:
0 2 * * 0 /usr/local/bin/backup-lfs.sh        # Weekly full backup
0 3 * * 1-6 /usr/local/bin/backup-lfs-incremental.sh  # Daily incremental
```

### Restore Procedures

**Full restore:**
```bash
# 1. Stop service
sudo systemctl stop repos-mcp

# 2. Extract backup
sudo tar -xzf /backups/repos-mcp/lfs-full-20260122.tar.gz -C /

# 3. Fix permissions
sudo chown -R git-lfs:git-lfs /var/lib/repos-mcp

# 4. Start service
sudo systemctl start repos-mcp

# 5. Verify
ssh git@localhost -p 2222 git-lfs-authenticate org/repo upload
```

**Single repository restore:**
```bash
# Extract specific repository
sudo tar -xzf /backups/repos-mcp/lfs-full-20260122.tar.gz \
  -C / \
  var/lib/repos-mcp/repositories/org/repo.git/lfs/

sudo chown -R git-lfs:git-lfs /var/lib/repos-mcp/repositories/org/repo.git/lfs/
```

### Disaster Scenarios

**Server failure:**
1. Provision new server
2. Install repos-mcp
3. Restore configuration from backup
4. Restore LFS storage from backup
5. Update DNS if needed
6. Notify users

**Storage corruption:**
1. Identify corrupted objects
2. Restore from backup
3. Verify object integrity
4. Notify affected repositories

**Security breach:**
1. Immediately rotate JWT secret
2. Audit access logs
3. Remove compromised SSH keys
4. Review all recent commits
5. Restore from pre-breach backup if needed

## Related Documentation

- [LFS Configuration](LFS_CONFIGURATION.md) - Configuration reference
- [LFS Security Model](LFS_SECURITY.md) - Security documentation
- [LFS Storage Backend](LFS_STORAGE.md) - Storage details
- [LFS Troubleshooting](LFS_TROUBLESHOOTING.md) - Issue resolution

## Administrator Checklist

### Initial Setup
- [ ] Server installed and configured
- [ ] JWT secret generated (32+ bytes)
- [ ] SSH host key generated
- [ ] TLS certificate configured (reverse proxy)
- [ ] Firewall rules configured
- [ ] Systemd service enabled
- [ ] Backup solution configured
- [ ] Monitoring configured

### Daily Operations
- [ ] Check service status
- [ ] Review error logs
- [ ] Monitor disk space
- [ ] Verify backups completed

### Weekly Operations
- [ ] Review authentication failures
- [ ] Check storage growth
- [ ] Audit access patterns
- [ ] Test restore procedure (monthly)

### Monthly Operations
- [ ] Rotate JWT secret
- [ ] Audit SSH keys
- [ ] Update software
- [ ] Review capacity planning
- [ ] Security audit
- [ ] Verify object integrity (sample)

### Quarterly Operations
- [ ] Full disaster recovery test
- [ ] Review and update documentation
- [ ] Security assessment
- [ ] Performance review and optimization
- [ ] Capacity expansion planning

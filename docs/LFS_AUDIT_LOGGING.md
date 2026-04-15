# Git LFS Audit Logging

Comprehensive audit logging system for tracking all Git LFS operations to support compliance requirements (SOC2, GDPR, HIPAA).

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Configuration](#configuration)
- [Event Types](#event-types)
- [Query API](#query-api)
- [Storage Backends](#storage-backends)
- [Compliance Considerations](#compliance-considerations)
- [Integration Examples](#integration-examples)
- [Troubleshooting](#troubleshooting)

## Overview

The LFS audit logging system records all LFS operations including uploads, downloads, batch requests, and errors. Each event is structured with consistent fields for easy querying and analysis.

**Key Benefits:**
- Track all LFS operations for security and compliance
- Query logs by tenant, repository, time range, and event type
- Privacy controls for GDPR compliance
- Configurable retention and filtering
- Non-blocking async logging for performance

## Features

✅ **Comprehensive Event Tracking**
- Batch requests (upload/download operations)
- Individual file uploads and downloads
- Quota exceeded events
- Verification failures
- Error conditions

✅ **Flexible Querying**
- Filter by tenant and repository
- Time range filtering (RFC3339 timestamps)
- Configurable result limits
- RESTful HTTP API

✅ **Privacy & Compliance**
- Optional IP address logging (GDPR)
- Configurable retention periods
- Filter by operation success/failure
- Structured JSON format for log aggregation

✅ **Performance**
- Async fire-and-forget logging (non-blocking)
- Thread-safe concurrent access
- Automatic memory management

## Configuration

### Basic Configuration

Add to your `config.yaml`:

```yaml
lfs:
  enabled: true
  # ... other LFS settings ...
  
  audit:
    enabled: true
    retention_days: 90
    include_ip_addresses: true
    log_successful_operations: true
    log_failed_operations: true
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `false` | Enable/disable audit logging |
| `retention_days` | integer | `90` | How long to retain audit logs (days) |
| `include_ip_addresses` | boolean | `true` | Log client IP addresses |
| `log_successful_operations` | boolean | `true` | Log successful operations |
| `log_failed_operations` | boolean | `true` | Log failed operations |

### Example Configurations

**Compliance-Focused (Maximum Logging)**
```yaml
audit:
  enabled: true
  retention_days: 365
  include_ip_addresses: true
  log_successful_operations: true
  log_failed_operations: true
```

**Privacy-Focused (GDPR)**
```yaml
audit:
  enabled: true
  retention_days: 30
  include_ip_addresses: false
  log_successful_operations: false
  log_failed_operations: true
```

**Debug Mode (Errors Only)**
```yaml
audit:
  enabled: true
  retention_days: 7
  include_ip_addresses: true
  log_successful_operations: false
  log_failed_operations: true
```

## Event Types

### AuditEventType Enum

| Event Type | Description | When Triggered |
|------------|-------------|----------------|
| `LfsBatchRequest` | Batch API request | Client requests batch upload/download |
| `LfsUpload` | File upload | Object successfully uploaded |
| `LfsDownload` | File download | Object successfully downloaded |
| `LfsDelete` | File deletion | Object deleted (future) |
| `LfsQuotaExceeded` | Quota limit hit | Upload exceeds quota |
| `LfsVerifyFailed` | Verification failed | SHA256 hash mismatch |
| `LfsError` | General error | Storage error, not found, etc. |

### Event Structure

Each audit event contains:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2026-01-22T16:30:00Z",
  "event_type": "LfsUpload",
  "tenant": "acme-corp",
  "repo": "my-project",
  "user": "alice@example.com",
  "ip_address": "203.0.113.42",
  "oid": "abc123def456...",
  "size": 1048576,
  "success": true,
  "error_message": null,
  "metadata": {
    "operation": "upload",
    "user_agent": "git-lfs/2.13.3"
  }
}
```

### Field Descriptions

| Field | Type | Optional | Description |
|-------|------|----------|-------------|
| `id` | UUID | No | Unique event identifier |
| `timestamp` | DateTime | No | Event timestamp (UTC) |
| `event_type` | String | No | Type of event (see above) |
| `tenant` | String | No | Tenant identifier |
| `repo` | String | No | Repository name |
| `user` | String | Yes | Username (if authenticated) |
| `ip_address` | String | Yes | Client IP (if enabled) |
| `oid` | String | Yes | LFS object OID (SHA256) |
| `size` | Integer | Yes | Object size in bytes |
| `success` | Boolean | No | Operation success status |
| `error_message` | String | Yes | Error details (if failed) |
| `metadata` | Object | Yes | Additional key-value data |

## Query API

### Endpoint

```
GET /lfs/audit/logs
```

### Query Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `tenant` | string | No | Filter by tenant |
| `repo` | string | No | Filter by repository |
| `start_time` | RFC3339 | No | Start of time range |
| `end_time` | RFC3339 | No | End of time range |
| `limit` | integer | No | Max results (default: 100, max: 1000) |

### Example Requests

**All Events (Last 100)**
```bash
curl http://localhost:8090/lfs/audit/logs
```

**Filter by Tenant**
```bash
curl "http://localhost:8090/lfs/audit/logs?tenant=acme-corp"
```

**Filter by Repository**
```bash
curl "http://localhost:8090/lfs/audit/logs?tenant=acme-corp&repo=my-project"
```

**Time Range Query**
```bash
curl "http://localhost:8090/lfs/audit/logs?start_time=2026-01-01T00:00:00Z&end_time=2026-01-31T23:59:59Z"
```

**Combined Filters**
```bash
curl "http://localhost:8090/lfs/audit/logs?tenant=acme-corp&repo=my-project&start_time=2026-01-20T00:00:00Z&limit=50"
```

### Response Format

**Success Response**
```json
{
  "enabled": true,
  "events": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "timestamp": "2026-01-22T16:30:00Z",
      "event_type": "LfsUpload",
      "tenant": "acme-corp",
      "repo": "my-project",
      "oid": "abc123...",
      "size": 1048576,
      "success": true
    }
  ],
  "count": 1,
  "limit": 100
}
```

**Disabled Response**
```json
{
  "enabled": false,
  "message": "Audit logging is not enabled"
}
```

## Storage Backends

### In-Memory Storage (Default)

**Current Implementation:**
- Thread-safe with RwLock
- Automatic pruning when capacity exceeded
- Default capacity: 10,000 events
- **Not persistent** - logs lost on restart

**Use Cases:**
- Development and testing
- Small deployments with low traffic
- Temporary audit logging

### Production Storage Options

⚠️ **Important:** The in-memory storage is NOT suitable for production. Choose a persistent backend:

#### 1. Database Storage (Recommended)

**PostgreSQL Example:**
```rust
pub struct PostgresAuditStorage {
    pool: PgPool,
}

#[async_trait::async_trait]
impl AuditStorage for PostgresAuditStorage {
    async fn store(&self, event: AuditEvent) -> Result<(), AuditError> {
        sqlx::query!(
            "INSERT INTO audit_logs (id, timestamp, event_type, tenant, repo, oid, size, success) 
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            event.id, event.timestamp, event.event_type, event.tenant, event.repo,
            event.oid, event.size, event.success
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
    
    async fn query(...) -> Result<Vec<AuditEvent>, AuditError> {
        // Query implementation
    }
}
```

**Benefits:**
- Durable storage
- Advanced querying (indexes, full-text search)
- Scalable for large volumes
- Backup and replication support

#### 2. Log Aggregation Systems

**ELK Stack (Elasticsearch, Logstash, Kibana)**
```rust
pub struct ElasticsearchAuditStorage {
    client: Elasticsearch,
}

// Ship events to Elasticsearch for:
// - Centralized logging
// - Advanced search and analytics
// - Visual dashboards (Kibana)
// - Long-term retention with lifecycle management
```

**Grafana Loki**
```rust
pub struct LokiAuditStorage {
    endpoint: String,
}

// Stream events to Loki for:
// - Cost-effective log storage
// - Integration with Grafana dashboards
// - Label-based querying
```

#### 3. Cloud Services

**AWS CloudWatch Logs**
- Integrated AWS logging
- Retention policies
- CloudWatch Insights queries

**Azure Monitor**
- Azure-native logging
- Log Analytics workspace
- KQL querying

**GCP Cloud Logging**
- Google Cloud logging
- Log sinks and exports
- BigQuery integration

## Compliance Considerations

### SOC 2 Compliance

**Required Audit Trail:**
✅ Track all access to LFS objects (uploads/downloads)
✅ Record timestamps and user identities
✅ Log security-relevant events (failures, quota exceeded)
✅ Retain logs for audit period (typically 1 year)
✅ Protect logs from tampering (immutable storage)

**Configuration:**
```yaml
audit:
  enabled: true
  retention_days: 365
  include_ip_addresses: true
  log_successful_operations: true
  log_failed_operations: true
```

### GDPR Compliance

**Personal Data Considerations:**
- IP addresses are personal data
- User identifiers may be personal data
- Right to erasure (delete user's logs)
- Data minimization (log only what's needed)

**Privacy-Friendly Configuration:**
```yaml
audit:
  enabled: true
  retention_days: 30
  include_ip_addresses: false  # Don't log IPs
  log_successful_operations: false  # Only log issues
  log_failed_operations: true
```

**Recommendations:**
1. Document audit logging in privacy policy
2. Implement log anonymization for old data
3. Provide user data export capability
4. Honor deletion requests (RTBF)

### HIPAA Compliance

**Access Logging Requirements:**
✅ Log all access to ePHI (if LFS stores health data)
✅ Record user identity, timestamp, action
✅ Retain logs for 6 years
✅ Ensure logs are tamper-proof
✅ Regular audit log reviews

**Configuration:**
```yaml
audit:
  enabled: true
  retention_days: 2190  # 6 years
  include_ip_addresses: true
  log_successful_operations: true
  log_failed_operations: true
```

## Integration Examples

### Prometheus Metrics Integration

Query audit logs for metrics:

```bash
# Count failed operations in last hour
curl "http://localhost:8090/lfs/audit/logs?start_time=$(date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%SZ)&limit=1000" \
  | jq '[.events[] | select(.success == false)] | length'
```

### Alerting with Grafana

Create alerts based on audit log queries:

```yaml
# Alert on high error rate
- alert: LfsHighErrorRate
  expr: |
    rate(lfs_error_total[5m]) > 0.1
  annotations:
    summary: "High LFS error rate detected"
    description: "LFS error rate is {{ $value }} errors/sec"
```

### Log Export Script

Export audit logs to file:

```bash
#!/bin/bash
# export-audit-logs.sh

START_DATE="2026-01-01T00:00:00Z"
END_DATE="2026-01-31T23:59:59Z"
TENANT="acme-corp"

curl "http://localhost:8090/lfs/audit/logs?tenant=${TENANT}&start_time=${START_DATE}&end_time=${END_DATE}&limit=1000" \
  | jq '.events[]' \
  > audit-logs-${TENANT}-$(date +%Y%m%d).jsonl
```

### Automated Compliance Reports

Generate monthly compliance report:

```python
#!/usr/bin/env python3
import requests
from datetime import datetime, timedelta
import json

def generate_compliance_report(tenant, month):
    start = datetime(month.year, month.month, 1)
    end = (start + timedelta(days=32)).replace(day=1) - timedelta(seconds=1)
    
    response = requests.get(
        f"http://localhost:8090/lfs/audit/logs",
        params={
            "tenant": tenant,
            "start_time": start.isoformat() + "Z",
            "end_time": end.isoformat() + "Z",
            "limit": 10000
        }
    )
    
    events = response.json()["events"]
    
    report = {
        "tenant": tenant,
        "period": f"{start.date()} to {end.date()}",
        "total_operations": len(events),
        "uploads": len([e for e in events if e["event_type"] == "LfsUpload"]),
        "downloads": len([e for e in events if e["event_type"] == "LfsDownload"]),
        "errors": len([e for e in events if not e["success"]]),
        "unique_users": len(set(e.get("user") for e in events if e.get("user"))),
    }
    
    print(json.dumps(report, indent=2))

if __name__ == "__main__":
    generate_compliance_report("acme-corp", datetime(2026, 1, 1))
```

## Troubleshooting

### Issue: Audit logs not appearing

**Check 1: Is audit logging enabled?**
```bash
curl http://localhost:8090/lfs/audit/logs
# Should return "enabled": true
```

**Check 2: Configuration**
```yaml
lfs:
  audit:
    enabled: true  # Must be true
```

**Check 3: Logs for errors**
```bash
journalctl -u agentic-repos | grep -i audit
```

### Issue: Query returns empty results

**Verify time range:**
```bash
# Make sure timestamps are in RFC3339 format
curl "http://localhost:8090/lfs/audit/logs?start_time=2026-01-22T00:00:00Z"
```

**Check filters:**
```bash
# Remove filters one by one to find the issue
curl "http://localhost:8090/lfs/audit/logs"
curl "http://localhost:8090/lfs/audit/logs?tenant=test"
```

### Issue: Performance degradation

**Symptom:** Slow LFS operations

**Cause:** Audit logging blocking operations

**Solution:** Audit logging uses async fire-and-forget pattern, but check:

1. **Storage backend performance**
   ```bash
   # Check if storage backend is slow
   curl -w "@curl-format.txt" http://localhost:8090/lfs/audit/logs
   ```

2. **In-memory storage capacity**
   ```rust
   // Increase capacity if frequently pruning
   InMemoryAuditStorage::new(50000)  // Default: 10000
   ```

3. **Disable successful operations logging**
   ```yaml
   audit:
     log_successful_operations: false  # Only log errors
   ```

### Issue: Logs lost on restart

**Cause:** Using in-memory storage (not persistent)

**Solution:** Implement persistent storage backend:
- PostgreSQL / MySQL
- Elasticsearch
- Cloud logging service

See [Storage Backends](#storage-backends) section.

### Issue: Logs consuming too much memory

**Solution 1: Reduce retention**
```yaml
audit:
  retention_days: 7  # Shorter retention
```

**Solution 2: Filter events**
```yaml
audit:
  log_successful_operations: false  # Reduce volume
```

**Solution 3: Use persistent storage**
Switch to database or log aggregation system.

## Best Practices

### 1. Choose Appropriate Retention

```yaml
# Development
retention_days: 7

# Small production
retention_days: 30

# Compliance-driven
retention_days: 365  # SOC2
retention_days: 2190  # HIPAA (6 years)
```

### 2. Balance Privacy and Security

**High Security Environments:**
```yaml
include_ip_addresses: true
log_successful_operations: true
log_failed_operations: true
```

**Privacy-Focused:**
```yaml
include_ip_addresses: false
log_successful_operations: false
log_failed_operations: true
```

### 3. Use Persistent Storage in Production

Never use in-memory storage in production:
- Data loss on restart
- Limited capacity
- No backup/replication
- Can't survive crashes

### 4. Monitor Audit Log Health

Create alerts for:
- Audit logging failures
- Storage capacity issues
- Query performance degradation
- Unusual activity patterns

### 5. Regular Compliance Reviews

Schedule periodic reviews:
- Weekly: Check for errors and anomalies
- Monthly: Generate compliance reports
- Quarterly: Audit retention and privacy compliance
- Annually: Full audit log analysis

### 6. Secure the Audit Logs

- Restrict query API access (authentication/authorization)
- Encrypt logs at rest
- Use immutable storage where possible
- Regular backups with tested restore procedures

### 7. Test Your Audit System

```bash
# Smoke test
curl -X POST http://localhost:8090/lfs/tenant/repo/info/lfs/objects/batch -d '...'
curl http://localhost:8090/lfs/audit/logs?tenant=tenant&repo=repo | jq '.events | length'

# Should return > 0 events
```

## Additional Resources

- [LFS Configuration Reference](LFS_CONFIGURATION.md)
- [LFS Monitoring Guide](LFS_MONITORING.md)
- [LFS Production Deployment](LFS_PRODUCTION_DEPLOYMENT.md)
- [Git LFS Specification](https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md)

## Support

For issues or questions about audit logging:
1. Check this documentation
2. Review the [troubleshooting section](#troubleshooting)
3. Check the GitHub issues
4. Contact support at shift@someone.section.me

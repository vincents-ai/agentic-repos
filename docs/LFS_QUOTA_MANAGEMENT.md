# Git LFS Storage Quota Management

Complete guide to configuring and managing storage quotas for Git LFS in agentic-repos.

## Overview

The quota system enforces storage limits per tenant and repository, preventing runaway storage consumption and enabling fair resource allocation across tenants.

### Key Features

- **Hierarchical quotas**: Repository-specific > Tenant-specific > Default
- **Real-time enforcement**: Quota checked before allowing uploads
- **Usage tracking**: Automatic tracking of bytes used and object count
- **Prometheus metrics**: Monitor quota usage and violations
- **REST API**: Query quota status programmatically

---

## Configuration

### Enable Quota Enforcement

Add to `config.yaml`:

```yaml
lfs:
  enabled: true
  quota:
    enabled: true
    default_quota_bytes: 10737418240  # 10 GB default
    per_tenant_quotas:
      tenant1: 53687091200  # 50 GB for tenant1
      tenant2: 21474836480  # 20 GB for tenant2
    per_repo_quotas:
      tenant1/repo1: 107374182400  # 100 GB for specific repo
      tenant2/repo2: 10737418240   # 10 GB override
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `false` | Enable quota enforcement |
| `default_quota_bytes` | integer | `10737418240` (10GB) | Default quota for all repos |
| `per_tenant_quotas` | map | `{}` | Per-tenant quota overrides |
| `per_repo_quotas` | map | `{}` | Per-repository quota overrides (format: `tenant/repo`) |

### Quota Precedence

Quotas are applied in this order:

1. **Repository-specific quota** (`per_repo_quotas["tenant/repo"]`)
2. **Tenant-specific quota** (`per_tenant_quotas["tenant"]`)
3. **Default quota** (`default_quota_bytes`)

---

## Usage

### Check Quota Status

Query current quota usage for a repository:

```bash
curl http://localhost:8090/lfs/tenant1/repo1/quota
```

Response:

```json
{
  "enabled": true,
  "tenant": "tenant1",
  "repo": "repo1",
  "limit_bytes": 10737418240,
  "used_bytes": 2147483648,
  "available_bytes": 8589934592,
  "object_count": 42,
  "usage_percent": 20.0
}
```

### Quota Exceeded Error

When a quota is exceeded, the batch API returns an error for affected objects:

```bash
curl -X POST http://localhost:8090/lfs/tenant1/repo1/info/lfs/objects/batch \
  -H "Content-Type: application/vnd.git-lfs+json" \
  -d '{
    "operation": "upload",
    "objects": [{"oid": "abc123...", "size": 999999999999}]
  }'
```

Response:

```json
{
  "transfer": "basic",
  "objects": [
    {
      "oid": "abc123...",
      "size": 999999999999,
      "authenticated": true,
      "error": {
        "code": 507,
        "message": "Storage quota exceeded: Quota exceeded for tenant1/repo1: 2147483648 bytes used, 10737418240 bytes limit"
      }
    }
  ]
}
```

HTTP Status: `200 OK` (per LFS spec, errors are per-object)

---

## Monitoring

### Prometheus Metrics

The quota system exposes metrics for monitoring:

#### `lfs_quota_bytes_used`

Current storage bytes used per tenant/repo.

```promql
lfs_quota_bytes_used{tenant="tenant1",repo="repo1"}
```

#### `lfs_quota_objects_count`

Current number of LFS objects per tenant/repo.

```promql
lfs_quota_objects_count{tenant="tenant1",repo="repo1"}
```

#### `lfs_quota_exceeded_total`

Total number of quota exceeded errors.

```promql
rate(lfs_quota_exceeded_total{tenant="tenant1",repo="repo1"}[5m])
```

### Example Queries

**Quota usage by tenant:**

```promql
sum(lfs_quota_bytes_used) by (tenant)
```

**Repos near quota limit (>80%):**

Requires combining with config data externally since limits are static config.

**Quota exceeded rate:**

```promql
sum(rate(lfs_quota_exceeded_total[5m])) by (tenant, repo)
```

### Grafana Dashboard

Add these panels to your LFS dashboard:

**Quota Usage Gauge:**

```promql
(lfs_quota_bytes_used / <configured_limit>) * 100
```

**Quota Exceeded Rate:**

```promql
rate(lfs_quota_exceeded_total[5m])
```

---

## Alerting

### Recommended Alerts

**High Quota Usage:**

```yaml
- alert: LFSQuotaHighUsage
  expr: (lfs_quota_bytes_used / <limit>) > 0.8
  for: 10m
  labels:
    severity: warning
  annotations:
    summary: "LFS quota usage >80% for {{ $labels.tenant }}/{{ $labels.repo }}"
    description: "Repository is at {{ $value }}% of quota"
```

**Quota Exceeded Errors:**

```yaml
- alert: LFSQuotaExceeded
  expr: rate(lfs_quota_exceeded_total[5m]) > 0
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "LFS quota exceeded for {{ $labels.tenant }}/{{ $labels.repo }}"
    description: "Users are hitting quota limits ({{ $value }} errors/sec)"
```

---

## Operations

### Increase Quota

1. Update `config.yaml` with new quota value
2. Restart agentic-repos server
3. Verify with quota API endpoint

No data migration needed.

### Decrease Quota

**Warning**: Decreasing quota below current usage will prevent new uploads but won't delete existing objects.

1. Run garbage collection first to free space (see `LFS_GC.md`)
2. Update `config.yaml` with new quota value
3. Restart agentic-repos server
4. Monitor for quota exceeded errors

### Disable Quotas

Set `lfs.quota.enabled: false` in `config.yaml` and restart.

Existing usage data is preserved but not enforced.

---

## Troubleshooting

### Users Can't Upload Despite Available Quota

**Symptoms**: Quota API shows available space, but uploads fail with quota exceeded.

**Cause**: In-memory usage cache may be stale.

**Solution**: Restart the agentic-repos server to refresh usage cache from storage.

### Quota Shows 0 Usage Despite Objects

**Symptoms**: Quota API reports 0 bytes used, but objects exist in storage.

**Cause**: Usage is tracked incrementally on upload. Pre-existing objects aren't counted.

**Solution**: Implement usage refresh on server startup (future enhancement).

### Quota Not Enforced

**Checklist**:

1. Is `lfs.quota.enabled: true` in config?
2. Is the LFS feature flag enabled?
3. Was the server restarted after config change?
4. Check logs for quota manager initialization errors

---

## Best Practices

### Sizing Quotas

**Development environments:**
- Default: 5-10 GB per repo
- Sufficient for code + medium assets

**Production environments:**
- Default: 20-50 GB per repo
- Monitor actual usage and adjust

**Large media projects:**
- Per-repo overrides: 100-500 GB
- Consider S3 storage backend

### Monitoring Strategy

1. **Set up alerts** for >80% quota usage
2. **Track quota exceeded rate** to identify problematic repos
3. **Monitor tenant-level totals** for capacity planning
4. **Review quota usage monthly** and adjust limits

### Multi-Tenant Management

**Fair allocation:**

```yaml
per_tenant_quotas:
  free_tier: 5368709120      # 5 GB
  paid_tier: 53687091200     # 50 GB
  enterprise: 536870912000   # 500 GB
```

**Project-based:**

```yaml
per_repo_quotas:
  acme/mobile-app: 107374182400   # 100 GB (video assets)
  acme/web-app: 10737418240       # 10 GB (images only)
  acme/docs: 1073741824           # 1 GB (minimal)
```

---

## Architecture

### Usage Tracking

- **In-memory cache**: Fast quota checks with `RwLock`
- **Updated on upload**: Incremented after successful PUT
- **Thread-safe**: Supports concurrent uploads
- **Eventually consistent**: Cache may be slightly behind actual storage

### Quota Check Flow

```
1. Client requests upload via Batch API
2. Server checks quota: current_usage + object_size <= limit
3. If exceeded: return error in batch response
4. If OK: provide upload URL
5. On successful upload: increment usage counter
6. Update Prometheus metrics
```

### Limitations

**Current implementation:**

- Usage tracking is in-memory (lost on restart)
- No persistent usage database
- No automatic usage refresh from storage
- No quota enforcement on direct storage writes

**Future enhancements:**

- Persistent usage database (SQLite/PostgreSQL)
- Periodic usage refresh from storage
- Quota enforcement in storage backend layer
- Admin API for setting quotas dynamically

---

## API Reference

### GET /lfs/{tenant}/{repo}/quota

Get quota status for a repository.

**Response:**

```json
{
  "enabled": true,
  "tenant": "string",
  "repo": "string",
  "limit_bytes": 0,
  "used_bytes": 0,
  "available_bytes": 0,
  "object_count": 0,
  "usage_percent": 0.0
}
```

**Status Codes:**

- `200 OK`: Success
- `503 Service Unavailable`: LFS disabled

---

## Example Configurations

### Small Team (5-10 developers)

```yaml
lfs:
  quota:
    enabled: true
    default_quota_bytes: 10737418240  # 10 GB per repo
```

### SaaS Platform (Multi-Tenant)

```yaml
lfs:
  quota:
    enabled: true
    default_quota_bytes: 5368709120  # 5 GB default
    per_tenant_quotas:
      free_tier_tenant: 5368709120      # 5 GB
      pro_tier_tenant: 53687091200      # 50 GB
      enterprise_tenant: 536870912000   # 500 GB
```

### Media Production Studio

```yaml
lfs:
  quota:
    enabled: true
    default_quota_bytes: 53687091200  # 50 GB default
    per_repo_quotas:
      studio/feature-film: 1099511627776   # 1 TB (4K footage)
      studio/commercials: 107374182400     # 100 GB
      studio/archives: 5497558138880       # 5 TB (old projects)
```

---

## Security Considerations

### Quota Bypass Prevention

- Quotas are enforced server-side only
- No client-side quota checking
- Upload URLs expire after 1 hour (prevents hoarding)
- Hash verification prevents storage corruption attacks

### DoS Protection

- Quota system prevents storage exhaustion attacks
- Consider rate limiting on batch API (separate feature)
- Monitor `lfs_quota_exceeded_total` for abuse patterns

### Privacy

- Quota data includes no file content or names
- Usage metrics are per-repo aggregate only
- Consider GDPR requirements for usage logs

---

## Related Documentation

- [LFS Configuration Reference](LFS_CONFIGURATION.md)
- [LFS Monitoring Guide](LFS_MONITORING.md)
- [LFS Garbage Collection](LFS_GC.md) (when implemented)
- [Production Deployment Guide](LFS_PRODUCTION_DEPLOYMENT.md)

---

## Support

For issues or questions:

- GitHub Issues: https://github.com/vincents-ai/agentic-repos/issues
- Security issues: See [SECURITY.md](../SECURITY.md)

# Git LFS Monitoring Guide

This document describes the monitoring and observability capabilities for Git LFS operations in agentic-repos.

## Metrics Collection

agentic-repos exposes Prometheus-compatible metrics for Git LFS operations. All metrics are available at the `/metrics` endpoint.

### Available Metrics

#### Upload Metrics

**`lfs_uploads_total`** (Counter)
- Total number of LFS object uploads
- Labels: `tenant`, `repo`
- Incremented on successful PUT to `/lfs/{tenant}/{repo}/lfs/objects/{oid}`

**`lfs_upload_bytes_total`** (Counter)
- Total bytes uploaded to LFS storage
- Labels: `tenant`, `repo`
- Tracks cumulative data transfer for uploads

**`lfs_upload_duration_seconds`** (Histogram)
- Duration of LFS upload operations in seconds
- Labels: `tenant`, `repo`
- Buckets: Default Prometheus histogram buckets

#### Download Metrics

**`lfs_downloads_total`** (Counter)
- Total number of LFS object downloads
- Labels: `tenant`, `repo`
- Incremented on successful GET from `/lfs/{tenant}/{repo}/lfs/objects/{oid}`

**`lfs_download_bytes_total`** (Counter)
- Total bytes downloaded from LFS storage
- Labels: `tenant`, `repo`
- Tracks cumulative data transfer for downloads

**`lfs_download_duration_seconds`** (Histogram)
- Duration of LFS download operations in seconds
- Labels: `tenant`, `repo`
- Buckets: Default Prometheus histogram buckets

#### Batch API Metrics

**`lfs_batch_requests_total`** (Counter)
- Total number of LFS batch API requests
- Labels: `operation`, `tenant`, `repo`
- Operations: `Upload`, `Download`
- Incremented when batch API endpoint is called

#### Error Metrics

**`lfs_errors_total`** (Counter)
- Total number of LFS errors by type
- Labels: `error_type`, `tenant`, `repo`
- Error types:
  - `lfs_disabled`: LFS feature is disabled
  - `object_too_large`: Object exceeds max_object_size
  - `object_not_found`: Requested object does not exist
  - `hash_mismatch`: SHA256 verification failed
  - `storage_error`: Backend storage operation failed

## Prometheus Configuration

### Scrape Configuration

Add this to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'agentic-repos'
    static_configs:
      - targets: ['localhost:8090']
    scrape_interval: 15s
    metrics_path: '/metrics'
```

### Example Queries

```promql
# Total uploads per tenant
sum by (tenant) (lfs_uploads_total)

# Upload rate per repository (5m window)
rate(lfs_uploads_total[5m])

# Total data transferred (uploads + downloads)
sum(lfs_upload_bytes_total + lfs_download_bytes_total)

# Error rate by type
rate(lfs_errors_total[5m])

# 95th percentile upload duration
histogram_quantile(0.95, rate(lfs_upload_duration_seconds_bucket[5m]))

# Repositories with most errors
topk(10, sum by (tenant, repo) (lfs_errors_total))
```

## Grafana Dashboard

### Recommended Panels

1. **Upload/Download Volume**
   - Metric: `rate(lfs_uploads_total[5m])` and `rate(lfs_downloads_total[5m])`
   - Visualization: Time series graph

2. **Data Transfer Rate**
   - Metric: `rate(lfs_upload_bytes_total[5m]) + rate(lfs_download_bytes_total[5m])`
   - Visualization: Time series graph with bytes/sec

3. **Error Rate**
   - Metric: `rate(lfs_errors_total[5m])`
   - Visualization: Time series graph, grouped by error_type

4. **Operation Latency**
   - Metric: `histogram_quantile(0.95, rate(lfs_upload_duration_seconds_bucket[5m]))`
   - Visualization: Heatmap or time series

5. **Top Repositories by Usage**
   - Metric: `topk(10, sum by (tenant, repo) (lfs_uploads_total + lfs_downloads_total))`
   - Visualization: Bar gauge

### JSON Template

A complete Grafana dashboard template is available in `internal/monitoring/grafana-lfs-dashboard.json`.

## Alerting Rules

### Recommended Alerts

```yaml
groups:
  - name: lfs_alerts
    interval: 30s
    rules:
      - alert: HighLFSErrorRate
        expr: rate(lfs_errors_total[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High LFS error rate detected"
          description: "Error rate is {{ $value }} errors/sec for {{ $labels.tenant }}/{{ $labels.repo }}"

      - alert: LFSUploadLatencyHigh
        expr: histogram_quantile(0.95, rate(lfs_upload_duration_seconds_bucket[5m])) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "LFS upload latency is high"
          description: "P95 upload latency is {{ $value }}s for {{ $labels.tenant }}/{{ $labels.repo }}"

      - alert: LFSStorageErrors
        expr: increase(lfs_errors_total{error_type="storage_error"}[5m]) > 5
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "LFS storage backend errors"
          description: "Storage errors detected for {{ $labels.tenant }}/{{ $labels.repo }}"
```

## Testing Metrics

### Manual Testing

```bash
# Test metrics endpoint
curl http://localhost:8090/metrics

# Upload a test file and check metrics
echo "test data" > /tmp/test.bin
OID=$(sha256sum /tmp/test.bin | awk '{print $1}')

curl -X PUT "http://localhost:8090/lfs/test-tenant/test-repo/lfs/objects/$OID" \
  --data-binary @/tmp/test.bin

# Check metrics again
curl http://localhost:8090/metrics | grep lfs_uploads_total
```

### Automated Tests

Unit tests for metrics are included in `src/lfs/metrics.rs`:

```bash
cargo test lfs::metrics --lib
```

## Performance Considerations

1. **Cardinality**: Metrics are labeled by tenant and repo. Monitor cardinality if you have thousands of repositories.

2. **Collection Overhead**: Prometheus metric collection is lightweight (< 1ms per operation).

3. **Storage**: Default histogram buckets may need adjustment for your workload. Edit `src/lfs/metrics.rs` to customize.

## Troubleshooting

### Metrics Not Appearing

1. Verify LFS is enabled in config.yaml:
   ```yaml
   features:
     git_lfs: true
   lfs:
     enabled: true
   ```

2. Check metrics endpoint is accessible:
   ```bash
   curl http://localhost:8090/metrics
   ```

3. Verify Prometheus is scraping:
   ```bash
   # Check Prometheus targets
   curl http://localhost:9090/api/v1/targets
   ```

### Zero Values

- Metrics are initialized lazily on first use
- Perform at least one LFS operation to see non-zero values
- Check `lfs_batch_requests_total` first - it increments on any batch API call

### Missing Labels

- Ensure operations are going through the instrumented endpoints
- Direct storage operations bypass metrics - all access should be via HTTP API

## See Also

- [Git LFS Configuration](LFS_CONFIGURATION.md)
- [Git LFS Implementation](../internal/docs/GIT_LFS_DESIGN.md)
- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Dashboard Best Practices](https://grafana.com/docs/grafana/latest/dashboards/build-dashboards/best-practices/)

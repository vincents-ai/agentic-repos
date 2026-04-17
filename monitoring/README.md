# Git LFS Monitoring Setup

This directory contains monitoring configuration for agentic-repos Git LFS operations.

## Files

- **`grafana-lfs-dashboard.json`** - Grafana dashboard template with 10 panels
- **`prometheus.yml`** - Prometheus scrape configuration
- **`alerts.yml`** - Prometheus alerting rules (11 alerts)

## Quick Setup

### 1. Prometheus

```bash
# Create Prometheus configuration directory
mkdir -p /etc/prometheus

# Copy configuration files
cp prometheus.yml /etc/prometheus/
cp alerts.yml /etc/prometheus/

# Start Prometheus
prometheus --config.file=/etc/prometheus/prometheus.yml
```

Prometheus will scrape metrics from `http://localhost:8090/metrics` every 15 seconds.

### 2. Grafana Dashboard

1. Open Grafana UI (default: `http://localhost:3000`)
2. Navigate to **Dashboards → Import**
3. Upload `grafana-lfs-dashboard.json`
4. Select your Prometheus datasource
5. Click **Import**

The dashboard includes:
- Upload/Download rate time series
- Data transfer bandwidth
- Error rate tracking
- Upload/Download latency percentiles (P50, P95, P99)
- Top 10 repositories by usage
- Summary statistics (total uploads, downloads, data, errors)

### 3. Alertmanager (Optional)

```yaml
# alertmanager.yml
route:
  receiver: 'default'
  group_by: ['alertname', 'tenant', 'repo']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 4h

receivers:
  - name: 'default'
    slack_configs:
      - api_url: 'YOUR_SLACK_WEBHOOK_URL'
        channel: '#alerts'
        title: 'LFS Alert: {{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.description }}{{ end }}'
```

## Dashboard Panels

### 1. LFS Upload/Download Rate
- **Type**: Time series
- **Query**: `rate(lfs_uploads_total[$__rate_interval])`, `rate(lfs_downloads_total[$__rate_interval])`
- **Purpose**: Track operation volume per tenant/repo

### 2. LFS Data Transfer Rate
- **Type**: Time series
- **Query**: `rate(lfs_upload_bytes_total[$__rate_interval])`, `rate(lfs_download_bytes_total[$__rate_interval])`
- **Unit**: Bytes per second
- **Purpose**: Monitor bandwidth usage

### 3. LFS Error Rate
- **Type**: Time series
- **Query**: `rate(lfs_errors_total[$__rate_interval])`
- **Labels**: By error_type, tenant, repo
- **Purpose**: Track error patterns by type

### 4. LFS Upload Latency
- **Type**: Time series
- **Query**: `histogram_quantile(0.50/0.95/0.99, rate(lfs_upload_duration_seconds_bucket[$__rate_interval]))`
- **Purpose**: P50/P95/P99 latency tracking

### 5. LFS Download Latency
- **Type**: Time series
- **Query**: Similar to upload latency
- **Purpose**: Download performance monitoring

### 6. Top 10 Repositories
- **Type**: Table
- **Query**: `topk(10, sum by (tenant, repo) (lfs_uploads_total + lfs_downloads_total))`
- **Purpose**: Identify heaviest LFS users

### 7-10. Summary Stats
- **Type**: Stat panels
- **Metrics**: Total uploads, downloads, bytes transferred, errors
- **Purpose**: At-a-glance health indicators

## Alert Rules

### Critical Alerts

1. **LFSStorageErrors** - Storage backend failures (5 errors in 5m)
2. **LFSHashMismatchErrors** - Data corruption detected (any in 10m)
3. **LFSMetricsDown** - Monitoring failure (2m)

### Warning Alerts

4. **HighLFSErrorRate** - Error rate > 0.1/sec for 5m
5. **LFSUploadLatencyHigh** - P95 > 10s for 5m
6. **LFSDownloadLatencyHigh** - P95 > 10s for 5m
7. **LFSErrorBudgetBurn** - Error rate > 1% SLO
8. **LFSLatencySLOViolation** - P99 > 30s SLO

### Info Alerts

9. **LFSObjectTooLargeErrors** - Size limit rejections
10. **LFSNoActivity** - Unexpected silence
11. **LFSHighThroughput** - Capacity planning indicator (>100MB/s)

## Testing

### Generate Test Metrics

```bash
# Start agentic-repos
cargo run -- --config config.yaml

# Generate upload traffic
for i in {1..10}; do
  echo "test data $i" > /tmp/test$i.bin
  OID=$(sha256sum /tmp/test$i.bin | awk '{print $1}')
  curl -X PUT "http://localhost:8090/lfs/test-tenant/test-repo/lfs/objects/$OID" \
    --data-binary @/tmp/test$i.bin
done

# Check metrics
curl http://localhost:8090/metrics | grep lfs_
```

### Verify Prometheus Scraping

```bash
# Check Prometheus targets
curl http://localhost:9090/api/v1/targets | jq '.data.activeTargets[] | select(.labels.job=="agentic-repos")'

# Query metrics via Prometheus API
curl 'http://localhost:9090/api/v1/query?query=lfs_uploads_total'
```

### Import Dashboard

```bash
# Using Grafana API
curl -X POST http://admin:admin@localhost:3000/api/dashboards/db \
  -H "Content-Type: application/json" \
  -d @grafana-lfs-dashboard.json
```

## Customization

### Adjust Scrape Interval

Edit `prometheus.yml`:
```yaml
scrape_configs:
  - job_name: 'agentic-repos'
    scrape_interval: 30s  # Change from 15s
```

### Add Multi-Instance Support

```yaml
scrape_configs:
  - job_name: 'agentic-repos'
    static_configs:
      - targets:
          - 'agentic-repos-1:8090'
          - 'agentic-repos-2:8090'
          - 'agentic-repos-3:8090'
        labels:
          cluster: 'production'
```

### Tune Alert Thresholds

Edit `alerts.yml` to adjust:
- Error rate thresholds
- Latency SLO targets
- Alert evaluation intervals
- For periods

## Retention

### Prometheus Data Retention

```bash
# Default: 15 days
prometheus --storage.tsdb.retention.time=30d
```

### Grafana Snapshot

Export dashboard as JSON for version control:
```bash
# From Grafana UI: Dashboard → Share → Export → Save to file
```

## Troubleshooting

### No Data in Grafana

1. Check Prometheus is scraping:
   ```bash
   curl http://localhost:9090/api/v1/targets
   ```

2. Verify agentic-repos is exposing metrics:
   ```bash
   curl http://localhost:8090/metrics
   ```

3. Check Grafana datasource configuration

### Alerts Not Firing

1. Verify alert rules are loaded:
   ```bash
   curl http://localhost:9090/api/v1/rules
   ```

2. Check alert evaluation:
   ```bash
   curl http://localhost:9090/api/v1/alerts
   ```

3. Verify Alertmanager is reachable

### High Cardinality Warnings

If you have thousands of repos, consider:
- Aggregating metrics by tenant only
- Using recording rules
- Adjusting retention policies

## Production Recommendations

1. **Use recording rules** for complex queries (percentiles)
2. **Set up Alertmanager** for alert routing and deduplication
3. **Enable authentication** on Prometheus/Grafana
4. **Configure remote write** for long-term storage
5. **Set up high availability** Prometheus with Thanos/Cortex

## See Also

- [LFS Monitoring Guide](../../docs/LFS_MONITORING.md)
- [LFS Configuration](../../docs/LFS_CONFIGURATION.md)
- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)

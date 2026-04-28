# Git LFS Bandwidth Throttling

Rate limiting system for Git LFS transfers to prevent network saturation and ensure fair resource allocation in multi-tenant environments.

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Configuration](#configuration)
- [How It Works](#how-it-works)
- [Monitoring](#monitoring)
- [Tuning Guide](#tuning-guide)
- [Troubleshooting](#troubleshooting)
- [Examples](#examples)
- [Best Practices](#best-practices)

## Overview

The LFS bandwidth throttling system uses a token bucket algorithm to limit the rate of data transfer for uploads and downloads. This prevents individual tenants or repositories from consuming excessive network bandwidth and degrading performance for other users.

**Key Benefits:**
- Prevent network saturation from large file transfers
- Fair bandwidth allocation across multiple tenants
- Support for burst transfers while maintaining sustained limits
- Hierarchical limits (global, per-tenant, per-repository)
- Non-blocking async implementation

## Features

✅ **Token Bucket Rate Limiting**
- Smooth traffic shaping without hard cutoffs
- Configurable burst capacity for better user experience
- Automatic token refill based on elapsed time

✅ **Hierarchical Limits**
- Global default bandwidth limit
- Per-tenant overrides for premium customers
- Per-repository fine-grained control
- Most specific limit takes precedence

✅ **Performance & Observability**
- Async non-blocking waits (other requests continue)
- Prometheus metrics for monitoring
- Track wait times and throttled bytes
- Thread-safe concurrent operation

✅ **Flexible Configuration**
- Enable/disable without code changes
- Hot-reload configuration support
- Optional throttling (disabled by default)

## Configuration

### Basic Configuration

Add to your `config.yaml`:

```yaml
lfs:
  enabled: true
  
  throttle:
    enabled: true
    default_bytes_per_second: 10485760  # 10 MB/s
    burst_size: 52428800  # 50 MB
    per_tenant_limits: {}
    per_repo_limits: {}
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `false` | Enable/disable bandwidth throttling |
| `default_bytes_per_second` | integer | `10485760` | Default rate limit (10 MB/s) |
| `burst_size` | integer | `52428800` | Maximum burst size (50 MB) |
| `per_tenant_limits` | map | `{}` | Per-tenant rate limits (bytes/sec) |
| `per_repo_limits` | map | `{}` | Per-repository rate limits (bytes/sec) |

### Bandwidth Units Reference

```
1 MB/s  = 1,048,576 bytes/sec
10 MB/s = 10,485,760 bytes/sec
100 MB/s = 104,857,600 bytes/sec
1 GB/s  = 1,073,741,824 bytes/sec
```

### Example Configurations

**Small Team (Simple)**
```yaml
throttle:
  enabled: true
  default_bytes_per_second: 10485760  # 10 MB/s for all users
  burst_size: 52428800  # 50 MB burst
```

**Multi-Tenant SaaS (Per-Tenant Limits)**
```yaml
throttle:
  enabled: true
  default_bytes_per_second: 5242880  # 5 MB/s default
  burst_size: 26214400  # 25 MB burst
  per_tenant_limits:
    premium-tenant: 104857600  # 100 MB/s for premium tier
    enterprise-corp: 209715200  # 200 MB/s for enterprise
    free-tier: 1048576  # 1 MB/s for free users
```

**Mixed Workload (Per-Repository Overrides)**
```yaml
throttle:
  enabled: true
  default_bytes_per_second: 10485760  # 10 MB/s default
  burst_size: 52428800  # 50 MB burst
  per_tenant_limits:
    acme-corp: 52428800  # 50 MB/s for Acme Corp
  per_repo_limits:
    acme-corp/ml-models: 209715200  # 200 MB/s for ML repo
    acme-corp/archive: 5242880  # 5 MB/s for archive repo
```

**High-Bandwidth Environment**
```yaml
throttle:
  enabled: true
  default_bytes_per_second: 104857600  # 100 MB/s default
  burst_size: 524288000  # 500 MB burst
```

### Limit Hierarchy

When multiple limits are configured, the most specific limit takes precedence:

1. **Per-repository limit** (most specific)
   - Format: `tenant/repo`
   - Example: `acme-corp/ml-models`

2. **Per-tenant limit**
   - Format: `tenant`
   - Example: `acme-corp`

3. **Default global limit** (least specific)
   - Applied when no tenant or repo override exists

**Example:**
```yaml
throttle:
  default_bytes_per_second: 10485760  # 10 MB/s
  per_tenant_limits:
    acme-corp: 52428800  # 50 MB/s for all Acme Corp repos
  per_repo_limits:
    acme-corp/ml-models: 104857600  # 100 MB/s for ML repo specifically
```

Result:
- `acme-corp/ml-models` → **100 MB/s** (repo override)
- `acme-corp/other-repo` → **50 MB/s** (tenant limit)
- `other-tenant/repo` → **10 MB/s** (default)

## How It Works

### Token Bucket Algorithm

The throttling system uses a **token bucket algorithm**:

1. **Bucket Capacity**: Maximum number of tokens (bytes) available
   - Set via `burst_size` configuration
   - Allows temporary bursts above sustained rate

2. **Refill Rate**: Rate at which tokens are added to bucket
   - Set via `default_bytes_per_second` (or per-tenant/repo overrides)
   - Tokens added continuously based on elapsed time

3. **Consumption**: Each transfer consumes tokens equal to bytes transferred
   - If enough tokens available: transfer proceeds immediately
   - If not enough tokens: request waits until tokens available

### Flow Diagram

```
Upload/Download Request (1000 bytes)
    ↓
Check throttler enabled?
    ↓ (yes)
Get rate limit (hierarchical lookup)
    ↓
Calculate tokens available
    ↓
Enough tokens? → YES → Consume tokens → Proceed
    ↓ (no)
Calculate wait time
    ↓
Async sleep (non-blocking)
    ↓
Retry consumption → Proceed
```

### Async Non-Blocking Behavior

The throttler uses `tokio::time::sleep` for waiting:
- **Blocked request**: Only the throttled request waits
- **Other requests**: Continue processing normally
- **Server**: Remains responsive to other operations

Example:
```
Request A: Upload 100 MB → Throttled (waits 10 seconds)
Request B: Upload 1 MB → Proceeds immediately (different tenant)
Request C: Download 5 MB → Proceeds immediately (different operation)
```

### Token Refill Calculation

Tokens refill based on elapsed time:

```rust
elapsed_seconds = time_since_last_refill
tokens_to_add = refill_rate * elapsed_seconds
new_tokens = min(current_tokens + tokens_to_add, capacity)
```

Example with 10 MB/s rate:
- After 1 second: +10,485,760 tokens
- After 0.5 seconds: +5,242,880 tokens
- After 5 seconds: +52,428,800 tokens (capped at burst_size)

## Monitoring

### Prometheus Metrics

The throttling system exports two key metrics:

**1. Wait Time Histogram**
```
lfs_throttle_wait_seconds{tenant, repo, operation}
```
- Type: Histogram
- Labels: tenant, repo, operation (upload/download)
- Description: Time spent waiting for bandwidth throttle
- Use: Identify over-throttled tenants/repos

**2. Throttled Bytes Counter**
```
lfs_throttle_bytes_limited_total{tenant, repo, operation}
```
- Type: Counter
- Labels: tenant, repo, operation (upload/download)
- Description: Total bytes subject to throttling
- Use: Track bandwidth consumption patterns

### Prometheus Queries

**Average wait time per tenant (last 5m)**
```promql
rate(lfs_throttle_wait_seconds_sum[5m]) / rate(lfs_throttle_wait_seconds_count[5m])
```

**Throttled bandwidth by operation**
```promql
rate(lfs_throttle_bytes_limited_total[5m]) by (operation)
```

**Top 10 throttled repositories**
```promql
topk(10, rate(lfs_throttle_bytes_limited_total[1h]) by (tenant, repo))
```

**Requests experiencing wait > 1 second**
```promql
histogram_quantile(0.95, rate(lfs_throttle_wait_seconds_bucket[5m])) > 1
```

**Bandwidth utilization (actual vs limit)**
```promql
rate(lfs_throttle_bytes_limited_total[5m]) / rate_limit_bytes_per_second
```

### Grafana Dashboard

Example Grafana panel configuration:

**Panel 1: Throttle Wait Time**
```json
{
  "title": "LFS Throttle Wait Time (p95)",
  "targets": [{
    "expr": "histogram_quantile(0.95, rate(lfs_throttle_wait_seconds_bucket[5m])) by (tenant)"
  }]
}
```

**Panel 2: Bandwidth Consumption**
```json
{
  "title": "LFS Bandwidth Usage (MB/s)",
  "targets": [{
    "expr": "rate(lfs_throttle_bytes_limited_total[5m]) / 1048576"
  }]
}
```

**Panel 3: Top Throttled Tenants**
```json
{
  "title": "Top 10 Throttled Tenants",
  "targets": [{
    "expr": "topk(10, sum(rate(lfs_throttle_bytes_limited_total[5m])) by (tenant))"
  }]
}
```

### Alerting Examples

**Alert: Excessive Throttling**
```yaml
- alert: LFSExcessiveThrottling
  expr: histogram_quantile(0.95, rate(lfs_throttle_wait_seconds_bucket[5m])) > 5
  for: 10m
  labels:
    severity: warning
  annotations:
    summary: "LFS uploads/downloads experiencing >5s wait times"
    description: "Tenant {{ $labels.tenant }} is heavily throttled"
```

**Alert: Bandwidth Saturation**
```yaml
- alert: LFSBandwidthSaturation
  expr: rate(lfs_throttle_bytes_limited_total[5m]) / configured_limit_bytes > 0.9
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "LFS bandwidth usage >90% of limit"
```

## Tuning Guide

### Determining Appropriate Limits

**1. Measure Current Bandwidth Usage**

Check actual network bandwidth consumption:
```bash
# Monitor network interface
iftop -i eth0

# Check bandwidth over time
nload -u MB eth0
```

**2. Calculate Sustained Rate**

Formula:
```
sustained_rate = available_bandwidth * utilization_target
```

Example with 1 Gbps link targeting 80% utilization:
```
sustained_rate = 1,000 Mbps * 0.8 = 800 Mbps = 100 MB/s
```

**3. Allocate Per-Tenant Limits**

For multi-tenant environments:
```
per_tenant_limit = sustained_rate / number_of_active_tenants * priority_multiplier
```

Example with 100 MB/s total, 5 active tenants:
- Premium tenants (3): 30 MB/s each
- Standard tenants (2): 5 MB/s each

**4. Set Burst Size**

Recommended: 5-10x sustained rate
```
burst_size = sustained_rate * 5
```

Example:
- Sustained rate: 10 MB/s
- Burst size: 50 MB (5x multiplier)

### Optimization Strategies

**Small Files (< 1 MB)**
- Higher burst_size to reduce latency
- Burst = 50-100x sustained rate

**Large Files (> 100 MB)**
- Lower burst_size to smooth transfers
- Burst = 2-5x sustained rate

**Mixed Workload**
- Use per-repo limits for known heavy repositories
- Keep default_bytes_per_second conservative
- Allow bursts for occasional large transfers

**High-Bandwidth Network (10 Gbps+)**
- Consider disabling throttling entirely
- Or set very high limits (1 GB/s+)
- Focus on per-tenant fairness instead

### Load Balancing Considerations

When running multiple servers behind a load balancer:

**Per-Server Limit Calculation**
```
per_server_limit = total_desired_limit / number_of_servers
```

Example with 100 MB/s desired, 4 servers:
```yaml
throttle:
  default_bytes_per_second: 26214400  # 25 MB/s per server
```

**Note**: Throttling is per-server, not globally coordinated.

## Troubleshooting

### Slow Upload/Download Performance

**Symptoms:**
- Uploads/downloads taking longer than expected
- High wait times in metrics

**Diagnosis:**
```promql
# Check if throttling is causing delays
histogram_quantile(0.95, rate(lfs_throttle_wait_seconds_bucket[5m])) by (tenant, repo)

# Compare to configured limits
rate(lfs_throttle_bytes_limited_total[5m]) / configured_limit
```

**Solutions:**
1. Increase `default_bytes_per_second` if consistently hitting limits
2. Add per-tenant overrides for premium customers
3. Increase `burst_size` for better burst handling
4. Check if network bandwidth actually available

### Excessive Throttling for Small Files

**Symptoms:**
- Small files experiencing unexpected delays
- Burst capacity exhausted quickly

**Diagnosis:**
```promql
# Check burst exhaustion patterns
rate(lfs_throttle_wait_seconds_count[5m]) by (tenant, repo)
```

**Solutions:**
1. Increase `burst_size` to handle more simultaneous small files
2. Consider disabling throttling for small transfers (< 1 MB)
3. Adjust per-repo limits for high-frequency repositories

### Uneven Distribution Across Tenants

**Symptoms:**
- Some tenants always hitting limits
- Others never throttled

**Diagnosis:**
```promql
# Compare usage across tenants
sum(rate(lfs_throttle_bytes_limited_total[1h])) by (tenant)
```

**Solutions:**
1. Add per-tenant limits to enforce fairness
2. Lower default limit to spread bandwidth more evenly
3. Implement tenant priority tiers

### Configuration Not Taking Effect

**Symptoms:**
- Changes to config.yaml not reflected
- Throttling behavior unchanged

**Diagnosis:**
1. Check throttling is enabled: `throttle.enabled: true`
2. Verify configuration loaded: check server logs at startup
3. Ensure application restarted after config change

**Solutions:**
1. Restart server after configuration changes
2. Check for YAML syntax errors
3. Verify file permissions on config.yaml

### High Wait Times Despite Low Usage

**Symptoms:**
- Wait times high even with low overall bandwidth
- Token bucket frequently empty

**Possible Causes:**
1. `default_bytes_per_second` set too low
2. `burst_size` insufficient for workload
3. Multiple concurrent transfers from same tenant/repo

**Solutions:**
1. Profile actual bandwidth usage patterns
2. Adjust limits based on real workload
3. Consider time-of-day based limits

## Examples

### Example 1: Development Environment

Simple setup for small development team:

```yaml
lfs:
  enabled: true
  throttle:
    enabled: true
    default_bytes_per_second: 10485760  # 10 MB/s
    burst_size: 104857600  # 100 MB
```

**Use Case:** Prevent any single developer from saturating office network.

### Example 2: Multi-Tenant SaaS Platform

Tiered bandwidth allocation:

```yaml
lfs:
  enabled: true
  throttle:
    enabled: true
    default_bytes_per_second: 1048576  # 1 MB/s (free tier)
    burst_size: 10485760  # 10 MB
    
    per_tenant_limits:
      startup-plan: 5242880  # 5 MB/s
      professional-plan: 20971520  # 20 MB/s
      enterprise-plan: 104857600  # 100 MB/s
```

**Use Case:** Fair resource allocation across different customer tiers.

### Example 3: CI/CD Optimization

High bandwidth for CI systems, lower for users:

```yaml
lfs:
  enabled: true
  throttle:
    enabled: true
    default_bytes_per_second: 10485760  # 10 MB/s default
    burst_size: 52428800  # 50 MB
    
    per_tenant_limits:
      ci-system: 209715200  # 200 MB/s for CI
      
    per_repo_limits:
      ci-system/build-cache: 524288000  # 500 MB/s for build cache
```

**Use Case:** Prioritize automated systems while protecting interactive users.

### Example 4: Research/ML Workload

High bandwidth for data science teams:

```yaml
lfs:
  enabled: true
  throttle:
    enabled: true
    default_bytes_per_second: 5242880  # 5 MB/s default
    burst_size: 52428800  # 50 MB
    
    per_tenant_limits:
      ml-team: 104857600  # 100 MB/s
      
    per_repo_limits:
      ml-team/datasets: 524288000  # 500 MB/s for datasets
      ml-team/models: 209715200  # 200 MB/s for models
```

**Use Case:** Support large dataset transfers while maintaining general limits.

### Example 5: Geographic Distribution

Different limits for different regions:

```yaml
lfs:
  enabled: true
  throttle:
    enabled: true
    default_bytes_per_second: 10485760  # 10 MB/s
    burst_size: 52428800  # 50 MB
    
    per_tenant_limits:
      us-east-datacenter: 104857600  # 100 MB/s (high-bandwidth region)
      eu-west-datacenter: 52428800  # 50 MB/s (medium bandwidth)
      apac-office: 10485760  # 10 MB/s (limited bandwidth)
```

**Use Case:** Adapt to varying network capacity across locations.

## Best Practices

### Configuration Management

**1. Start Conservative**
- Begin with lower limits (5-10 MB/s)
- Monitor actual usage patterns
- Gradually increase based on data

**2. Use Per-Tenant Overrides Sparingly**
- Too many overrides = difficult to manage
- Consider tier-based limits instead
- Document reason for each override

**3. Monitor Before Enforcing**
- Run with high limits initially
- Collect baseline metrics (1-2 weeks)
- Set realistic limits based on data

### Network Planning

**1. Leave Headroom**
- Never throttle to 100% of available bandwidth
- Target 70-80% utilization maximum
- Account for non-LFS traffic

**2. Consider Peak Times**
- Identify peak usage periods
- Set limits to handle peak smoothly
- Consider time-based limit adjustment

**3. Account for Protocol Overhead**
- TCP/IP overhead: ~5-10%
- HTTP overhead: ~2-5%
- Set limits accordingly

### Operational Guidelines

**1. Regular Review**
- Review throttle metrics monthly
- Adjust limits based on growth
- Remove unused per-tenant overrides

**2. Communication**
- Inform users of bandwidth policies
- Provide guidance on optimal usage
- Explain wait times in error messages

**3. Capacity Planning**
- Track bandwidth growth trends
- Plan network upgrades proactively
- Budget for increased capacity

### Testing

**1. Load Testing**
- Test with realistic file sizes
- Verify throttling works under load
- Confirm limits enforced correctly

**2. Burst Testing**
- Upload files larger than burst_size
- Verify smooth degradation to sustained rate
- Check no excessive wait times

**3. Multi-Tenant Testing**
- Simulate multiple concurrent tenants
- Verify fair bandwidth allocation
- Test hierarchy resolution

### Security Considerations

**1. DoS Protection**
- Throttling helps prevent bandwidth exhaustion attacks
- Set reasonable default limits
- Monitor for anomalous patterns

**2. Fair Use Policies**
- Document bandwidth policies
- Enforce limits consistently
- Provide upgrade paths for heavy users

**3. Monitoring & Alerting**
- Alert on suspicious bandwidth spikes
- Track per-tenant usage patterns
- Investigate repeated limit hits

---

## Related Documentation

- [LFS Configuration](LFS_CONFIGURATION.md) - General LFS setup
- [LFS Monitoring](LFS_MONITORING.md) - Prometheus metrics and dashboards
- [LFS Quota Management](LFS_QUOTA_MANAGEMENT.md) - Storage quotas
- [LFS Production Deployment](LFS_PRODUCTION_DEPLOYMENT.md) - Production best practices
- [LFS Troubleshooting](LFS_TROUBLESHOOTING.md) - Common issues and solutions

---

## Summary

Bandwidth throttling is essential for multi-tenant Git LFS deployments to ensure fair resource allocation and prevent network saturation. The token bucket algorithm provides smooth rate limiting with burst support, while hierarchical limits enable flexible configuration for different use cases.

**Key Takeaways:**
- Start with conservative limits and adjust based on monitoring
- Use per-tenant overrides for priority customers
- Set burst_size to 5-10x sustained rate for good UX
- Monitor metrics regularly to optimize configuration
- Leave headroom (20-30%) in network capacity

For additional help, see [LFS Troubleshooting](LFS_TROUBLESHOOTING.md) or contact support.

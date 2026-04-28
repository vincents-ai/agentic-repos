use lazy_static::lazy_static;
use prometheus::{
    register_histogram_vec, register_int_counter_vec, Encoder, HistogramVec, IntCounterVec,
    TextEncoder,
};

lazy_static! {
    pub static ref LFS_UPLOADS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_uploads_total",
        "Total number of LFS object uploads",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_uploads_total metric");
    pub static ref LFS_DOWNLOADS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_downloads_total",
        "Total number of LFS object downloads",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_downloads_total metric");
    pub static ref LFS_UPLOAD_BYTES_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_upload_bytes_total",
        "Total bytes uploaded to LFS storage",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_upload_bytes_total metric");
    pub static ref LFS_DOWNLOAD_BYTES_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_download_bytes_total",
        "Total bytes downloaded from LFS storage",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_download_bytes_total metric");
    pub static ref LFS_ERRORS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_errors_total",
        "Total number of LFS errors",
        &["error_type", "tenant", "repo"]
    )
    .expect("Failed to create lfs_errors_total metric");
    pub static ref LFS_BATCH_REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_batch_requests_total",
        "Total number of LFS batch API requests",
        &["operation", "tenant", "repo"]
    )
    .expect("Failed to create lfs_batch_requests_total metric");
    pub static ref LFS_UPLOAD_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "lfs_upload_duration_seconds",
        "Duration of LFS upload operations in seconds",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_upload_duration_seconds metric");
    pub static ref LFS_DOWNLOAD_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "lfs_download_duration_seconds",
        "Duration of LFS download operations in seconds",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_download_duration_seconds metric");
    pub static ref LFS_GC_OBJECTS_SCANNED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_gc_objects_scanned_total",
        "Total number of LFS objects scanned during garbage collection",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_gc_objects_scanned_total metric");
    pub static ref LFS_GC_OBJECTS_DELETED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_gc_objects_deleted_total",
        "Total number of LFS objects deleted during garbage collection",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_gc_objects_deleted_total metric");
    pub static ref LFS_GC_BYTES_FREED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_gc_bytes_freed_total",
        "Total bytes freed during garbage collection",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_gc_bytes_freed_total metric");
    pub static ref LFS_GC_RUNS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_gc_runs_total",
        "Total number of garbage collection runs",
        &["tenant", "repo", "status"]
    )
    .expect("Failed to create lfs_gc_runs_total metric");
    pub static ref LFS_QUOTA_BYTES_USED: IntCounterVec = register_int_counter_vec!(
        "lfs_quota_bytes_used",
        "Current LFS storage bytes used per tenant/repo",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_quota_bytes_used metric");
    pub static ref LFS_QUOTA_OBJECTS_COUNT: IntCounterVec = register_int_counter_vec!(
        "lfs_quota_objects_count",
        "Current number of LFS objects per tenant/repo",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_quota_objects_count metric");
    pub static ref LFS_QUOTA_EXCEEDED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_quota_exceeded_total",
        "Total number of quota exceeded errors",
        &["tenant", "repo"]
    )
    .expect("Failed to create lfs_quota_exceeded_total metric");
    pub static ref LFS_THROTTLE_WAIT_SECONDS: HistogramVec = register_histogram_vec!(
        "lfs_throttle_wait_seconds",
        "Time spent waiting for bandwidth throttle",
        &["tenant", "repo", "operation"]
    )
    .expect("Failed to create lfs_throttle_wait_seconds metric");
    pub static ref LFS_THROTTLE_BYTES_LIMITED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_throttle_bytes_limited_total",
        "Total bytes subject to throttling",
        &["tenant", "repo", "operation"]
    )
    .expect("Failed to create lfs_throttle_bytes_limited_total metric");
    pub static ref S3_MULTIPART_UPLOADS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "s3_multipart_uploads_total",
        "Total number of S3 multipart uploads initiated",
        &["bucket"]
    )
    .expect("Failed to create s3_multipart_uploads_total metric");
    pub static ref S3_MULTIPART_PARTS_UPLOADED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "s3_multipart_parts_uploaded_total",
        "Total number of S3 multipart parts uploaded",
        &["bucket"]
    )
    .expect("Failed to create s3_multipart_parts_uploaded_total metric");
    pub static ref S3_MULTIPART_UPLOAD_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "s3_multipart_upload_duration_seconds",
        "Duration of S3 multipart upload operations in seconds",
        &["bucket"]
    )
    .expect("Failed to create s3_multipart_upload_duration_seconds metric");
    pub static ref S3_MULTIPART_BYTES_UPLOADED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "s3_multipart_bytes_uploaded_total",
        "Total bytes uploaded via S3 multipart",
        &["bucket"]
    )
    .expect("Failed to create s3_multipart_bytes_uploaded_total metric");
    pub static ref S3_SIMPLE_UPLOAD_TOTAL: IntCounterVec = register_int_counter_vec!(
        "s3_simple_upload_total",
        "Total number of S3 simple PUT uploads",
        &["bucket"]
    )
    .expect("Failed to create s3_simple_upload_total metric");
    pub static ref LFS_LOCK_CLEANUP_TOTAL: IntCounterVec = register_int_counter_vec!(
        "lfs_lock_cleanup_total",
        "Total number of expired locks cleaned up",
        &[]
    )
    .expect("Failed to create lfs_lock_cleanup_total metric");
}

pub fn record_upload(tenant: &str, repo: &str, bytes: u64) {
    LFS_UPLOADS_TOTAL.with_label_values(&[tenant, repo]).inc();
    LFS_UPLOAD_BYTES_TOTAL
        .with_label_values(&[tenant, repo])
        .inc_by(bytes);
}

pub fn record_download(tenant: &str, repo: &str, bytes: u64) {
    LFS_DOWNLOADS_TOTAL.with_label_values(&[tenant, repo]).inc();
    LFS_DOWNLOAD_BYTES_TOTAL
        .with_label_values(&[tenant, repo])
        .inc_by(bytes);
}

pub fn record_error(error_type: &str, tenant: &str, repo: &str) {
    LFS_ERRORS_TOTAL
        .with_label_values(&[error_type, tenant, repo])
        .inc();
}

pub fn record_batch_request(operation: &str, tenant: &str, repo: &str) {
    LFS_BATCH_REQUESTS_TOTAL
        .with_label_values(&[operation, tenant, repo])
        .inc();
}

pub fn record_gc_scan(tenant: &str, repo: &str, count: u64) {
    LFS_GC_OBJECTS_SCANNED_TOTAL
        .with_label_values(&[tenant, repo])
        .inc_by(count);
}

pub fn record_gc_deletion(tenant: &str, repo: &str, bytes: u64) {
    LFS_GC_OBJECTS_DELETED_TOTAL
        .with_label_values(&[tenant, repo])
        .inc();
    LFS_GC_BYTES_FREED_TOTAL
        .with_label_values(&[tenant, repo])
        .inc_by(bytes);
}

pub fn record_gc_run(tenant: &str, repo: &str, status: &str) {
    LFS_GC_RUNS_TOTAL
        .with_label_values(&[tenant, repo, status])
        .inc();
}

pub fn record_quota_usage(tenant: &str, repo: &str, bytes: u64, _objects: u64) {
    LFS_QUOTA_BYTES_USED
        .with_label_values(&[tenant, repo])
        .reset();
    LFS_QUOTA_BYTES_USED
        .with_label_values(&[tenant, repo])
        .inc_by(bytes);
}

pub fn record_quota_exceeded(tenant: &str, repo: &str) {
    LFS_QUOTA_EXCEEDED_TOTAL
        .with_label_values(&[tenant, repo])
        .inc();
}

pub fn record_throttle_wait(tenant: &str, repo: &str, operation: &str, wait_seconds: f64) {
    LFS_THROTTLE_WAIT_SECONDS
        .with_label_values(&[tenant, repo, operation])
        .observe(wait_seconds);
}

pub fn record_throttle_bytes(tenant: &str, repo: &str, operation: &str, bytes: u64) {
    LFS_THROTTLE_BYTES_LIMITED_TOTAL
        .with_label_values(&[tenant, repo, operation])
        .inc_by(bytes);
}

pub fn record_s3_multipart_upload(bucket: &str, num_parts: u64, bytes: u64, duration_secs: f64) {
    S3_MULTIPART_UPLOADS_TOTAL
        .with_label_values(&[bucket])
        .inc();
    S3_MULTIPART_PARTS_UPLOADED_TOTAL
        .with_label_values(&[bucket])
        .inc_by(num_parts);
    S3_MULTIPART_BYTES_UPLOADED_TOTAL
        .with_label_values(&[bucket])
        .inc_by(bytes);
    S3_MULTIPART_UPLOAD_DURATION_SECONDS
        .with_label_values(&[bucket])
        .observe(duration_secs);
}

pub fn record_s3_simple_upload(bucket: &str) {
    S3_SIMPLE_UPLOAD_TOTAL.with_label_values(&[bucket]).inc();
}

pub fn record_lock_cleanup(count: u64) {
    LFS_LOCK_CLEANUP_TOTAL.with_label_values(&[]).inc_by(count);
}

pub fn encode_metrics() -> Result<String, Box<dyn std::error::Error>> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_upload() {
        let tenant = "test_tenant";
        let repo = "test_repo";
        let bytes = 1024;

        let before_count = LFS_UPLOADS_TOTAL.with_label_values(&[tenant, repo]).get();
        let before_bytes = LFS_UPLOAD_BYTES_TOTAL
            .with_label_values(&[tenant, repo])
            .get();

        record_upload(tenant, repo, bytes);

        let after_count = LFS_UPLOADS_TOTAL.with_label_values(&[tenant, repo]).get();
        let after_bytes = LFS_UPLOAD_BYTES_TOTAL
            .with_label_values(&[tenant, repo])
            .get();

        assert_eq!(after_count, before_count + 1);
        assert_eq!(after_bytes, before_bytes + bytes);
    }

    #[test]
    fn test_record_download() {
        let tenant = "test_tenant_dl";
        let repo = "test_repo_dl";
        let bytes = 2048;

        let before_count = LFS_DOWNLOADS_TOTAL.with_label_values(&[tenant, repo]).get();
        let before_bytes = LFS_DOWNLOAD_BYTES_TOTAL
            .with_label_values(&[tenant, repo])
            .get();

        record_download(tenant, repo, bytes);

        let after_count = LFS_DOWNLOADS_TOTAL.with_label_values(&[tenant, repo]).get();
        let after_bytes = LFS_DOWNLOAD_BYTES_TOTAL
            .with_label_values(&[tenant, repo])
            .get();

        assert_eq!(after_count, before_count + 1);
        assert_eq!(after_bytes, before_bytes + bytes);
    }

    #[test]
    fn test_record_error() {
        let error_type = "not_found";
        let tenant = "test_tenant_err";
        let repo = "test_repo_err";

        let before = LFS_ERRORS_TOTAL
            .with_label_values(&[error_type, tenant, repo])
            .get();

        record_error(error_type, tenant, repo);

        let after = LFS_ERRORS_TOTAL
            .with_label_values(&[error_type, tenant, repo])
            .get();

        assert_eq!(after, before + 1);
    }

    #[test]
    fn test_record_batch_request() {
        let operation = "upload";
        let tenant = "test_tenant_batch";
        let repo = "test_repo_batch";

        let before = LFS_BATCH_REQUESTS_TOTAL
            .with_label_values(&[operation, tenant, repo])
            .get();

        record_batch_request(operation, tenant, repo);

        let after = LFS_BATCH_REQUESTS_TOTAL
            .with_label_values(&[operation, tenant, repo])
            .get();

        assert_eq!(after, before + 1);
    }

    #[test]
    fn test_encode_metrics() {
        record_upload("encode_test", "repo", 100);
        let result = encode_metrics();
        assert!(result.is_ok());
        let metrics_text = result.unwrap();
        assert!(metrics_text.contains("lfs_uploads_total"));
        assert!(metrics_text.contains("lfs_upload_bytes_total"));
    }

    #[test]
    fn test_record_throttle_wait() {
        let tenant = "test_tenant_throttle";
        let repo = "test_repo_throttle";
        let operation = "upload";
        let wait_seconds = 0.5;

        record_throttle_wait(tenant, repo, operation, wait_seconds);

        let encoded = encode_metrics().unwrap();
        assert!(encoded.contains("lfs_throttle_wait_seconds"));
    }

    #[test]
    fn test_record_throttle_bytes() {
        let tenant = "test_tenant_throttle_bytes";
        let repo = "test_repo_throttle_bytes";
        let operation = "download";
        let bytes = 1048576;

        let before = LFS_THROTTLE_BYTES_LIMITED_TOTAL
            .with_label_values(&[tenant, repo, operation])
            .get();

        record_throttle_bytes(tenant, repo, operation, bytes);

        let after = LFS_THROTTLE_BYTES_LIMITED_TOTAL
            .with_label_values(&[tenant, repo, operation])
            .get();

        assert_eq!(after, before + bytes);
    }
}

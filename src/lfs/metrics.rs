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

pub fn encode_metrics() -> Result<String, prometheus::Error> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).to_string())
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
}

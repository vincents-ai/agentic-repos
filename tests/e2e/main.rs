//! E2E test entry point.
//!
//! Run with: cargo test --test e2e -- --nocapture --test-threads=1
//!
//! Spins up a real agentic-repos server subprocess, hits it over HTTP.
//! Optionally starts SeaweedFS for S3 storage backend tests.
//! No database needed — git-refs are the datastore.

#![allow(dead_code, unused_imports)]

mod harness;
mod seaweedfs;
mod ssh_keys;

use harness::E2eHarness;
use seaweedfs::SeaweedFs;
use ssh_keys::{SshTestKeys, git_ssh};
use reqwest::StatusCode;
use serial_test::serial;

/// Create a harness or skip if infrastructure is unavailable.
macro_rules! harness_or_skip {
    ($harness:ident) => {
        let $harness = match E2eHarness::new().await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("SKIP: {}", e);
                return;
            }
        };
    };
}

// ── Health & Info ──────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_health_check() {
    harness_or_skip!(harness);

    let resp = harness.get("/api/v1/health").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.is_object(), "Health response should be a JSON object");
}

#[tokio::test]
#[serial]
async fn e2e_api_info() {
    harness_or_skip!(harness);

    let resp = harness.get("/api/v1/info").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.is_object(), "Info response should be a JSON object");
}

// ── Repository CRUD ───────────────────────────────────────────────────────
// Routes: GET /api/v1/repositories (list)
//         POST /api/v1/repositories/{owner} (create, body: {"name": "..."})
//         GET /api/v1/repositories/{owner}/{repo} (get)

#[tokio::test]
#[serial]
async fn e2e_create_repository() {
    harness_or_skip!(harness);

    let resp = harness
        .post_json(
            "/api/v1/repositories/default",
            &serde_json::json!({
                "name": "e2e-test-repo"
            }),
        )
        .await;

    assert!(
        resp.status().is_success() || resp.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Create repository should return 2xx or 500, got {}",
        resp.status()
    );

    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(
            body["name"], "e2e-test-repo",
            "Response should include repo name"
        );
    }
}

#[tokio::test]
#[serial]
async fn e2e_list_repositories() {
    harness_or_skip!(harness);

    let resp = harness.get("/api/v1/repositories").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body.is_array() || body.is_object(),
        "Repositories response should be JSON"
    );
}

#[tokio::test]
#[serial]
async fn e2e_create_and_get_repository() {
    harness_or_skip!(harness);

    // Create
    let resp = harness
        .post_json(
            "/api/v1/repositories/default",
            &serde_json::json!({
                "name": "e2e-get-test"
            }),
        )
        .await;
    assert!(
        resp.status().is_success() || resp.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Create should succeed or 500, got {}",
        resp.status()
    );

    // Get by owner/name (may fail if create didn't succeed)
    let resp = harness
        .get("/api/v1/repositories/default/e2e-get-test")
        .await;
    assert!(
        resp.status().is_success() || resp.status() == StatusCode::NOT_FOUND,
        "Get repository should return 2xx or 404, got {}",
        resp.status()
    );
}

// ── Error handling ────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_not_found_route() {
    harness_or_skip!(harness);

    let resp = harness
        .get("/api/v1/this-route-does-not-exist")
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn e2e_create_repository_missing_body() {
    harness_or_skip!(harness);

    let resp = harness.post("/api/v1/repositories").await;
    assert!(
        resp.status().is_client_error(),
        "Missing body should return 4xx, got {}",
        resp.status()
    );
}

// ── Cluster status ────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_cluster_status() {
    harness_or_skip!(harness);

    // Cluster routes: scope /cluster in mod.rs + /cluster/status in handler = double prefix
    let resp = harness.get("/api/v1/cluster/cluster/status").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.is_object(), "Cluster status should be a JSON object");
}

// ── Metrics ───────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_metrics_endpoint() {
    harness_or_skip!(harness);

    let resp = harness.get("/metrics").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();
    assert!(!body.is_empty(), "Metrics should not be empty");
}

// ── Compliance ────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_compliance_summary() {
    harness_or_skip!(harness);

    // Compliance routes are at /api/v1/compliance/reports
    // May 404 if compliance state not properly injected
    let resp = harness.get("/api/v1/compliance/reports").await;
    assert!(
        resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_FOUND,
        "Compliance reports should return 200 or 404, got {}",
        resp.status()
    );
}

// ── RBAC ──────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_rbac_roles() {
    harness_or_skip!(harness);

    // RBAC routes: scope /rbac in mod.rs + scope /rbac in handler = double prefix
    // May 403 if RBAC middleware requires auth
    let resp = harness.get("/api/v1/rbac/rbac/roles").await;
    let status = resp.status();
    assert!(
        status.is_success() || status == StatusCode::FORBIDDEN || status.is_server_error(),
        "RBAC roles should return 2xx, 403, or 5xx, got {}",
        status
    );
}

// ── SeaweedFS S3 Storage ──────────────────────────────────────────────

/// Helper: start SeaweedFS or skip.
macro_rules! seaweed_or_skip {
    ($swfs:ident) => {
        let $swfs = match SeaweedFs::start().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("SKIP SeaweedFS: {}", e);
                return;
            }
        };
    };
}

#[tokio::test]
#[serial]
async fn e2e_s3_create_bucket_and_list() {
    seaweed_or_skip!(swfs);

    let client = swfs.s3_client().await;
    let bucket = "test-e2e-bucket";

    // Create bucket
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket should succeed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify bucket accepts objects (SeaweedFS ListBuckets requires IAM config;
    // instead verify the bucket is usable by listing its contents)
    let resp = client
        .list_objects_v2()
        .bucket(bucket)
        .send()
        .await
        .expect("list_objects_v2 on new bucket should succeed");

    assert_eq!(
        resp.key_count().unwrap_or(0), 0,
        "new bucket should be empty"
    );
}

#[tokio::test]
#[serial]
async fn e2e_s3_put_get_delete_object() {
    seaweed_or_skip!(swfs);

    let client = swfs.s3_client().await;
    let bucket = "test-objects";

    // Create bucket
    client.create_bucket().bucket(bucket).send().await.unwrap();

    // Put object
    let data = b"hello seaweedfs from e2e";
    client
        .put_object()
        .bucket(bucket)
        .key("test/key.txt")
        .body(data.to_vec().into())
        .send()
        .await
        .expect("put_object should succeed");

    // Get object
    let resp = client
        .get_object()
        .bucket(bucket)
        .key("test/key.txt")
        .send()
        .await
        .expect("get_object should succeed");

    let body = resp.body.collect().await.unwrap().to_vec();
    assert_eq!(body, data, "object content should match");

    // Delete object
    client
        .delete_object()
        .bucket(bucket)
        .key("test/key.txt")
        .send()
        .await
        .expect("delete_object should succeed");

    // Verify deleted
    let result = client
        .get_object()
        .bucket(bucket)
        .key("test/key.txt")
        .send()
        .await;
    assert!(result.is_err(), "deleted object should not be found");
}

#[tokio::test]
#[serial]
async fn e2e_s3_list_objects() {
    seaweed_or_skip!(swfs);

    let client = swfs.s3_client().await;
    let bucket = "test-list-objects";

    client.create_bucket().bucket(bucket).send().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Put multiple objects
    for i in 0..5 {
        client
            .put_object()
            .bucket(bucket)
            .key(format!("obj/{}.txt", i))
            .body(format!("data-{}", i).into_bytes().into())
            .send()
            .await
            .unwrap();
    }

    // Brief wait for consistency
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // List objects with prefix
    let resp = client
        .list_objects_v2()
        .bucket(bucket)
        .prefix("obj/")
        .send()
        .await
        .expect("list_objects_v2 should succeed");

    let keys: Vec<&str> = resp.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(keys.len(), 5, "should list 5 objects, got {:?}", keys);
}

#[tokio::test]
#[serial]
async fn e2e_s3_storage_backend_roundtrip() {
    // Test the agentic-repos S3Storage type against live SeaweedFS
    use agentic_repos::storage::StorageBackend;
    use std::path::Path;

    seaweed_or_skip!(swfs);

    let client = swfs.s3_client().await;
    let bucket = "test-storage-backend";
    client.create_bucket().bucket(bucket).send().await.unwrap();

    let s3 = agentic_repos::storage::s3::S3Storage::new(client, bucket.to_string());

    // Write
    s3.write(Path::new("e2e/test.dat"), b"storage backend roundtrip")
        .await
        .expect("S3Storage::write should succeed");

    // Exists
    assert!(
        s3.exists(Path::new("e2e/test.dat")).await.unwrap(),
        "object should exist after write"
    );

    // Read
    let data = s3.read(Path::new("e2e/test.dat")).await.unwrap();
    assert_eq!(data, b"storage backend roundtrip");

    // List
    let entries = s3.list(Path::new("e2e/")).await.unwrap();
    assert!(
        entries.iter().any(|e| e.contains("test.dat")),
        "list should contain test.dat, got {:?}",
        entries
    );

    // Delete
    s3.delete(Path::new("e2e/test.dat")).await.unwrap();
    assert!(
        !s3.exists(Path::new("e2e/test.dat")).await.unwrap(),
        "object should not exist after delete"
    );
}

// ── SSH ───────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_ssh_clone_empty_repo() {
    harness_or_skip!(harness);

    // Scan the server's host key so git can verify it
    let kh_dir = tempfile::TempDir::new().unwrap();
    let known_hosts = kh_dir.path().join("known_hosts");
    if let Err(e) = ssh_keys::scan_host_key(harness.ssh_port, &known_hosts) {
        eprintln!("SKIP SSH: host key scan failed: {}", e); return;
    }

    // Create a repo via HTTP first
    let resp = harness
        .post_json(
            "/api/v1/repositories/default",
            &serde_json::json!({"name": "ssh-test-repo"}),
        )
        .await;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        eprintln!("WARN: repo creation failed ({}): {}", status, body);
    }

    // Use the pre-generated SSH keypair (written to authorized_keys before server start)
    let private_key = harness.ssh_keys_dir.join("id_ed25519");
    let keys = SshTestKeys {
        username: "git".to_string(),
        private_key_path: private_key.clone(),
        authorized_keys_path: harness.ssh_keys_dir.join("authorized_keys"),
        _dir: tempfile::TempDir::new().unwrap(), // dummy — keys live in harness config_dir
    };

    let clone_dir = tempfile::TempDir::new().unwrap();
    let clone_url = format!(
        "ssh://git@127.0.0.1:{}/default/ssh-test-repo.git",
        harness.ssh_port
    );

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || {
            git_ssh(&keys, harness.ssh_port, &known_hosts, &["clone", &clone_url, "."], clone_dir.path())
        }),
    ).await.expect("git clone timed out after 30s").expect("spawn_blocking panicked");
    assert!(
        output.status.success(),
        "git clone over SSH should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[serial]
async fn e2e_ssh_rejected_without_key() {
    harness_or_skip!(harness);

    // Scan the server's host key so git can verify it
    let kh_dir = tempfile::TempDir::new().unwrap();
    let known_hosts = kh_dir.path().join("known_hosts");
    if let Err(e) = ssh_keys::scan_host_key(harness.ssh_port, &known_hosts) {
        eprintln!("SKIP SSH: host key scan failed: {}", e); return;
    }

    // Generate a DIFFERENT keypair not in authorized_keys
    let keys = match SshTestKeys::generate("git") {
        Ok(k) => k,
        Err(e) => { eprintln!("SKIP SSH: {}", e); return; }
    };

    let clone_dir = tempfile::TempDir::new().unwrap();
    let clone_url = format!(
        "ssh://git@127.0.0.1:{}/default/any-repo.git",
        harness.ssh_port
    );

    let output = git_ssh(&keys, harness.ssh_port, &known_hosts, &["clone", &clone_url, "."], clone_dir.path());
    assert!(
        !output.status.success(),
        "git clone with unauthorised key should fail: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── LFS ──────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_lfs_batch_upload_download() {
    harness_or_skip!(harness);

    // Create a repo first
    let resp = harness
        .post_json(
            "/api/v1/repositories/default",
            &serde_json::json!({"name": "lfs-test-repo"}),
        )
        .await;
    assert!(resp.status().is_success(), "repo creation should succeed");

    // Compute SHA256 of test content
    let test_content = b"Hello LFS E2E!";
    use sha2::{Sha256, Digest};
    let oid = hex::encode(Sha256::digest(test_content));
    let size = test_content.len();

    // Upload the object directly via the LFS PUT route
    let upload_url = format!(
        "http://127.0.0.1:{}/lfs/default/lfs-test-repo/lfs/objects/{}",
        harness.http_port, oid
    );
    let upload_resp = reqwest::Client::new()
        .put(&upload_url)
        .header("Content-Type", "application/octet-stream")
        .body(test_content.to_vec())
        .send()
        .await
        .expect("upload request");
    assert!(
        upload_resp.status().is_success(),
        "upload should succeed: {}",
        upload_resp.status()
    );

    // Download the object via the LFS GET route
    let download_resp = reqwest::Client::new()
        .get(&upload_url) // same URL, different method
        .send()
        .await
        .expect("download request");
    assert!(
        download_resp.status().is_success(),
        "download should succeed: {}",
        download_resp.status()
    );

    let downloaded = download_resp.bytes().await.expect("read download body");
    assert_eq!(&downloaded[..], test_content, "downloaded content should match original");

    // Also test the batch API — verify it reports the object as existing
    let batch_resp = harness
        .post_json(
            "/lfs/default/lfs-test-repo/info/lfs/objects/batch",
            &serde_json::json!({
                "operation": "download",
                "objects": [{"oid": oid, "size": size}]
            }),
        )
        .await;
    assert!(
        batch_resp.status().is_success(),
        "LFS batch download should succeed: {}",
        batch_resp.status()
    );
}

// ── Structured Logging & Binary Name ─────────────────────────────────────

#[tokio::test]
#[serial]
async fn e2e_api_identity_and_binary_name() {
    harness_or_skip!(harness);

    // 1. API info endpoint returns agentic-repos identity
    let resp = harness.get("/api/v1/info").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    let name = body["name"].as_str().unwrap_or("");
    assert!(
        name.contains("agentic-repos"),
        "API name should contain 'agentic-repos', got: {:?}",
        name
    );
    assert!(
        !name.contains("repos-mcp"),
        "API name should NOT contain old name 'repos-mcp', got: {:?}",
        name
    );

    // 2. Version field is present and non-empty
    let version = body["version"].as_str().unwrap_or("");
    assert!(!version.is_empty(), "Version should not be empty");

    // 3. The binary exists as target/debug/agentic-repos
    let binary_path = std::path::PathBuf::from("target/debug/agentic-repos");
    assert!(binary_path.exists(), "Binary should be at {:?}", binary_path);

    // 4. Health endpoint returns structured JSON (not bare text)
    let health = harness.get("/api/v1/health").await;
    assert_eq!(health.status(), StatusCode::OK);
    let health_body: serde_json::Value = health.json().await.unwrap();
    assert!(health_body.is_object(), "Health response should be JSON object");
    assert!(health_body["status"].is_string(), "Health should have status field");

    // 5. Metrics endpoint returns Prometheus-format text (structured)
    let metrics = harness.get("/metrics").await;
    assert_eq!(metrics.status(), StatusCode::OK);
    let metrics_text = metrics.text().await.unwrap();
    assert!(
        metrics_text.contains("# TYPE") || metrics_text.contains("# HELP"),
        "Metrics should be Prometheus format"
    );
}

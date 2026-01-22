use repos_mcp::config::LfsConfig;
use repos_mcp::lfs::{
    BatchRequest, BatchResponse, GitRef, LfsOperation, LfsTokenGenerator, Operation, RequestObject,
    Transfer,
};

#[test]
fn test_lfs_jwt_authentication_flow() {
    let lfs_config = LfsConfig {
        enabled: true,
        max_object_size: 5 * 1024 * 1024 * 1024,
        token_expiration_secs: 3600,
        jwt_secret: "test-secret-key-for-integration-test".to_string(),
        base_url: "http://localhost:8090".to_string(),
        storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
            .to_string(),
        locking_enabled: false,
        lock_expiration_secs: 86400,
    };

    let tenant = "test-tenant";
    let repo = "test-repo.git";
    let operation = LfsOperation::Download;

    let generator = LfsTokenGenerator::new(lfs_config.jwt_secret.clone());

    let token = generator
        .generate_token(tenant, repo, operation, lfs_config.token_expiration_secs)
        .expect("Failed to generate token");

    assert!(!token.is_empty());

    let claims = generator
        .validate_token(&token)
        .expect("Failed to validate token");

    assert_eq!(claims.tenant, "test-tenant");
    assert_eq!(claims.repo, "test-repo.git");
    assert_eq!(claims.operation, LfsOperation::Download);
    assert!(claims.exp > chrono::Utc::now().timestamp() as u64);
}

#[test]
fn test_lfs_jwt_with_batch_request_structure() {
    let lfs_config = LfsConfig {
        enabled: true,
        max_object_size: 5 * 1024 * 1024 * 1024,
        token_expiration_secs: 3600,
        jwt_secret: "test-secret-key-batch".to_string(),
        base_url: "http://localhost:8090".to_string(),
        storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
            .to_string(),
        locking_enabled: false,
        lock_expiration_secs: 86400,
    };

    let generator = LfsTokenGenerator::new(lfs_config.jwt_secret.clone());
    let token = generator
        .generate_token("tenant1", "repo1.git", LfsOperation::Upload, 3600)
        .expect("Token generation failed");

    let claims = generator
        .validate_token(&token)
        .expect("Token validation failed");

    assert_eq!(claims.tenant, "tenant1");
    assert_eq!(claims.repo, "repo1.git");
    assert_eq!(claims.operation, LfsOperation::Upload);

    let batch_request = BatchRequest {
        operation: Operation::Upload,
        transfers: Some(vec!["basic".to_string()]),
        git_ref: Some(GitRef {
            name: "refs/heads/main".to_string(),
        }),
        objects: vec![
            RequestObject {
                oid: "a".repeat(64),
                size: 1024,
            },
            RequestObject {
                oid: "b".repeat(64),
                size: 2048,
            },
        ],
        hash_algo: Some("sha256".to_string()),
    };

    assert!(matches!(batch_request.operation, Operation::Upload));
    assert_eq!(batch_request.objects.len(), 2);
    assert_eq!(batch_request.objects[0].size, 1024);
    assert_eq!(batch_request.objects[1].size, 2048);
}

#[test]
fn test_jwt_claims_tenant_isolation() {
    let secret = "shared-secret-for-multi-tenant-test".to_string();
    let generator = LfsTokenGenerator::new(secret);

    let token_tenant1 = generator
        .generate_token("tenant1", "repo.git", LfsOperation::Download, 3600)
        .expect("Token generation failed");

    let token_tenant2 = generator
        .generate_token("tenant2", "repo.git", LfsOperation::Download, 3600)
        .expect("Token generation failed");

    let claims1 = generator
        .validate_token(&token_tenant1)
        .expect("Validation failed");
    let claims2 = generator
        .validate_token(&token_tenant2)
        .expect("Validation failed");

    assert_eq!(claims1.tenant, "tenant1");
    assert_eq!(claims2.tenant, "tenant2");
    assert_ne!(token_tenant1, token_tenant2);
    assert_ne!(claims1.tenant, claims2.tenant);
}

#[test]
fn test_jwt_operation_scoping() {
    let secret = "operation-scope-test-secret".to_string();
    let generator = LfsTokenGenerator::new(secret);

    let download_token = generator
        .generate_token("tenant", "repo.git", LfsOperation::Download, 3600)
        .expect("Download token failed");

    let upload_token = generator
        .generate_token("tenant", "repo.git", LfsOperation::Upload, 3600)
        .expect("Upload token failed");

    let download_claims = generator
        .validate_token(&download_token)
        .expect("Download validation failed");
    let upload_claims = generator
        .validate_token(&upload_token)
        .expect("Upload validation failed");

    assert_eq!(download_claims.operation, LfsOperation::Download);
    assert_eq!(upload_claims.operation, LfsOperation::Upload);
    assert_ne!(download_token, upload_token);
}

#[test]
fn test_jwt_expiration_validation() {
    let secret = "expiration-test-secret".to_string();
    let generator = LfsTokenGenerator::new(secret);

    let short_lived_token = generator
        .generate_token("tenant", "repo.git", LfsOperation::Download, 1)
        .expect("Token generation failed");

    let claims = generator
        .validate_token(&short_lived_token)
        .expect("Initial validation should succeed");

    let now = chrono::Utc::now().timestamp() as u64;
    assert!(claims.exp > now, "Token should not be expired yet");
    assert!(
        claims.exp <= now + 2,
        "Token should expire within 2 seconds"
    );
}

#[test]
fn test_lfs_config_validation() {
    let config = LfsConfig::default();

    assert!(!config.enabled);
    assert_eq!(config.max_object_size, 5 * 1024 * 1024 * 1024);
    assert_eq!(config.token_expiration_secs, 3600);
    assert!(!config.jwt_secret.is_empty());
    assert!(!config.base_url.is_empty());
    assert!(!config.locking_enabled);
}

#[test]
fn test_batch_response_structure() {
    let batch_response = BatchResponse {
        transfer: Transfer::Basic,
        objects: vec![],
        hash_algo: "sha256".to_string(),
    };

    assert!(matches!(batch_response.transfer, Transfer::Basic));
    assert_eq!(batch_response.hash_algo, "sha256");
    assert!(batch_response.objects.is_empty());
}

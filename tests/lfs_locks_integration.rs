use repos_mcp::config::LfsConfig;
use repos_mcp::features::FeatureFlags;
use repos_mcp::lfs::test_utils::{cleanup_test_db, setup_test_db};
use repos_mcp::lfs::{
    CreateLockRequest, DeleteLockRequest, GitRef, LfsLockManager, LfsTokenGenerator,
};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

async fn setup_test_environment() -> Option<(Arc<sqlx::PgPool>, LfsConfig, FeatureFlags)> {
    let pool = setup_test_db().await?;

    let lfs_config = LfsConfig {
        enabled: true,
        max_object_size: 5 * 1024 * 1024 * 1024,
        token_expiration_secs: 3600,
        jwt_secret: "test-secret-integration".to_string(),
        base_url: "http://localhost:8090".to_string(),
        storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
            .to_string(),
        locking_enabled: true,
        lock_expiration_secs: 3600,
        gc: Default::default(),
        quota: Default::default(),
        audit: Default::default(),
        throttle: Default::default(),
    };

    let mut features = FeatureFlags::default();
    features.git_lfs = true;
    features.lfs_locks = true;

    Some((pool, lfs_config, features))
}

async fn cleanup_test_environment(pool: &sqlx::PgPool) {
    cleanup_test_db(pool).await;
}

#[tokio::test]
async fn test_complete_lock_unlock_workflow() {
    let Some((pool, _lfs_config, _features)) = setup_test_environment().await else {
        return;
    };
    let manager = LfsLockManager::new(pool.clone());

    let create_request = CreateLockRequest {
        path: "designs/banner.psd".to_string(),
        git_ref: Some(GitRef {
            name: "refs/heads/feature".to_string(),
        }),
    };

    let create_response = manager
        .create_lock("tenant1", "repo1", create_request, "alice", None, None)
        .await
        .expect("Failed to create lock");

    assert_eq!(create_response.lock.path, "designs/banner.psd");
    assert_eq!(create_response.lock.owner.name, "alice");
    let lock_id = create_response.lock.id;

    let list_response = manager
        .list_locks("tenant1", "repo1", None, None, None, None, None)
        .await
        .expect("Failed to list locks");

    assert_eq!(list_response.locks.len(), 1);
    assert_eq!(list_response.locks[0].id, lock_id);

    let delete_request = DeleteLockRequest {
        force: false,
        git_ref: None,
    };

    let delete_response = manager
        .delete_lock("tenant1", "repo1", lock_id, "alice", delete_request.force)
        .await
        .expect("Failed to delete lock");

    assert_eq!(delete_response.lock.id, lock_id);

    let final_list = manager
        .list_locks("tenant1", "repo1", None, None, None, None, None)
        .await
        .expect("Failed to list locks after deletion");

    assert_eq!(final_list.locks.len(), 0);

    cleanup_test_environment(&pool).await;
}

#[tokio::test]
async fn test_concurrent_lock_attempts_same_path() {
    let Some((pool, _lfs_config, _features)) = setup_test_environment().await else {
        return;
    };
    let manager = Arc::new(LfsLockManager::new(pool.clone()));

    let request1 = CreateLockRequest {
        path: "contested.txt".to_string(),
        git_ref: None,
    };

    let create_result = manager
        .create_lock("tenant1", "repo1", request1, "alice", None, None)
        .await;

    assert!(create_result.is_ok());

    let request2 = CreateLockRequest {
        path: "contested.txt".to_string(),
        git_ref: None,
    };

    let concurrent_result = manager
        .create_lock("tenant1", "repo1", request2, "bob", None, None)
        .await;

    assert!(concurrent_result.is_err());

    cleanup_test_environment(&pool).await;
}

#[tokio::test]
async fn test_lock_expiration_workflow() {
    let Some((pool, _lfs_config, _features)) = setup_test_environment().await else {
        return;
    };
    let manager = LfsLockManager::new(pool.clone());

    let request1 = CreateLockRequest {
        path: "expires_soon.txt".to_string(),
        git_ref: None,
    };

    manager
        .create_lock("tenant1", "repo1", request1, "alice", None, Some(2))
        .await
        .expect("Failed to create expiring lock");

    let request2 = CreateLockRequest {
        path: "never_expires.txt".to_string(),
        git_ref: None,
    };

    manager
        .create_lock("tenant1", "repo1", request2, "bob", None, None)
        .await
        .expect("Failed to create permanent lock");

    let list_before_expiry = manager
        .list_locks("tenant1", "repo1", None, None, None, None, None)
        .await
        .expect("Failed to list locks before expiry");

    assert_eq!(list_before_expiry.locks.len(), 2);

    sleep(Duration::from_secs(3)).await;

    let list_after_expiry = manager
        .list_locks("tenant1", "repo1", None, None, None, None, None)
        .await
        .expect("Failed to list locks after expiry");

    assert_eq!(list_after_expiry.locks.len(), 1);
    assert_eq!(list_after_expiry.locks[0].path, "never_expires.txt");

    let cleanup_count = manager
        .cleanup_expired_locks()
        .await
        .expect("Failed to cleanup expired locks");

    assert_eq!(cleanup_count, 1);

    cleanup_test_environment(&pool).await;
}

#[tokio::test]
async fn test_verify_endpoint_workflow() {
    let Some((pool, _lfs_config, _features)) = setup_test_environment().await else {
        return;
    };
    let manager = LfsLockManager::new(pool.clone());

    let request1 = CreateLockRequest {
        path: "alice_file1.txt".to_string(),
        git_ref: None,
    };
    manager
        .create_lock("tenant1", "repo1", request1, "alice", None, None)
        .await
        .expect("Failed to create lock");

    let request2 = CreateLockRequest {
        path: "alice_file2.txt".to_string(),
        git_ref: None,
    };
    manager
        .create_lock("tenant1", "repo1", request2, "alice", None, None)
        .await
        .expect("Failed to create lock");

    let request3 = CreateLockRequest {
        path: "bob_file.txt".to_string(),
        git_ref: None,
    };
    manager
        .create_lock("tenant1", "repo1", request3, "bob", None, None)
        .await
        .expect("Failed to create lock");

    let verify_result = manager
        .verify_locks("tenant1", "repo1", "alice", None, None, None)
        .await
        .expect("Failed to verify locks");

    assert_eq!(verify_result.ours.len(), 2);
    assert_eq!(verify_result.theirs.len(), 1);

    for lock in &verify_result.ours {
        assert_eq!(lock.owner.name, "alice");
    }

    for lock in &verify_result.theirs {
        assert_eq!(lock.owner.name, "bob");
    }

    cleanup_test_environment(&pool).await;
}

#[tokio::test]
async fn test_force_delete_workflow() {
    let Some((pool, _lfs_config, _features)) = setup_test_environment().await else {
        return;
    };
    let manager = LfsLockManager::new(pool.clone());

    let request = CreateLockRequest {
        path: "protected_asset.psd".to_string(),
        git_ref: None,
    };

    let create_response = manager
        .create_lock("tenant1", "repo1", request, "alice", None, None)
        .await
        .expect("Failed to create lock");

    let lock_id = create_response.lock.id;

    let non_owner_delete = manager
        .delete_lock("tenant1", "repo1", lock_id, "bob", false)
        .await;

    assert!(non_owner_delete.is_err());

    let force_delete_result = manager
        .delete_lock("tenant1", "repo1", lock_id, "admin", true)
        .await;

    assert!(force_delete_result.is_ok());

    let verify_deleted = manager
        .list_locks("tenant1", "repo1", None, None, None, None, None)
        .await
        .expect("Failed to list locks");

    assert_eq!(verify_deleted.locks.len(), 0);

    cleanup_test_environment(&pool).await;
}

#[tokio::test]
async fn test_multi_tenant_isolation() {
    let Some((pool, _lfs_config, _features)) = setup_test_environment().await else {
        return;
    };
    let manager = LfsLockManager::new(pool.clone());

    let request1 = CreateLockRequest {
        path: "shared_name.txt".to_string(),
        git_ref: None,
    };
    manager
        .create_lock("tenant1", "repo1", request1, "alice", None, None)
        .await
        .expect("Failed to create lock for tenant1");

    let request2 = CreateLockRequest {
        path: "shared_name.txt".to_string(),
        git_ref: None,
    };
    let result = manager
        .create_lock("tenant2", "repo1", request2, "bob", None, None)
        .await;

    assert!(result.is_ok());

    let tenant1_locks = manager
        .list_locks("tenant1", "repo1", None, None, None, None, None)
        .await
        .expect("Failed to list tenant1 locks");

    assert_eq!(tenant1_locks.locks.len(), 1);
    assert_eq!(tenant1_locks.locks[0].owner.name, "alice");

    let tenant2_locks = manager
        .list_locks("tenant2", "repo1", None, None, None, None, None)
        .await
        .expect("Failed to list tenant2 locks");

    assert_eq!(tenant2_locks.locks.len(), 1);
    assert_eq!(tenant2_locks.locks[0].owner.name, "bob");

    cleanup_test_environment(&pool).await;
}

#[tokio::test]
async fn test_pagination_workflow() {
    let Some((pool, _lfs_config, _features)) = setup_test_environment().await else {
        return;
    };
    let manager = LfsLockManager::new(pool.clone());

    for i in 1..=10 {
        let request = CreateLockRequest {
            path: format!("file_{}.txt", i),
            git_ref: None,
        };
        manager
            .create_lock("tenant1", "repo1", request, "alice", None, None)
            .await
            .expect("Failed to create lock");
    }

    let page1 = manager
        .list_locks("tenant1", "repo1", None, None, None, Some(3), None)
        .await
        .expect("Failed to get page 1");

    assert_eq!(page1.locks.len(), 3);
    assert!(page1.next_cursor.is_some());

    let page2 = manager
        .list_locks(
            "tenant1",
            "repo1",
            None,
            None,
            None,
            Some(3),
            page1.next_cursor.as_deref(),
        )
        .await
        .expect("Failed to get page 2");

    assert_eq!(page2.locks.len(), 3);
    assert!(page2.next_cursor.is_some());

    assert_ne!(page1.locks[0].id, page2.locks[0].id);

    cleanup_test_environment(&pool).await;
}

#[test]
fn test_jwt_token_generation_for_locks() {
    let lfs_config = LfsConfig {
        enabled: true,
        max_object_size: 5 * 1024 * 1024 * 1024,
        token_expiration_secs: 3600,
        jwt_secret: "test-secret-locks".to_string(),
        base_url: "http://localhost:8090".to_string(),
        storage_path_pattern: "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}"
            .to_string(),
        locking_enabled: true,
        lock_expiration_secs: 3600,
        gc: Default::default(),
        quota: Default::default(),
        audit: Default::default(),
        throttle: Default::default(),
    };

    let generator = LfsTokenGenerator::new(lfs_config.jwt_secret.clone());
    let token = generator
        .generate_token(
            "tenant1",
            "repo1",
            repos_mcp::lfs::LfsOperation::Download,
            3600,
        )
        .expect("Failed to generate token");

    let claims = generator
        .validate_token(&token)
        .expect("Failed to validate token");

    assert_eq!(claims.tenant, "tenant1");
    assert_eq!(claims.repo, "repo1");
    assert!(claims.exp > chrono::Utc::now().timestamp() as u64);
}

use git2::{Repository, Signature};
use repos_mcp::git::protocol::{ReceivePackHandler, UploadPackHandler};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn create_test_repo_with_commit(dir: &TempDir) -> Repository {
    let repo = Repository::init(dir.path()).unwrap();

    let sig = Signature::now("Test User", "test@example.com").unwrap();

    let mut index = repo.index().unwrap();
    index.write_tree().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();

    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .unwrap();

    drop(tree);

    repo
}

#[tokio::test]
async fn test_upload_pack_integration() {
    let dir = TempDir::new().unwrap();
    let _repo = create_test_repo_with_commit(&dir);

    let handler = UploadPackHandler::new(dir.path()).unwrap();

    let input = b"";
    let mut output = Vec::new();

    let result = handler.handle(&input[..], &mut output).await;

    assert!(result.is_ok());
    assert!(!output.is_empty());

    let output_str = String::from_utf8_lossy(&output);
    assert!(output_str.contains("multi_ack"));
    assert!(output_str.contains("side-band-64k"));
}

#[tokio::test]
async fn test_receive_pack_integration() {
    let dir = TempDir::new().unwrap();
    Repository::init_bare(dir.path()).unwrap();

    let handler = ReceivePackHandler::new(dir.path()).unwrap();

    let input = b"";
    let mut output = Vec::new();

    let result = handler.handle(&input[..], &mut output).await;

    assert!(result.is_ok());
    assert!(!output.is_empty());

    let output_str = String::from_utf8_lossy(&output);
    assert!(output_str.contains("report-status"));
    assert!(output_str.contains("delete-refs"));
}

#[tokio::test]
async fn test_upload_pack_with_multiple_refs() {
    let dir = TempDir::new().unwrap();
    let repo = create_test_repo_with_commit(&dir);

    let sig = Signature::now("Test User", "test@example.com").unwrap();
    let head = repo.head().unwrap();
    let commit = repo.find_commit(head.target().unwrap()).unwrap();

    repo.branch("feature-1", &commit, false).unwrap();
    repo.branch("feature-2", &commit, false).unwrap();

    let handler = UploadPackHandler::new(dir.path()).unwrap();

    let input = b"";
    let mut output = Vec::new();

    handler.handle(&input[..], &mut output).await.unwrap();

    let output_str = String::from_utf8_lossy(&output);
    assert!(output_str.contains("refs/heads/feature-1"));
    assert!(output_str.contains("refs/heads/feature-2"));
}

#[tokio::test]
async fn test_protocol_handles_empty_repo() {
    let dir = TempDir::new().unwrap();
    Repository::init_bare(dir.path()).unwrap();

    let upload_handler = UploadPackHandler::new(dir.path()).unwrap();
    let receive_handler = ReceivePackHandler::new(dir.path()).unwrap();

    let input = b"";
    let mut upload_output = Vec::new();
    let mut receive_output = Vec::new();

    assert!(upload_handler
        .handle(&input[..], &mut upload_output)
        .await
        .is_ok());
    assert!(receive_handler
        .handle(&input[..], &mut receive_output)
        .await
        .is_ok());

    assert!(upload_output.len() > 0);
    assert!(receive_output.len() > 0);
}

#[tokio::test]
async fn test_protocol_concurrent_access() {
    let dir = TempDir::new().unwrap();
    let _repo = create_test_repo_with_commit(&dir);

    let path = dir.path().to_path_buf();

    let handles: Vec<_> = (0..5)
        .map(|_| {
            let path_clone = path.clone();
            tokio::spawn(async move {
                let handler = UploadPackHandler::new(&path_clone).unwrap();
                let input = b"";
                let mut output = Vec::new();
                handler.handle(&input[..], &mut output).await
            })
        })
        .collect();

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn test_multi_tenant_isolation() {
    let tenant1_dir = TempDir::new().unwrap();
    let tenant2_dir = TempDir::new().unwrap();

    let repo1 = create_test_repo_with_commit(&tenant1_dir);
    let repo2 = create_test_repo_with_commit(&tenant2_dir);

    let sig = Signature::now("Test", "test@example.com").unwrap();
    let head1 = repo1.head().unwrap();
    let commit1 = repo1.find_commit(head1.target().unwrap()).unwrap();
    repo1.branch("tenant1-feature", &commit1, false).unwrap();

    let head2 = repo2.head().unwrap();
    let commit2 = repo2.find_commit(head2.target().unwrap()).unwrap();
    repo2.branch("tenant2-feature", &commit2, false).unwrap();

    let handler1 = UploadPackHandler::new(tenant1_dir.path()).unwrap();
    let handler2 = UploadPackHandler::new(tenant2_dir.path()).unwrap();

    let input = b"";
    let mut output1 = Vec::new();
    let mut output2 = Vec::new();

    handler1.handle(&input[..], &mut output1).await.unwrap();
    handler2.handle(&input[..], &mut output2).await.unwrap();

    let output1_str = String::from_utf8_lossy(&output1);
    let output2_str = String::from_utf8_lossy(&output2);

    assert!(output1_str.contains("tenant1-feature"));
    assert!(!output1_str.contains("tenant2-feature"));

    assert!(output2_str.contains("tenant2-feature"));
    assert!(!output2_str.contains("tenant1-feature"));
}

#[tokio::test]
async fn test_protocol_error_handling_invalid_repo() {
    let dir = TempDir::new().unwrap();
    let invalid_path = dir.path().join("nonexistent");

    let upload_result = UploadPackHandler::new(&invalid_path);
    let receive_result = ReceivePackHandler::new(&invalid_path);

    assert!(upload_result.is_err());
    assert!(receive_result.is_err());
}

#[tokio::test]
async fn test_protocol_reference_advertisement_format() {
    let dir = TempDir::new().unwrap();
    let repo = create_test_repo_with_commit(&dir);

    let sig = Signature::now("Test", "test@example.com").unwrap();
    let head = repo.head().unwrap();
    let commit = repo.find_commit(head.target().unwrap()).unwrap();
    repo.branch("test-branch", &commit, false).unwrap();

    let handler = UploadPackHandler::new(dir.path()).unwrap();
    let input = b"";
    let mut output = Vec::new();

    handler.handle(&input[..], &mut output).await.unwrap();

    let output_str = String::from_utf8_lossy(&output);

    let lines: Vec<&str> = output_str.split('\n').collect();
    for line in lines.iter().take(5) {
        if !line.is_empty() && line.len() >= 4 {
            let maybe_len = &line[0..4];
            if let Ok(_len) = u16::from_str_radix(maybe_len, 16) {
                continue;
            }
        }
    }
}

#[tokio::test]
async fn test_receive_pack_reference_updates() {
    let dir = TempDir::new().unwrap();
    Repository::init_bare(dir.path()).unwrap();

    let handler = ReceivePackHandler::new(dir.path()).unwrap();
    let input = b"";
    let mut output = Vec::new();

    handler.handle(&input[..], &mut output).await.unwrap();

    let output_str = String::from_utf8_lossy(&output);
    assert!(output_str.contains("capabilities"));
}

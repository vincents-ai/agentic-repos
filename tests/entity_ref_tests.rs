//! Integration tests for entity ref lifecycle.
//!
//! These tests verify:
//! - Writing entity refs via AdpStore creates readable refs
//! - Entity refs appear in list_refs queries
//! - Schema-typed entity queries work end-to-end
//! - Entity refs can be read back with correct data

#[cfg(test)]
mod entity_ref_tests {
    use agentic_repos::adp::storage::AdpStore;
    use serde_json::json;
    use tempfile::TempDir;

    fn init_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        // Initialize a bare git repo
        let repo_path = dir.path();
        gix::init(repo_path).expect("Failed to init test repo");
        dir
    }

    // ── Test 1: Write and read entity ref ──────────────────────────────

    #[test]
    fn test_write_and_read_entity_ref() {
        let dir = init_test_repo();
        let repo_path = dir.path();

        let entity_data = json!({
            "id": "task-001",
            "title": "Test task",
            "status": "in_progress"
        });

        AdpStore::write_ref_blob(repo_path, "refs/engram/task/task-001", &entity_data)
            .expect("Write should succeed");

        let read_data = AdpStore::read_ref_blob(repo_path, "refs/engram/task/task-001")
            .expect("Read should succeed");

        assert_eq!(read_data["id"], "task-001");
        assert_eq!(read_data["title"], "Test task");
        assert_eq!(read_data["status"], "in_progress");
    }

    // ── Test 2: List refs with prefix ──────────────────────────────────

    #[test]
    fn test_list_entity_refs_by_prefix() {
        let dir = init_test_repo();
        let repo_path = dir.path();

        // Write multiple entity refs
        for i in 1..=3 {
            let data = json!({"id": format!("task-{:03}", i), "title": format!("Task {}", i)});
            AdpStore::write_ref_blob(
                repo_path,
                &format!("refs/engram/task/task-{:03}", i),
                &data,
            )
            .expect("Write should succeed");
        }

        // Write a different namespace
        AdpStore::write_ref_blob(
            repo_path,
            "refs/engram/session/sess-001",
            &json!({"id": "sess-001"}),
        )
        .expect("Write should succeed");

        // List only task refs
        let refs = AdpStore::list_refs(repo_path, "refs/engram/task/")
            .expect("List should succeed");

        assert_eq!(refs.len(), 3);
        assert!(refs.iter().all(|r| r.starts_with("refs/engram/task/")));
    }

    // ── Test 3: Overwrite existing entity ref ──────────────────────────

    #[test]
    fn test_overwrite_entity_ref() {
        let dir = init_test_repo();
        let repo_path = dir.path();

        AdpStore::write_ref_blob(
            repo_path,
            "refs/engram/task/task-100",
            &json!({"status": "todo"}),
        )
        .expect("First write should succeed");

        AdpStore::write_ref_blob(
            repo_path,
            "refs/engram/task/task-100",
            &json!({"status": "done"}),
        )
        .expect("Overwrite should succeed");

        let data = AdpStore::read_ref_blob(repo_path, "refs/engram/task/task-100")
            .expect("Read should succeed");
        assert_eq!(data["status"], "done");
    }

    // ── Test 4: Read nonexistent ref returns error ─────────────────────

    #[test]
    fn test_read_nonexistent_ref_returns_error() {
        let dir = init_test_repo();
        let repo_path = dir.path();

        let result = AdpStore::read_ref_blob(repo_path, "refs/engram/task/nonexistent");
        assert!(result.is_err(), "Reading nonexistent ref should fail");
    }

    // ── Test 5: Schema registration and lookup ─────────────────────────

    #[test]
    fn test_schema_registration_and_list() {
        let dir = init_test_repo();
        let repo_path = dir.path();

        // Write a schema definition
        let schema = json!({
            "id": "test-entity",
            "namespace_pattern": "refs/test/entities/*",
            "title": "Test Entity",
            "version": "1.0",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "schema": {"type": "object"}
        });
        AdpStore::write_ref_blob(
            repo_path,
            "refs/adp/schema/test-entity",
            &schema,
        )
        .expect("Schema write should succeed");

        let schemas = AdpStore::list_schemas(repo_path).expect("List schemas should succeed");
        assert!(!schemas.is_empty(), "Should have at least one schema");

        let found = schemas.iter().find(|s| s.id == "test-entity");
        assert!(found.is_some(), "Should find test-entity schema");
        let found = found.unwrap();
        assert_eq!(found.namespace_pattern, "refs/test/entities/*");
        assert_eq!(found.title, "Test Entity");
    }

    // ── Test 6: Empty prefix lists nothing ─────────────────────────────

    #[test]
    fn test_list_refs_empty_repo() {
        let dir = init_test_repo();
        let repo_path = dir.path();

        let refs = AdpStore::list_refs(repo_path, "refs/engram/task/")
            .expect("List should succeed on empty repo");
        assert!(refs.is_empty(), "Should have no refs in empty repo");
    }
}

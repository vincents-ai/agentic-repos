//! Integration tests for ADP schema + access REST API.
//!
//! Tests the data layer: AdpStore CRUD, CallerIdentity resolution,
//! schema glob matching, ref classification, and validation logic.

#[cfg(test)]
mod tests {
    use agentic_repos::adp::access::{AccessConfig, PathRule, Permission};
    use agentic_repos::adp::schema::RefSchema;
    use agentic_repos::adp::storage::AdpStore;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_schema(id: &str) -> RefSchema {
        RefSchema {
            id: id.to_string(),
            namespace_pattern: format!("refs/engram/{}/*", id),
            title: format!("{} Schema", id),
            description: Some(format!("Schema for {}", id)),
            version: "1.0.0".to_string(),
            author: Some("test".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            schema: serde_json::json!({ "type": "object" }),
            ui: None,
        }
    }

    fn init_bare_repo(dir: &PathBuf) {
        std::fs::create_dir_all(dir).unwrap();
        gix::init_bare(dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // Schema CRUD tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_schema_write_read_roundtrip() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        let schema = make_schema("task");
        AdpStore::write_schema(&repo_path, &schema).unwrap();

        let read_back = AdpStore::read_schema(&repo_path, "task").unwrap();
        assert!(read_back.is_some());
        assert_eq!(read_back.unwrap().id, "task");
    }

    #[test]
    fn test_schema_not_found() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        let result = AdpStore::read_schema(&repo_path, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_schemas() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        AdpStore::write_schema(&repo_path, &make_schema("task")).unwrap();
        AdpStore::write_schema(&repo_path, &make_schema("session")).unwrap();

        let schemas = AdpStore::list_schemas(&repo_path).unwrap();
        assert_eq!(schemas.len(), 2);
        let ids: Vec<&str> = schemas.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"task"));
        assert!(ids.contains(&"session"));
    }

    #[test]
    fn test_schema_version_sidecars() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        // Write the same schema twice to create two versions
        AdpStore::write_schema(&repo_path, &make_schema("task")).unwrap();
        AdpStore::write_schema(&repo_path, &make_schema("task")).unwrap();

        // List should still return only 1 schema (skipping sidecars)
        let schemas = AdpStore::list_schemas(&repo_path).unwrap();
        assert_eq!(schemas.len(), 1);

        // But raw gix refs should include sidecars
        let gix_repo = gix::open(&repo_path).unwrap();
        let platform = gix_repo.references().unwrap();
        let iter = platform.all().unwrap();
        let all_refs: Vec<String> = iter
            .filter_map(|r| r.ok())
            .map(|r| r.name().as_bstr().to_string())
            .filter(|name| name.starts_with("refs/adp/schema/"))
            .collect();
        // Should have the main ref + 2 version sidecars = 3
        assert_eq!(all_refs.len(), 3, "expected 3 refs, got: {:?}", all_refs);
    }

    #[test]
    fn test_schema_delete() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        AdpStore::write_schema(&repo_path, &make_schema("task")).unwrap();

        // Delete reference via gix
        let refname = "refs/adp/schema/task";
        let repo = gix::open(&repo_path).unwrap();
        use gix::refs::transaction::{Change, PreviousValue, RefEdit};
        repo.edit_reference(RefEdit {
            change: Change::Delete {
                expected: PreviousValue::MustExist,
                log: gix::refs::transaction::RefLog::AndReference,
            },
            name: refname.try_into().unwrap(),
            deref: false,
        }).unwrap();

        let result = AdpStore::read_schema(&repo_path, "task").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_schema_update_preserves_version_history() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        // Write v1
        let mut schema = make_schema("task");
        schema.version = "1.0.0".to_string();
        AdpStore::write_schema(&repo_path, &schema).unwrap();

        // Update to v2
        schema.version = "2.0.0".to_string();
        AdpStore::write_schema(&repo_path, &schema).unwrap();

        // Should have 2 sidecars + 1 main = 3 refs (via raw gix listing)
        let gix_repo = gix::open(&repo_path).unwrap();
        let platform = gix_repo.references().unwrap();
        let iter = platform.all().unwrap();
        let all_refs: Vec<String> = iter
            .filter_map(|r| r.ok())
            .map(|r| r.name().as_bstr().to_string())
            .filter(|name| name.starts_with("refs/adp/schema/"))
            .collect();
        assert_eq!(all_refs.len(), 3, "expected 3 refs, got: {:?}", all_refs);

        // Current schema should be v2
        let current = AdpStore::read_schema(&repo_path, "task").unwrap().unwrap();
        assert_eq!(current.version, "2.0.0");
    }

    // -----------------------------------------------------------------------
    // Access config tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_access_config_roundtrip() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec!["bob".to_string()],
            rules: vec![PathRule {
                user: "carol".to_string(),
                paths: vec!["refs/heads/*".to_string()],
                permissions: vec![Permission::Pull],
            }],
        };

        AdpStore::write_access_config(&repo_path, &config).unwrap();

        let read_back = AdpStore::read_access_config(&repo_path).unwrap().unwrap();
        assert_eq!(read_back.owners, vec!["alice"]);
        assert_eq!(read_back.admins, vec!["bob"]);
        assert_eq!(read_back.rules.len(), 1);
    }

    #[test]
    fn test_access_config_not_found() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        let result = AdpStore::read_access_config(&repo_path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_access_config_update_creates_version() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        // Write initial config
        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec![],
            rules: vec![],
        };
        AdpStore::write_access_config(&repo_path, &config).unwrap();

        // Update config (add an admin)
        let updated = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec!["bob".to_string()],
            rules: vec![],
        };
        AdpStore::write_access_config(&repo_path, &updated).unwrap();

        // Verify the update
        let read_back = AdpStore::read_access_config(&repo_path).unwrap().unwrap();
        assert_eq!(read_back.admins, vec!["bob"]);
    }

    // -----------------------------------------------------------------------
    // CallerIdentity resolution tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_caller_identity_owner() {
        use agentic_repos::adp::identity::CallerIdentity;

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec!["bob".to_string()],
            rules: vec![],
        };

        let id = CallerIdentity::resolve("alice", &config);
        assert!(id.is_owner);
        assert!(id.is_admin);
        assert!(id.can_see_private_refs());
    }

    #[test]
    fn test_caller_identity_admin() {
        use agentic_repos::adp::identity::CallerIdentity;

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec!["bob".to_string()],
            rules: vec![],
        };

        let id = CallerIdentity::resolve("bob", &config);
        assert!(!id.is_owner);
        assert!(id.is_admin);
        assert!(id.can_see_private_refs());
    }

    #[test]
    fn test_caller_identity_regular_user() {
        use agentic_repos::adp::identity::CallerIdentity;

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec!["bob".to_string()],
            rules: vec![],
        };

        let id = CallerIdentity::resolve("carol", &config);
        assert!(!id.is_owner);
        assert!(!id.is_admin);
        assert!(!id.can_see_private_refs());
    }

    #[test]
    fn test_owner_is_also_admin() {
        use agentic_repos::adp::identity::CallerIdentity;

        let config = AccessConfig {
            version: "1".to_string(),
            owners: vec!["alice".to_string()],
            admins: vec![],
            rules: vec![],
        };

        // Owner should also be admin
        let id = CallerIdentity::resolve("alice", &config);
        assert!(id.is_owner);
        assert!(id.is_admin);
    }

    // -----------------------------------------------------------------------
    // Schema glob resolution tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_schema_resolution_specificity() {
        use agentic_repos::adp::glob::resolve_schema;

        let now = chrono::Utc::now();
        let generic = RefSchema {
            id: "generic".to_string(),
            namespace_pattern: "refs/engram/**".to_string(),
            title: "Generic".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: now,
            updated_at: now,
            schema: serde_json::Value::Null,
            ui: None,
        };
        let specific = RefSchema {
            id: "task".to_string(),
            namespace_pattern: "refs/engram/task/*".to_string(),
            title: "Task".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: now,
            updated_at: now,
            schema: serde_json::Value::Null,
            ui: None,
        };

        let schemas = vec![generic, specific];

        // refs/engram/task/abc should match "task" (more specific)
        let result = resolve_schema(&schemas, "refs/engram/task/abc");
        assert_eq!(result.unwrap().id, "task");

        // refs/engram/session/xyz should match "generic" (only one that matches)
        let result = resolve_schema(&schemas, "refs/engram/session/xyz");
        assert_eq!(result.unwrap().id, "generic");

        // refs/heads/main should match nothing
        let result = resolve_schema(&schemas, "refs/heads/main");
        assert!(result.is_none());
    }

    #[test]
    fn test_schema_resolution_no_match() {
        use agentic_repos::adp::glob::resolve_schema;

        let now = chrono::Utc::now();
        let schema = RefSchema {
            id: "task".to_string(),
            namespace_pattern: "refs/engram/task/*".to_string(),
            title: "Task".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            created_at: now,
            updated_at: now,
            schema: serde_json::Value::Null,
            ui: None,
        };

        let schemas = vec![schema];
        assert!(resolve_schema(&schemas, "refs/heads/main").is_none());
    }

    // -----------------------------------------------------------------------
    // ADP ref visibility tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_adp_ref_classification() {
        use agentic_repos::adp::{is_adp, is_private, is_public_adp};

        // Public ADP refs
        assert!(is_public_adp("refs/adp/schema/engram-task"));
        assert!(is_adp("refs/adp/schema/engram-task"));

        // Private ADP refs
        assert!(is_private("refs/adp/access/config"));
        assert!(is_private("refs/adp/config/repo"));
        assert!(!is_public_adp("refs/adp/access/config"));

        // User-defined private
        assert!(is_private("refs/secrets/my-secret"));
        assert!(!is_adp("refs/secrets/my-secret"));

        // Regular refs
        assert!(!is_adp("refs/heads/main"));
        assert!(!is_private("refs/heads/main"));
        assert!(!is_public_adp("refs/heads/main"));
    }

    #[test]
    fn test_version_sidecar_detection() {
        use agentic_repos::adp::is_version_sidecar;

        assert!(is_version_sidecar("refs/adp/schema/v1/engram-task"));
        assert!(is_version_sidecar("refs/adp/access/v3/config"));
        assert!(is_version_sidecar("refs/adp/config/v10/repo"));
        assert!(!is_version_sidecar("refs/adp/schema/engram-task"));
        assert!(!is_version_sidecar("refs/adp/access/config"));
        assert!(!is_version_sidecar("refs/heads/main"));
    }

    // -----------------------------------------------------------------------
    // Ref listing tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_refs_by_prefix() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        AdpStore::write_schema(&repo_path, &make_schema("task")).unwrap();
        AdpStore::write_schema(&repo_path, &make_schema("session")).unwrap();

        // List only task schema refs
        let refs = AdpStore::list_refs(&repo_path, "refs/adp/schema/task").unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "refs/adp/schema/task");

        // List all schema refs
        let refs = AdpStore::list_refs(&repo_path, "refs/adp/schema/").unwrap();
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_list_refs_empty() {
        let tmpdir = TempDir::new().unwrap();
        let repo_path = tmpdir.path().join("test.git");
        init_bare_repo(&repo_path);

        let refs = AdpStore::list_refs(&repo_path, "refs/adp/").unwrap();
        assert!(refs.is_empty());
    }

    // -----------------------------------------------------------------------
    // Bootstrap tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bootstrap_creates_system_repo() {
        use agentic_repos::adp::bootstrap::bootstrap_system_repo;

        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        let system_path = dir.path().join("_adp/system");
        assert!(system_path.exists());
    }

    #[test]
    fn test_bootstrap_is_idempotent() {
        use agentic_repos::adp::bootstrap::bootstrap_system_repo;

        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        let system_path = dir.path().join("_adp/system");
        assert!(system_path.exists());
    }

    #[test]
    fn test_bootstrap_writes_defaults() {
        use agentic_repos::adp::bootstrap::bootstrap_system_repo;
        use agentic_repos::adp::SERVER_CONFIG_REF;

        let dir = TempDir::new().unwrap();
        bootstrap_system_repo(dir.path()).unwrap();

        let system_path = dir.path().join("_adp/system");
        assert!(AdpStore::read_ref_blob(&system_path, SERVER_CONFIG_REF).is_ok());
        assert!(AdpStore::read_access_config(&system_path)
            .unwrap()
            .is_some());
    }
}

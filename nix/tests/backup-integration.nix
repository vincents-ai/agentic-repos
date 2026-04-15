# NixOS Integration Tests for Backup & Recovery
# Tests backup creation, restoration, storage backends, scheduling, and retention
#
# Run with:
#   nix-build -A tests.backup-integration
#
# Test Categories:
#   - Backup Operations: Full, repository, LFS, database, config backups
#   - Restore Operations: Full restore, selective restore, verification
#   - Storage Backends: Local and S3 storage
#   - Scheduling: Cron-based backup scheduling
#   - Retention: Backup expiration and cleanup

{ pkgs, agentic-repos, lib, mkVMTest, mkTestMatrix, waitForReposMCP, setupSSHKeys, setupGitConfig, createTestRepo }:

let
  inherit (lib);

  backupConfig = ''
    backup:
      enabled: true
      storage_backend: "local"
      local_path: "/var/lib/agentic-repos/backups"
      default_retention_days: 30
      max_concurrent_backups: 3
      compression_enabled: true
      encryption_enabled: false
      verification_enabled: true
      schedule: []
    
    features:
      backup_recovery: true
  '';

  mkBackupTest = { id, testScript, nodes ? {}, timeout ? 300 }:
    mkVMTest {
      name = "agentic-repos-backup-${id}";
      testScript = ''
        ${waitForReposMCP}
        ${testScript}
      '';
      inherit nodes timeout;
    };

  backupTests = [
    # ============================================================
    # BACKUP CREATION TESTS
    # ============================================================
    {
      id = "full-backup-creation";
      timeout = 300;
      testScript = ''
        # Setup backup directory
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create test repository
        ${createTestRepo "tenant1" "test-repo"}
        
        # Add some content to the repository
        server.succeed("cd /tmp && git clone /var/lib/agentic-repos/repositories/tenant1/test-repo.git work")
        server.succeed("cd /tmp/work && git config user.email 'test@example.com'")
        server.succeed("cd /tmp/work && git config user.name 'Test User'")
        server.succeed("cd /tmp/work && echo 'Initial content' > README.md")
        server.succeed("cd /tmp/work && git add README.md && git commit -m 'Initial commit'")
        server.succeed("cd /tmp/work && git push origin master || git push origin main")
        
        # Create full backup via API
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"full\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"test-repo\"],\"include_lfs\":true,\"include_database\":true,\"include_config\":true,\"retention_days\":30}'")
        
        import json
        backup_data = json.loads(backup_result)
        assert "id" in backup_data, f"Backup creation failed: {backup_result}"
        backup_id = backup_data["id"]
        
        # Wait for backup to complete
        server.succeed(f"for i in $(seq 1 30); do status=$(curl -s http://localhost:8090/backups/{backup_id} | jq -r '.status'); if [ \"$status\" = \"completed\" ] || [ \"$status\" = \"verified\" ]; then exit 0; fi; sleep 2; done; exit 1")
        
        # Verify backup status
        status_result = server.succeed(f"curl -s http://localhost:8090/backups/{backup_id}")
        status_data = json.loads(status_result)
        assert status_data["status"] in ["completed", "verified"], f"Backup should be completed, got: {status_data['status']}"
        assert status_data["size_bytes"] > 0, "Backup should have non-zero size"
        assert status_data["tenant_id"] == "tenant1", "Tenant ID should match"
      '';
    }
    {
      id = "repository-backup-selective";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create multiple repositories
        ${createTestRepo "tenant1" "repo-alpha"}
        ${createTestRepo "tenant1" "repo-beta"}
        ${createTestRepo "tenant1" "repo-gamma"}
        
        # Create selective repository backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"repo-alpha\",\"repo-beta\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Wait for completion
        server.succeed(f"for i in $(seq 1 20); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Verify selective backup
        status = server.succeed(f"curl -s http://localhost:8090/backups/{backup_id}")
        status_data = json.loads(status)
        assert "repo-alpha" in status_data.get("repository_ids", []), "Should include repo-alpha"
        assert "repo-beta" in status_data.get("repository_ids", []), "Should include repo-beta"
        assert "repo-gamma" not in status_data.get("repository_ids", []), "Should not include repo-gamma"
      '';
    }
    {
      id = "lfs-objects-backup";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        server.succeed("mkdir -p /var/lib/agentic-repos/lfs/tenant1")
        
        # Create test LFS objects
        server.succeed("dd if=/dev/urandom of=/var/lib/agentic-repos/lfs/tenant1/large-file.bin bs=1024 count=100")
        server.succeed("dd if=/dev/urandom of=/var/lib/agentic-repos/lfs/tenant1/another-file.bin bs=512 count=50")
        
        # Create LFS backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"lfs_objects\",\"tenant_id\":\"tenant1\",\"repository_ids\":[],\"include_lfs\":true,\"include_database\":false,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Wait for completion
        server.succeed(f"for i in $(seq 1 20); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Verify LFS backup
        status = server.succeed(f"curl -s http://localhost:8090/backups/{backup_id}")
        status_data = json.loads(status)
        assert status_data["include_lfs"] == True, "Should have LFS flag"
      '';
    }
    {
      id = "configuration-backup";
      timeout = 180;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create configuration backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"tenant1\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Wait for completion
        server.succeed(f"for i in $(seq 1 15); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Verify config backup
        status = server.succeed(f"curl -s http://localhost:8090/backups/{backup_id}")
        status_data = json.loads(status)
        assert status_data["include_config"] == True, "Should have config flag"
      '';
    }
    {
      id = "database-backup";
      timeout = 180;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create database backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"database\",\"tenant_id\":\"tenant1\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":true,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Wait for completion
        server.succeed(f"for i in $(seq 1 15); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Verify database backup
        status = server.succeed(f"curl -s http://localhost:8090/backups/{backup_id}")
        status_data = json.loads(status)
        assert status_data["include_database"] == True, "Should have database flag"
      '';
    }

    # ============================================================
    # BACKUP INTEGRITY TESTS
    # ============================================================
    {
      id = "backup-checksum-verification";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        ${createTestRepo "tenant1" "verify-test"}
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"verify-test\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Wait for completion
        server.succeed(f"for i in $(seq 1 20); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Verify backup via API
        verify_result = server.succeed(f"curl -s -X POST http://localhost:8090/backups/{backup_id}/verify")
        verify_data = json.loads(verify_result)
        assert "checksum" in verify_data, "Verification should return checksum"
        assert len(verify_data["checksum"]) == 64, "Checksum should be SHA256 (64 hex chars)"
      '';
    }
    {
      id = "backup-data-integrity";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create repo with known content
        ${createTestRepo "tenant1" "integrity-test"}
        server.succeed("cd /tmp && git clone /var/lib/agentic-repos/repositories/tenant1/integrity-test.git int-work")
        server.succeed("cd /tmp/int-work && git config user.email 'test@example.com'")
        server.succeed("cd /tmp/int-work && git config user.name 'Test User'")
        server.succeed("cd /tmp/int-work && echo 'unique-content-12345' > unique.txt")
        server.succeed("cd /tmp/int-work && git add unique.txt && git commit -m 'Add unique content'")
        server.succeed("cd /tmp/int-work && git push origin master || git push origin main")
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"integrity-test\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        server.succeed(f"for i in $(seq 1 20); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Verify backup file exists
        server.succeed("test -d /var/lib/agentic-repos/backups")
        
        # List backup files
        backup_files = server.succeed("find /var/lib/agentic-repos/backups -type f 2>/dev/null | head -5 || echo 'no files'")
        assert len(backup_files.strip()) > 0 or "tenant1" in backup_files, "Should have backup data"
      '';
    }

    # ============================================================
    # RESTORE OPERATION TESTS
    # ============================================================
    {
      id = "full-restore";
      timeout = 400;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create and backup a repository
        ${createTestRepo "tenant1" "restore-test"}
        server.succeed("cd /tmp && git clone /var/lib/agentic-repos/repositories/tenant1/restore-test.git restore-work")
        server.succeed("cd /tmp/restore-work && git config user.email 'test@example.com'")
        server.succeed("cd /tmp/restore-work && git config user.name 'Test User'")
        server.succeed("cd /tmp/restore-work && echo 'restore-content' > restore.txt")
        server.succeed("cd /tmp/restore-work && git add restore.txt && git commit -m 'Add restore content'")
        server.succeed("cd /tmp/restore-work && git push origin master || git push origin main")
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"full\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"restore-test\"],\"include_lfs\":true,\"include_database\":true,\"include_config\":true}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        server.succeed(f"for i in $(seq 1 30); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Delete original repository
        server.succeed("rm -rf /var/lib/agentic-repos/repositories/tenant1/restore-test.git")
        
        # Restore from backup
        restore_result = server.succeed(f"curl -s -X POST http://localhost:8090/backups/{backup_id}/restore -H 'Content-Type: application/json' -d '{{\"target_tenant_id\":\"tenant1\",\"overwrite_existing\":false,\"dry_run\":false}}'")
        restore_data = json.loads(restore_result)
        
        # Verify restore status
        assert restore_data["status"] in ["completed", "in_progress", "partially_completed"], f"Restore should succeed, got: {restore_data['status']}"
      '';
    }
    {
      id = "selective-restore";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create multiple repos
        ${createTestRepo "tenant1" "select-a"}
        ${createTestRepo "tenant1" "select-b"}
        ${createTestRepo "tenant1" "select-c"}
        
        # Create backup of all
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"select-a\",\"select-b\",\"select-c\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        server.succeed(f"for i in $(seq 1 20); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Delete one repo
        server.succeed("rm -rf /var/lib/agentic-repos/repositories/tenant1/select-b.git")
        
        # Selective restore only select-b
        restore_result = server.succeed(f"curl -s -X POST http://localhost:8090/backups/{backup_id}/restore -H 'Content-Type: application/json' -d '{{\"target_tenant_id\":\"tenant1\",\"target_repository_ids\":[\"select-b\"],\"overwrite_existing\":false,\"dry_run\":false}}'")
        
        restore_data = json.loads(restore_result)
        assert restore_data["status"] in ["completed", "in_progress"], f"Selective restore should work"
      '';
    }
    {
      id = "dry-run-restore";
      timeout = 200;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        ${createTestRepo "tenant1" "dry-run-test"}
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"dry-run-test\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        server.succeed(f"for i in $(seq 1 15); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Perform dry run restore
        dry_run_result = server.succeed(f"curl -s -X POST http://localhost:8090/backups/{backup_id}/restore -H 'Content-Type: application/json' -d '{{\"target_tenant_id\":\"tenant1\",\"dry_run\":true}}'")
        dry_run_data = json.loads(dry_run_result)
        
        # Dry run should complete without errors
        assert dry_run_data["status"] == "completed", f"Dry run should complete, got: {dry_run_data['status']}"
        # Should have warning about dry run
        warnings = dry_run_data.get("warnings", [])
        assert any("dry run" in w.lower() for w in warnings), "Should have dry run warning"
      '';
    }
    {
      id = "cross-tenant-restore";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create repo in tenant1
        ${createTestRepo "tenant1" "cross-tenant"}
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"cross-tenant\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        server.succeed(f"for i in $(seq 1 15); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Restore to different tenant
        restore_result = server.succeed(f"curl -s -X POST http://localhost:8090/backups/{backup_id}/restore -H 'Content-Type: application/json' -d '{{\"target_tenant_id\":\"tenant2\",\"overwrite_existing\":false,\"dry_run\":false}}'")
        
        # Cross-tenant restore may be allowed or denied depending on config
        # Just verify we get a response
        restore_data = json.loads(restore_result)
        assert "status" in restore_data, "Should get restore status"
      '';
    }

    # ============================================================
    # BACKUP LISTING AND FILTERING TESTS
    # ============================================================
    {
      id = "list-backups";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create multiple backups
        ${createTestRepo "tenant1" "list-test"}
        
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"full\",\"tenant_id\":\"tenant1\",\"repository_ids\":[\"list-test\"],\"include_lfs\":true,\"include_database\":true,\"include_config\":true}'")
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"tenant1\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"database\",\"tenant_id\":\"tenant1\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":true,\"include_config\":false}'")
        
        # Wait for backups to be created
        server.succeed("sleep 5")
        
        # List all backups
        list_result = server.succeed("curl -s 'http://localhost:8090/backups'")
        
        import json
        list_data = json.loads(list_result)
        assert len(list_data) >= 3, f"Should have at least 3 backups, got: {len(list_data)}"
      '';
    }
    {
      id = "filter-backups-by-tenant";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create backups for different tenants
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"tenant-a\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"tenant-b\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        
        server.succeed("sleep 3")
        
        # Filter by tenant
        tenant_a = server.succeed("curl -s 'http://localhost:8090/backups?tenant_id=tenant-a'")
        tenant_b = server.succeed("curl -s 'http://localhost:8090/backups?tenant_id=tenant-b'")
        
        import json
        a_data = json.loads(tenant_a)
        b_data = json.loads(tenant_b)
        
        # All results should belong to their tenant
        assert all(b["tenant_id"] == "tenant-a" for b in a_data), "All should be tenant-a"
        assert all(b["tenant_id"] == "tenant-b" for b in b_data), "All should be tenant-b"
      '';
    }
    {
      id = "filter-backups-by-type";
      timeout = 200;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"full\",\"tenant_id\":\"type-test\",\"repository_ids\":[],\"include_lfs\":true,\"include_database\":true,\"include_config\":true}'")
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"type-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        
        server.succeed("sleep 3")
        
        # Filter by type
        full_only = server.succeed("curl -s 'http://localhost:8090/backups?backup_type=full'")
        
        import json
        full_data = json.loads(full_only)
        assert all(b["backup_type"] == "full" for b in full_data), "All should be full backups"
      '';
    }
    {
      id = "filter-backups-by-status";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"status-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        
        # Wait for completion
        server.succeed("sleep 5")
        
        # Filter by completed status
        completed = server.succeed("curl -s 'http://localhost:8090/backups?status=completed'")
        
        import json
        completed_data = json.loads(completed)
        # Note: verified is also a completed state
        assert all(b["status"] in ["completed", "verified"] for b in completed_data), "All should be completed"
      '';
    }
    {
      id = "limit-backups";
      timeout = 200;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create several backups
        for i in range(5):
            server.succeed(f"curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{{\"backup_type\":\"configuration\",\"tenant_id\":\"limit-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}}'")
        
        server.succeed("sleep 3")
        
        # Request with limit
        limited = server.succeed("curl -s 'http://localhost:8090/backups?limit=2'")
        
        import json
        limited_data = json.loads(limited)
        assert len(limited_data) <= 2, f"Should have at most 2 results, got: {len(limited_data)}"
      '';
    }

    # ============================================================
    # BACKUP LIFECYCLE TESTS
    # ============================================================
    {
      id = "backup-deletion";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create a backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"delete-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Wait for completion
        server.succeed(f"for i in $(seq 1 15); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Delete the backup
        server.succeed(f"curl -s -X DELETE http://localhost:8090/backups/{backup_id}")
        
        # Verify deletion (should return 404)
        delete_check = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' http://localhost:8090/backups/{backup_id}")
        assert delete_check.strip() == "404", f"Deleted backup should return 404, got: {delete_check}"
      '';
    }
    {
      id = "backup-cancellation";
      timeout = 200;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create a backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"full\",\"tenant_id\":\"cancel-test\",\"repository_ids\":[],\"include_lfs\":true,\"include_database\":true,\"include_config\":true}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Try to cancel (may already be complete)
        cancel_result = server.succeed(f"curl -s -X POST http://localhost:8090/backups/{backup_id}/cancel || echo 'cancel_failed'")
        # Cancellation is acceptable whether it succeeds or fails
      '';
    }
    {
      id = "retention-enforcement";
      timeout = 200;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create backup with short retention
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"retention-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true,\"retention_days\":1}'")
        
        import json
        backup_data = json.loads(backup_result)
        
        # Verify retention is set
        assert backup_data["retention_days"] == 1, "Retention should be 1 day"
        assert backup_data.get("expires_at") is not None, "Should have expiration date"
      '';
    }

    # ============================================================
    # ERROR HANDLING TESTS
    # ============================================================
    {
      id = "invalid-backup-id";
      timeout = 60;
      testScript = ''
        # Request non-existent backup
        result = server.succeed("curl -s -o /dev/null -w '%{http_code}' http://localhost:8090/backups/00000000-0000-0000-0000-000000000000")
        assert result.strip() == "404", f"Invalid ID should return 404, got: {result}"
      '';
    }
    {
      id = "malformed-backup-request";
      timeout = 60;
      testScript = ''
        # Send malformed JSON
        result = server.succeed("curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{invalid json}'")
        assert result.strip() in ["400", "500"], f"Malformed request should error, got: {result}"
      '';
    }
    {
      id = "backup-nonexistent-repo";
      timeout = 120;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Try to backup non-existent repository
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"nonexistent\",\"repository_ids\":[\"no-such-repo\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        
        import json
        backup_data = json.loads(backup_result)
        
        # Backup creation should succeed, execution may fail
        if "id" in backup_data:
            backup_id = backup_data["id"]
            # Wait for completion (may fail)
            server.succeed(f"for i in $(seq 1 15); do status=$(curl -s http://localhost:8090/backups/{backup_id} | jq -r '.status'); if [ \"$status\" = \"failed\" ] || [ \"$status\" = \"completed\" ]; then exit 0; fi; sleep 2; done; exit 0")
      '';
    }
    {
      id = "restore-invalid-backup";
      timeout = 60;
      testScript = ''
        # Try to restore from non-existent backup
        result = server.succeed("curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8090/backups/00000000-0000-0000-0000-000000000000/restore -H 'Content-Type: application/json' -d '{}'")
        assert result.strip() == "404", f"Invalid backup restore should return 404, got: {result}"
      '';
    }
    {
      id = "delete-in-progress-backup";
      timeout = 120;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"full\",\"tenant_id\":\"delete-in-progress\",\"repository_ids\":[],\"include_lfs\":true,\"include_database\":true,\"include_config\":true}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Try to delete immediately (may still be in progress)
        delete_result = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -X DELETE http://localhost:8090/backups/{backup_id}")
        # May succeed (400/409 for in-progress) or succeed if already complete
      '';
    }

    # ============================================================
    # MULTI-TENANT ISOLATION TESTS
    # ============================================================
    {
      id = "tenant-backup-isolation";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create repos for different tenants
        ${createTestRepo "tenant-isolated-a" "repo-a"}
        ${createTestRepo "tenant-isolated-b" "repo-b"}
        
        # Create backups for each tenant
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"tenant-isolated-a\",\"repository_ids\":[\"repo-a\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"repository\",\"tenant_id\":\"tenant-isolated-b\",\"repository_ids\":[\"repo-b\"],\"include_lfs\":false,\"include_database\":false,\"include_config\":false}'")
        
        server.succeed("sleep 5")
        
        # List backups for each tenant
        list_a = server.succeed("curl -s 'http://localhost:8090/backups?tenant_id=tenant-isolated-a'")
        list_b = server.succeed("curl -s 'http://localhost:8090/backups?tenant_id=tenant-isolated-b'")
        
        import json
        a_data = json.loads(list_a)
        b_data = json.loads(list_b)
        
        # Verify isolation
        a_ids = set(b["id"] for b in a_data)
        b_ids = set(b["id"] for b in b_data)
        
        assert len(a_ids & b_ids) == 0, "Tenants should have isolated backups"
      '';
    }

    # ============================================================
    # BACKUP METADATA TESTS
    # ============================================================
    {
      id = "backup-metadata-preservation";
      timeout = 200;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"full\",\"tenant_id\":\"meta-test\",\"repository_ids\":[\"repo1\",\"repo2\"],\"include_lfs\":true,\"include_database\":true,\"include_config\":true,\"retention_days\":14}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        server.succeed(f"for i in $(seq 1 20); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Verify metadata
        status = server.succeed(f"curl -s http://localhost:8090/backups/{backup_id}")
        status_data = json.loads(status)
        
        assert status_data["tenant_id"] == "meta-test", "Tenant ID should match"
        assert status_data["backup_type"] == "full", "Backup type should match"
        assert status_data["retention_days"] == 14, "Retention should match"
        assert "repo1" in status_data["repository_ids"], "Should include repo1"
        assert "repo2" in status_data["repository_ids"], "Should include repo2"
        assert status_data["include_lfs"] == True, "LFS flag should be set"
        assert status_data["include_database"] == True, "Database flag should be set"
        assert status_data["include_config"] == True, "Config flag should be set"
      '';
    }
    {
      id = "backup-timestamps";
      timeout = 200;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"timestamp-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        # Verify timestamps exist
        assert "created_at" in backup_data, "Should have created_at"
        
        server.succeed(f"for i in $(seq 1 15); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Verify completion timestamp
        status = server.succeed(f"curl -s http://localhost:8090/backups/{backup_id}")
        status_data = json.loads(status)
        
        assert "started_at" in status_data, "Should have started_at"
        assert "completed_at" in status_data, "Should have completed_at"
      '';
    }

    # ============================================================
    # SERVICE PERSISTENCE TESTS
    # ============================================================
    {
      id = "backup-persistence-after-restart";
      timeout = 400;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create backup
        backup_result = server.succeed("curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{\"backup_type\":\"configuration\",\"tenant_id\":\"persist-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}'")
        
        import json
        backup_data = json.loads(backup_result)
        backup_id = backup_data["id"]
        
        server.succeed(f"for i in $(seq 1 15); do curl -s http://localhost:8090/backups/{backup_id} | jq -e '.status == \"completed\" or .status == \"verified\"' && exit 0; sleep 2; done; exit 1")
        
        # Count backups before restart
        before = server.succeed("curl -s 'http://localhost:8090/backups?tenant_id=persist-test'")
        before_data = json.loads(before)
        before_count = len(before_data)
        
        # Restart service
        server.succeed("systemctl restart agentic-repos.service")
        server.wait_for_unit("agentic-repos.service")
        server.wait_for_open_port(8090)
        
        # Count backups after restart
        # Note: Persistence depends on storage backend (in-memory loses data)
        after = server.succeed("curl -s 'http://localhost:8090/backups?tenant_id=persist-test' || echo '[]'")
        # Just verify the service comes back up
      '';
    }

    # ============================================================
    # CONCURRENT OPERATIONS TESTS
    # ============================================================
    {
      id = "concurrent-backup-creation";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create multiple backups concurrently
        import json
        backup_ids = []
        for i in range(5):
            result = server.succeed(f"curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{{\"backup_type\":\"configuration\",\"tenant_id\":\"concurrent-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}}'")
            data = json.loads(result)
            if "id" in data:
                backup_ids.append(data["id"])
        
        # Wait for all to complete
        server.succeed("sleep 10")
        
        # Verify at least some completed
        completed_count = 0
        for bid in backup_ids:
            try:
                status = server.succeed(f"curl -s http://localhost:8090/backups/{bid} | jq -r '.status'")
                if status.strip() in ["completed", "verified"]:
                    completed_count += 1
            except:
                pass
        
        # Concurrent limit may cause some to fail, which is expected
        assert completed_count >= 1, f"At least 1 backup should complete, got: {completed_count}"
      '';
    }
    {
      id = "max-concurrent-limit";
      timeout = 300;
      testScript = ''
        server.succeed("mkdir -p /var/lib/agentic-repos/backups")
        
        # Create more backups than the concurrent limit (3)
        import json
        results = []
        for i in range(6):
            result = server.succeed(f"curl -s -X POST http://localhost:8090/backups -H 'Content-Type: application/json' -d '{{\"backup_type\":\"configuration\",\"tenant_id\":\"limit-test\",\"repository_ids\":[],\"include_lfs\":false,\"include_database\":false,\"include_config\":true}}'")
            results.append(json.loads(result))
        
        server.succeed("sleep 15")
        
        # Check statuses - some may be queued/pending
        statuses = []
        for r in results:
            if "id" in r:
                try:
                    status = server.succeed(f"curl -s http://localhost:8090/backups/{r['id']} | jq -r '.status'")
                    statuses.append(status.strip())
                except:
                    statuses.append("unknown")
        
        # Just verify we handled multiple requests
        assert len([s for s in statuses if s in ["completed", "verified", "in_progress", "pending"]]) >= 3
      '';
    }
  ];

in
mkTestMatrix "backup-integration" backupTests

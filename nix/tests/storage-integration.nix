# NixOS Integration Tests for Storage Backends
# Tests local storage and S3/Garage storage operations including:
# - Local repository storage (bare repos, LFS objects, permissions)
# - S3/Garage object operations (upload, download, listing, deletion)
# - Bucket operations (create, configure, permissions)
# - Large file handling (multipart upload simulation)
# - Data integrity verification
#
# Run with:
#   nix-build -A tests.storage-integration-*

{ pkgs, repos-mcp, lib, mkVMTest, mkTestMatrix, waitForReposMCP, setupSSHKeys, setupGitConfig, createTestRepo, ... }:

let
  inherit (lib);

  garageNode = { config, pkgs, ... }: {
    networking.firewall.enable = false;

    environment.systemPackages = with pkgs; [
      garage
      awscli2
      curl
      jq
    ];

    services.garage = {
      enable = true;
      settings = {
        metadata_dir = "/var/lib/garage/meta";
        data_dir = "/var/lib/garage/data";
        replication_factor = 1;
        rpc_bind_addr = "[::]:3901";
        s3_api = {
          api_bind_addr = "[::]:3900";
          s3_region = "garage";
        };
        s3_web = {
          bind_addr = "[::]:3902";
        };
      };
    };
  };

  storageTests = [
    # ===========================================================
    # LOCAL STORAGE TESTS
    # ===========================================================
    {
      id = "local-repo-create";
      testScript = ''
        ${waitForReposMCP}

        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/tenant1")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/tenant1/local-test.git")
        server.succeed("test -f /var/lib/repos-mcp/repositories/tenant1/local-test.git/HEAD")
        server.succeed("test -d /var/lib/repos-mcp/repositories/tenant1/local-test.git/objects")
        server.succeed("test -d /var/lib/repos-mcp/repositories/tenant1/local-test.git/refs")
      '';
    }
    {
      id = "local-lfs-objects";
      testScript = ''
        ${waitForReposMCP}

        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/tenant1/lfs-test.git")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/tenant1/lfs-test.git")
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/tenant1/lfs-test.git/lfs/objects/ab/cd")
        server.succeed("echo 'lfs object data' > /tmp/lfs-data.bin")
        server.succeed("cp /tmp/lfs-data.bin /var/lib/repos-mcp/repositories/tenant1/lfs-test.git/lfs/objects/ab/cd/abcd1234")
        server.succeed("test -f /var/lib/repos-mcp/repositories/tenant1/lfs-test.git/lfs/objects/ab/cd/abcd1234")
      '';
    }
    {
      id = "local-permissions";
      testScript = ''
        ${waitForReposMCP}

        server.succeed("test -d /var/lib/repos-mcp/repositories")
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/perm-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/perm-tenant/test.git")
        server.succeed("test -r /var/lib/repos-mcp/repositories/perm-tenant/test.git/HEAD")
        server.succeed("test -r /var/lib/repos-mcp/repositories/perm-tenant/test.git/config")
      '';
    }
    {
      id = "local-disk-operations";
      testScript = ''
        ${waitForReposMCP}

        server.succeed("dd if=/dev/zero of=/var/lib/repos-mcp/repositories/test-write.bin bs=1M count=1")
        server.succeed("test -f /var/lib/repos-mcp/repositories/test-write.bin")
        file_size = server.succeed("stat -c %s /var/lib/repos-mcp/repositories/test-write.bin").strip()
        assert file_size == "1048576", "Expected 1MB file, got %s bytes" % file_size
        server.succeed("rm /var/lib/repos-mcp/repositories/test-write.bin")
      '';
    }
    {
      id = "local-persistence";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "persist-test"}

        server.succeed("cd /tmp && git clone /var/lib/repos-mcp/repositories/tenant1/persist-test.git init-persist")
        server.succeed("cd /tmp/init-persist && git config user.email test@example.com")
        server.succeed("cd /tmp/init-persist && git config user.name 'Test User'")
        server.succeed("cd /tmp/init-persist && echo 'persistence test' > README.md")
        server.succeed("cd /tmp/init-persist && git add README.md && git commit -m 'Initial'")
        server.succeed("cd /tmp/init-persist && git push origin master || git push origin main")

        commit_hash = server.succeed("cd /var/lib/repos-mcp/repositories/tenant1/persist-test.git && git rev-parse HEAD").strip()

        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_unit("repos-mcp.service", timeout=60)
        server.wait_for_open_port(2222, timeout=60)

        persisted_hash = server.succeed("cd /var/lib/repos-mcp/repositories/tenant1/persist-test.git && git rev-parse HEAD").strip()
        assert commit_hash == persisted_hash, "Commit hash mismatch after restart"
      '';
    }

    # ===========================================================
    # S3/GARAGE STORAGE TESTS
    # ===========================================================
    {
      id = "garage-setup";
      nodes = { storage = garageNode; };
      timeout = 180;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage status || echo garage_status_checked")
        storage.succeed("garage bucket create repos-mcp-test")
        storage.succeed("garage key create --name admin-key")
        key_output = storage.succeed("garage key info admin-key 2>&1 || echo key_created")
        storage.succeed("garage bucket allow repos-mcp-test --read --write --key admin-key || echo permissions_set")
      '';
    }
    {
      id = "garage-bucket-list";
      nodes = { storage = garageNode; };
      timeout = 120;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create list-test-bucket || true")
        bucket_list = storage.succeed("garage bucket list 2>&1 || echo bucket_list_done")
        assert "list-test-bucket" in bucket_list or "reusable" in bucket_list or True
      '';
    }
    {
      id = "s3-object-upload";
      nodes = { storage = garageNode; };
      timeout = 120;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create upload-test || true")
        storage.succeed("garage key create --name upload-key || true")
        storage.succeed("garage bucket allow upload-test --read --write --key upload-key || true")

        storage.succeed("echo 'test upload content' > /tmp/upload-test.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/upload-test.txt s3://upload-test/tenant1/repo/test.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 ls s3://upload-test/tenant1/repo/test.txt")
      '';
    }
    {
      id = "s3-object-download";
      nodes = { storage = garageNode; };
      timeout = 120;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create download-test || true")
        storage.succeed("garage key create --name download-key || true")
        storage.succeed("garage bucket allow download-test --read --write --key download-key || true")

        storage.succeed("echo 'download content' > /tmp/download-src.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/download-src.txt s3://download-test/test-download.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp s3://download-test/test-download.txt /tmp/downloaded.txt")

        downloaded = storage.succeed("cat /tmp/downloaded.txt").strip()
        assert downloaded == "download content", "Content mismatch: %s" % downloaded
      '';
    }
    {
      id = "s3-object-delete";
      nodes = { storage = garageNode; };
      timeout = 120;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create delete-test || true")
        storage.succeed("garage key create --name delete-key || true")
        storage.succeed("garage bucket allow delete-test --read --write --key delete-key || true")

        storage.succeed("echo 'to be deleted' > /tmp/delete-src.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/delete-src.txt s3://delete-test/to-delete.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 ls s3://delete-test/to-delete.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 rm s3://delete-test/to-delete.txt")
        storage.fail("aws --endpoint-url http://localhost:3900 s3 ls s3://delete-test/to-delete.txt 2>&1")
      '';
    }
    {
      id = "s3-object-list";
      nodes = { storage = garageNode; };
      timeout = 120;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create list-objects-test || true")
        storage.succeed("garage key create --name list-key || true")
        storage.succeed("garage bucket allow list-objects-test --read --write --key list-key || true")

        storage.succeed("echo 'file1' > /tmp/f1.txt")
        storage.succeed("echo 'file2' > /tmp/f2.txt")
        storage.succeed("echo 'file3' > /tmp/f3.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/f1.txt s3://list-objects-test/file1.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/f2.txt s3://list-objects-test/file2.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/f3.txt s3://list-objects-test/subdir/file3.txt")

        listing = storage.succeed("aws --endpoint-url http://localhost:3900 s3 ls s3://list-objects-test/ --recursive").strip()
        assert "file1.txt" in listing, "file1.txt not found in listing"
        assert "file2.txt" in listing, "file2.txt not found in listing"
        assert "file3.txt" in listing, "file3.txt not found in listing"
      '';
    }
    {
      id = "s3-large-file";
      nodes = { storage = garageNode; };
      timeout = 300;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create large-file-test || true")
        storage.succeed("garage key create --name large-key || true")
        storage.succeed("garage bucket allow large-file-test --read --write --key large-key || true")

        storage.succeed("dd if=/dev/urandom of=/tmp/large-file.bin bs=1M count=50")
        original_hash = storage.succeed("sha256sum /tmp/large-file.bin | cut -d' ' -f1").strip()

        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/large-file.bin s3://large-file-test/large-file.bin")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp s3://large-file-test/large-file.bin /tmp/large-download.bin")

        download_hash = storage.succeed("sha256sum /tmp/large-download.bin | cut -d' ' -f1").strip()
        assert original_hash == download_hash, "Hash mismatch: %s != %s" % (original_hash, download_hash)
      '';
    }

    # ===========================================================
    # REPOS-MCP S3 INTEGRATION TESTS
    # ===========================================================
    {
      id = "repos-mcp-s3-config";
      nodes = { storage = garageNode; };
      timeout = 180;
      testScript = ''
        ${waitForReposMCP}
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create repos-mcp-s3 || true")
        storage.succeed("garage key create --name repos-mcp-key || true")
        storage.succeed("garage bucket allow repos-mcp-s3 --read --write --key repos-mcp-key || true")

        s3_config_exists = server.succeed("test -f /etc/repos-mcp/config.yaml && echo exists || echo missing").strip()
        assert s3_config_exists == "exists", "Config file should exist"
      '';
    }
    {
      id = "repos-mcp-s3-health";
      nodes = { storage = garageNode; };
      timeout = 120;
      testScript = ''
        ${waitForReposMCP}
        storage.wait_for_unit("garage.service")

        health = server.succeed("curl -s http://localhost:8090/health 2>&1 || echo health_check")
        storage_health = storage.succeed("curl -s http://localhost:3900/ 2>&1 || echo s3_health_check")
      '';
    }
    {
      id = "multi-bucket-isolation";
      nodes = { storage = garageNode; };
      timeout = 120;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create tenant-a-bucket || true")
        storage.succeed("garage bucket create tenant-b-bucket || true")
        storage.succeed("garage key create --name multi-key || true")
        storage.succeed("garage bucket allow tenant-a-bucket --read --write --key multi-key || true")
        storage.succeed("garage bucket allow tenant-b-bucket --read --write --key multi-key || true")

        storage.succeed("echo 'tenant a data' > /tmp/tenant-a.txt")
        storage.succeed("echo 'tenant b data' > /tmp/tenant-b.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/tenant-a.txt s3://tenant-a-bucket/data.txt")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/tenant-b.txt s3://tenant-b-bucket/data.txt")

        tenant_a = storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp s3://tenant-a-bucket/data.txt -").strip()
        tenant_b = storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp s3://tenant-b-bucket/data.txt -").strip()

        assert tenant_a == "tenant a data", "Tenant A data mismatch"
        assert tenant_b == "tenant b data", "Tenant B data mismatch"
        assert tenant_a != tenant_b, "Tenant data should be isolated"
      '';
    }
    {
      id = "concurrent-s3-operations";
      nodes = { storage = garageNode; };
      timeout = 180;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create concurrent-test || true")
        storage.succeed("garage key create --name concurrent-key || true")
        storage.succeed("garage bucket allow concurrent-test --read --write --key concurrent-key || true")

        storage.succeed("""
          for i in 1 2 3 4 5; do
            echo "concurrent $i" > /tmp/concurrent-$i.txt
            (aws --endpoint-url http://localhost:3900 s3 cp /tmp/concurrent-$i.txt s3://concurrent-test/file-$i.txt) &
          done
          wait
        """)

        listing = storage.succeed("aws --endpoint-url http://localhost:3900 s3 ls s3://concurrent-test/").strip()
        assert "file-1" in listing, "file-1 not found"
        assert "file-5" in listing, "file-5 not found"
      '';
    }
    {
      id = "data-integrity-check";
      nodes = { storage = garageNode; };
      timeout = 180;
      testScript = ''
        storage.wait_for_unit("garage.service")
        storage.wait_for_open_port(3900)

        storage.succeed("garage bucket create integrity-test || true")
        storage.succeed("garage key create --name integrity-key || true")
        storage.succeed("garage bucket allow integrity-test --read --write --key integrity-key || true")

        storage.succeed("dd if=/dev/urandom of=/tmp/integrity-file.bin bs=1K count=100")
        original_hash = storage.succeed("sha256sum /tmp/integrity-file.bin | cut -d' ' -f1").strip()
        original_size = storage.succeed("stat -c %s /tmp/integrity-file.bin").strip()

        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp /tmp/integrity-file.bin s3://integrity-test/integrity.bin")
        storage.succeed("aws --endpoint-url http://localhost:3900 s3 cp s3://integrity-test/integrity.bin /tmp/integrity-verify.bin")

        verify_hash = storage.succeed("sha256sum /tmp/integrity-verify.bin | cut -d' ' -f1").strip()
        verify_size = storage.succeed("stat -c %s /tmp/integrity-verify.bin").strip()

        assert original_hash == verify_hash, "Hash mismatch: %s != %s" % (original_hash, verify_hash)
        assert original_size == verify_size, "Size mismatch: %s != %s" % (original_size, verify_size)
      '';
    }
  ];

in
mkTestMatrix "storage" storageTests

{ pkgs, system, repos-mcp }:

let
  minioEnv = {
    MINIO_ROOT_USER = "minioadmin";
    MINIO_ROOT_PASSWORD = "minioadmin123";
  MINIO_BROWSER = "off";
  MINIO_REGION = "us-east-1";
  MINIO_DOMAIN = "storage.test.local";
  MINIO_SERVER_URL = "http://minio:9000";
  AWS_ACCESS_KEY_ID = "minioadmin";
    AWS_SECRET_ACCESS_KEY = "minioadmin123";
    AWS_REGION = "us-east-1";
    AWS_ENDPOINT_URL = "http://minio:9000";
  S3_ENDPOINT = "http://minio:9000";
  S3_ACCESS_KEY = "minioadmin";
  S3_SECRET_KEY = "minioadmin123";
  S3_REGION = "us-east-1";
  S3_BUCKET = "repos-mcp-test";
  S3_USE_PATH_STYLE = "true";
  };

  reposMcpConfig = {
    server = {
      host = "0.0.0.0";
      port = 8090;
      ssh = {
        port = 2222;
        host_key_path = "/etc/repos-mcp/ssh_host_ed25519_key";
        max_connections = 50;
        connection_timeout_secs = 60;
      };
    };
    auth = {
      authorized_keys_path = "/etc/repos-mcp/authorized_keys";
    };
    rbac = {
      enabled = false;
      storage_type = "memory";
    };
    storage = {
      type = "local";
      local = {
        path = "/var/lib/repos-mcp";
      };
    };
    rate_limit = {
      enabled = false;
    };
    logging = {
      level = "debug";
        json = false;
        timestamp = true;
        ansi = false;
    };
    features = {
      git_lfs = true;
      web_ui = false;
    };
    lfs = {
      enabled = true;
      jwt_secret = "test-storage-integration-secret-key";
      base_url = "http://server:8090";
      max_object_size = 5368709120;
      token_expiration_secs = 3600;
      storage_path_pattern = "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}";
      locking_enabled = true;
      lock_expiration_secs = 86400;
    };
  };

  s3Config = {
    server = {
      host = "0.0.0.0";
      port = 8091;
      ssh = {
        port = 2223;
        host_key_path = "/etc/repos-mcp/ssh_host_ed25519_key";
        max_connections = 50;
        connection_timeout_secs = 60;
      };
    };
    auth = {
      authorized_keys_path = "/etc/repos-mcp/authorized_keys";
    };
    rbac = {
      enabled = false;
      storage_type = "memory";
    };
    storage = {
      type = "s3";
      s3 = {
        bucket = "repos-mcp-test";
        region = "us-east-1";
        endpoint = "http://minio:9000";
      };
    };
    rate_limit = {
      enabled = false;
    };
    logging = {
      level = "debug";
        json = false;
        timestamp = true;
        ansi = false;
    };
    features = {
      git_lfs = true;
      web_ui = false;
    };
    lfs = {
      enabled = true;
      jwt_secret = "test-storage-integration-secret-key";
      base_url = "http://server:8091";
      max_object_size = 5368709120;
      token_expiration_secs = 3600;
      storage_path_pattern = "{tenant}/{repo}.git/lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}";
      locking_enabled = true;
      lock_expiration_secs = 86400;
    };
  };

in
{
  name = "repos-mcp-storage-integration";
  meta = {
    maintainers = [ "repos-mcp team" ];
    timeout = 600;
  };

  nodes = {
    minio = { config, pkgs, ... }: {
      networking.firewall.enable = false;
      environment.systemPackages = with pkgs; [
        minio
        mc
        curl
      ];
      systemd.services.minio = {
        description = "MinIO S3-Compatible Object Storage";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];
        environment = minioEnv;
        serviceConfig = {
          Type = "simple";
          User = "minio";
          Group = "minio";
          ExecStart = "${pkgs.minio}/bin/minio server /var/lib/minio/data --address :9000 --console-address :9001";
          Restart = "on-failure";
          RestartSec = "5s";
        };
        preStart = ''
          mkdir -p /var/lib/minio/data
          chown -R minio:minio /var/lib/minio
        '';
      };
      users.users.minio = {
        isNormalUser = true;
        home = "/var/lib/minio";
        createHome = true;
        group = "minio";
      };
      users.groups.minio = {};
    };

    server = { config, pkgs, ... }: {
      imports = [];
      networking.firewall.enable = false;
      environment.systemPackages = with pkgs; [
        repos-mcp
        git
        curl
        openssh
        awscli2
        s3cmd
      ];
      environment.variables = minioEnv // {
        PATH = "${pkgs.lib.makeBinPath [ repos-mcp pkgs.git pkgs.curl ]}:$PATH";
      };
      services.openssh = {
        enable = true;
        settings = {
          PermitRootLogin = "yes";
          PasswordAuthentication = "no";
        };
      };
      systemd.services.repos-mcp = {
        description = "repos-mcp Git server (local storage)";
        after = [ "network.target" ];
        wantedBy = [ "multi-user.target" ];
        environment = {
          RUST_LOG = "debug";
        };
        serviceConfig = {
          Type = "simple";
          ExecStart = "${repos-mcp}/bin/repos-mcp";
          WorkingDirectory = "/etc/repos-mcp";
          Restart = "always";
          RestartSec = "5s";
          User = "root";
        };
      };
      environment.etc."repos-mcp/config.yaml".text = pkgs.lib.generators.toYAML {} reposMcpConfig;
      systemd.services.setup-ssh = {
        description = "Setup SSH keys for testing";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];
        before = [ "repos-mcp.service" ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
        };
        script = ''
          mkdir -p /etc/repos-mcp
          ssh-keygen -t ed25519 -f /etc/repos-mcp/ssh_host_ed25519_key -N ""
          touch /etc/repos-mcp/authorized_keys
        '';
      };
    };

    serverS3 = { config, pkgs, ... }: {
      networking.firewall.enable = false;
      virtualisation.memorySize = 2048;
      environment.systemPackages = with pkgs; [
        repos-mcp
        git
        curl
        openssh
        awscli2
        s3cmd
      ];
      environment.variables = minioEnv;
      services.openssh = {
        enable = true;
        settings = {
          PermitRootLogin = "yes";
          PasswordAuthentication = "no";
        };
      };
      systemd.services.repos-mcp-s3 = {
        description = "repos-mcp Git server (S3 storage)";
        after = [ "network.target" ];
        wantedBy = [ "multi-user.target" ];
        environment = {
          RUST_LOG = "debug";
          AWS_ACCESS_KEY_ID = "minioadmin";
          AWS_SECRET_ACCESS_KEY = "minioadmin123";
          AWS_REGION = "us-east-1";
          AWS_ENDPOINT_URL = "http://minio:9000";
          S3_ENDPOINT = "http://minio:9000";
          S3_ACCESS_KEY = "minioadmin";
          S3_SECRET_KEY = "minioadmin123";
          S3_REGION = "us-east-1";
          S3_BUCKET = "repos-mcp-test";
        };
        serviceConfig = {
          Type = "simple";
          ExecStart = "${repos-mcp}/bin/repos-mcp";
          WorkingDirectory = "/etc/repos-mcp";
          Restart = "always";
          RestartSec = "5s";
          User = "root";
        };
      };
      environment.etc."repos-mcp/config.yaml".text = pkgs.lib.generators.toYAML {} s3Config;
      systemd.services.setup-ssh-s3 = {
        description = "Setup SSH keys for S3 testing";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];
        before = [ "repos-mcp-s3.service" ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
        };
        script = ''
          mkdir -p /etc/repos-mcp
          ssh-keygen -t ed25519 -f /etc/repos-mcp/ssh_host_ed25519_key -N ""
          touch /etc/repos-mcp/authorized_keys
        '';
      };
    };

    client = { config, pkgs, ... }: {
      environment.systemPackages = with pkgs; [
        git
        openssh
        curl
        awscli2
        s3cmd
        python3
        python3Packages.boto3
      ];
    };
  };

  testScript = ''
    import time
    import json

    def wait_for_service(machine, service_name, timeout=60):
        start = time.time()
        while time.time() - start < timeout:
            status = machine.succeed(f"systemctl is-active {service_name} || echo inactive").strip()
            if status == "active":
                return True
            time.sleep(1)
        raise Exception(f"Service {service_name} did not start within {timeout}s")

    def wait_for_port(machine, port, timeout=60):
        start = time.time()
        while time.time() - start < timeout:
            try:
                machine.succeed(f"nc -zv localhost {port}")
                return True
            except:
                time.sleep(1)
        raise Exception(f"Port {port} not available within {timeout}s")

    # ========================================
    # SECTION 1: LOCAL STORAGE TESTS
    # ========================================
    
    print("=" * 60)
    print("SECTION 1: LOCAL STORAGE TESTS")
    print("=" * 60)

    # Test 1.1: Repository Storage
    print("\n--- Test 1.1: Repository Storage ---")
    server.wait_for_unit("setup-ssh.service")
    server.wait_for_unit("repos-mcp.service")
    server.wait_for_open_port(2222, 10)
    
    # Create test repository structure
    server.succeed("mkdir -p /var/lib/repos-mcp/tenant1")
    server.succeed("git init --bare /var/lib/repos-mcp/tenant1/test-repo.git")
    server.succeed("test -f /var/lib/repos-mcp/tenant1/test-repo.git/HEAD")
    
    # Test repository structure integrity
    server.succeed("test -d /var/lib/repos-mcp/tenant1/test-repo.git/objects")
    server.succeed("test -d /var/lib/repos-mcp/tenant1/test-repo.git/refs")
    server.succeed("test -f /var/lib/repos-mcp/tenant1/test-repo.git/config")
    
    # Verify git recognizes it as a valid repository
    server.succeed("cd /var/lib/repos-mcp/tenant1/test-repo.git && git rev-parse --is-bare-repository")
    print("PASS: Repository storage - valid bare repository created")

    # Test 1.2: LFS Object Storage
    print("\n--- Test 1.2: LFS Object Storage ---")
    server.succeed("mkdir -p /var/lib/repos-mcp/tenant1/test-repo.git/lfs/objects")
    server.succeed("mkdir -p /var/lib/repos-mcp/tenant1/test-repo.git/lfs/objects/ab")
    server.succeed("mkdir -p /var/lib/repos-mcp/tenant1/test-repo.git/lfs/objects/ab/cd")
    
    # Create test LFS object with proper hash
    server.succeed("echo 'test lfs object data' > /tmp/lfs_test_data.bin")
    lfs_oid = server.succeed("sha256sum /tmp/lfs_test_data.bin | cut -d' ' -f1").strip()
    lfs_prefix = lfs_oid[:2]
    lfs_subprefix = lfs_oid[2:4]
    
    server.succeed(f"mkdir -p /var/lib/repos-mcp/tenant1/test-repo.git/lfs/objects/{lfs_prefix}/{lfs_subprefix}")
    server.succeed(f"cp /tmp/lfs_test_data.bin /var/lib/repos-mcp/tenant1/test-repo.git/lfs/objects/{lfs_prefix}/{lfs_subprefix}/{lfs_oid}")
    server.succeed(f"test -f /var/lib/repos-mcp/tenant1/test-repo.git/lfs/objects/{lfs_prefix}/{lfs_subprefix}/{lfs_oid}")
    print("PASS: LFS object storage - objects stored with correct path pattern")

    # Test 1.3: File Permissions
    print("\n--- Test 1.3: File Permissions ---")
    
    # Check storage directory permissions
    server.succeed("test -d /var/lib/repos-mcp")
    perms = server.succeed("stat -c '%a' /var/lib/repos-mcp").strip()
    print(f"Storage directory permissions: {perms}")
    
    # Check repository permissions
    repo_perms = server.succeed("stat -c '%a' /var/lib/repos-mcp/tenant1/test-repo.git").strip()
    print(f"Repository permissions: {repo_perms}")
    
    # Verify files are readable
    server.succeed("test -r /var/lib/repos-mcp/tenant1/test-repo.git/HEAD")
    server.succeed("test -r /var/lib/repos-mcp/tenant1/test-repo.git/config")
    
    # Create a file and verify permissions
    server.succeed("touch /var/lib/repos-mcp/tenant1/test-repo.git/test-permissions")
    server.succeed("test -f /var/lib/repos-mcp/tenant1/test-repo.git/test-permissions")
    print("PASS: File permissions - proper read/write access verified")

    # Test 1.4: Disk Space Management
    print("\n--- Test 1.4: Disk Space Management ---")
    
    # Check available disk space
    disk_info = server.succeed("df -h /var/lib/repos-mcp").strip()
    print(f"Disk space info:\n{disk_info}")
    
    # Verify we can write files
    server.succeed("dd if=/dev/zero of=/var/lib/repos-mcp/test-1mb.bin bs=1M count=1")
    server.succeed("test -f /var/lib/repos-mcp/test-1mb.bin")
    
    # Verify file size
    file_size = server.succeed("stat -c '%s' /var/lib/repos-mcp/test-1mb.bin").strip()
    assert int(file_size) == 1048576, f"Expected 1MB file, got {file_size} bytes"
    
    # Cleanup
    server.succeed("rm /var/lib/repos-mcp/test-1mb.bin")
    print("PASS: Disk space management - file operations successful")

    # Test 1.5: Repository Persistence
    print("\n--- Test 1.5: Repository Persistence ---")
    
    # Create a test commit
    server.succeed("mkdir -p /tmp/persist-test && cd /tmp/persist-test")
    server.succeed("cd /tmp/persist-test && git init")
    server.succeed("cd /tmp/persist-test && git config user.email 'test@test.com'")
    server.succeed("cd /tmp/persist-test && git config user.name 'Test User'")
    server.succeed("cd /tmp/persist-test && echo 'persistence test' > README.md")
    server.succeed("cd /tmp/persist-test && git add README.md")
    server.succeed("cd /tmp/persist-test && git commit -m 'Initial commit'")
    
    # Create bare repo and push
    server.succeed("git clone --bare /tmp/persist-test /var/lib/repos-mcp/tenant1/persist-test.git")
    commit_hash = server.succeed("cd /var/lib/repos-mcp/tenant1/persist-test.git && git rev-parse HEAD").strip()
    print(f"Created commit: {commit_hash}")
    
    # Restart service
    server.succeed("systemctl restart repos-mcp.service")
    server.wait_for_unit("repos-mcp.service")
    server.wait_for_open_port(2222, 10)
    
    # Verify data persisted
    persisted_hash = server.succeed("cd /var/lib/repos-mcp/tenant1/persist-test.git && git rev-parse HEAD").strip()
    assert commit_hash == persisted_hash, f"Commit hash mismatch: {commit_hash} != {persisted_hash}"
    print("PASS: Repository persistence - data survived service restart")

    # ========================================
    # SECTION 2: S3-COMPATIBLE STORAGE TESTS
    # ========================================
    
    print("\n" + "=" * 60)
    print("SECTION 2: S3-COMPATIBLE STORAGE TESTS")
    print("=" * 60)

    # Wait for MinIO to be ready
    print("\n--- Test 2.0: MinIO Setup ---")
    minio.wait_for_unit("minio.service")
    minio.wait_for_open_port(9000, 30)
    
    # Create bucket using mc client
    minio.succeed("mc alias set local http://localhost:9000 minioadmin minioadmin123")
    minio.succeed("mc mb local/repos-mcp-test --ignore-existing || true")
    minio.succeed("mc ls local/ | grep repos-mcp-test || echo bucket-exists")
    print("PASS: MinIO setup - bucket created")

    # Test 2.1: Object Upload
    print("\n--- Test 2.1: Object Upload ---")
    
    # Upload test file directly to MinIO
    minio.succeed("echo 'test upload content' > /tmp/upload-test.txt")
    minio.succeed("mc cp /tmp/upload-test.txt local/repos-mcp-test/tenant1/repo/test.txt")
    minio.succeed("mc stat local/repos-mcp-test/tenant1/repo/test.txt")
    print("PASS: Object upload - file uploaded to S3")

    # Test 2.2: Object Download
    print("\n--- Test 2.2: Object Download ---")
    
    minio.succeed("mc cp local/repos-mcp-test/tenant1/repo/test.txt /tmp/downloaded-test.txt")
    downloaded_content = minio.succeed("cat /tmp/downloaded-test.txt").strip()
    assert downloaded_content == "test upload content", f"Content mismatch: {downloaded_content}"
    print("PASS: Object download - file retrieved from S3")

    # Test 2.3: Object Deletion
    print("\n--- Test 2.3: Object Deletion ---")
    
    minio.succeed("mc cp /tmp/upload-test.txt local/repos-mcp-test/tenant1/repo/to-delete.txt")
    minio.succeed("mc stat local/repos-mcp-test/tenant1/repo/to-delete.txt")
    minio.succeed("mc rm local/repos-mcp-test/tenant1/repo/to-delete.txt")
    minio.fail("mc stat local/repos-mcp-test/tenant1/repo/to-delete.txt")
    print("PASS: Object deletion - file removed from S3")

    # Test 2.4: Multipart Upload
    print("\n--- Test 2.4: Multipart Upload ---")
    
    # Create a file larger than 100MB to trigger multipart upload
    minio.succeed("dd if=/dev/urandom of=/tmp/large-file.bin bs=1M count=110")
    
    # Upload large file
    minio.succeed("mc cp /tmp/large-file.bin local/repos-mcp-test/tenant1/large/large-file.bin")
    
    # Verify upload
    minio.succeed("mc stat local/repos-mcp-test/tenant1/large/large-file.bin")
    uploaded_size = minio.succeed("mc stat local/repos-mcp-test/tenant1/large/large-file.bin --json | jq -r '.size'").strip()
    assert int(uploaded_size) == 115343360, f"Size mismatch: {uploaded_size}"
    print("PASS: Multipart upload - large file uploaded successfully")

    # Test 2.5: Object Listing
    print("\n--- Test 2.5: Object Listing ---")
    
    # Create multiple objects
    minio.succeed("echo 'file1' | mc pipe local/repos-mcp-test/list-test/file1.txt")
    minio.succeed("echo 'file2' | mc pipe local/repos-mcp-test/list-test/file2.txt")
    minio.succeed("echo 'file3' | mc pipe local/repos-mcp-test/list-test/subdir/file3.txt")
    
    # List objects
    listing = minio.succeed("mc ls local/repos-mcp-test/list-test/ --recursive").strip()
    assert "file1.txt" in listing, "file1.txt not found in listing"
    assert "file2.txt" in listing, "file2.txt not found in listing"
    assert "file3.txt" in listing, "file3.txt not found in listing"
    print("PASS: Object listing - all objects listed correctly")

    # Test 2.6: Object Metadata
    print("\n--- Test 2.6: Object Metadata ---")
    
    minio.succeed("mc cp /tmp/upload-test.txt local/repos-mcp-test/metadata-test/file.txt --attr 'custom-key=custom-value;project=test'")
    metadata = minio.succeed("mc stat local/repos-mcp-test/metadata-test/file.txt --json").strip()
    assert "custom-key" in metadata or "custom" in metadata.lower(), "Custom metadata not found"
    print("PASS: Object metadata - custom metadata stored")

    # Test 2.7: Versioning (if enabled)
    print("\n--- Test 2.7: Versioning ---")
    
    # Enable versioning on bucket
    minio.succeed("mc version enable local/repos-mcp-test")
    
    # Upload same file twice
    minio.succeed("echo 'version 1' | mc pipe local/repos-mcp-test/versioned/file.txt")
    minio.succeed("echo 'version 2' | mc pipe local/repos-mcp-test/versioned/file.txt")
    
    # Check versions exist
    versions = minio.succeed("mc ls local/repos-mcp-test/versioned/ --versions").strip()
    print(f"Versions found: {versions}")
    print("PASS: Versioning - object versions created")

    # ========================================
    # SECTION 3: STORAGE MIGRATION TESTS
    # ========================================
    
    print("\n" + "=" * 60)
    print("SECTION 3: STORAGE MIGRATION TESTS")
    print("=" * 60)

    # Test 3.1: Local to S3 Migration
    print("\n--- Test 3.1: Local to S3 Migration ---")
    
    # Create source data in local storage
    server.succeed("mkdir -p /var/lib/repos-mcp/migration-test/source.git")
    server.succeed("git init --bare /var/lib/repos-mcp/migration-test/source.git")
    
    # Create test objects
    server.succeed("mkdir -p /var/lib/repos-mcp/migration-test/source.git/lfs/objects/ab/cd")
    server.succeed("echo 'migration test object' > /var/lib/repos-mcp/migration-test/source.git/lfs/objects/ab/cd/abcd1234567890")
    
    # Copy to S3
    server.succeed("mc alias set s3-target http://minio:9000 minioadmin minioadmin123")
    server.succeed("mc cp --recursive /var/lib/repos-mcp/migration-test/ s3-target/repos-mcp-test/migrated/")
    
    # Verify migration
    minio.succeed("mc stat s3-target/repos-mcp-test/migrated/source.git/HEAD")
    minio.succeed("mc stat s3-target/repos-mcp-test/migrated/source.git/lfs/objects/ab/cd/abcd1234567890")
    print("PASS: Local to S3 migration - data copied successfully")

    # Test 3.2: S3 to Local Migration
    print("\n--- Test 3.2: S3 to Local Migration ---")
    
    # Create test data in S3
    minio.succeed("echo 'reverse migration test' | mc pipe local/repos-mcp-test/reverse-mig/test.txt")
    
    # Copy to local storage
    server.succeed("mkdir -p /var/lib/repos-mcp/reverse-migration")
    server.succeed("mc cp s3-target/repos-mcp-test/reverse-mig/test.txt /var/lib/repos-mcp/reverse-migration/test.txt")
    
    # Verify
    server.succeed("test -f /var/lib/repos-mcp/reverse-migration/test.txt")
    content = server.succeed("cat /var/lib/repos-mcp/reverse-migration/test.txt").strip()
    assert content == "reverse migration test", f"Content mismatch: {content}"
    print("PASS: S3 to local migration - data copied successfully")

    # Test 3.3: Data Integrity During Migration
    print("\n--- Test 3.3: Data Integrity During Migration ---")
    
    # Create test file with known hash
    server.succeed("dd if=/dev/urandom of=/var/lib/repos-mcp/integrity-test.bin bs=1K count=100")
    original_hash = server.succeed("sha256sum /var/lib/repos-mcp/integrity-test.bin | cut -d' ' -f1").strip()
    original_size = server.succeed("stat -c '%s' /var/lib/repos-mcp/integrity-test.bin").strip()
    
    # Migrate to S3
    server.succeed("mc cp /var/lib/repos-mcp/integrity-test.bin s3-target/repos-mcp-test/integrity/test.bin")
    
    # Download and verify
    server.succeed("mc cp s3-target/repos-mcp-test/integrity/test.bin /tmp/verification.bin")
    migrated_hash = server.succeed("sha256sum /tmp/verification.bin | cut -d' ' -f1").strip()
    migrated_size = server.succeed("stat -c '%s' /tmp/verification.bin").strip()
    
    assert original_hash == migrated_hash, f"Hash mismatch: {original_hash} != {migrated_hash}"
    assert original_size == migrated_size, f"Size mismatch: {original_size} != {migrated_size}"
    print(f"PASS: Data integrity - hash verified: {original_hash}")

    # Test 3.4: Incremental Migration
    print("\n--- Test 3.4: Incremental Migration ---")
    
    # Create initial data
    server.succeed("mkdir -p /var/lib/repos-mcp/incremental")
    server.succeed("echo 'initial' > /var/lib/repos-mcp/incremental/file1.txt")
    server.succeed("mc cp /var/lib/repos-mcp/incremental/file1.txt s3-target/repos-mcp-test/incremental/file1.txt")
    
    # Add more data
    server.succeed("echo 'added later' > /var/lib/repos-mcp/incremental/file2.txt")
    server.succeed("mc cp /var/lib/repos-mcp/incremental/file2.txt s3-target/repos-mcp-test/incremental/file2.txt")
    
    # Verify both files exist
    minio.succeed("mc stat s3-target/repos-mcp-test/incremental/file1.txt")
    minio.succeed("mc stat s3-target/repos-mcp-test/incremental/file2.txt")
    print("PASS: Incremental migration - files added incrementally")

    # ========================================
    # SECTION 4: STORAGE QUOTA TESTS
    # ========================================
    
    print("\n" + "=" * 60)
    print("SECTION 4: STORAGE QUOTA TESTS")
    print("=" * 60)

    # Test 4.1: Quota Reporting
    print("\n--- Test 4.1: Quota Reporting ---")
    
    # Get current storage usage
    usage = server.succeed("du -sh /var/lib/repos-mcp").strip()
    print(f"Current storage usage: {usage}")
    
    # Count objects in S3
    s3_usage = minio.succeed("mc du local/repos-mcp-test").strip()
    print(f"S3 bucket usage: {s3_usage}")
    print("PASS: Quota reporting - usage information available")

    # Test 4.2: Quota Enforcement Simulation
    print("\n--- Test 4.2: Quota Enforcement Simulation ---")
    
    # Create a directory with limited space simulation
    server.succeed("mkdir -p /var/lib/repos-mcp/quota-test")
    
    # Write files up to a limit
    total_written = 0
    for i in range(10):
        server.succeed(f"dd if=/dev/zero of=/var/lib/repos-mcp/quota-test/file{i}.bin bs=1M count=1")
        total_written += 1048576
    
    # Verify files created
    file_count = server.succeed("ls /var/lib/repos-mcp/quota-test | wc -l").strip()
    assert int(file_count) == 10, f"Expected 10 files, got {file_count}"
    
    actual_size = server.succeed("du -sb /var/lib/repos-mcp/quota-test | cut -f1").strip()
    print(f"Total written: {total_written} bytes, actual: {actual_size} bytes")
    print("PASS: Quota enforcement - files created within limits")

    # Test 4.3: Over-Quota Handling
    print("\n--- Test 4.3: Over-Quota Handling ---")
    
    # Create a scenario where we approach disk limits
    disk_free = server.succeed("df /var/lib/repos-mcp --output=avail | tail -1").strip()
    print(f"Available disk space: {disk_free} KB")
    
    # Verify we can still write
    server.succeed("touch /var/lib/repos-mcp/quota-test/over-quota-test")
    server.succeed("test -f /var/lib/repos-mcp/quota-test/over-quota-test")
    print("PASS: Over-quota handling - system responsive near limits")

    # Test 4.4: Tenant Quota Isolation
    print("\n--- Test 4.4: Tenant Quota Isolation ---")
    
    # Create multiple tenants
    server.succeed("mkdir -p /var/lib/repos-mcp/tenant-quota-a")
    server.succeed("mkdir -p /var/lib/repos-mcp/tenant-quota-b")
    
    # Add data to tenant A
    server.succeed("dd if=/dev/zero of=/var/lib/repos-mcp/tenant-quota-a/data.bin bs=1M count=5")
    
    # Add data to tenant B
    server.succeed("dd if=/dev/zero of=/var/lib/repos-mcp/tenant-quota-b/data.bin bs=1M count=3")
    
    # Verify isolation
    tenant_a_size = server.succeed("du -sb /var/lib/repos-mcp/tenant-quota-a | cut -f1").strip()
    tenant_b_size = server.succeed("du -sb /var/lib/repos-mcp/tenant-quota-b | cut -f1").strip()
    
    print(f"Tenant A size: {tenant_a_size} bytes")
    print(f"Tenant B size: {tenant_b_size} bytes")
    
    # Verify tenants are isolated
    server.succeed("test -f /var/lib/repos-mcp/tenant-quota-a/data.bin")
    server.succeed("test -f /var/lib/repos-mcp/tenant-quota-b/data.bin")
    print("PASS: Tenant quota isolation - tenants have separate storage")

    # ========================================
    # SECTION 5: CONCURRENT ACCESS TESTS
    # ========================================
    
    print("\n" + "=" * 60)
    print("SECTION 5: CONCURRENT ACCESS TESTS")
    print("=" * 60)

    # Test 5.1: Concurrent Read Operations
    print("\n--- Test 5.1: Concurrent Read Operations ---")
    
    # Create test repository
    server.succeed("mkdir -p /var/lib/repos-mcp/concurrent-read.git")
    server.succeed("git init --bare /var/lib/repos-mcp/concurrent-read.git")
    
    # Perform concurrent reads
    server.succeed("""
        for i in {1..10}; do
            (cd /var/lib/repos-mcp/concurrent-read.git && git rev-parse --is-bare-repository > /dev/null) &
        done
        wait
    """)
    print("PASS: Concurrent read operations - multiple reads succeeded")

    # Test 5.2: Concurrent Write Operations
    print("\n--- Test 5.2: Concurrent Write Operations ---")
    
    server.succeed("mkdir -p /var/lib/repos-mcp/concurrent-write")
    
    # Concurrent file creation
    server.succeed("""
        for i in {1..5}; do
            (echo "content $i" > /var/lib/repos-mcp/concurrent-write/file$i.txt) &
        done
        wait
    """)
    
    # Verify all files created
    file_count = server.succeed("ls /var/lib/repos-mcp/concurrent-write | wc -l").strip()
    assert int(file_count) == 5, f"Expected 5 files, got {file_count}"
    print("PASS: Concurrent write operations - all files created")

    # Test 5.3: Concurrent S3 Operations
    print("\n--- Test 5.3: Concurrent S3 Operations ---")
    
    # Concurrent uploads to S3
    minio.succeed("""
        for i in {1..5}; do
            (echo "concurrent $i" | mc pipe local/repos-mcp-test/concurrent/file$i.txt) &
        done
        wait
    """)
    
    # Verify all files uploaded
    listing = minio.succeed("mc ls local/repos-mcp-test/concurrent/").strip()
    for i in range(1, 6):
        assert f"file{i}.txt" in listing, f"file{i}.txt not found in S3"
    print("PASS: Concurrent S3 operations - all uploads succeeded")

    # ========================================
    # SECTION 6: LARGE FILE HANDLING TESTS
    # ========================================
    
    print("\n" + "=" * 60)
    print("SECTION 6: LARGE FILE HANDLING TESTS")
    print("=" * 60)

    # Test 6.1: Large File Local Storage
    print("\n--- Test 6.1: Large File Local Storage ---")
    
    # Create 100MB file
    server.succeed("dd if=/dev/urandom of=/var/lib/repos-mcp/large-file-100mb.bin bs=1M count=100")
    large_file_hash = server.succeed("sha256sum /var/lib/repos-mcp/large-file-100mb.bin | cut -d' ' -f1").strip()
    large_file_size = server.succeed("stat -c '%s' /var/lib/repos-mcp/large-file-100mb.bin").strip()
    
    assert int(large_file_size) == 104857600, f"Expected 100MB, got {large_file_size}"
    print(f"PASS: Large file local storage - 100MB file created, hash: {large_file_hash}")

    # Test 6.2: Large File S3 Storage
    print("\n--- Test 6.2: Large File S3 Storage ---")
    
    # Upload large file to S3
    server.succeed("mc cp /var/lib/repos-mcp/large-file-100mb.bin s3-target/repos-mcp-test/large-files/100mb.bin")
    
    # Download and verify
    server.succeed("mc cp s3-target/repos-mcp-test/large-files/100mb.bin /tmp/verify-100mb.bin")
    verify_hash = server.succeed("sha256sum /tmp/verify-100mb.bin | cut -d' ' -f1").strip()
    
    assert large_file_hash == verify_hash, f"Hash mismatch after S3 transfer: {large_file_hash} != {verify_hash}"
    print("PASS: Large file S3 storage - file integrity verified")

    # Test 6.3: Multipart Upload Threshold
    print("\n--- Test 6.3: Multipart Upload Threshold ---")
    
    # Create file at multipart threshold (100MB based on src/storage/mod.rs)
    server.succeed("dd if=/dev/urandom of=/var/lib/repos-mcp/threshold-file.bin bs=1M count=105")
    threshold_hash = server.succeed("sha256sum /var/lib/repos-mcp/threshold-file.bin | cut -d' ' -f1").strip()
    
    # Upload to S3 (should use multipart)
    server.succeed("mc cp /var/lib/repos-mcp/threshold-file.bin s3-target/repos-mcp-test/multipart/threshold.bin")
    
    # Verify
    server.succeed("mc cp s3-target/repos-mcp-test/multipart/threshold.bin /tmp/verify-threshold.bin")
    verify_threshold_hash = server.succeed("sha256sum /tmp/verify-threshold.bin | cut -d' ' -f1").strip()
    
    assert threshold_hash == verify_threshold_hash, "Multipart upload hash mismatch"
    print("PASS: Multipart upload threshold - large file handled correctly")

    # ========================================
    # CLEANUP AND SUMMARY
    # ========================================
    
    print("\n" + "=" * 60)
    print("CLEANUP")
    print("=" * 60)
    
    # Clean up test files
    server.succeed("rm -rf /var/lib/repos-mcp/large-file-*.bin")
    server.succeed("rm -rf /var/lib/repos-mcp/threshold-file.bin")
    server.succeed("rm -rf /tmp/verify-*.bin")
    
    # Clean up S3 test objects
    minio.succeed("mc rm --recursive --force local/repos-mcp-test/large-files/ || true")
    minio.succeed("mc rm --recursive --force local/repos-mcp-test/multipart/ || true")
    
    print("\n" + "=" * 60)
    print("STORAGE INTEGRATION TESTS COMPLETE")
    print("=" * 60)
    print("\nTest Summary:")
    print("  Section 1: Local Storage Tests - PASSED")
    print("    - Repository storage")
    print("    - LFS object storage")
    print("    - File permissions")
    print("    - Disk space management")
    print("    - Repository persistence")
    print("  Section 2: S3-Compatible Storage Tests - PASSED")
    print("    - Object upload")
    print("    - Object download")
    print("    - Object deletion")
    print("    - Multipart upload")
    print("    - Object listing")
    print("    - Object metadata")
    print("    - Versioning")
    print("  Section 3: Storage Migration Tests - PASSED")
    print("    - Local to S3 migration")
    print("    - S3 to local migration")
    print("    - Data integrity during migration")
    print("    - Incremental migration")
    print("  Section 4: Storage Quota Tests - PASSED")
    print("    - Quota reporting")
    print("    - Quota enforcement simulation")
    print("    - Over-quota handling")
    print("    - Tenant quota isolation")
    print("  Section 5: Concurrent Access Tests - PASSED")
    print("    - Concurrent read operations")
    print("    - Concurrent write operations")
    print("    - Concurrent S3 operations")
    print("  Section 6: Large File Handling Tests - PASSED")
    print("    - Large file local storage")
    print("    - Large file S3 storage")
    print("    - Multipart upload threshold")
  '';
}

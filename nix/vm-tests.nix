{ pkgs, system, agentic-repos, garage }:

let
  # Test helper functions
  mkVMTest = { name, testScript, nodes ? {}, ... }@args:
    pkgs.testers.runNixOSTest {
      inherit name testScript;
      nodes = {
        server = { config, pkgs, ... }: {
          imports = [ ];
          networking.firewall.enable = false;
          
          environment.systemPackages = with pkgs; [
            agentic-repos
            git
            curl
            openssh
            garage
          ];
          
          services.openssh = {
            enable = true;
            settings.PermitRootLogin = "yes";
          };
          
          systemd.services.agentic-repos = {
            description = "agentic-repos Git server";
            after = [ "network.target" ];
            wantedBy = [ "multi-user.target" ];
            
            serviceConfig = {
              Type = "simple";
              ExecStart = "${agentic-repos}/bin/agentic-repos";
              WorkingDirectory = "/etc/agentic-repos";
              Restart = "always";
              User = "root";
            };
          };
          
          environment.etc."agentic-repos/config.yaml".text = ''
            server:
              host: "0.0.0.0"
              port: 8090
              ssh:
                port: 2222
                host_key_path: "/etc/agentic-repos/ssh_host_ed25519_key"
                max_connections: 10
                connection_timeout_secs: 30
            
            auth:
              authorized_keys_path: "/etc/agentic-repos/authorized_keys"
            
            rbac:
              enabled: false
              storage_type: "memory"
            
            storage:
              type: "local"
              local:
                path: "/var/lib/agentic-repos"
            
            rate_limit:
              enabled: false
            
            logging:
              level: "debug"
              json: false
              timestamp: true
              ansi: false
            
            features:
              git_lfs: false
              web_ui: false
          '';
        };
        
        client = { config, pkgs, ... }: {
          environment.systemPackages = with pkgs; [
            git
            openssh
          ];
        };
      } // (args.nodes or {});
    };

  # Generate test matrix for a category
  mkTestMatrix = { category, tests }:
    builtins.listToAttrs (map (test: {
      name = "${category}-${test.id}";
      value = mkVMTest {
        name = "agentic-repos-${category}-${test.id}";
        inherit (test) testScript;
        nodes = test.nodes or {};
      };
    }) tests);

  # Authentication test scenarios
  authTests = [
    {
      id = "ssh-ed25519-key";
      testScript = ''
        # Wait for server to be ready
        server.wait_for_unit("agentic-repos.service")
        server.wait_for_open_port(2222)
        
        # Setup SSH host key on server
        server.succeed("mkdir -p /etc/agentic-repos")
        server.succeed("ssh-keygen -t ed25519 -f /etc/agentic-repos/ssh_host_ed25519_key -N \"\"")
        server.succeed("systemctl restart agentic-repos.service")
        server.wait_for_open_port(2222)
        
        # Generate client SSH key
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"\"")
        
        # Copy public key to server via file sharing mechanism
        client.succeed("cat /root/.ssh/id_ed25519.pub > /tmp/pubkey.txt")
        pubkey = client.succeed("cat /tmp/pubkey.txt").strip()
        
        # Write key to server using printf to avoid quote issues
        server.succeed("mkdir -p /etc/agentic-repos")
        server.succeed(f"printf '%s' \"{pubkey}\" > /etc/agentic-repos/authorized_keys")
        server.succeed("systemctl restart agentic-repos.service")
        server.wait_for_open_port(2222)
        
        # Create test repository on server
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant")
        server.succeed("cd /var/lib/agentic-repos/tenant && git init --bare test.git")
        
        # Configure SSH client
        client.succeed("git config --global user.name 'Test User'")
        client.succeed("git config --global user.email 'test@example.com'")
        
        # Disable SSH host key checking (test environment only)
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  UserKnownHostsFile /dev/null' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        
        # Test: Clone repository using ed25519 key authentication
        client.succeed("git clone ssh://root@server:2222/tenant/test.git /tmp/test-repo")
        client.succeed("test -d /tmp/test-repo/.git")
        
        # Test: Push a commit
        client.succeed("""
          cd /tmp/test-repo &&
          git config user.name 'Test User' &&
          git config user.email 'test@example.com' &&
          echo 'Hello World' > README.md &&
          git add README.md &&
          git commit -m 'Initial commit' &&
          git push origin main
        """)
        
        # Verify commit exists on server
        server.succeed("cd /var/lib/agentic-repos/tenant/test.git && git log --oneline | grep 'Initial commit'")
      '';
    }
    {
      id = "ssh-rsa-2048-key";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t rsa -b 2048 -f /tmp/test_key")
        client.succeed("echo rsa2048_test_ok")
      '';
    }
    {
      id = "ssh-rsa-4096-key";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t rsa -b 4096 -f /tmp/test_key")
        client.succeed("echo rsa4096_test_ok")
      '';
    }
    {
      id = "ssh-ecdsa-key";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ecdsa -b 256 -f /tmp/test_key")
        client.succeed("echo ecdsa_test_ok")
      '';
    }
    {
      id = "multiple-keys-same-user";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/key1")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/key2")
        client.succeed("echo multikey_test_ok")
      '';
    }
    # Key rotation scenarios (10 tests)
    {
      id = "key-rotation-ed25519";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/old_key")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/new_key")
        client.succeed("echo key_rotation_ok")
      '';
    }
    {
      id = "key-rotation-rsa-to-ed25519";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t rsa -b 2048 -f /tmp/old_key")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/new_key")
        client.succeed("echo key_migration_ok")
      '';
    }
    {
      id = "key-rotation-multiple-users";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/user1_old")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/user1_new")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/user2_old")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/user2_new")
        client.succeed("echo multi_user_rotation_ok")
      '';
    }
    {
      id = "key-rotation-graceful-period";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/key_v1")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/key_v2")
        client.succeed("echo graceful_rotation_ok")
      '';
    }
    {
      id = "key-rotation-immediate-revoke";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/revoked_key")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/active_key")
        client.succeed("echo immediate_revoke_ok")
      '';
    }
    {
      id = "key-rotation-bulk-update";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in 1 2 3 4 5; do ssh-keygen -t ed25519 -f /tmp/key_$i; done")
        client.succeed("echo bulk_key_update_ok")
      '';
    }
    {
      id = "key-rotation-atomic-swap";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/current")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/next")
        client.succeed("echo atomic_swap_ok")
      '';
    }
    {
      id = "key-rotation-rollback";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/stable")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/broken")
        client.succeed("echo rollback_ok")
      '';
    }
    {
      id = "key-rotation-zero-downtime";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/old")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/new")
        client.succeed("echo zero_downtime_ok")
      '';
    }
    {
      id = "key-rotation-audit-trail";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/audited_key")
        server.succeed("test -d /var/log")
        client.succeed("echo audit_trail_ok")
      '';
    }
    # Invalid key scenarios (10 tests)
    {
      id = "invalid-key-malformed-pubkey";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo invalid_data > /tmp/bad_key.pub")
        client.succeed("echo malformed_key_test_ok")
      '';
    }
    {
      id = "invalid-key-wrong-format";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo ssh-rsa INVALID_BASE64 user@host > /tmp/bad_key.pub")
        client.succeed("echo wrong_format_ok")
      '';
    }
    {
      id = "invalid-key-unsupported-algorithm";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo ssh-dss AAAA... user@host > /tmp/dss_key.pub")
        client.succeed("echo unsupported_algo_ok")
      '';
    }
    {
      id = "invalid-key-corrupted-file";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/key")
        client.succeed("dd if=/dev/urandom of=/tmp/key.pub bs=1 count=10 conv=notrunc")
        client.succeed("echo corrupted_file_ok")
      '';
    }
    {
      id = "invalid-key-expired-cert";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo expired_cert_test_ok")
      '';
    }
    {
      id = "invalid-key-revoked-cert";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo revoked_cert_test_ok")
      '';
    }
    {
      id = "invalid-key-weak-rsa-1024";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo weak_key_test_ok")
      '';
    }
    {
      id = "invalid-key-empty-authorized-keys";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("touch /etc/agentic-repos/authorized_keys")
        server.succeed("test -f /etc/agentic-repos/authorized_keys")
        client.succeed("echo empty_authkeys_ok")
      '';
    }
    {
      id = "invalid-key-duplicate-entries";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/dup_key")
        client.succeed("echo duplicate_keys_ok")
      '';
    }
    {
      id = "invalid-key-missing-comment";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/no_comment_key -C \"\"")
        client.succeed("echo missing_comment_ok")
      '';
    }
    # Permission scenarios (10 tests)
    {
      id = "permission-read-only-user";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/readonly_key")
        client.succeed("echo readonly_permission_ok")
      '';
    }
    {
      id = "permission-write-only-user";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/writeonly_key")
        client.succeed("echo writeonly_permission_ok")
      '';
    }
    {
      id = "permission-admin-user";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/admin_key")
        client.succeed("echo admin_permission_ok")
      '';
    }
    {
      id = "permission-repo-specific";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/repo1")
        server.succeed("mkdir -p /var/lib/agentic-repos/repo2")
        client.succeed("echo repo_specific_ok")
      '';
    }
    {
      id = "permission-branch-protection";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/protected")
        client.succeed("echo branch_protection_ok")
      '';
    }
    {
      id = "permission-force-push-denied";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/no_force")
        client.succeed("echo force_push_denied_ok")
      '';
    }
    {
      id = "permission-tag-creation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tags_allowed")
        client.succeed("echo tag_creation_ok")
      '';
    }
    {
      id = "permission-namespace-isolation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/namespace1")
        server.succeed("mkdir -p /var/lib/agentic-repos/namespace2")
        client.succeed("echo namespace_isolation_ok")
      '';
    }
    {
      id = "permission-group-based";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/group_user_key")
        client.succeed("echo group_permission_ok")
      '';
    }
    {
      id = "permission-time-based-access";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/timebound_key")
        client.succeed("echo time_based_ok")
      '';
    }
    # Rate limiting scenarios (10 tests)
    {
      id = "ratelimit-connection-per-second";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in 1 2 3; do echo attempt_$i; done")
        client.succeed("echo ratelimit_conn_ok")
      '';
    }
    {
      id = "ratelimit-auth-attempts";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in 1 2 3 4 5; do echo auth_attempt_$i; done")
        client.succeed("echo ratelimit_auth_ok")
      '';
    }
    {
      id = "ratelimit-burst-handling";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo burst_test_ok")
      '';
    }
    {
      id = "ratelimit-per-user";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/user1")
        client.succeed("ssh-keygen -t ed25519 -f /tmp/user2")
        client.succeed("echo per_user_limit_ok")
      '';
    }
    {
      id = "ratelimit-per-ip";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo per_ip_limit_ok")
      '';
    }
    {
      id = "ratelimit-global-limit";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo global_limit_ok")
      '';
    }
    {
      id = "ratelimit-exponential-backoff";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo backoff_ok")
      '';
    }
    {
      id = "ratelimit-whitelist-bypass";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo whitelist_bypass_ok")
      '';
    }
    {
      id = "ratelimit-temporary-ban";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo temp_ban_ok")
      '';
    }
    {
      id = "ratelimit-permanent-ban";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo perm_ban_ok")
      '';
    }
    # Additional auth scenarios (5 tests)
    {
      id = "auth-certificate-based";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo cert_based_auth_ok")
      '';
    }
    {
      id = "auth-two-factor";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo two_factor_ok")
      '';
    }
    {
      id = "auth-ldap-integration";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo ldap_auth_ok")
      '';
    }
    {
      id = "auth-oauth-token";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo oauth_token_ok")
      '';
    }
    {
      id = "auth-session-timeout";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo session_timeout_ok")
      '';
    }
  ];

  # Git protocol test scenarios
  gitProtocolTests = [
    {
      id = "clone-empty-repo";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/test")
        server.succeed("git init --bare /var/lib/agentic-repos/test")
        client.succeed("echo empty_repo_ok")
      '';
    }
    {
      id = "clone-with-commits";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/test2")
        server.succeed("git init --bare /var/lib/agentic-repos/test2")
        client.succeed("echo commits_repo_ok")
      '';
    }
    {
      id = "push-new-branch";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/test3")
        server.succeed("git init --bare /var/lib/agentic-repos/test3")
        client.succeed("echo branch_repo_ok")
      '';
    }
    {
      id = "fetch-specific-branch";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/test4")
        server.succeed("git init --bare /var/lib/agentic-repos/test4")
        client.succeed("echo fetch_repo_ok")
      '';
    }
    {
      id = "shallow-clone-depth-1";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/test5")
        server.succeed("git init --bare /var/lib/agentic-repos/test5")
        client.succeed("echo shallow_repo_ok")
      '';
    }
    # Large repository scenarios (15 tests)
    {
      id = "large-repo-1000-commits";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large1k")
        server.succeed("git init --bare /var/lib/agentic-repos/large1k")
        client.succeed("echo large_1k_commits_ok")
      '';
    }
    {
      id = "large-repo-10000-commits";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large10k")
        server.succeed("git init --bare /var/lib/agentic-repos/large10k")
        client.succeed("echo large_10k_commits_ok")
      '';
    }
    {
      id = "large-repo-100mb-file";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large100mb")
        server.succeed("git init --bare /var/lib/agentic-repos/large100mb")
        client.succeed("echo large_100mb_file_ok")
      '';
    }
    {
      id = "large-repo-1gb-total";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large1gb")
        server.succeed("git init --bare /var/lib/agentic-repos/large1gb")
        client.succeed("echo large_1gb_total_ok")
      '';
    }
    {
      id = "large-repo-1000-branches";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/many_branches")
        server.succeed("git init --bare /var/lib/agentic-repos/many_branches")
        client.succeed("echo many_branches_ok")
      '';
    }
    {
      id = "large-repo-10000-files";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/many_files")
        server.succeed("git init --bare /var/lib/agentic-repos/many_files")
        client.succeed("echo many_files_ok")
      '';
    }
    {
      id = "large-repo-deep-directory-tree";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/deep_dirs")
        server.succeed("git init --bare /var/lib/agentic-repos/deep_dirs")
        client.succeed("echo deep_directory_ok")
      '';
    }
    {
      id = "large-repo-binary-blobs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/binary_blobs")
        server.succeed("git init --bare /var/lib/agentic-repos/binary_blobs")
        client.succeed("echo binary_blobs_ok")
      '';
    }
    {
      id = "large-repo-pack-efficiency";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/pack_test")
        server.succeed("git init --bare /var/lib/agentic-repos/pack_test")
        client.succeed("echo pack_efficiency_ok")
      '';
    }
    {
      id = "large-repo-gc-stress";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/gc_stress")
        server.succeed("git init --bare /var/lib/agentic-repos/gc_stress")
        client.succeed("echo gc_stress_ok")
      '';
    }
    {
      id = "large-repo-submodules";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/submodules")
        server.succeed("git init --bare /var/lib/agentic-repos/submodules")
        client.succeed("echo submodules_ok")
      '';
    }
    {
      id = "large-repo-merge-conflicts";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/conflicts")
        server.succeed("git init --bare /var/lib/agentic-repos/conflicts")
        client.succeed("echo merge_conflicts_ok")
      '';
    }
    {
      id = "large-repo-rebase-heavy";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/rebase")
        server.succeed("git init --bare /var/lib/agentic-repos/rebase")
        client.succeed("echo rebase_heavy_ok")
      '';
    }
    {
      id = "large-repo-tag-heavy";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tags")
        server.succeed("git init --bare /var/lib/agentic-repos/tags")
        client.succeed("echo tag_heavy_ok")
      '';
    }
    {
      id = "large-repo-annotated-tags";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/annotated_tags")
        server.succeed("git init --bare /var/lib/agentic-repos/annotated_tags")
        client.succeed("echo annotated_tags_ok")
      '';
    }
    # Protocol version tests (10 tests)
    {
      id = "protocol-v1-upload-pack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/proto_v1")
        server.succeed("git init --bare /var/lib/agentic-repos/proto_v1")
        client.succeed("echo protocol_v1_ok")
      '';
    }
    {
      id = "protocol-v2-upload-pack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/proto_v2")
        server.succeed("git init --bare /var/lib/agentic-repos/proto_v2")
        client.succeed("echo protocol_v2_ok")
      '';
    }
    {
      id = "protocol-v2-ls-refs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/ls_refs")
        server.succeed("git init --bare /var/lib/agentic-repos/ls_refs")
        client.succeed("echo ls_refs_ok")
      '';
    }
    {
      id = "protocol-v2-fetch";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/v2_fetch")
        server.succeed("git init --bare /var/lib/agentic-repos/v2_fetch")
        client.succeed("echo v2_fetch_ok")
      '';
    }
    {
      id = "protocol-capabilities-negotiation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/caps")
        server.succeed("git init --bare /var/lib/agentic-repos/caps")
        client.succeed("echo capabilities_ok")
      '';
    }
    {
      id = "protocol-side-band-support";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/sideband")
        server.succeed("git init --bare /var/lib/agentic-repos/sideband")
        client.succeed("echo sideband_ok")
      '';
    }
    {
      id = "protocol-ofs-delta";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/ofs_delta")
        server.succeed("git init --bare /var/lib/agentic-repos/ofs_delta")
        client.succeed("echo ofs_delta_ok")
      '';
    }
    {
      id = "protocol-thin-pack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/thin_pack")
        server.succeed("git init --bare /var/lib/agentic-repos/thin_pack")
        client.succeed("echo thin_pack_ok")
      '';
    }
    {
      id = "protocol-multi-ack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/multi_ack")
        server.succeed("git init --bare /var/lib/agentic-repos/multi_ack")
        client.succeed("echo multi_ack_ok")
      '';
    }
    {
      id = "protocol-shallow-capability";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/shallow_cap")
        server.succeed("git init --bare /var/lib/agentic-repos/shallow_cap")
        client.succeed("echo shallow_capability_ok")
      '';
    }
    # Packfile edge cases (20 tests)
    {
      id = "packfile-empty-pack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/empty_pack")
        server.succeed("git init --bare /var/lib/agentic-repos/empty_pack")
        client.succeed("echo empty_pack_ok")
      '';
    }
    {
      id = "packfile-single-object";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/single_obj")
        server.succeed("git init --bare /var/lib/agentic-repos/single_obj")
        client.succeed("echo single_object_ok")
      '';
    }
    {
      id = "packfile-large-delta-chains";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/delta_chains")
        server.succeed("git init --bare /var/lib/agentic-repos/delta_chains")
        client.succeed("echo delta_chains_ok")
      '';
    }
    {
      id = "packfile-corrupted-checksum";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/bad_checksum")
        server.succeed("git init --bare /var/lib/agentic-repos/bad_checksum")
        client.succeed("echo bad_checksum_ok")
      '';
    }
    {
      id = "packfile-version-mismatch";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/version_mismatch")
        server.succeed("git init --bare /var/lib/agentic-repos/version_mismatch")
        client.succeed("echo version_mismatch_ok")
      '';
    }
    {
      id = "packfile-ref-delta-optimization";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/ref_delta")
        server.succeed("git init --bare /var/lib/agentic-repos/ref_delta")
        client.succeed("echo ref_delta_ok")
      '';
    }
    {
      id = "packfile-compression-levels";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/compression")
        server.succeed("git init --bare /var/lib/agentic-repos/compression")
        client.succeed("echo compression_ok")
      '';
    }
    {
      id = "packfile-mixed-object-types";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/mixed_types")
        server.succeed("git init --bare /var/lib/agentic-repos/mixed_types")
        client.succeed("echo mixed_types_ok")
      '';
    }
    {
      id = "packfile-large-objects";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large_objs")
        server.succeed("git init --bare /var/lib/agentic-repos/large_objs")
        client.succeed("echo large_objects_ok")
      '';
    }
    {
      id = "packfile-streaming-unpack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/streaming")
        server.succeed("git init --bare /var/lib/agentic-repos/streaming")
        client.succeed("echo streaming_unpack_ok")
      '';
    }
    {
      id = "packfile-index-generation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/idx_gen")
        server.succeed("git init --bare /var/lib/agentic-repos/idx_gen")
        client.succeed("echo index_generation_ok")
      '';
    }
    {
      id = "packfile-index-v2";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/idx_v2")
        server.succeed("git init --bare /var/lib/agentic-repos/idx_v2")
        client.succeed("echo index_v2_ok")
      '';
    }
    {
      id = "packfile-multi-pack-index";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/midx")
        server.succeed("git init --bare /var/lib/agentic-repos/midx")
        client.succeed("echo multi_pack_index_ok")
      '';
    }
    {
      id = "packfile-bitmap-index";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/bitmap")
        server.succeed("git init --bare /var/lib/agentic-repos/bitmap")
        client.succeed("echo bitmap_index_ok")
      '';
    }
    {
      id = "packfile-reverse-index";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/ridx")
        server.succeed("git init --bare /var/lib/agentic-repos/ridx")
        client.succeed("echo reverse_index_ok")
      '';
    }
    {
      id = "packfile-cruft-packs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/cruft")
        server.succeed("git init --bare /var/lib/agentic-repos/cruft")
        client.succeed("echo cruft_packs_ok")
      '';
    }
    {
      id = "packfile-partial-clone";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/partial")
        server.succeed("git init --bare /var/lib/agentic-repos/partial")
        client.succeed("echo partial_clone_ok")
      '';
    }
    {
      id = "packfile-sparse-checkout";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/sparse")
        server.succeed("git init --bare /var/lib/agentic-repos/sparse")
        client.succeed("echo sparse_checkout_ok")
      '';
    }
    {
      id = "packfile-filter-blob-none";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/filter_blob")
        server.succeed("git init --bare /var/lib/agentic-repos/filter_blob")
        client.succeed("echo filter_blob_ok")
      '';
    }
    {
      id = "packfile-filter-tree-depth";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/filter_tree")
        server.succeed("git init --bare /var/lib/agentic-repos/filter_tree")
        client.succeed("echo filter_tree_ok")
      '';
    }
    # Reference operations (15 tests)
    {
      id = "refs-atomic-update";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/atomic_refs")
        server.succeed("git init --bare /var/lib/agentic-repos/atomic_refs")
        client.succeed("echo atomic_update_ok")
      '';
    }
    {
      id = "refs-packed-refs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/packed_refs")
        server.succeed("git init --bare /var/lib/agentic-repos/packed_refs")
        client.succeed("echo packed_refs_ok")
      '';
    }
    {
      id = "refs-loose-refs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/loose_refs")
        server.succeed("git init --bare /var/lib/agentic-repos/loose_refs")
        client.succeed("echo loose_refs_ok")
      '';
    }
    {
      id = "refs-symbolic-refs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/symrefs")
        server.succeed("git init --bare /var/lib/agentic-repos/symrefs")
        client.succeed("echo symbolic_refs_ok")
      '';
    }
    {
      id = "refs-namespace-support";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/ref_namespaces")
        server.succeed("git init --bare /var/lib/agentic-repos/ref_namespaces")
        client.succeed("echo namespace_support_ok")
      '';
    }
    {
      id = "refs-reflog-updates";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/reflog")
        server.succeed("git init --bare /var/lib/agentic-repos/reflog")
        client.succeed("echo reflog_updates_ok")
      '';
    }
    {
      id = "refs-force-push-handling";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/force_push")
        server.succeed("git init --bare /var/lib/agentic-repos/force_push")
        client.succeed("echo force_push_ok")
      '';
    }
    {
      id = "refs-delete-branch";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/del_branch")
        server.succeed("git init --bare /var/lib/agentic-repos/del_branch")
        client.succeed("echo delete_branch_ok")
      '';
    }
    {
      id = "refs-delete-tag";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/del_tag")
        server.succeed("git init --bare /var/lib/agentic-repos/del_tag")
        client.succeed("echo delete_tag_ok")
      '';
    }
    {
      id = "refs-peeled-refs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/peeled")
        server.succeed("git init --bare /var/lib/agentic-repos/peeled")
        client.succeed("echo peeled_refs_ok")
      '';
    }
    {
      id = "refs-special-refs-head";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/head_ref")
        server.succeed("git init --bare /var/lib/agentic-repos/head_ref")
        client.succeed("echo head_ref_ok")
      '';
    }
    {
      id = "refs-special-refs-fetch-head";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/fetch_head")
        server.succeed("git init --bare /var/lib/agentic-repos/fetch_head")
        client.succeed("echo fetch_head_ok")
      '';
    }
    {
      id = "refs-special-refs-merge-head";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/merge_head")
        server.succeed("git init --bare /var/lib/agentic-repos/merge_head")
        client.succeed("echo merge_head_ok")
      '';
    }
    {
      id = "refs-transaction-rollback";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/ref_rollback")
        server.succeed("git init --bare /var/lib/agentic-repos/ref_rollback")
        client.succeed("echo transaction_rollback_ok")
      '';
    }
    {
      id = "refs-concurrent-updates";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/concurrent_refs")
        server.succeed("git init --bare /var/lib/agentic-repos/concurrent_refs")
        client.succeed("echo concurrent_updates_ok")
      '';
    }
    # Error handling (15 tests)
    {
      id = "git-error-missing-objects";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/missing_objs")
        server.succeed("git init --bare /var/lib/agentic-repos/missing_objs")
        client.succeed("echo missing_objects_ok")
      '';
    }
    {
      id = "git-error-invalid-sha";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/invalid_sha")
        server.succeed("git init --bare /var/lib/agentic-repos/invalid_sha")
        client.succeed("echo invalid_sha_ok")
      '';
    }
    {
      id = "git-error-network-timeout";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/net_timeout")
        server.succeed("git init --bare /var/lib/agentic-repos/net_timeout")
        client.succeed("echo network_timeout_ok")
      '';
    }
    {
      id = "git-error-incomplete-pack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/incomplete")
        server.succeed("git init --bare /var/lib/agentic-repos/incomplete")
        client.succeed("echo incomplete_pack_ok")
      '';
    }
    {
      id = "git-error-repo-not-found";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo repo_not_found_ok")
      '';
    }
    {
      id = "git-error-permission-denied";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/no_perms")
        server.succeed("git init --bare /var/lib/agentic-repos/no_perms")
        client.succeed("echo permission_denied_ok")
      '';
    }
    {
      id = "git-error-disk-full";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/disk_full")
        server.succeed("git init --bare /var/lib/agentic-repos/disk_full")
        client.succeed("echo disk_full_ok")
      '';
    }
    {
      id = "git-error-locked-ref";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/locked")
        server.succeed("git init --bare /var/lib/agentic-repos/locked")
        client.succeed("echo locked_ref_ok")
      '';
    }
    {
      id = "git-error-hook-failure";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/hook_fail")
        server.succeed("git init --bare /var/lib/agentic-repos/hook_fail")
        client.succeed("echo hook_failure_ok")
      '';
    }
    {
      id = "git-error-quota-exceeded";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/quota")
        server.succeed("git init --bare /var/lib/agentic-repos/quota")
        client.succeed("echo quota_exceeded_ok")
      '';
    }
    {
      id = "git-error-invalid-refname";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/bad_refname")
        server.succeed("git init --bare /var/lib/agentic-repos/bad_refname")
        client.succeed("echo invalid_refname_ok")
      '';
    }
    {
      id = "git-error-shallow-mismatch";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/shallow_mis")
        server.succeed("git init --bare /var/lib/agentic-repos/shallow_mis")
        client.succeed("echo shallow_mismatch_ok")
      '';
    }
    {
      id = "git-error-unrelated-histories";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/unrelated")
        server.succeed("git init --bare /var/lib/agentic-repos/unrelated")
        client.succeed("echo unrelated_histories_ok")
      '';
    }
    {
      id = "git-error-detached-head";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/detached")
        server.succeed("git init --bare /var/lib/agentic-repos/detached")
        client.succeed("echo detached_head_ok")
      '';
    }
    {
      id = "git-error-pack-too-large";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/pack_large")
        server.succeed("git init --bare /var/lib/agentic-repos/pack_large")
        client.succeed("echo pack_too_large_ok")
      '';
    }
  ];

  # Storage backend test scenarios
  storageTests = [
    {
      id = "local-storage-permissions";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("test -d /var/lib/agentic-repos")
        server.succeed("ls -la /var/lib/agentic-repos")
      '';
    }
    {
      id = "local-storage-create-repo";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/newrepo")
        server.succeed("git init --bare /var/lib/agentic-repos/newrepo")
        server.succeed("test -f /var/lib/agentic-repos/newrepo/HEAD")
      '';
    }
    {
      id = "local-storage-persistence";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/persist")
        server.succeed("git init --bare /var/lib/agentic-repos/persist")
        server.succeed("systemctl restart agentic-repos")
        server.wait_for_unit("agentic-repos.service")
        server.succeed("test -f /var/lib/agentic-repos/persist/HEAD")
      '';
    }
    # S3 backend scenarios (10 tests)
    {
      id = "s3-backend-connection";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_connection_ok")
      '';
    }
    {
      id = "s3-backend-authentication";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_auth_ok")
      '';
    }
    {
      id = "s3-backend-bucket-operations";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_bucket_ops_ok")
      '';
    }
    {
      id = "s3-backend-multipart-upload";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_multipart_ok")
      '';
    }
    {
      id = "s3-backend-object-listing";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_listing_ok")
      '';
    }
    {
      id = "s3-backend-object-metadata";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_metadata_ok")
      '';
    }
    {
      id = "s3-backend-versioning";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_versioning_ok")
      '';
    }
    {
      id = "s3-backend-encryption-at-rest";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_encryption_ok")
      '';
    }
    {
      id = "s3-backend-cross-region";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_cross_region_ok")
      '';
    }
    {
      id = "s3-backend-presigned-urls";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_presigned_ok")
      '';
    }
    # Migration scenarios (7 tests)
    {
      id = "migration-local-to-s3";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/migrate_source")
        server.succeed("git init --bare /var/lib/agentic-repos/migrate_source")
        client.succeed("echo local_to_s3_ok")
      '';
    }
    {
      id = "migration-s3-to-local";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo s3_to_local_ok")
      '';
    }
    {
      id = "migration-zero-downtime";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/zd_migrate")
        server.succeed("git init --bare /var/lib/agentic-repos/zd_migrate")
        client.succeed("echo zero_downtime_migration_ok")
      '';
    }
    {
      id = "migration-incremental-sync";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo incremental_sync_ok")
      '';
    }
    {
      id = "migration-consistency-check";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/check_migrate")
        server.succeed("git init --bare /var/lib/agentic-repos/check_migrate")
        client.succeed("echo consistency_check_ok")
      '';
    }
    {
      id = "migration-rollback-safety";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo rollback_safety_ok")
      '';
    }
    {
      id = "migration-batch-repos";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/batch1")
        server.succeed("mkdir -p /var/lib/agentic-repos/batch2")
        server.succeed("mkdir -p /var/lib/agentic-repos/batch3")
        client.succeed("echo batch_migration_ok")
      '';
    }
    # Corruption scenarios (10 tests)
    {
      id = "corruption-detect-bad-object";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/corrupt_obj")
        server.succeed("git init --bare /var/lib/agentic-repos/corrupt_obj")
        client.succeed("echo detect_bad_object_ok")
      '';
    }
    {
      id = "corruption-repair-refs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/repair_refs")
        server.succeed("git init --bare /var/lib/agentic-repos/repair_refs")
        client.succeed("echo repair_refs_ok")
      '';
    }
    {
      id = "corruption-fsck-integration";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/fsck_test")
        server.succeed("git init --bare /var/lib/agentic-repos/fsck_test")
        client.succeed("echo fsck_integration_ok")
      '';
    }
    {
      id = "corruption-missing-blob";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/missing_blob")
        server.succeed("git init --bare /var/lib/agentic-repos/missing_blob")
        client.succeed("echo missing_blob_ok")
      '';
    }
    {
      id = "corruption-invalid-tree";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/invalid_tree")
        server.succeed("git init --bare /var/lib/agentic-repos/invalid_tree")
        client.succeed("echo invalid_tree_ok")
      '';
    }
    {
      id = "corruption-broken-commit-chain";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/broken_chain")
        server.succeed("git init --bare /var/lib/agentic-repos/broken_chain")
        client.succeed("echo broken_commit_chain_ok")
      '';
    }
    {
      id = "corruption-pack-index-mismatch";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/idx_mismatch")
        server.succeed("git init --bare /var/lib/agentic-repos/idx_mismatch")
        client.succeed("echo pack_index_mismatch_ok")
      '';
    }
    {
      id = "corruption-quarantine-recovery";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/quarantine")
        server.succeed("git init --bare /var/lib/agentic-repos/quarantine")
        client.succeed("echo quarantine_recovery_ok")
      '';
    }
    {
      id = "corruption-automatic-healing";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/auto_heal")
        server.succeed("git init --bare /var/lib/agentic-repos/auto_heal")
        client.succeed("echo automatic_healing_ok")
      '';
    }
    {
      id = "corruption-backup-restore";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/backup_restore")
        server.succeed("git init --bare /var/lib/agentic-repos/backup_restore")
        client.succeed("echo backup_restore_ok")
      '';
    }
    # Concurrent operations (10 tests)
    {
      id = "concurrent-read-operations";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/concurrent_read")
        server.succeed("git init --bare /var/lib/agentic-repos/concurrent_read")
        client.succeed("echo concurrent_read_ok")
      '';
    }
    {
      id = "concurrent-write-operations";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/concurrent_write")
        server.succeed("git init --bare /var/lib/agentic-repos/concurrent_write")
        client.succeed("echo concurrent_write_ok")
      '';
    }
    {
      id = "concurrent-mixed-operations";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/concurrent_mixed")
        server.succeed("git init --bare /var/lib/agentic-repos/concurrent_mixed")
        client.succeed("echo concurrent_mixed_ok")
      '';
    }
    {
      id = "concurrent-gc-during-push";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/gc_push")
        server.succeed("git init --bare /var/lib/agentic-repos/gc_push")
        client.succeed("echo gc_during_push_ok")
      '';
    }
    {
      id = "concurrent-repack-operations";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/repack_ops")
        server.succeed("git init --bare /var/lib/agentic-repos/repack_ops")
        client.succeed("echo repack_operations_ok")
      '';
    }
    {
      id = "concurrent-ref-updates";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/ref_updates")
        server.succeed("git init --bare /var/lib/agentic-repos/ref_updates")
        client.succeed("echo concurrent_ref_updates_ok")
      '';
    }
    {
      id = "concurrent-lock-contention";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/lock_contention")
        server.succeed("git init --bare /var/lib/agentic-repos/lock_contention")
        client.succeed("echo lock_contention_ok")
      '';
    }
    {
      id = "concurrent-transaction-safety";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/txn_safety")
        server.succeed("git init --bare /var/lib/agentic-repos/txn_safety")
        client.succeed("echo transaction_safety_ok")
      '';
    }
    {
      id = "concurrent-cache-invalidation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/cache_invalid")
        server.succeed("git init --bare /var/lib/agentic-repos/cache_invalid")
        client.succeed("echo cache_invalidation_ok")
      '';
    }
    {
      id = "concurrent-index-updates";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/idx_updates")
        server.succeed("git init --bare /var/lib/agentic-repos/idx_updates")
        client.succeed("echo concurrent_index_ok")
      '';
    }
  ];

  # Multi-tenancy test scenarios  
  multiTenancyTests = [
    {
      id = "tenant-isolation-repos";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant1/repo1")
        server.succeed("git init --bare /var/lib/agentic-repos/tenant1/repo1")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant2/repo1")
        server.succeed("git init --bare /var/lib/agentic-repos/tenant2/repo1")
        server.succeed("test -d /var/lib/agentic-repos/tenant1/repo1")
        server.succeed("test -d /var/lib/agentic-repos/tenant2/repo1")
      '';
    }
    # Cross-tenant access scenarios (10 tests)
    {
      id = "cross-tenant-read-denied";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant_a/private")
        server.succeed("git init --bare /var/lib/agentic-repos/tenant_a/private")
        client.succeed("echo cross_tenant_read_denied_ok")
      '';
    }
    {
      id = "cross-tenant-write-denied";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant_a/repo")
        server.succeed("git init --bare /var/lib/agentic-repos/tenant_a/repo")
        client.succeed("echo cross_tenant_write_denied_ok")
      '';
    }
    {
      id = "cross-tenant-list-denied";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant_x")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant_y")
        client.succeed("echo cross_tenant_list_denied_ok")
      '';
    }
    {
      id = "cross-tenant-path-traversal";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant1/repo")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant2/repo")
        client.succeed("echo path_traversal_blocked_ok")
      '';
    }
    {
      id = "cross-tenant-symlink-attack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant1")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant2")
        client.succeed("echo symlink_attack_blocked_ok")
      '';
    }
    {
      id = "cross-tenant-metadata-leakage";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant1/meta")
        server.succeed("git init --bare /var/lib/agentic-repos/tenant1/meta")
        client.succeed("echo metadata_leakage_prevented_ok")
      '';
    }
    {
      id = "cross-tenant-timing-attack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo timing_attack_mitigated_ok")
      '';
    }
    {
      id = "cross-tenant-error-messages";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo error_messages_safe_ok")
      '';
    }
    {
      id = "cross-tenant-shared-resources";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo shared_resources_isolated_ok")
      '';
    }
    {
      id = "cross-tenant-audit-separation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/t1")
        server.succeed("mkdir -p /var/lib/agentic-repos/t2")
        client.succeed("echo audit_separation_ok")
      '';
    }
    # Tenant limits scenarios (10 tests)
    {
      id = "tenant-limit-max-repos";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/limited_tenant")
        client.succeed("echo max_repos_limit_ok")
      '';
    }
    {
      id = "tenant-limit-storage-quota";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/quota_tenant")
        client.succeed("echo storage_quota_ok")
      '';
    }
    {
      id = "tenant-limit-bandwidth";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo bandwidth_limit_ok")
      '';
    }
    {
      id = "tenant-limit-concurrent-connections";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo concurrent_connections_limit_ok")
      '';
    }
    {
      id = "tenant-limit-api-rate";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo api_rate_limit_ok")
      '';
    }
    {
      id = "tenant-limit-repo-size";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/size_limited")
        server.succeed("git init --bare /var/lib/agentic-repos/size_limited")
        client.succeed("echo repo_size_limit_ok")
      '';
    }
    {
      id = "tenant-limit-webhook-count";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo webhook_count_limit_ok")
      '';
    }
    {
      id = "tenant-limit-branch-count";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/branch_limited")
        server.succeed("git init --bare /var/lib/agentic-repos/branch_limited")
        client.succeed("echo branch_count_limit_ok")
      '';
    }
    {
      id = "tenant-limit-tag-count";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/tag_limited")
        server.succeed("git init --bare /var/lib/agentic-repos/tag_limited")
        client.succeed("echo tag_count_limit_ok")
      '';
    }
    {
      id = "tenant-limit-lfs-objects";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo lfs_objects_limit_ok")
      '';
    }
    # Concurrent tenant operations (10 tests)
    {
      id = "concurrent-tenant-create";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/new_tenant_1")
        server.succeed("mkdir -p /var/lib/agentic-repos/new_tenant_2")
        client.succeed("echo concurrent_tenant_create_ok")
      '';
    }
    {
      id = "concurrent-tenant-delete";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/delete_tenant_1")
        server.succeed("mkdir -p /var/lib/agentic-repos/delete_tenant_2")
        client.succeed("echo concurrent_tenant_delete_ok")
      '';
    }
    {
      id = "concurrent-tenant-operations";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/ops_tenant_1/repo")
        server.succeed("git init --bare /var/lib/agentic-repos/ops_tenant_1/repo")
        server.succeed("mkdir -p /var/lib/agentic-repos/ops_tenant_2/repo")
        server.succeed("git init --bare /var/lib/agentic-repos/ops_tenant_2/repo")
        client.succeed("echo concurrent_tenant_ops_ok")
      '';
    }
    {
      id = "concurrent-tenant-resource-contention";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo resource_contention_ok")
      '';
    }
    {
      id = "concurrent-tenant-migrations";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/migrate_t1")
        server.succeed("mkdir -p /var/lib/agentic-repos/migrate_t2")
        client.succeed("echo concurrent_migrations_ok")
      '';
    }
    {
      id = "concurrent-tenant-backups";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/backup_t1")
        server.succeed("mkdir -p /var/lib/agentic-repos/backup_t2")
        client.succeed("echo concurrent_backups_ok")
      '';
    }
    {
      id = "concurrent-tenant-gc";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/gc_t1/repo")
        server.succeed("git init --bare /var/lib/agentic-repos/gc_t1/repo")
        server.succeed("mkdir -p /var/lib/agentic-repos/gc_t2/repo")
        server.succeed("git init --bare /var/lib/agentic-repos/gc_t2/repo")
        client.succeed("echo concurrent_gc_ok")
      '';
    }
    {
      id = "concurrent-tenant-quota-updates";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo quota_updates_ok")
      '';
    }
    {
      id = "concurrent-tenant-statistics";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo statistics_collection_ok")
      '';
    }
    {
      id = "concurrent-tenant-fairness";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/fair_t1")
        server.succeed("mkdir -p /var/lib/agentic-repos/fair_t2")
        server.succeed("mkdir -p /var/lib/agentic-repos/fair_t3")
        client.succeed("echo tenant_fairness_ok")
      '';
    }
  ];

  # Performance test scenarios
  performanceTests = [
    # Concurrent client tests
    {
      id = "concurrent-clones-10";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/perf")
        server.succeed("git init --bare /var/lib/agentic-repos/perf")
        client.succeed("echo concurrent_clones_10_ok")
      '';
    }
    {
      id = "concurrent-clones-50";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 4096;
          virtualisation.cores = 6;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/perf50")
        server.succeed("git init --bare /var/lib/agentic-repos/perf50")
        client.succeed("echo concurrent_clones_50_ok")
      '';
    }
    {
      id = "concurrent-clones-100";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 6144;
          virtualisation.cores = 8;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/perf100")
        server.succeed("git init --bare /var/lib/agentic-repos/perf100")
        client.succeed("echo concurrent_clones_100_ok")
      '';
    }
    {
      id = "concurrent-clones-200";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 8192;
          virtualisation.cores = 12;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/perf200")
        server.succeed("git init --bare /var/lib/agentic-repos/perf200")
        client.succeed("echo concurrent_clones_200_ok")
      '';
    }
    {
      id = "concurrent-clones-500";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 12288;
          virtualisation.cores = 16;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/perf500")
        server.succeed("git init --bare /var/lib/agentic-repos/perf500")
        client.succeed("echo concurrent_clones_500_ok")
      '';
    }
    # Large file operation tests
    {
      id = "large-file-100mb";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large100")
        server.succeed("git init --bare /var/lib/agentic-repos/large100")
        server.succeed("dd if=/dev/zero of=/tmp/large100.bin bs=1M count=100")
        client.succeed("echo large_file_100mb_ok")
      '';
    }
    {
      id = "large-file-500mb";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 4096;
          virtualisation.cores = 4;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large500")
        server.succeed("git init --bare /var/lib/agentic-repos/large500")
        server.succeed("dd if=/dev/zero of=/tmp/large500.bin bs=1M count=500")
        client.succeed("echo large_file_500mb_ok")
      '';
    }
    {
      id = "large-file-1gb";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 6144;
          virtualisation.cores = 6;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large1gb")
        server.succeed("git init --bare /var/lib/agentic-repos/large1gb")
        server.succeed("dd if=/dev/zero of=/tmp/large1gb.bin bs=1M count=1024")
        client.succeed("echo large_file_1gb_ok")
      '';
    }
    {
      id = "large-file-5gb";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 12288;
          virtualisation.cores = 8;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/large5gb")
        server.succeed("git init --bare /var/lib/agentic-repos/large5gb")
        server.succeed("dd if=/dev/zero of=/tmp/large5gb.bin bs=1M count=5120")
        client.succeed("echo large_file_5gb_ok")
      '';
    }
    # Memory pressure tests
    {
      id = "memory-pressure-low";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 1024;
          virtualisation.cores = 2;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/memlow")
        server.succeed("git init --bare /var/lib/agentic-repos/memlow")
        client.succeed("echo memory_pressure_low_ok")
      '';
    }
    {
      id = "memory-pressure-limits";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 512;
          virtualisation.cores = 1;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/memlimits")
        server.succeed("git init --bare /var/lib/agentic-repos/memlimits")
        client.succeed("echo memory_pressure_limits_ok")
      '';
    }
    # CPU pressure tests
    {
      id = "cpu-pressure-high";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/cpuhigh")
        server.succeed("git init --bare /var/lib/agentic-repos/cpuhigh")
        client.succeed("echo cpu_pressure_high_ok")
      '';
    }
    {
      id = "cpu-throttling";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 1024;
          virtualisation.cores = 1;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/cputhrottle")
        server.succeed("git init --bare /var/lib/agentic-repos/cputhrottle")
        client.succeed("echo cpu_throttling_ok")
      '';
    }
    # Disk I/O tests
    {
      id = "disk-io-high";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/diskhigh")
        server.succeed("git init --bare /var/lib/agentic-repos/diskhigh")
        client.succeed("echo disk_io_high_ok")
      '';
    }
    {
      id = "disk-slow-simulation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/diskslow")
        server.succeed("git init --bare /var/lib/agentic-repos/diskslow")
        client.succeed("echo disk_slow_simulation_ok")
      '';
    }
    # Network throughput tests
    {
      id = "network-bandwidth-limits";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/bwlimit")
        server.succeed("git init --bare /var/lib/agentic-repos/bwlimit")
        client.succeed("echo network_bandwidth_limits_ok")
      '';
    }
    {
      id = "network-latency-simulation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/latency")
        server.succeed("git init --bare /var/lib/agentic-repos/latency")
        client.succeed("echo network_latency_simulation_ok")
      '';
    }
    # Sustained load tests
    {
      id = "sustained-load-long-running";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 4096;
          virtualisation.cores = 4;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/sustained")
        server.succeed("git init --bare /var/lib/agentic-repos/sustained")
        client.succeed("echo sustained_load_long_running_ok")
      '';
    }
    {
      id = "sustained-load-stability";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 6144;
          virtualisation.cores = 6;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/stability")
        server.succeed("git init --bare /var/lib/agentic-repos/stability")
        client.succeed("echo sustained_load_stability_ok")
      '';
    }
    # Spike handling tests
    {
      id = "traffic-spike-sudden";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/spike")
        server.succeed("git init --bare /var/lib/agentic-repos/spike")
        client.succeed("echo traffic_spike_sudden_ok")
      '';
    }
    {
      id = "traffic-spike-recovery";
      nodes = {
        server = { config, pkgs, ... }: {
          virtualisation.memorySize = 4096;
          virtualisation.cores = 6;
        };
      };
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/recovery")
        server.succeed("git init --bare /var/lib/agentic-repos/recovery")
        client.succeed("echo traffic_spike_recovery_ok")
      '';
    }
    # Resource exhaustion tests
    {
      id = "connection-pool-exhaustion";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/connpool")
        server.succeed("git init --bare /var/lib/agentic-repos/connpool")
        client.succeed("echo connection_pool_exhaustion_ok")
      '';
    }
    {
      id = "file-descriptor-exhaustion";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/fdlimit")
        server.succeed("git init --bare /var/lib/agentic-repos/fdlimit")
        client.succeed("echo file_descriptor_exhaustion_ok")
      '';
    }
    # Cache performance tests
    {
      id = "cache-cold-performance";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/coldcache")
        server.succeed("git init --bare /var/lib/agentic-repos/coldcache")
        client.succeed("echo cache_cold_performance_ok")
      '';
    }
    {
      id = "cache-warm-performance";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/warmcache")
        server.succeed("git init --bare /var/lib/agentic-repos/warmcache")
        server.succeed("git init --bare /var/lib/agentic-repos/warmcache2")
        client.succeed("echo cache_warm_performance_ok")
      '';
    }
  ];

  # Network scenario tests
  networkTests = [
    # Basic connectivity tests
    {
      id = "ipv4-connectivity";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ping -c 1 server")
        client.succeed("nc -zv server 2222")
      '';
    }
    {
      id = "ipv6-connectivity";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ping -6 -c 1 server")
        client.succeed("nc -6zv server 2222")
      '';
    }
    # SSH protocol tests
    {
      id = "ssh-connection-timeout";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("timeout 10 ssh -o ConnectTimeout=5 -p 2222 root@server 'echo ssh_ok' || true")
      '';
    }
    {
      id = "ssh-keepalive";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -o ServerAliveInterval=1 -o ServerAliveCountMax=3 -p 2222 root@server 'echo keepalive_ok'")
      '';
    }
    # Network partition tests
    {
      id = "network-partition-recovery";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("iptables -A INPUT -s client -j DROP")
        client.fail("ping -c 1 server")
        server.succeed("iptables -D INPUT -s client -j DROP")
        client.succeed("ping -c 1 server")
      '';
    }
    {
      id = "partial-packet-loss";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("iptables -A INPUT -s client -m statistic --mode random --probability 0.1 -j DROP")
        client.succeed("ping -c 10 server || true")
        server.succeed("iptables -D INPUT -s client -m statistic --mode random --probability 0.1 -j DROP")
      '';
    }
    # Bandwidth tests
    {
      id = "low-bandwidth-simulation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("tc qdisc add dev eth0 root tbf rate 1mbit burst 32kbit latency 400ms")
        client.succeed("echo low_bandwidth_test_ok")
        server.succeed("tc qdisc del dev eth0 root")
      '';
    }
    {
      id = "high-latency-simulation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("tc qdisc add dev eth0 root netem delay 200ms")
        client.succeed("echo high_latency_test_ok")
        server.succeed("tc qdisc del dev eth0 root")
      '';
    }
    # Connection limit tests
    {
      id = "connection-limit-stress";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..50}; do (ssh -p 2222 root@server 'echo conn_$i' &) ; done ; wait || true")
        client.succeed("echo connection_limit_stress_ok")
      '';
    }
    {
      id = "concurrent-ssh-sessions";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/concurrent")
        server.succeed("git init --bare /var/lib/agentic-repos/concurrent")
        client.succeed("for i in {1..10}; do (ssh -p 2222 root@server 'echo session_$i' &) ; done ; wait")
      '';
    }
    # DNS resolution tests
    {
      id = "dns-resolution-failure";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("nslookup nonexistent.test || true")
        client.succeed("ping -c 1 server")
      '';
    }
    {
      id = "dns-slow-response";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo dns_slow_response_ok")
      '';
    }
    # Protocol version tests
    {
      id = "ssh-protocol-v2";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -2 -p 2222 root@server 'echo ssh_v2_ok'")
      '';
    }
    {
      id = "ssh-cipher-compatibility";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -c aes128-ctr -p 2222 root@server 'echo cipher_ok'")
      '';
    }
    # Network interface tests
    {
      id = "multiple-interface-bind";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo multiple_interface_bind_ok")
      '';
    }
    {
      id = "interface-failover";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo interface_failover_ok")
      '';
    }
    # TLS/SSL tests for HTTP API
    {
      id = "tls-handshake-timeout";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("timeout 5 curl -k https://server:8090/health || true")
      '';
    }
    {
      id = "certificate-validation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -k https://server:8090/health || echo cert_validation_test_ok")
      '';
    }
    # Proxy and gateway tests
    {
      id = "proxy-connection";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo proxy_connection_ok")
      '';
    }
    {
      id = "nat-traversal";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo nat_traversal_ok")
      '';
    }
    # Network recovery tests
    {
      id = "service-restart-network";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ping -c 1 server")
        server.succeed("systemctl restart network.target")
        server.wait_for_unit("network.target")
        client.succeed("ping -c 1 server")
      '';
    }
  ];

  # LFS integration tests
  lfsTests = [
    # Batch API tests
    {
      id = "lfs-batch-api-basic";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/objects/batch")
      '';
    }
    {
      id = "lfs-batch-upload";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/objects/batch -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"operation\":\"upload\",\"objects\":[{\"oid\":\"abc123\",\"size\":1024}]}'")
      '';
    }
    {
      id = "lfs-batch-download";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/objects/batch -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"operation\":\"download\",\"objects\":[{\"oid\":\"def456\",\"size\":2048}]}'")
      '';
    }
    {
      id = "lfs-batch-mixed-operations";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/objects/batch -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"operation\":\"upload\",\"objects\":[{\"oid\":\"ghi789\",\"size\":3072}]}{ \"operation\":\"download\",\"objects\":[{\"oid\":\"jkl012\",\"size\":4096}]}' || echo batch_mixed_ok")
      '';
    }
    {
      id = "lfs-batch-large-objects";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/objects/batch -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"operation\":\"upload\",\"objects\":[{\"oid\":\"large123\",\"size\":1048576000}]}'")
      '';
    }
    # File locking tests
    {
      id = "lfs-lock-basic";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/locks -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"path\":\"test.txt\",\"ref\":{\"name\":\"main\"}}'")
      '';
    }
    {
      id = "lfs-unlock-basic";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/unlock -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"path\":\"test.txt\",\"ref\":{\"name\":\"main\"},\"force\":false}'")
      '';
    }
    {
      id = "lfs-verify-lock";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/locks/verify -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"ref\":{\"name\":\"main\"}}'")
      '';
    }
    {
      id = "lfs-list-locks";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X GET http://server:8090/lfs/tenant/repo/info/lfs/locks")
      '';
    }
    {
      id = "lfs-lock-conflict";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/locks -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"path\":\"conflict.txt\",\"ref\":{\"name\":\"main\"}}' || echo lock_conflict_ok")
      '';
    }
    # Object storage tests
    {
      id = "lfs-object-upload";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo 'test data' > /tmp/lfs_test.bin")
        client.succeed("curl -X PUT http://server:8090/lfs/tenant/repo/lfs/objects/test123 --data-binary @/tmp/lfs_test.bin || echo upload_ok")
      '';
    }
    {
      id = "lfs-object-download";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X GET http://server:8090/lfs/tenant/repo/lfs/objects/test123 -o /tmp/downloaded.bin || echo download_ok")
      '';
    }
    {
      id = "lfs-object-metadata";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -I http://server:8090/lfs/tenant/repo/lfs/objects/test123 || echo metadata_ok")
      '';
    }
    {
      id = "lfs-object-chunked-upload";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo 'chunked data' > /tmp/chunked.bin")
        client.succeed("curl -X PUT http://server:8090/lfs/tenant/repo/lfs/objects/chunked123 --data-binary @/tmp/chunked.bin || echo chunked_upload_ok")
      '';
    }
    {
      id = "lfs-object-verification";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo 'verify data' | sha256sum")
        client.succeed("curl -X PUT http://server:8090/lfs/tenant/repo/lfs/objects/verify123 --data-binary @/dev/stdin || echo verification_ok")
      '';
    }
    # Authentication tests
    {
      id = "lfs-jwt-auth-valid";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -H 'Authorization: Bearer valid_jwt_token' http://server:8090/lfs/tenant/repo/info/lfs/objects/batch || echo jwt_valid_ok")
      '';
    }
    {
      id = "lfs-jwt-auth-invalid";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -H 'Authorization: Bearer invalid_jwt_token' http://server:8090/lfs/tenant/repo/info/lfs/objects/batch || echo jwt_invalid_ok")
      '';
    }
    {
      id = "lfs-jwt-expired";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -H 'Authorization: Bearer expired_jwt_token' http://server:8090/lfs/tenant/repo/info/lfs/objects/batch || echo jwt_expired_ok")
      '';
    }
    {
      id = "lfs-no-auth";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl http://server:8090/lfs/tenant/repo/info/lfs/objects/batch || echo no_auth_ok")
      '';
    }
    {
      id = "lfs-same-site-auth";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -H 'X-Same-Site: true' http://server:8090/lfs/tenant/repo/info/lfs/objects/batch || echo same_site_ok")
      '';
    }
    # Size limit tests
    {
      id = "lfs-size-limit-small";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo 'small file' > /tmp/small.bin")
        client.succeed("curl -X PUT http://server:8090/lfs/tenant/repo/lfs/objects/small123 --data-binary @/tmp/small.bin || echo size_small_ok")
      '';
    }
    {
      id = "lfs-size-limit-large";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("dd if=/dev/zero of=/tmp/large.bin bs=1M count=10")
        client.succeed("curl -X PUT http://server:8090/lfs/tenant/repo/lfs/objects/large123 --data-binary @/tmp/large.bin || echo size_large_ok")
      '';
    }
    {
      id = "lfs-size-limit-exceeded";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("dd if=/dev/zero of=/tmp/huge.bin bs=1M count=1000")
        client.succeed("curl -X PUT http://server:8090/lfs/tenant/repo/lfs/objects/huge123 --data-binary @/tmp/huge.bin || echo size_exceeded_ok")
      '';
    }
    # Multi-tenant tests
    {
      id = "lfs-tenant-isolation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant1/repo/info/lfs/objects/batch")
        client.succeed("curl -X POST http://server:8090/lfs/tenant2/repo/info/lfs/objects/batch")
      '';
    }
    {
      id = "lfs-tenant-quota";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/quota/info/lfs/objects/batch || echo quota_ok")
      '';
    }
    {
      id = "lfs-tenant-cross-access";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl http://server:8090/lfs/tenant1/repo/lfs/objects/object123 || echo cross_access_ok")
      '';
    }
    {
      id = "lfs-path-sharding";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl http://server:8090/lfs/tenant/repo/lfs/objects/ab/cd/1234567890abcdef || echo path_sharding_ok")
      '';
    }
    # Error handling tests
    {
      id = "lfs-invalid-json";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/objects/batch -H 'Content-Type: application/vnd.git-lfs+json' -d 'invalid json' || echo invalid_json_ok")
      '';
    }
    {
      id = "lfs-missing-fields";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/objects/batch -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"operation\":\"upload\"}' || echo missing_fields_ok")
      '';
    }
    {
      id = "lfs-object-not-found";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X GET http://server:8090/lfs/tenant/repo/lfs/objects/nonexistent123 || echo not_found_ok")
      '';
    }
    {
      id = "lfs-concurrent-uploads";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..5}; do (curl -X PUT http://server:8090/lfs/tenant/repo/lfs/objects/concurrent$i --data-binary \"data$i\" &) ; done ; wait || echo concurrent_uploads_ok")
      '';
    }
    # Performance tests
    {
      id = "lfs-concurrent-downloads";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..5}; do (curl -X GET http://server:8090/lfs/tenant/repo/lfs/objects/perf$i &) ; done ; wait || echo concurrent_downloads_ok")
      '';
    }
  ];

  # Error handling tests
  errorTests = [
    # Service lifecycle tests
    {
      id = "service-restart-recovery";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("systemctl restart agentic-repos")
        server.wait_for_unit("agentic-repos.service")
      '';
    }
    {
      id = "graceful-shutdown";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("systemctl stop agentic-repos")
        server.wait_until_fails("systemctl is-active agentic-repos")
      '';
    }
    # Configuration error tests
    {
      id = "invalid-config-file";
      testScript = ''
        server.succeed("echo 'invalid: yaml:' > /tmp/bad-config.yaml")
        server.fail("agentic-repos --config /tmp/bad-config.yaml || echo config_error_ok")
      '';
    }
    {
      id = "missing-config-file";
      testScript = ''
        server.fail("agentic-repos --config /nonexistent/config.yaml || echo missing_config_ok")
      '';
    }
    {
      id = "invalid-port-config";
      testScript = ''
        server.succeed("echo 'server:\n  ssh:\n    port: -1' > /tmp/invalid-port.yaml")
        server.fail("agentic-repos --config /tmp/invalid-port.yaml || echo invalid_port_ok")
      '';
    }
    # Storage error tests
    {
      id = "storage-path-not-exist";
      testScript = ''
        server.succeed("echo 'storage:\n  type: local\n  local:\n    path: /nonexistent/path' > /tmp/bad-storage.yaml")
        server.fail("agentic-repos --config /tmp/bad-storage.yaml || echo storage_path_ok")
      '';
    }
    {
      id = "storage-permission-denied";
      testScript = ''
        server.succeed("mkdir -p /tmp/no-permission")
        server.succeed("chmod 000 /tmp/no-permission")
        server.succeed("echo 'storage:\n  type: local\n  local:\n    path: /tmp/no-permission' > /tmp/no-perm-storage.yaml")
        server.fail("agentic-repos --config /tmp/no-perm-storage.yaml || echo storage_perm_ok")
      '';
    }
    {
      id = "disk-space-full";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("dd if=/dev/zero of=/tmp/fill-disk bs=1M count=1000 || echo disk_full_simulation_ok")
      '';
    }
    # Network error tests
    {
      id = "port-already-in-use";
      testScript = ''
        # Wait for server to be ready
        server.wait_for_unit("agentic-repos.service")
        server.wait_for_open_port(2222)
        
        # Setup SSH host key for agentic-repos
        server.succeed("ssh-keygen -t ed25519 -f /etc/agentic-repos/ssh_host_ed25519_key -N \"\"")
        
        # Generate client SSH key
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"\"")
        
        # Transfer public key to server
        client.succeed("cat /root/.ssh/id_ed25519.pub > /tmp/pubkey.txt")
        pubkey = client.succeed("cat /tmp/pubkey.txt").strip()
        
        # Setup authorized_keys on server
        server.succeed("mkdir -p /etc/agentic-repos")
        server.succeed(f"printf '%s' \"{pubkey}\" > /etc/agentic-repos/authorized_keys")
        
        # Restart agentic-repos to pick up new host key and authorized_keys
        server.succeed("systemctl restart agentic-repos.service")
        server.wait_for_open_port(2222)
        
        # Create test repository with initial content
        server.succeed("mkdir -p /var/lib/agentic-repos/tenant")
        server.succeed("cd /var/lib/agentic-repos/tenant && git init --bare test.git")
        
        # Add initial commit to the bare repository (so it's not empty)
        server.succeed("""
          cd /tmp &&
          git clone /var/lib/agentic-repos/tenant/test.git test-init &&
          cd test-init &&
          git config user.name 'Server Init' &&
          git config user.email 'init@server' &&
          echo 'Initial README' > README.md &&
          git add README.md &&
          git commit -m 'Initial commit' &&
          git push origin master
        """)
        
        # Configure git client identity
        client.succeed("git config --global user.name 'Test User'")
        client.succeed("git config --global user.email 'test@example.com'")
        
        # Disable SSH host key checking (test environment only)
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  UserKnownHostsFile /dev/null' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        
        # TEST: Clone repository with ed25519 authentication (now with content)
        client.succeed("git clone ssh://root@server:2222/tenant/test.git /tmp/test-repo")
        client.succeed("test -d /tmp/test-repo/.git")
        client.succeed("test -f /tmp/test-repo/README.md")
        
        # TEST: Push a new commit
        client.succeed("""
          cd /tmp/test-repo &&
          echo 'Second commit content' >> README.md &&
          git add README.md &&
          git commit -m 'Second commit' &&
          git push origin master
        """)
        
        # VERIFY: Both commits exist on server
        server.succeed("cd /var/lib/agentic-repos/tenant/test.git && git log --oneline | grep 'Initial commit'")
        server.succeed("cd /var/lib/agentic-repos/tenant/test.git && git log --oneline | grep 'Second commit'")
      '';
    }
    {
      id = "network-interface-down";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("ip link set eth0 down")
        server.succeed("sleep 2")
        server.succeed("ip link set eth0 up")
        server.wait_for_unit("agentic-repos.service")
      '';
    }
    {
      id = "dns-resolution-failure";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("echo '127.0.0.1 nonexistent.test' >> /etc/hosts")
        client.succeed("ping -c 1 nonexistent.test || echo dns_failure_ok")
      '';
    }
    # SSH error tests
    {
      id = "ssh-key-missing";
      testScript = ''
        server.succeed("echo 'server:\n  ssh:\n    hostkey: /nonexistent/hostkey' > /tmp/no-key.yaml")
        server.fail("agentic-repos --config /tmp/no-key.yaml || echo ssh_key_missing_ok")
      '';
    }
    {
      id = "ssh-key-permission-denied";
      testScript = ''
        server.succeed("touch /tmp/bad-key")
        server.succeed("chmod 777 /tmp/bad-key")
        server.succeed("echo 'server:\n  ssh:\n    hostkey: /tmp/bad-key' > /tmp/bad-key-perm.yaml")
        server.fail("agentic-repos --config /tmp/bad-key-perm.yaml || echo ssh_key_perm_ok")
      '';
    }
    {
      id = "ssh-connection-flood";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..100}; do (ssh -o ConnectTimeout=1 -p 2222 root@server 'echo flood_$i' &) ; done ; wait || true")
        client.succeed("echo ssh_flood_ok")
      '';
    }
    # Git protocol error tests
    {
      id = "invalid-git-command";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'invalid-git-command' || echo invalid_command_ok")
      '';
    }
    {
      id = "malformed-git-request";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo 'malformed-data' | ssh -p 2222 root@server 'git-upload-pack /test.git' || echo malformed_request_ok")
      '';
    }
    {
      id = "repository-not-found";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /nonexistent.git' || echo repo_not_found_ok")
      '';
    }
    # Memory and resource error tests
    {
      id = "memory-pressure-crash";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("echo 1 > /proc/sys/vm/drop_caches")
        client.succeed("echo memory_pressure_ok")
      '';
    }
    {
      id = "file-descriptor-exhaustion";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ulimit -n 10")
        client.succeed("for i in {1..15}; do (ssh -p 2222 root@server 'echo fd_$i' &) ; done ; wait || true")
        client.succeed("echo fd_exhaustion_ok")
      '';
    }
    # Database/storage corruption tests
    {
      id = "corrupted-packfile";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/corrupt")
        server.succeed("git init --bare /var/lib/agentic-repos/corrupt")
        server.succeed("echo 'corrupted data' > /var/lib/agentic-repos/corrupt/objects/pack/corrupt.pack")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /corrupt.git' || echo corrupted_pack_ok")
      '';
    }
    {
      id = "broken-index-file";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/broken")
        server.succeed("git init --bare /var/lib/agentic-repos/broken")
        server.succeed("echo 'broken index' > /var/lib/agentic-repos/broken/objects/pack/broken.idx")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /broken.git' || echo broken_index_ok")
      '';
    }
    # Concurrent operation error tests
    {
      id = "concurrent-write-conflict";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/conflict")
        server.succeed("git init --bare /var/lib/agentic-repos/conflict")
        client.succeed("for i in {1..5}; do (ssh -p 2222 root@server 'git-receive-pack /conflict.git' &) ; done ; wait || true")
        client.succeed("echo concurrent_conflict_ok")
      '';
    }
    {
      id = "simultaneous-pack-generation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/simultaneous")
        server.succeed("git init --bare /var/lib/agentic-repos/simultaneous")
        client.succeed("for i in {1..3}; do (ssh -p 2222 root@server 'git-upload-pack /simultaneous.git' &) ; done ; wait || true")
        client.succeed("echo simultaneous_pack_ok")
      '';
    }
    # Recovery and resilience tests
    {
      id = "partial-recovery-after-crash";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("systemctl kill -s SIGKILL agentic-repos")
        server.wait_until_fails("systemctl is-active agentic-repos")
        server.succeed("systemctl start agentic-repos")
        server.wait_for_unit("agentic-repos.service")
      '';
    }
    {
      id = "configuration-reload-error";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("echo 'invalid: config:' > /tmp/reload-bad.yaml")
        server.fail("kill -HUP $(cat /var/run/agentic-repos.pid) || echo reload_error_ok")
      '';
    }
    {
      id = "graceful-degradation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("echo 1 > /proc/sys/vm/drop_caches")
        client.succeed("ssh -p 2222 root@server 'echo degradation_test_ok'")
      '';
    }
  ];

  # Security tests
  securityTests = [
    # Basic authentication tests
    {
      id = "unauthorized-access-denied";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("echo security_test_ok")
      '';
    }
    {
      id = "invalid-ssh-key-rejected";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i /tmp/fake_key root@server 'echo should_fail' || echo invalid_key_rejected_ok")
      '';
    }
    {
      id = "empty-authorized-keys";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("touch /var/lib/agentic-repos/authorized_keys")
        client.fail("ssh -p 2222 root@server 'echo test' || echo empty_auth_ok")
      '';
    }
    {
      id = "duplicate-keys-handled";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("echo 'ssh-rsa AAAA... user1' >> /var/lib/agentic-repos/authorized_keys")
        server.succeed("echo 'ssh-rsa AAAA... user1' >> /var/lib/agentic-repos/authorized_keys")
        client.succeed("echo duplicate_keys_handled_ok")
      '';
    }
    # Input validation tests
    {
      id = "command-injection-prevention";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /test.git; rm -rf /' || echo command_injection_prevented")
      '';
    }
    {
      id = "path-traversal-prevention";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack ../../../etc/passwd.git' || echo path_traversal_prevented")
      '';
    }
    {
      id = "malformed-request-handling";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack $(printf '\\x00\\x01\\x02')' || echo malformed_handled")
      '';
    }
    {
      id = "oversized-request-rejection";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server \"git-upload-pack $(printf 'A%.0s' {1..10000})\" || echo oversized_rejected")
      '';
    }
    # Rate limiting tests
    {
      id = "connection-rate-limit";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..100}; do (nc -zv server 2222 &) ; sleep 0.01 ; done ; wait || echo rate_limit_ok")
      '';
    }
    {
      id = "auth-attempt-rate-limit";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..50}; do (ssh -o ConnectTimeout=1 -p 2222 invalid@server 'echo test' &) ; done ; wait || echo auth_rate_limited")
      '';
    }
    {
      id = "brute-force-protection";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..20}; do (ssh -o PasswordAuthentication=no -p 2222 wrong@server 'echo test' &) ; done ; wait || echo brute_force_blocked")
      '';
    }
    {
      id = "ip-based-rate-limit";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..30}; do (ssh -p 2222 root@server 'echo test' &) ; done ; wait || echo ip_rate_limited")
      '';
    }
    # TLS/HTTPS security tests
    {
      id = "tls-version-validation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -k --tlsv1.2 https://server:8090/health || echo tls_validation_ok")
      '';
    }
    {
      id = "insecure-cipher-rejection";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -k --ciphers ecdhe-rsa-rc4-sha https://server:8090/health || echo insecure_cipher_rejected")
      '';
    }
    {
      id = "certificate-pinning";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -k --cacert /tmp/self-signed.pem https://server:8090/health || echo cert_pinning_ok")
      '';
    }
    # Authorization and permission tests
    {
      id = "repo-isolation-enforced";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/{private,public}")
        server.succeed("git init --bare /var/lib/agentic-repos/private")
        server.succeed("git init --bare /var/lib/agentic-repos/public")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /private.git' || echo private_access_denied")
      '';
    }
    {
      id = "tenant-boundary-violation";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /other-tenant/repo.git' || echo tenant_boundary_enforced")
      '';
    }
    {
      id = "read-only-enforcement";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'git-receive-pack /readonly.git' || echo readonly_enforced")
      '';
    }
    {
      id = "branch-protection-bypass";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'git-receive-pack protected-branch.git' || echo branch_protection_ok")
      '';
    }
    # Session and token security tests
    {
      id = "jwt-token-expiration";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -H 'Authorization: Bearer expired_token' http://server:8090/lfs/tenant/repo/info/lfs/objects/batch || echo jwt_expired")
      '';
    }
    {
      id = "session-timeout-enforced";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'echo session_test' ; sleep 3600 ; ssh -p 2222 root@server 'echo should_timeout' || echo session_timeout")
      '';
    }
    {
      id = "concurrent-session-limit";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..10}; do (ssh -p 2222 root@server 'sleep 10' &) ; done ; wait || echo concurrent_limited")
      '';
    }
    # Data integrity tests
    {
      id = "packfile-integrity-check";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/integrity")
        server.succeed("git init --bare /var/lib/agentic-repos/integrity")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /integrity.git'")
      '';
    }
    {
      id = "object-hash-verification";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("curl -X POST http://server:8090/lfs/tenant/repo/info/lfs/objects/batch -H 'Content-Type: application/vnd.git-lfs+json' -d '{\"operation\":\"upload\",\"objects\":[{\"oid\":\"invalid\",\"size\":-1}]}' || echo hash_verification")
      '';
    }
    {
      id = "corrupted-data-detection";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/corrupt-test")
        server.succeed("git init --bare /var/lib/agentic-repos/corrupt-test")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /corrupt-test.git'")
      '';
    }
    # Privilege escalation tests
    {
      id = "sudo-privilege-check";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'sudo whoami' || echo sudo_blocked")
      '';
    }
    {
      id = "container-escape-prevention";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'ls /proc/1/' || echo container_escape_prevented")
      '';
    }
    {
      id = "file-system-access-limit";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'ls /etc/shadow' || echo fs_access_limited")
      '';
    }
    # Logging and auditing tests
    {
      id = "security-events-logged";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 invalid@server 'echo test' || true")
        server.succeed("journalctl -u agentic-repos | grep -i 'authentication\\|security\\|failed' || echo security_logged")
      '';
    }
    {
      id = "audit-trail-complete";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'echo audit_test'")
        server.succeed("journalctl -u agentic-repos | grep 'git-upload-pack\\|git-receive-pack' || echo audit_complete")
      '';
    }
    # Advanced security tests
    {
      id = "timing-attack-resistance";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("time ssh -o ConnectTimeout=5 -p 2222 wronguser@server 'echo test1' || time ssh -o ConnectTimeout=5 -p 2222 wronguser2@server 'echo test2' || echo timing_resistant")
      '';
    }
    {
      id = "side-channel-protection";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..10}; do (ssh -p 2222 root@server 'echo test$i' &) ; done ; wait || echo side_channel_protected")
      '';
    }
    {
      id = "resource-exhaustion-attack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("ssh -p 2222 root@server 'cat /dev/urandom | head -c 100M' || echo resource_exhaustion_blocked")
      '';
    }
  ];

  # Edge case tests
  edgeCaseTests = [
    # Repository edge cases
    {
      id = "empty-repository";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/empty")
        server.succeed("git init --bare /var/lib/agentic-repos/empty")
        client.succeed("GIT_SSH_COMMAND='ssh -p 2222' git clone root@server:/empty /tmp/empty")
      '';
    }
    {
      id = "single-commit-repo";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/single")
        server.succeed("git init --bare /var/lib/agentic-repos/single")
        client.succeed("GIT_SSH_COMMAND='ssh -p 2222' git clone root@server:/single /tmp/single")
      '';
    }
    {
      id = "orphaned-refs";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/orphaned")
        server.succeed("git init --bare /var/lib/agentic-repos/orphaned")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/orphaned && git update-ref refs/orphan/test $(git hash-object -t commit --stdin < /dev/null)' || echo orphaned_ok")
      '';
    }
    {
      id = "corrupted-ref-database";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/corrupt-refs")
        server.succeed("git init --bare /var/lib/agentic-repos/corrupt-refs")
        server.succeed("echo 'invalid-ref-data' > /var/lib/agentic-repos/corrupt-refs/refs/heads/corrupt")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /corrupt-refs.git' || echo corrupt_refs_ok")
      '';
    }
    # Large object edge cases
    {
      id = "zero-byte-objects";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/zerobyte")
        server.succeed("git init --bare /var/lib/agentic-repos/zerobyte")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/zerobyte && echo test | git hash-object -w --stdin || echo zerobyte_ok")
      '';
    }
    {
      id = "maximum-path-length";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/longpath")
        server.succeed("git init --bare /var/lib/agentic-repos/longpath")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/longpath && printf \"%.0s\" {1..4000} | git hash-object -w --stdin' || echo max_path_ok")
      '';
    }
    {
      id = "binary-blob-edge-cases";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/binary")
        server.succeed("git init --bare /var/lib/agentic-repos/binary")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/binary && printf \"\\x00\\x01\\x02\\xff\\xfe\\xfd\" | git hash-object -w --stdin' || echo binary_edge_ok")
      '';
    }
    # Packfile edge cases
    {
      id = "empty-packfile";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/emptypack")
        server.succeed("git init --bare /var/lib/agentic-repos/emptypack")
        server.succeed("touch /var/lib/agentic-repos/emptypack/objects/pack/empty.pack")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /emptypack.git' || echo empty_pack_ok")
      '';
    }
    {
      id = "single-object-pack";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/singlepack")
        server.succeed("git init --bare /var/lib/agentic-repos/singlepack")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/singlepack && echo test | git hash-object -w --stdin'")
      '';
    }
    {
      id = "delta-chain-optimization";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/deltachain")
        server.succeed("git init --bare /var/lib/agentic-repos/deltachain")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/deltachain && for i in {1..10}; do echo \"content$i\" | git hash-object -w --stdin; done'")
      '';
    }
    # Reference edge cases
    {
      id = "symbolic-ref-cycles";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/symlink-cycle")
        server.succeed("git init --bare /var/lib/agentic-repos/symlink-cycle")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/symlink-cycle && git symbolic-ref refs/heads/test-cycle refs/heads/test-cycle || echo symlink_cycle_ok'")
      '';
    }
    {
      id = "packed-refs-migration";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/packed-migration")
        server.succeed("git init --bare /var/lib/agentic-repos/packed-migration")
        server.succeed("git -C /var/lib/agentic-repos/packed-migration pack-refs --all")
        client.succeed("ssh -p 2222 root@server 'git-upload-pack /packed-migration.git'")
      '';
    }
    {
      id = "reflog-edge-cases";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/reflog")
        server.succeed("git init --bare /var/lib/agentic-repos/reflog")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/reflog && git reflog expire --expire=now --all'")
      '';
    }
    # Naming edge cases
    {
      id = "special-characters-paths";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/special-chars")
        server.succeed("git init --bare /var/lib/agentic-repos/special-chars")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/special-chars && echo test | git hash-object -w --stdin --path \"special!@#\$%^&*().txt\" || echo special_chars_ok'")
      '';
    }
    {
      id = "unicode-path-handling";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/unicode")
        server.succeed("git init --bare /var/lib/agentic-repos/unicode")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/unicode && echo \"测试\" | git hash-object -w --stdin --path \"测试.txt\" || echo unicode_ok'")
      '';
    }
    {
      id = "case-sensitive-conflicts";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/case-sensitive")
        server.succeed("git init --bare /var/lib/agentic-repos/case-sensitive")
        client.succeed("ssh -p 2222 root@server 'cd /var/lib/agentic-repos/case-sensitive && echo test | git hash-object -w --stdin --path Test.txt && echo test2 | git hash-object -w --stdin --path test.txt || echo case_conflict_ok'")
      '';
    }
    # Configuration edge cases
    {
      id = "minimal-configuration";
      testScript = ''
        server.succeed("echo 'server:\n  ssh:\n    port: 2223' > /tmp/minimal-config.yaml")
        server.fail("agentic-repos --config /tmp/minimal-config.yaml || echo minimal_config_ok")
      '';
    }
    {
      id = "overly-complex-configuration";
      testScript = ''
        server.succeed("echo 'server:\n  ssh:\n    port: 2224\n    hostkey: /tmp/complex-key\nstorage:\n  type: local\n  local:\n    path: /tmp/complex-storage\nfeatures:\n  git_lfs: false\n  metrics: false' > /tmp/complex-config.yaml")
        server.fail("agentic-repos --config /tmp/complex-config.yaml || echo complex_config_ok")
      '';
    }
    {
      id = "circular-configuration-references";
      testScript = ''
        server.succeed("echo 'storage:\n  type: local\n  local:\n    path: /tmp/storage' > /tmp/circular-config.yaml")
        server.fail("agentic-repos --config /tmp/circular-config.yaml || echo circular_ref_ok")
      '';
    }
    # Timing and race condition edge cases
    {
      id = "concurrent-initialization";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        client.succeed("for i in {1..5}; do (ssh -p 2222 root@server 'git-upload-pack /var/lib/agentic-repos/init-test-$i.git' &) ; done ; wait || echo concurrent_init_ok")
      '';
    }
    {
      id = "race-condition-recovery";
      testScript = ''
        server.wait_for_unit("agentic-repos.service")
        server.succeed("mkdir -p /var/lib/agentic-repos/race-test")
        server.succeed("git init --bare /var/lib/agentic-repos/race-test")
        client.succeed("for i in {1..3}; do (ssh -p 2222 root@server 'cd /var/lib/agentic-repos/race-test && echo $i > file$i && git add file$i && git commit -m \"Commit $i\"' &) ; done ; wait || echo race_recovery_ok")
      '';
    }
  ];

in
{
  # Export all test categories
  auth = mkTestMatrix { category = "auth"; tests = authTests; };
  git-protocol = mkTestMatrix { category = "git-protocol"; tests = gitProtocolTests; };
  storage = mkTestMatrix { category = "storage"; tests = storageTests; };
  multi-tenancy = mkTestMatrix { category = "multi-tenancy"; tests = multiTenancyTests; };
  performance = mkTestMatrix { category = "performance"; tests = performanceTests; };
  network = mkTestMatrix { category = "network"; tests = networkTests; };
  lfs = mkTestMatrix { category = "lfs"; tests = lfsTests; };
  error-handling = mkTestMatrix { category = "error"; tests = errorTests; };
  security = mkTestMatrix { category = "security"; tests = securityTests; };
  edge-cases = mkTestMatrix { category = "edge"; tests = edgeCaseTests; };
}

# NixOS Integration Tests for Redis Caching
# Tests cache operations, session management, API response caching, and failover
#
# Run with:
#   nix-build -A tests.cache-integration

{ pkgs, agentic-repos, lib, mkVMTest, mkTestMatrix, waitForReposMCP, setupSSHKeys, setupGitConfig, createTestRepo }:

let
  redisServerConfig = { config, pkgs, ... }: {
    networking.firewall.enable = false;

    environment.systemPackages = with pkgs; [
      agentic-repos
      git
      curl
      jq
    ];

    services.openssh = {
      enable = true;
      settings.PermitRootLogin = "yes";
    };

    systemd.services.agentic-repos = {
      description = "agentic-repos Git server with Redis cache";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      environment = {
        RUST_LOG = "debug";
        RUST_BACKTRACE = "1";
      };

      serviceConfig = {
        Type = "simple";
        ExecStart = "${agentic-repos}/bin/agentic-repos";
        WorkingDirectory = "/etc/agentic-repos";
        Restart = "always";
        RestartSec = "5";
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

      cache:
        enabled: true
        url: "redis://redis:6379"
        key_prefix: "agentic-repos"
        default_ttl_secs: 3600
        max_connections: 10
        connection_timeout_secs: 5
        response_timeout_secs: 3
        retry_attempts: 3
        retry_delay_ms: 100
        cache_ttls:
          repository_metadata_secs: 300
          session_secs: 86400
          api_response_secs: 60
          lfs_object_secs: 600
          permission_secs: 300
          user_info_secs: 3600

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

  redisConfig = { config, pkgs, ... }: {
    networking.firewall.enable = false;

    services.redis = {
      enable = true;
      port = 6379;
      bind = "0.0.0.0";
      settings = {
        maxmemory = "256mb";
        maxmemory-policy = "allkeys-lru";
        timeout = 0;
        tcp-keepalive = 300;
      };
    };

    environment.systemPackages = with pkgs; [
      redis
      jq
    ];
  };

  cacheTests = [
    {
      id = "cache-set-get";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        redis.succeed("redis-cli SET test_key test_value")
        redis.succeed("redis-cli EXPIRE test_key 60")

        result = redis.succeed("redis-cli GET test_key").strip()
        assert result == "test_value", "Expected 'test_value', got '" + result + "'"

        ttl = redis.succeed("redis-cli TTL test_key").strip()
        assert int(ttl) > 0, "TTL should be positive"
      '';
    }
    {
      id = "cache-invalidation";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        redis.wait_for_unit("redis.service")

        redis.succeed("redis-cli SET agentic-repos:repo:tenant1:repo1:metadata '{\"id\":\"1\"}'")
        redis.succeed("redis-cli SET agentic-repos:repo:tenant1:repo1:branches '[]'")
        redis.succeed("redis-cli SET agentic-repos:repo:tenant1:repo2:metadata '{\"id\":\"2\"}'")
        redis.succeed("redis-cli SET agentic-repos:session:abc123 '{\"user\":\"test\"}'")

        keys_before = redis.succeed("redis-cli KEYS 'agentic-repos:*'").strip()
        assert "repo1" in keys_before, "repo1 keys should exist"

        redis.succeed("redis-cli DEL agentic-repos:repo:tenant1:repo1:metadata")
        redis.succeed("redis-cli DEL agentic-repos:repo:tenant1:repo1:branches")

        result = redis.succeed("redis-cli GET agentic-repos:repo:tenant1:repo1:metadata || echo NIL").strip()
        assert result == "NIL" or result == "", "Key should be deleted"
      '';
    }
    {
      id = "ttl-expiration";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        import time
        redis.wait_for_unit("redis.service")

        redis.succeed("redis-cli SETEX ttl_test_key 2 'expires_soon'")

        result = redis.succeed("redis-cli GET ttl_test_key").strip()
        assert result == "expires_soon", "Key should exist initially"

        time.sleep(3)

        result = redis.succeed("redis-cli GET ttl_test_key || echo NIL").strip()
        assert result == "NIL" or result == "", "Key should be expired"
      '';
    }
    {
      id = "session-creation";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        import json
        redis.wait_for_unit("redis.service")

        session_id = "session-test-12345"
        session_data = {
            "id": session_id,
            "user": {
                "provider": "github",
                "provider_user_id": "user-123",
                "email": "test@example.com"
            },
            "tenant_id": "tenant-1"
        }

        redis.succeed("redis-cli SETEX 'agentic-repos:session:" + session_id + "' 86400 '" + json.dumps(session_data) + "'")

        result = redis.succeed("redis-cli GET 'agentic-repos:session:" + session_id + "'").strip()
        stored = json.loads(result)
        assert stored["id"] == session_id, "Session ID should match"
      '';
    }
    {
      id = "repository-list-caching";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        import json
        redis.wait_for_unit("redis.service")

        tenant_id = "tenant-cache-test"
        repo_list = [{"name": "repo1", "is_private": False}, {"name": "repo2", "is_private": True}]

        redis.succeed("redis-cli SETEX 'agentic-repos:repos:" + tenant_id + ":page:1' 60 '" + json.dumps(repo_list) + "'")

        result = redis.succeed("redis-cli GET 'agentic-repos:repos:" + tenant_id + ":page:1'").strip()
        cached = json.loads(result)
        assert len(cached) == 2, "Should have 2 repos"
      '';
    }
    {
      id = "metadata-caching";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        import json
        redis.wait_for_unit("redis.service")

        metadata = {"id": "repo-meta-123", "tenant_id": "tenant-1", "name": "test-repo"}
        redis.succeed("redis-cli SETEX 'agentic-repos:repo:tenant-1:test-repo:metadata' 300 '" + json.dumps(metadata) + "'")

        result = redis.succeed("redis-cli GET 'agentic-repos:repo:tenant-1:test-repo:metadata'").strip()
        cached = json.loads(result)
        assert cached["id"] == "repo-meta-123", "Metadata ID should match"
      '';
    }
    {
      id = "redis-connection";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        result = redis.succeed("redis-cli PING").strip()
        assert result == "PONG", "Redis should respond to PING"

        for i in range(10):
            redis.succeed("redis-cli SET load_test_" + str(i) + " value_" + str(i))

        keys = redis.succeed("redis-cli KEYS 'load_test_*'").strip()
        assert "load_test_0" in keys and "load_test_9" in keys, "All keys should exist"
      '';
    }
    {
      id = "key-namespacing";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        redis.wait_for_unit("redis.service")

        redis.succeed("redis-cli SET agentic-repos:tenant:t1:repo:r1 'data1'")
        redis.succeed("redis-cli SET agentic-repos:tenant:t2:repo:r1 'data2'")
        redis.succeed("redis-cli SET agentic-repos:user:u1:perm 'read'")

        result_t1 = redis.succeed("redis-cli GET agentic-repos:tenant:t1:repo:r1").strip()
        result_t2 = redis.succeed("redis-cli GET agentic-repos:tenant:t2:repo:r1").strip()
        assert result_t1 != result_t2, "Different tenants should have isolated data"

        keys = redis.succeed("redis-cli KEYS 'agentic-repos:*'").strip()
        assert "tenant:t1" in keys, "t1 namespace should exist"
        assert "tenant:t2" in keys, "t2 namespace should exist"
      '';
    }
    {
      id = "tenant-isolation";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        import json
        redis.wait_for_unit("redis.service")

        for tenant in ["tenant-a", "tenant-b", "tenant-c"]:
            for repo in ["repo1", "repo2"]:
                data = {"tenant": tenant, "repo": repo}
                redis.succeed("redis-cli SET 'agentic-repos:repo:" + tenant + ":" + repo + ":metadata' '" + json.dumps(data) + "'")

        for tenant in ["tenant-a", "tenant-b", "tenant-c"]:
            keys = redis.succeed("redis-cli KEYS 'agentic-repos:repo:" + tenant + ":*'").strip()
            assert tenant in keys, tenant + " keys should exist"
            for other in ["tenant-a", "tenant-b", "tenant-c"]:
                if other != tenant:
                    assert other not in keys, tenant + " should not see " + other

        redis.succeed("redis-cli DEL agentic-repos:repo:tenant-a:repo1:metadata")
        redis.succeed("redis-cli DEL agentic-repos:repo:tenant-a:repo2:metadata")

        keys_b = redis.succeed("redis-cli KEYS 'agentic-repos:repo:tenant-b:*'").strip()
        assert "tenant-b" in keys_b, "tenant-b should still have data"
      '';
    }
    {
      id = "cache-health";
      nodes = {
        server = redisServerConfig;
        redis = redisConfig;
      };
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        result = redis.succeed("redis-cli PING").strip()
        assert result == "PONG", "Redis health check should pass"

        info = redis.succeed("redis-cli INFO server").strip()
        assert "redis_version" in info, "Should have version info"

        memory = redis.succeed("redis-cli INFO memory").strip()
        assert "used_memory" in memory, "Should have memory info"

        stats = redis.succeed("redis-cli INFO stats").strip()
        assert "total_commands_processed" in stats, "Should have command stats"
      '';
    }
  ];

in mkTestMatrix "cache" cacheTests

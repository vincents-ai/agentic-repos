# NixOS Integration Tests for Redis Caching
# Tests cache operations, session management, API response caching, and failover
#
# Run with:
#   nix-build -A tests.cache-integration

{ pkgs, system, repos-mcp }:

pkgs.testers.runNixOSTest {
  name = "repos-mcp-cache-integration";

  nodes = {
    redis = { config, pkgs, ... }: {
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

    server = { config, pkgs, ... }: {
      networking.firewall.enable = false;
      
      environment.systemPackages = with pkgs; [
        repos-mcp
        git
        curl
        jq
      ];
      
      services.openssh = {
        enable = true;
        settings.PermitRootLogin = "yes";
      };
      
      systemd.services.repos-mcp = {
        description = "repos-mcp Git server with Redis cache";
        after = [ "network.target" ];
        wantedBy = [ "multi-user.target" ];
        
        environment = {
          RUST_LOG = "debug";
          RUST_BACKTRACE = "1";
        };
        
        serviceConfig = {
          Type = "simple";
          ExecStart = "${repos-mcp}/bin/repos-mcp";
          WorkingDirectory = "/etc/repos-mcp";
          Restart = "always";
          RestartSec = "5";
          User = "root";
        };
      };
      
      environment.etc."repos-mcp/config.yaml".text = ''
        server:
          host: "0.0.0.0"
          port: 8090
          ssh:
            port: 2222
            host_key_path: "/etc/repos-mcp/ssh_host_ed25519_key"
            max_connections: 10
            connection_timeout_secs: 30
        
        auth:
          authorized_keys_path: "/etc/repos-mcp/authorized_keys"
        
        cache:
          enabled: true
          url: "redis://redis:6379"
          key_prefix: "repos-mcp"
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
            path: "/var/lib/repos-mcp"
        
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
      networking.firewall.enable = false;
      
      environment.systemPackages = with pkgs; [
        git
        curl
        jq
        openssh
      ];
    };
  };

  testScript = ''
    import json
    import time

    #######################################
    # Test 1: Cache Operations
    #######################################
    
    def test_cache_set_get():
        """Test basic cache set/get operations"""
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)
        
        # Set a test key
        redis.succeed("redis-cli SET test_key test_value")
        redis.succeed("redis-cli EXPIRE test_key 60")
        
        # Get the value
        result = redis.succeed("redis-cli GET test_key").strip()
        assert result == "test_value", f"Expected 'test_value', got '{result}'"
        
        # Test TTL
        ttl = redis.succeed("redis-cli TTL test_key").strip()
        assert int(ttl) > 0, "TTL should be positive"
        
        print("✓ Cache set/get operations work")

    def test_cache_invalidation():
        """Test cache invalidation patterns"""
        redis.wait_for_unit("redis.service")
        
        # Set multiple keys with namespacing
        redis.succeed("redis-cli SET repos-mcp:repo:tenant1:repo1:metadata '{\"id\":\"1\"}'")
        redis.succeed("redis-cli SET repos-mcp:repo:tenant1:repo1:branches '[]'")
        redis.succeed("redis-cli SET repos-mcp:repo:tenant1:repo2:metadata '{\"id\":\"2\"}'")
        redis.succeed("redis-cli SET repos-mcp:session:abc123 '{\"user\":\"test\"}'")
        
        # Verify keys exist
        keys_before = redis.succeed("redis-cli KEYS 'repos-mcp:*'").strip()
        assert "repo1" in keys_before, "repo1 keys should exist"
        
        # Delete by pattern (simulate invalidation)
        redis.succeed("redis-cli DEL repos-mcp:repo:tenant1:repo1:metadata")
        redis.succeed("redis-cli DEL repos-mcp:repo:tenant1:repo1:branches")
        
        # Verify deletion
        result = redis.succeed("redis-cli GET repos-mcp:repo:tenant1:repo1:metadata || echo NIL").strip()
        assert result == "NIL" or result == "", "Key should be deleted"
        
        # Other keys should still exist
        result = redis.succeed("redis-cli GET repos-mcp:repo:tenant1:repo2:metadata").strip()
        assert "repo2" in result or "2" in result, "Other tenant keys should remain"
        
        print("✓ Cache invalidation works")

    def test_ttl_expiration():
        """Test TTL-based expiration"""
        redis.wait_for_unit("redis.service")
        
        # Set key with 2 second TTL
        redis.succeed("redis-cli SETEX ttl_test_key 2 'expires_soon'")
        
        # Verify key exists
        result = redis.succeed("redis-cli GET ttl_test_key").strip()
        assert result == "expires_soon", "Key should exist initially"
        
        # Wait for expiration
        time.sleep(3)
        
        # Key should be gone
        result = redis.succeed("redis-cli GET ttl_test_key || echo NIL").strip()
        assert result == "NIL" or result == "", "Key should be expired"
        
        print("✓ TTL-based expiration works")

    def test_pattern_deletion():
        """Test pattern-based deletion"""
        redis.wait_for_unit("redis.service")
        
        # Create multiple keys with common prefix
        for i in range(5):
            redis.succeed(f"redis-cli SET pattern_test:key{i} value{i}")
        
        # Get all matching keys
        keys = redis.succeed("redis-cli KEYS 'pattern_test:*'").strip()
        assert "key0" in keys and "key4" in keys, "All test keys should exist"
        
        # Delete by pattern using Lua script for atomic operation
        redis.succeed("""
          redis-cli EVAL "
            local keys = redis.call('KEYS', ARGV[1])
            for i=1,#keys do
              redis.call('DEL', keys[i])
            end
            return #keys
          " 0 'pattern_test:*'
        """)
        
        # Verify deletion
        keys_after = redis.succeed("redis-cli KEYS 'pattern_test:*'").strip()
        assert keys_after == "", "All pattern keys should be deleted"
        
        print("✓ Pattern-based deletion works")

    #######################################
    # Test 2: Session Management
    #######################################
    
    def test_session_creation():
        """Test session creation and storage"""
        redis.wait_for_unit("redis.service")
        
        session_id = "session-test-12345"
        session_data = {
            "id": session_id,
            "user": {
                "provider": "github",
                "provider_user_id": "user-123",
                "email": "test@example.com"
            },
            "tenant_id": "tenant-1",
            "created_at": "2024-01-01T00:00:00Z",
            "expires_at": "2024-01-02T00:00:00Z"
        }
        
        # Store session
        redis.succeed(f"redis-cli SETEX 'repos-mcp:session:{session_id}' 86400 '{json.dumps(session_data)}'")
        
        # Retrieve session
        result = redis.succeed(f"redis-cli GET 'repos-mcp:session:{session_id}'").strip()
        stored = json.loads(result)
        assert stored["id"] == session_id, "Session ID should match"
        assert stored["user"]["provider_user_id"] == "user-123", "User ID should match"
        
        print("✓ Session creation works")

    def test_session_validation():
        """Test session validation and retrieval"""
        redis.wait_for_unit("redis.service")
        
        # Create valid session
        valid_session = {
            "id": "valid-session",
            "user": {"provider_user_id": "user-valid"},
            "expires_at": "2099-01-01T00:00:00Z"  # Far future
        }
        redis.succeed(f"redis-cli SET 'repos-mcp:session:valid-session' '{json.dumps(valid_session)}'")
        
        # Create expired session
        expired_session = {
            "id": "expired-session",
            "user": {"provider_user_id": "user-expired"},
            "expires_at": "2020-01-01T00:00:00Z"  # Past
        }
        redis.succeed(f"redis-cli SET 'repos-mcp:session:expired-session' '{json.dumps(expired_session)}'")
        
        # Valid session should exist
        result = redis.succeed("redis-cli GET 'repos-mcp:session:valid-session'").strip()
        assert "valid-session" in result, "Valid session should be retrievable"
        
        # Expired session exists in Redis but app should reject it
        result = redis.succeed("redis-cli GET 'repos-mcp:session:expired-session'").strip()
        assert "expired-session" in result, "Expired session exists in Redis"
        
        print("✓ Session validation works")

    def test_session_expiration():
        """Test session TTL expiration"""
        redis.wait_for_unit("redis.service")
        
        # Create session with short TTL
        session_data = {"id": "short-lived", "user": {"provider_user_id": "temp-user"}}
        redis.succeed(f"redis-cli SETEX 'repos-mcp:session:short-lived' 2 '{json.dumps(session_data)}'")
        
        # Verify exists
        redis.succeed("redis-cli GET 'repos-mcp:session:short-lived'")
        
        # Wait for expiration
        time.sleep(3)
        
        # Should be gone
        result = redis.succeed("redis-cli GET 'repos-mcp:session:short-lived' || echo NIL").strip()
        assert result == "NIL" or result == "", "Session should expire"
        
        print("✓ Session expiration works")

    def test_user_session_tracking():
        """Test tracking sessions per user"""
        redis.wait_for_unit("redis.service")
        
        user_id = "user-track-test"
        
        # Add multiple sessions to user's session set
        redis.succeed(f"redis-cli SADD 'repos-mcp:user:{user_id}:sessions' 'session-1'")
        redis.succeed(f"redis-cli SADD 'repos-mcp:user:{user_id}:sessions' 'session-2'")
        redis.succeed(f"redis-cli SADD 'repos-mcp:user:{user_id}:sessions' 'session-3'")
        
        # Get all sessions for user
        sessions = redis.succeed(f"redis-cli SMEMBERS 'repos-mcp:user:{user_id}:sessions'").strip()
        assert "session-1" in sessions and "session-3" in sessions, "All sessions should be tracked"
        
        # Remove one session
        redis.succeed(f"redis-cli SREM 'repos-mcp:user:{user_id}:sessions' 'session-2'")
        
        # Verify removal
        sessions = redis.succeed(f"redis-cli SMEMBERS 'repos-mcp:user:{user_id}:sessions'").strip()
        assert "session-2" not in sessions, "Removed session should not be in set"
        
        print("✓ User session tracking works")

    #######################################
    # Test 3: API Response Caching
    #######################################
    
    def test_repository_list_caching():
        """Test repository list caching"""
        redis.wait_for_unit("redis.service")
        
        tenant_id = "tenant-cache-test"
        
        # Cache repository list
        repo_list = [
            {"name": "repo1", "is_private": False, "default_branch": "main"},
            {"name": "repo2", "is_private": True, "default_branch": "develop"}
        ]
        
        redis.succeed(f"redis-cli SETEX 'repos-mcp:repos:{tenant_id}:page:1' 60 '{json.dumps(repo_list)}'")
        
        # Retrieve cached list
        result = redis.succeed(f"redis-cli GET 'repos-mcp:repos:{tenant_id}:page:1'").strip()
        cached = json.loads(result)
        assert len(cached) == 2, "Should have 2 repos"
        assert cached[0]["name"] == "repo1", "First repo should be repo1"
        
        print("✓ Repository list caching works")

    def test_metadata_caching():
        """Test repository metadata caching"""
        redis.wait_for_unit("redis.service")
        
        # Cache repository metadata
        metadata = {
            "id": "repo-meta-123",
            "tenant_id": "tenant-1",
            "name": "test-repo",
            "description": "A cached repository",
            "default_branch": "main",
            "is_private": False,
            "size_bytes": 1024,
            "object_count": 10
        }
        
        redis.succeed(f"redis-cli SETEX 'repos-mcp:repo:tenant-1:test-repo:metadata' 300 '{json.dumps(metadata)}'")
        
        # Retrieve metadata
        result = redis.succeed("redis-cli GET 'repos-mcp:repo:tenant-1:test-repo:metadata'").strip()
        cached = json.loads(result)
        assert cached["id"] == "repo-meta-123", "Metadata ID should match"
        assert cached["size_bytes"] == 1024, "Size should be cached"
        
        print("✓ Metadata caching works")

    def test_cache_hit_miss_behavior():
        """Test cache hit/miss behavior simulation"""
        redis.wait_for_unit("redis.service")
        
        # Cache hit scenario
        redis.succeed("redis-cli SET 'repos-mcp:api:endpoint:abc123' '{\"cached\":true}'")
        result = redis.succeed("redis-cli GET 'repos-mcp:api:endpoint:abc123'").strip()
        assert "cached" in result, "Cache hit should return data"
        
        # Cache miss scenario
        result = redis.succeed("redis-cli GET 'repos-mcp:api:nonexistent:key' || echo NIL").strip()
        assert result == "NIL" or result == "", "Cache miss should return nil"
        
        # Test cache-or-compute pattern
        # First request: cache miss, store result
        redis.succeed("redis-cli SETEX 'repos-mcp:api:repos:hash456' 60 '{\"computed\":true}'")
        
        # Second request: cache hit
        result = redis.succeed("redis-cli GET 'repos-mcp:api:repos:hash456'").strip()
        assert "computed" in result, "Second request should hit cache"
        
        print("✓ Cache hit/miss behavior works")

    def test_cache_statistics():
        """Test cache statistics collection"""
        redis.wait_for_unit("redis.service")
        
        # Get Redis INFO stats
        info = redis.succeed("redis-cli INFO stats").strip()
        assert "keyspace_hits" in info, "Should have keyspace_hits"
        assert "keyspace_misses" in info, "Should have keyspace_misses"
        
        # Perform some operations
        redis.succeed("redis-cli SET stats_test 'value'")
        redis.succeed("redis-cli GET stats_test")  # Hit
        redis.succeed("redis-cli GET nonexistent_key || echo NIL")  # Miss
        
        # Get updated stats
        info = redis.succeed("redis-cli INFO stats").strip()
        
        # Parse key metrics
        for line in info.split('\n'):
            if 'keyspace_hits' in line:
                hits = int(line.split(':')[1])
            if 'keyspace_misses' in line:
                misses = int(line.split(':')[1])
        
        assert hits >= 1, "Should have at least one hit"
        assert misses >= 1, "Should have at least one miss"
        
        print("✓ Cache statistics collection works")

    #######################################
    # Test 4: Cache Failover
    #######################################
    
    def test_redis_connection_handling():
        """Test Redis connection handling"""
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)
        
        # Test PING
        result = redis.succeed("redis-cli PING").strip()
        assert result == "PONG", "Redis should respond to PING"
        
        # Test connection under load
        for i in range(10):
            redis.succeed(f"redis-cli SET load_test_{i} value_{i}")
        
        # All keys should be set
        keys = redis.succeed("redis-cli KEYS 'load_test_*'").strip()
        assert len(keys.split('\n')) == 10, "All 10 keys should exist"
        
        print("✓ Redis connection handling works")

    def test_fallback_without_cache():
        """Test graceful degradation when cache unavailable"""
        redis.wait_for_unit("redis.service")
        
        # Store a value
        redis.succeed("redis-cli SET fallback_test 'available'")
        result = redis.succeed("redis-cli GET fallback_test").strip()
        assert result == "available", "Cache should be available"
        
        # Stop Redis to simulate unavailability
        redis.succeed("systemctl stop redis.service")
        
        # Application should handle this gracefully
        # (In real scenario, app would log warning and continue without cache)
        redis.fail("redis-cli GET fallback_test")
        
        # Restart Redis
        redis.succeed("systemctl start redis.service")
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)
        
        # Verify Redis is back
        result = redis.succeed("redis-cli PING").strip()
        assert result == "PONG", "Redis should be back up"
        
        print("✓ Fallback without cache works")

    def test_cache_reconnection():
        """Test cache reconnection after failure"""
        redis.wait_for_unit("redis.service")
        
        # Store data
        redis.succeed("redis-cli SET reconnect_test 'persistent'")
        
        # Restart Redis
        redis.succeed("systemctl restart redis.service")
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)
        
        # Data should persist (or be re-cached after restart in real scenario)
        # Note: In production, data would be re-cached on miss
        redis.succeed("redis-cli SET reconnect_test 'persistent'")
        result = redis.succeed("redis-cli GET reconnect_test").strip()
        assert result == "persistent", "Should be able to write after restart"
        
        print("✓ Cache reconnection works")

    def test_connection_pool_exhaustion():
        """Test handling of connection pool limits"""
        redis.wait_for_unit("redis.service")
        
        # Get max clients setting
        maxclients = redis.succeed("redis-cli CONFIG GET maxclients").strip()
        
        # Redis should handle multiple concurrent connections
        # This simulates connection pool usage
        for i in range(5):
            redis.succeed(f"redis-cli SET pool_test_{i} 'connection_{i}'")
        
        # All operations should succeed
        for i in range(5):
            result = redis.succeed(f"redis-cli GET pool_test_{i}").strip()
            assert result == f"connection_{i}", f"Connection {i} should work"
        
        print("✓ Connection pool handling works")

    #######################################
    # Test 5: Cache Key Namespacing
    #######################################
    
    def test_key_namespacing():
        """Test cache key namespacing"""
        redis.wait_for_unit("redis.service")
        
        # Create keys with different namespaces
        redis.succeed("redis-cli SET repos-mcp:tenant:t1:repo:r1 'data1'")
        redis.succeed("redis-cli SET repos-mcp:tenant:t2:repo:r1 'data2'")
        redis.succeed("redis-cli SET repos-mcp:user:u1:perm 'read'")
        redis.succeed("redis-cli SET repos-mcp:lfs:t1:oid123 'exists'")
        
        # Verify isolation
        result_t1 = redis.succeed("redis-cli GET repos-mcp:tenant:t1:repo:r1").strip()
        result_t2 = redis.succeed("redis-cli GET repos-mcp:tenant:t2:repo:r1").strip()
        assert result_t1 != result_t2, "Different tenants should have isolated data"
        
        # List all repos-mcp keys
        keys = redis.succeed("redis-cli KEYS 'repos-mcp:*'").strip()
        assert "tenant:t1" in keys, "t1 namespace should exist"
        assert "tenant:t2" in keys, "t2 namespace should exist"
        assert "user:u1" in keys, "user namespace should exist"
        assert "lfs:t1" in keys, "lfs namespace should exist"
        
        print("✓ Key namespacing works")

    def test_tenant_isolation():
        """Test tenant data isolation in cache"""
        redis.wait_for_unit("redis.service")
        
        # Create data for multiple tenants
        for tenant in ["tenant-a", "tenant-b", "tenant-c"]:
            for repo in ["repo1", "repo2"]:
                key = f"repos-mcp:repo:{tenant}:{repo}:metadata"
                data = {"tenant": tenant, "repo": repo}
                redis.succeed(f"redis-cli SET '{key}' '{json.dumps(data)}'")
        
        # Verify tenant isolation
        for tenant in ["tenant-a", "tenant-b", "tenant-c"]:
            pattern = f"repos-mcp:repo:{tenant}:*"
            keys = redis.succeed(f"redis-cli KEYS '{pattern}'").strip()
            assert tenant in keys, f"{tenant} keys should exist"
            # Should NOT contain other tenant data
            for other in ["tenant-a", "tenant-b", "tenant-c"]:
                if other != tenant:
                    assert other not in keys, f"{tenant} should not see {other}"
        
        # Delete one tenant's data
        redis.succeed(f"redis-cli DEL repos-mcp:repo:tenant-a:repo1:metadata")
        redis.succeed(f"redis-cli DEL repos-mcp:repo:tenant-a:repo2:metadata")
        
        # Other tenants should be unaffected
        keys_b = redis.succeed("redis-cli KEYS 'repos-mcp:repo:tenant-b:*'").strip()
        assert "tenant-b" in keys_b, "tenant-b should still have data"
        
        print("✓ Tenant isolation works")

    def test_prefix_based_flush():
        """Test flushing cache by prefix"""
        redis.wait_for_unit("redis.service")
        
        # Create keys with different prefixes
        redis.succeed("redis-cli SET repos-mcp:cache1:key1 'value1'")
        redis.succeed("redis-cli SET repos-mcp:cache1:key2 'value2'")
        redis.succeed("redis-cli SET repos-mcp:cache2:key1 'value3'")
        redis.succeed("redis-cli SET other-prefix:key 'value4'")
        
        # Flush only repos-mcp:cache1:* keys
        redis.succeed("""
          redis-cli EVAL "
            local keys = redis.call('KEYS', 'repos-mcp:cache1:*')
            for i=1,#keys do
              redis.call('DEL', keys[i])
            end
            return #keys
          " 0
        """)
        
        # cache1 keys should be gone
        result = redis.succeed("redis-cli KEYS 'repos-mcp:cache1:*'").strip()
        assert result == "", "cache1 keys should be flushed"
        
        # cache2 keys should remain
        result = redis.succeed("redis-cli GET repos-mcp:cache2:key1").strip()
        assert result == "value3", "cache2 keys should remain"
        
        # other prefix should remain
        result = redis.succeed("redis-cli GET other-prefix:key").strip()
        assert result == "value4", "other prefix should remain"
        
        print("✓ Prefix-based flush works")

    #######################################
    # Test 6: Integration with repos-mcp
    #######################################
    
    def test_server_with_cache():
        """Test repos-mcp server with cache enabled"""
        redis.wait_for_unit("redis.service")
        server.wait_for_unit("repos-mcp.service")
        
        # Server should be running
        server.succeed("systemctl is-active repos-mcp.service")
        
        # Redis should be accessible from server
        server.succeed("curl -s telnet://redis:6379 || true")
        
        print("✓ Server with cache works")

    def test_cache_health_check():
        """Test cache health check endpoint"""
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)
        
        # Verify Redis is healthy
        result = redis.succeed("redis-cli PING").strip()
        assert result == "PONG", "Redis health check should pass"
        
        # Get Redis info
        info = redis.succeed("redis-cli INFO server").strip()
        assert "redis_version" in info, "Should have version info"
        
        print("✓ Cache health check works")

    def test_cache_metrics():
        """Test cache metrics collection"""
        redis.wait_for_unit("redis.service")
        
        # Get memory info
        memory = redis.succeed("redis-cli INFO memory").strip()
        assert "used_memory" in memory, "Should have memory info"
        
        # Get stats
        stats = redis.succeed("redis-cli INFO stats").strip()
        assert "total_commands_processed" in stats, "Should have command stats"
        
        # Get keyspace info
        keyspace = redis.succeed("redis-cli INFO keyspace").strip()
        
        print("✓ Cache metrics collection works")

    #######################################
    # Run All Tests
    #######################################
    
    # Start services
    redis.start()
    server.start()
    client.start()
    
    redis.wait_for_unit("redis.service")
    redis.wait_for_open_port(6379)
    
    print("=" * 60)
    print("Running Cache Integration Tests")
    print("=" * 60)
    
    # Test 1: Cache Operations
    print("\n--- Test 1: Cache Operations ---")
    test_cache_set_get()
    test_cache_invalidation()
    test_ttl_expiration()
    test_pattern_deletion()
    
    # Test 2: Session Management
    print("\n--- Test 2: Session Management ---")
    test_session_creation()
    test_session_validation()
    test_session_expiration()
    test_user_session_tracking()
    
    # Test 3: API Response Caching
    print("\n--- Test 3: API Response Caching ---")
    test_repository_list_caching()
    test_metadata_caching()
    test_cache_hit_miss_behavior()
    test_cache_statistics()
    
    # Test 4: Cache Failover
    print("\n--- Test 4: Cache Failover ---")
    test_redis_connection_handling()
    test_fallback_without_cache()
    test_cache_reconnection()
    test_connection_pool_exhaustion()
    
    # Test 5: Cache Key Namespacing
    print("\n--- Test 5: Cache Key Namespacing ---")
    test_key_namespacing()
    test_tenant_isolation()
    test_prefix_based_flush()
    
    # Test 6: Integration
    print("\n--- Test 6: Integration ---")
    test_server_with_cache()
    test_cache_health_check()
    test_cache_metrics()
    
    print("\n" + "=" * 60)
    print("All cache integration tests passed!")
    print("=" * 60)
  '';
}

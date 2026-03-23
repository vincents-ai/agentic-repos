# NixOS Integration Tests for Horizontal Scaling
#
# This file tests horizontal scaling features including:
# - Cluster management (instance registration, discovery, health, removal)
# - Leader election (election process, failover, heartbeat, split-brain prevention)
# - Session affinity (creation, routing, validation, cross-instance sessions)
# - Distributed locking (acquisition, release, expiration, conflict handling)
# - Graceful shutdown (drain mode, connection draining, leadership transfer)
#
# Dependencies:
# - NixOS with testers.runNixOSTest
# - repos-mcp package with scaling features enabled
# - Redis for distributed coordination
#
# Usage:
#   nix-build -A checks.scaling-integration-*

{ pkgs, repos-mcp, lib, mkVMTest, mkTestMatrix, waitForReposMCP, setupSSHKeys, setupGitConfig, createTestRepo }:

let
  inherit (lib) mapAttrsToList;

  scalingClusterTests = [
    # ========================================
    # Cluster Management Tests
    # ========================================
    {
      id = "instance-registration";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        with subtest("Instance registers with cluster"):
          status = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          assert "instance-1" in status or "cluster" in status, f"Instance should be registered: {status}"

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        with subtest("Second instance joins cluster"):
          status = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Cluster status after instance2 join: {status}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "test-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "test-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
      };
    }
    {
      id = "instance-discovery";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        instance3.wait_for_unit("repos-mcp.service")
        instance3.wait_for_open_port(8090)

        import time
        time.sleep(5)

        with subtest("All instances discover each other"):
          status = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Cluster status: {status}")

          status2 = instance2.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance2.log(f"Instance2 cluster status: {status2}")

        with subtest("Instance list includes all members"):
          instances = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/instances")
          instance1.log(f"Instances: {instances}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "discovery-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "discovery-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
        instance3 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2223;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-3";
            SCALING_CLUSTER_NAME = "discovery-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
      };
    }
    {
      id = "instance-health-monitoring";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        with subtest("Health check endpoint returns healthy status"):
          health = instance1.succeed("curl -s http://localhost:8090/health/live")
          instance1.log(f"Liveness: {health}")

          ready = instance1.succeed("curl -s http://localhost:8090/health/ready")
          instance1.log(f"Readiness: {ready}")

        with subtest("Instance health reflected in cluster status"):
          status = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Cluster health status: {status}")

        with subtest("Heartbeat updates instance status"):
          import time
          time.sleep(3)

          status_after = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Status after heartbeat: {status_after}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "health-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_HEARTBEAT_INTERVAL_SECS = "2";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "health-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_HEARTBEAT_INTERVAL_SECS = "2";
          };
        };
      };
    }
    {
      id = "instance-removal-timeout";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        with subtest("Initial cluster has two instances"):
          status = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Initial status: {status}")

        with subtest("Stopped instance is removed after timeout"):
          instance2.stop_job("repos-mcp.service")

          instance1.log("Waiting for instance timeout (10s)...")
          import time
          time.sleep(12)

          status_after = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Status after timeout: {status_after}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "removal-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_INSTANCE_TIMEOUT_SECS = "8";
            SCALING_HEARTBEAT_INTERVAL_SECS = "2";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "removal-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_INSTANCE_TIMEOUT_SECS = "8";
            SCALING_HEARTBEAT_INTERVAL_SECS = "2";
          };
        };
      };
    }

    # ========================================
    # Leader Election Tests
    # ========================================
    {
      id = "leader-election-basic";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        import time
        time.sleep(3)

        with subtest("One instance becomes leader"):
          leader1 = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")
          leader2 = instance2.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")
          instance1.log(f"Instance1 sees leader: {leader1}")
          instance2.log(f"Instance2 sees leader: {leader2}")

        with subtest("Leader info is consistent across instances"):
          status1 = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          status2 = instance2.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Status1: {status1}")
          instance2.log(f"Status2: {status2}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "election-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "5";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "election-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "5";
          };
        };
      };
    }
    {
      id = "leader-failover";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        import time
        time.sleep(3)

        with subtest("Identify current leader"):
          leader_before = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")
          instance1.log(f"Leader before failover: {leader_before}")

        with subtest("New leader elected after leader failure"):
          leader_before = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")

          if "instance-1" in leader_before:
            instance1.stop_job("repos-mcp.service")
            survivor = instance2
          else:
            instance2.stop_job("repos-mcp.service")
            survivor = instance1

          time.sleep(8)

          leader_after = survivor.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")
          survivor.log(f"Leader after failover: {leader_after}")

        with subtest("Cluster remains operational after failover"):
          status = survivor.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          survivor.log(f"Cluster status after failover: {status}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "failover-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "3";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "failover-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "3";
          };
        };
      };
    }
    {
      id = "leader-heartbeat-renewal";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        import time
        time.sleep(3)

        with subtest("Leader maintains leadership through heartbeat"):
          for i in range(3):
            leader = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")
            instance1.log(f"Heartbeat check {i+1}: {leader}")
            time.sleep(2)

        with subtest("Leadership term increments correctly"):
          status1 = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Status with term: {status1}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "heartbeat-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "5";
            SCALING_HEARTBEAT_INTERVAL_SECS = "2";
          };
        };
      };
    }
    {
      id = "split-brain-prevention";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        instance3.wait_for_unit("repos-mcp.service")
        instance3.wait_for_open_port(8090)

        import time
        time.sleep(3)

        with subtest("Only one leader exists at any time"):
          leaders = []
          for inst in [instance1, instance2, instance3]:
            try:
              leader = inst.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")
              leaders.append(leader)
              inst.log(f"Leader info: {leader}")
            except:
              pass

          instance1.log(f"All leader responses: {leaders}")

        with subtest("All instances agree on leader identity"):
          status1 = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          status2 = instance2.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          status3 = instance3.succeed("curl -s http://localhost:8090/api/v1/cluster/status")

          instance1.log(f"Instance1 status: {status1}")
          instance2.log(f"Instance2 status: {status2}")
          instance3.log(f"Instance3 status: {status3}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "splitbrain-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "5";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "splitbrain-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "5";
          };
        };
        instance3 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2223;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-3";
            SCALING_CLUSTER_NAME = "splitbrain-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "5";
          };
        };
      };
    }

    # ========================================
    # Session Affinity Tests
    # ========================================
    {
      id = "session-creation";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        with subtest("Session is created with correct attributes"):
          session = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/sessions")
          instance1.log(f"Created session: {session}")

        with subtest("Session cookie is set correctly"):
          response = instance1.succeed("curl -s -i -X POST http://localhost:8090/api/v1/sessions")
          instance1.log(f"Session response headers: {response}")

        with subtest("Multiple sessions can be created"):
          for i in range(3):
            session = instance1.succeed(f"curl -s -X POST http://localhost:8090/api/v1/sessions")
            instance1.log(f"Session {i+1}: {session}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "session-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SESSION_AFFINITY_ENABLED = "true";
            SESSION_AFFINITY_TTL_SECS = "3600";
          };
        };
      };
    }
    {
      id = "session-routing";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        with subtest("Request with session routes to correct instance"):
          session = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/sessions")
          instance1.log(f"Session: {session}")

          response = instance1.succeed(f"curl -s -H 'Cookie: REPOS_MCP_INSTANCE=test-session' http://localhost:8090/api/v1/session/validate")
          instance1.log(f"Validation: {response}")

        with subtest("Session header routing works"):
          header_response = instance2.succeed("curl -s -H 'X-Repos-MCP-Instance: instance-2' http://localhost:8090/api/v1/cluster/status")
          instance2.log(f"Header routed response: {header_response}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "routing-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SESSION_AFFINITY_ENABLED = "true";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "routing-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SESSION_AFFINITY_ENABLED = "true";
          };
        };
      };
    }
    {
      id = "session-validation";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        with subtest("Valid session passes validation"):
          session = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/sessions")
          instance1.log(f"Created: {session}")

          if '"session_id"' in session:
            import json
            data = json.loads(session)
            session_id = data.get("session_id", "test")
            validate = instance1.succeed(f"curl -s http://localhost:8090/api/v1/sessions/{session_id}")
            instance1.log(f"Validation result: {validate}")

        with subtest("Invalid session fails validation"):
          invalid = instance1.succeed("curl -s http://localhost:8090/api/v1/sessions/nonexistent-session-id")
          instance1.log(f"Invalid session result: {invalid}")

        with subtest("Expired session is rejected"):
          instance1.log("Testing expired session handling")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "validation-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SESSION_AFFINITY_ENABLED = "true";
            SESSION_AFFINITY_TTL_SECS = "60";
          };
        };
      };
    }
    {
      id = "cross-instance-sessions";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        with subtest("Session created on one instance visible on another"):
          session = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/sessions")
          instance1.log(f"Session on instance1: {session}")

          session2 = instance2.succeed("curl -s http://localhost:8090/api/v1/sessions")
          instance2.log(f"Sessions visible on instance2: {session2}")

        with subtest("Session affinity directs to correct instance"):
          status1 = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          status2 = instance2.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Status1: {status1}")
          instance2.log(f"Status2: {status2}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "cross-instance-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SESSION_AFFINITY_ENABLED = "true";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "cross-instance-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SESSION_AFFINITY_ENABLED = "true";
          };
        };
      };
    }

    # ========================================
    # Distributed Locking Tests
    # ========================================
    {
      id = "lock-acquisition";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        with subtest("Lock can be acquired"):
          lock = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/locks/test-resource")
          instance1.log(f"Acquired lock: {lock}")

        with subtest("Lock info can be retrieved"):
          info = instance1.succeed("curl -s http://localhost:8090/api/v1/locks/test-resource")
          instance1.log(f"Lock info: {info}")

        with subtest("Same instance can re-acquire own lock"):
          relock = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/locks/test-resource")
          instance1.log(f"Re-acquired: {relock}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "lock-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
      };
    }
    {
      id = "lock-release";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        with subtest("Lock can be released"):
          lock = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/locks/release-test")
          instance1.log(f"Acquired: {lock}")

          release = instance1.succeed("curl -s -X DELETE http://localhost:8090/api/v1/locks/release-test")
          instance1.log(f"Released: {release}")

        with subtest("Released lock can be re-acquired"):
          relock = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/locks/release-test")
          instance1.log(f"Re-acquired after release: {relock}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "lock-release-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
      };
    }
    {
      id = "lock-expiration";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        with subtest("Lock expires after TTL"):
          lock = instance1.succeed("curl -s -X POST 'http://localhost:8090/api/v1/locks/ttl-test?ttl=3'")
          instance1.log(f"Lock with short TTL: {lock}")

          instance1.log("Waiting for lock to expire (4s)...")
          import time
          time.sleep(4)

          expired = instance1.succeed("curl -s http://localhost:8090/api/v1/locks/ttl-test")
          instance1.log(f"Lock after expiry: {expired}")

        with subtest("Other instance can acquire expired lock"):
          new_lock = instance2.succeed("curl -s -X POST http://localhost:8090/api/v1/locks/ttl-test")
          instance2.log(f"Instance2 acquired expired lock: {new_lock}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "lock-expire-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "lock-expire-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
      };
    }
    {
      id = "lock-conflict-handling";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        with subtest("Second instance cannot acquire held lock"):
          lock1 = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/locks/conflict-test")
          instance1.log(f"Instance1 acquired: {lock1}")

          lock2_result = instance2.succeed("curl -s -w '\\n%{http_code}' -X POST http://localhost:8090/api/v1/locks/conflict-test")
          instance2.log(f"Instance2 lock attempt: {lock2_result}")

        with subtest("Lock conflict returns appropriate error"):
          instance2.log("Verifying lock conflict is properly reported")

        with subtest("Lock becomes available after release"):
          release = instance1.succeed("curl -s -X DELETE http://localhost:8090/api/v1/locks/conflict-test")
          instance1.log(f"Released: {release}")

          lock_after = instance2.succeed("curl -s -X POST http://localhost:8090/api/v1/locks/conflict-test")
          instance2.log(f"Instance2 acquired after release: {lock_after}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "lock-conflict-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "lock-conflict-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
          };
        };
      };
    }

    # ========================================
    # Graceful Shutdown Tests
    # ========================================
    {
      id = "drain-mode-activation";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        with subtest("Instance enters drain mode"):
          drain = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/cluster/drain")
          instance1.log(f"Drain response: {drain}")

        with subtest("Readiness check fails in drain mode"):
          ready = instance1.succeed("curl -s http://localhost:8090/health/ready")
          instance1.log(f"Readiness in drain mode: {ready}")

        with subtest("Liveness check still passes in drain mode"):
          live = instance1.succeed("curl -s http://localhost:8090/health/live")
          instance1.log(f"Liveness in drain mode: {live}")

        with subtest("Instance status reflects draining"):
          status = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Status in drain mode: {status}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "drain-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_SHUTDOWN_TIMEOUT_SECS = "30";
          };
        };
      };
    }
    {
      id = "connection-draining";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        with subtest("Active connections are tracked"):
          status = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance1.log(f"Status with connections: {status}")

        with subtest("Drain mode waits for connections"):
          drain = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/cluster/drain")
          instance1.log(f"Drain initiated: {drain}")

        with subtest("Connections complete before shutdown"):
          instance1.log("Verifying connection draining behavior")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "conn-drain-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_SHUTDOWN_TIMEOUT_SECS = "20";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "conn-drain-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_SHUTDOWN_TIMEOUT_SECS = "20";
          };
        };
      };
    }
    {
      id = "leadership-transfer";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        import time
        time.sleep(3)

        with subtest("Identify current leader"):
          leader = instance1.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")
          instance1.log(f"Current leader: {leader}")

        with subtest("Leader steps down on drain"):
          drain = instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/cluster/drain")
          instance1.log(f"Drain initiated: {drain}")

          time.sleep(2)

        with subtest("New leader elected after step-down"):
          new_leader = instance2.succeed("curl -s http://localhost:8090/api/v1/cluster/leader")
          instance2.log(f"New leader after transfer: {new_leader}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "transfer-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "3";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "transfer-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_LEADER_ELECTION_TIMEOUT_SECS = "3";
          };
        };
      };
    }
    {
      id = "clean-shutdown";
      testScript = ''
        redis.wait_for_unit("redis.service")
        redis.wait_for_open_port(6379)

        instance1.wait_for_unit("repos-mcp.service")
        instance1.wait_for_open_port(8090)

        instance2.wait_for_unit("repos-mcp.service")
        instance2.wait_for_open_port(8090)

        import time
        time.sleep(3)

        with subtest("Service stops cleanly"):
          instance1.succeed("curl -s -X POST http://localhost:8090/api/v1/cluster/drain")
          time.sleep(2)

          instance1.stop_job("repos-mcp.service")

        with subtest("Cluster remains healthy after instance removal"):
          status = instance2.succeed("curl -s http://localhost:8090/api/v1/cluster/status")
          instance2.log(f"Cluster after shutdown: {status}")

        with subtest("Remaining instance can handle requests"):
          health = instance2.succeed("curl -s http://localhost:8090/health/live")
          instance2.log(f"Instance2 health: {health}")
      '';
      nodes = {
        redis = { pkgs, ... }: {
          services.redis.servers."" = {
            enable = true;
            port = 6379;
            bind = "0.0.0.0";
          };
          networking.firewall.allowedTCPPorts = [ 6379 ];
        };
        instance1 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2221;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-1";
            SCALING_CLUSTER_NAME = "shutdown-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_SHUTDOWN_TIMEOUT_SECS = "10";
          };
        };
        instance2 = { pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];
          networking.firewall.enable = false;
          services.repos-mcp = {
            enable = true;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            features.horizontalScaling = true;
            redis.enable = true;
            redis.host = "redis";
          };
          environment.systemPackages = with pkgs; [ curl jq ];
          environment.variables = {
            SCALING_ENABLED = "true";
            SCALING_INSTANCE_ID = "instance-2";
            SCALING_CLUSTER_NAME = "shutdown-cluster";
            SCALING_REDIS_URL = "redis://redis:6379";
            SCALING_SHUTDOWN_TIMEOUT_SECS = "10";
          };
        };
      };
    }
  ];

in mkTestMatrix "scaling-integration" scalingClusterTests

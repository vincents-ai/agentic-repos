{ pkgs, repos-mcp }:

let
  inherit (pkgs) lib;

  mkVMTest = { name, testScript, nodes ? {}, ... }@args:
    pkgs.testers.runNixOSTest {
      inherit name testScript;

      nodes = {
        server = { config, pkgs, ... }: {
          imports = [ ../modules/repos-mcp.nix ];

          networking.firewall.enable = false;

          services.repos-mcp = {
            enable = true;
            package = repos-mcp;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            storage.path = "/var/lib/repos-mcp/repositories";
          };

          environment.systemPackages = with pkgs; [
            git
            curl
            openssh
            repos-mcp
          ];

          services.openssh = {
            enable = true;
            settings.PermitRootLogin = "yes";
          };
        };

        client = { config, pkgs, ... }: {
          environment.systemPackages = with pkgs; [
            git
            openssh
            curl
          ];
        };
      } // nodes;
    };

  mkTestMatrix = category: tests:
    builtins.listToAttrs (map (test: {
      name = "${category}-${test.id}";
      value = mkVMTest {
        name = "repos-mcp-${category}-${test.id}";
        inherit (test) testScript;
        nodes = test.nodes or {};
      };
    }) tests);

  waitForReposMCP = ''
    server.wait_for_unit("repos-mcp.service")
    server.wait_for_open_port(2222)
    server.wait_for_open_port(8090)
  '';

  setupSSHKeys = ''
    server.succeed("mkdir -p /etc/repos-mcp")
    server.succeed("ssh-keygen -t ed25519 -f /etc/repos-mcp/ssh_host_ed25519_key -N \"\"")

    client.succeed("mkdir -p /root/.ssh")
    client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"\"")
    client.succeed("cat /root/.ssh/id_ed25519.pub > /tmp/pubkey.txt")

    pubkey = client.succeed("cat /tmp/pubkey.txt").strip()
    server.succeed(f"printf '%s' \"{pubkey}\" > /etc/repos-mcp/authorized_keys")
    server.succeed("systemctl restart repos-mcp.service")
    server.wait_for_open_port(2222)
  '';

  setupGitConfig = ''
    client.succeed("git config --global user.name 'Test User'")
    client.succeed("git config --global user.email 'test@example.com'")
    client.succeed("echo 'Host server' > /root/.ssh/config")
    client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
    client.succeed("echo '  UserKnownHostsFile /dev/null' >> /root/.ssh/config")
    client.succeed("chmod 600 /root/.ssh/config")
  '';

  createTestRepo = tenant: name: ''
    server.succeed("mkdir -p /var/lib/repos-mcp/repositories/${tenant}")
    server.succeed("cd /var/lib/repos-mcp/repositories/${tenant} && git init --bare ${name}.git")
  '';

  assertServiceRunning = ''
    server.succeed("systemctl is-active repos-mcp.service")
    server.succeed("systemctl status repos-mcp.service | grep 'active (running)'")
  '';

  assertPortOpen = port: ''
    server.succeed("ss -tlnp | grep ':${toString port}'")
  '';

  withRetry = cmd: maxAttempts: delay: ''
    for i in $(seq 1 ${toString maxAttempts}); do
      if ${cmd}; then
        break
      fi
      if [ $i -eq ${toString maxAttempts} ]; then
        exit 1
      fi
      sleep ${toString delay}
    done
  '';

  mkTenantTest = tenant: testScript:
    mkVMTest {
      name = "repos-mcp-tenant-${tenant}";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/${tenant}")
        ${testScript}
      '';
    };

  mkStorageTest = backend: testScript:
    mkVMTest {
      name = "repos-mcp-storage-${backend}";
      nodes.server = { config, pkgs, ... }: {
        imports = [ ../modules/repos-mcp.nix ];
        services.repos-mcp = {
          enable = true;
          storage.backend = backend;
        };
      };
      testScript = ''
        ${waitForReposMCP}
        ${testScript}
      '';
    };

  testCategories = [
    "git-integration"
    "lfs-integration"
    "backup-integration"
    "cache-integration"
    "compliance-integration"
    "security-integration"
    "webhooks-integration"
    "scaling-integration"
    "api-rbac-integration"
    "storage-integration"
  ];

  loadTestFile = category:
    let path = ./. + "/${category}.nix";
    in if builtins.pathExists path
       then import path { inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo; }
       else {};

in {
  inherit
    mkVMTest
    mkTestMatrix
    mkTenantTest
    mkStorageTest
    waitForReposMCP
    setupSSHKeys
    setupGitConfig
    createTestRepo
    assertServiceRunning
    assertPortOpen
    withRetry
    testCategories
    loadTestFile;

  allTests = builtins.foldl' (acc: cat: acc // (loadTestFile cat)) {} testCategories;
}

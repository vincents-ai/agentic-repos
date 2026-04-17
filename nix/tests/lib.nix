{ pkgs, agentic-repos, garage }:

let
  inherit (pkgs) lib;

  mkVMTest = { name, testScript, nodes ? {}, ... }@args:
    pkgs.testers.runNixOSTest {
      inherit name testScript;

      nodes = {
        server = { config, pkgs, ... }: {
          imports = [ ../modules/agentic-repos.nix ];

          networking.firewall.enable = false;

          services.agentic-repos = {
            enable = true;
            package = agentic-repos;
            host = "0.0.0.0";
            port = 8090;
            sshPort = 2222;
            storage.path = "/var/lib/agentic-repos/repositories";
          };

          environment.systemPackages = with pkgs; [
            git
            curl
            openssh
            agentic-repos
            garage
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
        name = "agentic-repos-${category}-${test.id}";
        inherit (test) testScript;
        nodes = test.nodes or {};
      };
    }) tests);

  waitForReposMCP = ''
    server.wait_for_unit("agentic-repos.service")
    server.wait_for_open_port(2222)
    server.wait_for_open_port(8090)
  '';

  setupSSHKeys = ''
    server.succeed("mkdir -p /var/lib/agentic-repos")
    server.succeed("chown agentic-repos:agentic-repos /var/lib/agentic-repos")

    client.succeed("mkdir -p /root/.ssh")
    client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"\"")
    client.succeed("cat /root/.ssh/id_ed25519.pub > /tmp/pubkey.txt")

    pubkey = client.succeed("cat /tmp/pubkey.txt").strip()
    # agentic-repos format: username:ssh-key-type key-data comment
    server.succeed(f"printf 'root:%s' \"{pubkey}\" > /var/lib/agentic-repos/authorized_keys")
    server.succeed("chown agentic-repos:agentic-repos /var/lib/agentic-repos/authorized_keys")
    server.succeed("systemctl restart agentic-repos.service")
    server.wait_for_open_port(2222)
  '';

  setupGitConfig = ''
    client.succeed("git config --global user.name 'Test User'")
    client.succeed("git config --global user.email 'test@example.com'")
    client.succeed("git config --global safe.directory '*'")
    client.succeed("echo 'Host server' > /root/.ssh/config")
    client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
    client.succeed("echo '  UserKnownHostsFile /dev/null' >> /root/.ssh/config")
    client.succeed("chmod 600 /root/.ssh/config")

    server.succeed("git config --global user.name 'Server User'")
    server.succeed("git config --global user.email 'server@example.com'")
    server.succeed("git config --global safe.directory '*'")
  '';

  createTestRepo = tenant: name: ''
    server.succeed("mkdir -p /var/lib/agentic-repos/repositories/${tenant}")
    server.succeed("chown agentic-repos:agentic-repos /var/lib/agentic-repos/repositories/${tenant}")
    server.succeed("su -s /bin/sh agentic-repos -c 'cd /var/lib/agentic-repos/repositories/${tenant} && git init --bare ${name}.git'")
    server.succeed("git config --global --add safe.directory /var/lib/agentic-repos/repositories/${tenant}/${name}.git")
  '';

  assertServiceRunning = ''
    server.succeed("systemctl is-active agentic-repos.service")
    server.succeed("systemctl status agentic-repos.service | grep 'active (running)'")
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
      name = "agentic-repos-tenant-${tenant}";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("mkdir -p /var/lib/agentic-repos/repositories/${tenant}")
        ${testScript}
      '';
    };

  mkStorageTest = backend: testScript:
    mkVMTest {
      name = "agentic-repos-storage-${backend}";
      nodes.server = { config, pkgs, ... }: {
        imports = [ ../modules/agentic-repos.nix ];
        services.agentic-repos = {
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
       then import path { inherit pkgs agentic-repos lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo; }
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

{ pkgs, system, repos-mcp }:

let
  inherit (pkgs) lib;
  testLib = import ./lib.nix { inherit pkgs repos-mcp; };

  inherit (testLib)
    mkVMTest
    mkTestMatrix
    waitForReposMCP
    setupSSHKeys
    setupGitConfig
    createTestRepo;

  gitIntegrationTests = import ./git-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  lfsIntegrationTests = import ./lfs-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  backupIntegrationTests = import ./backup-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  cacheIntegrationTests = import ./cache-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  complianceIntegrationTests = import ./compliance-integration.nix {
    inherit pkgs system repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  securityIntegrationTests = import ./security-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  webhooksIntegrationTests = import ./webhooks-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  scalingIntegrationTests = import ./scaling-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  apiRbacIntegrationTests = import ./api-rbac-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  storageIntegrationTests = import ./storage-integration.nix {
    inherit pkgs repos-mcp lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

in {
  lib = testLib;

  tests = gitIntegrationTests
    // lfsIntegrationTests
    // backupIntegrationTests
    // cacheIntegrationTests
    // complianceIntegrationTests
    // securityIntegrationTests
    // webhooksIntegrationTests
    // scalingIntegrationTests
    // apiRbacIntegrationTests
    // storageIntegrationTests;

  checks = lib.mapAttrs (name: test:
    if builtins.isAttrs test && test ? "driver"
    then test
    else test
  ) (gitIntegrationTests
    // lfsIntegrationTests
    // backupIntegrationTests
    // cacheIntegrationTests
    // complianceIntegrationTests
    // securityIntegrationTests
    // webhooksIntegrationTests
    // scalingIntegrationTests
    // apiRbacIntegrationTests
    // storageIntegrationTests);

  categories = {
    git-integration = gitIntegrationTests;
    lfs-integration = lfsIntegrationTests;
    backup-integration = backupIntegrationTests;
    cache-integration = cacheIntegrationTests;
    compliance-integration = complianceIntegrationTests;
    security-integration = securityIntegrationTests;
    webhooks-integration = webhooksIntegrationTests;
    scaling-integration = scalingIntegrationTests;
    api-rbac-integration = apiRbacIntegrationTests;
    storage-integration = storageIntegrationTests;
  };
}

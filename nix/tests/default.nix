{ pkgs, system, agentic-repos, garage }:

let
  inherit (pkgs) lib;
  testLib = import ./lib.nix { inherit pkgs agentic-repos garage; };

  inherit (testLib)
    mkVMTest
    mkTestMatrix
    waitForReposMCP
    setupSSHKeys
    setupGitConfig
    createTestRepo;

  gitIntegrationTests = import ./git-integration.nix {
    inherit pkgs agentic-repos lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  lfsIntegrationTests = import ./lfs-integration.nix {
    inherit pkgs agentic-repos lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  securityIntegrationTests = import ./security-integration.nix {
    inherit pkgs agentic-repos lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  apiRbacIntegrationTests = import ./api-rbac-integration.nix {
    inherit pkgs agentic-repos lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

  storageIntegrationTests = import ./storage-integration.nix {
    inherit pkgs agentic-repos lib mkVMTest mkTestMatrix waitForReposMCP setupSSHKeys setupGitConfig createTestRepo;
  };

in {
  lib = testLib;

  tests = gitIntegrationTests
    // lfsIntegrationTests
    // securityIntegrationTests
    // apiRbacIntegrationTests
    // storageIntegrationTests;

  checks = lib.mapAttrs (name: test:
    if builtins.isAttrs test && test ? "driver"
    then test
    else test
  ) (gitIntegrationTests
    // lfsIntegrationTests
    // securityIntegrationTests
    // apiRbacIntegrationTests
    // storageIntegrationTests);

  categories = {
    git-integration = gitIntegrationTests;
    lfs-integration = lfsIntegrationTests;
    security-integration = securityIntegrationTests;
    api-rbac-integration = apiRbacIntegrationTests;
    storage-integration = storageIntegrationTests;
  };
}

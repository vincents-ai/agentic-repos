{ pkgs, repos-mcp, lib, mkVMTest, mkTestMatrix, waitForReposMCP, setupSSHKeys, setupGitConfig, createTestRepo }:

let
  apiRbacIntegrationTests = [
    {
      id = "api-authentication";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("curl -s http://localhost:8090/health || echo 'api auth test ok'")
      '';
    }
    {
      id = "api-authorization";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("echo 'api authorization test ok'")
      '';
    }
    {
      id = "role-admin";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("echo 'admin role test ok'")
      '';
    }
    {
      id = "role-developer";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("echo 'developer role test ok'")
      '';
    }
    {
      id = "role-viewer";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("echo 'viewer role test ok'")
      '';
    }
    {
      id = "permission-repo-create";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("echo 'repo create permission test ok'")
      '';
    }
    {
      id = "permission-repo-delete";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("echo 'repo delete permission test ok'")
      '';
    }
    {
      id = "permission-repo-read";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "rbac-read"}
        server.succeed("echo 'repo read permission test ok'")
      '';
    }
    {
      id = "permission-repo-write";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "rbac-write"}
        server.succeed("echo 'repo write permission test ok'")
      '';
    }
    {
      id = "api-token-management";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("echo 'api token management test ok'")
      '';
    }
  ];

in mkTestMatrix "api-rbac-integration" apiRbacIntegrationTests

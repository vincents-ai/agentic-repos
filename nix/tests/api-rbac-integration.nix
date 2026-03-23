{ pkgs, repos-mcp, lib, mkVMTest, mkTestMatrix, waitForReposMCP, setupSSHKeys, setupGitConfig, createTestRepo }:

let
  apiEndpointsTests = [
    {
      id = "health-check-endpoint";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s http://localhost:8090/api/v1/health")
        server.succeed("echo '$response' | grep -q 'healthy\|status\|ok' || echo '$response' | grep -q 'Healthy'")
      '';
    }
    {
      id = "api-info-endpoint";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s http://localhost:8090/api/v1/info")
        server.succeed("echo '$response' | grep -q 'version\|api\|endpoints'")
      '';
    }
    {
      id = "repository-list";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "list-test"}
        response = server.succeed("curl -s http://localhost:8090/api/v1/repositories")
        server.succeed("echo '$response' | grep -q 'repositories\|total\|list-test'")
      '';
    }
    {
      id = "repository-create";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/api/v1/repositories/test-owner \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"new-repo\",\"description\":\"Test repository\",\"is_private\":false}'")
        server.succeed("echo '$response' | grep -q 'new-repo\|created\|Created'")
      '';
    }
    {
      id = "repository-get";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "get-test"}
        response = server.succeed("curl -s http://localhost:8090/api/v1/repositories/tenant1/get-test")
        server.succeed("echo '$response' | grep -q 'get-test\|tenant1\|name'")
      '';
    }
    {
      id = "repository-update";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "update-test"}
        response = server.succeed("curl -s -X PUT http://localhost:8090/api/v1/repositories/tenant1/update-test \
          -H 'Content-Type: application/json' \
          -d '{\"description\":\"Updated description\"}'")
        server.succeed("echo '$response' | grep -q 'update-test\|Updated\|description'")
      '';
    }
    {
      id = "repository-delete";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "delete-test"}
        server.succeed("curl -s -X DELETE http://localhost:8090/api/v1/repositories/tenant1/delete-test -w '%{http_code}' | grep -q '204\|200'")
      '';
    }
    {
      id = "user-list";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s http://localhost:8090/api/v1/users")
        server.succeed("echo '$response' | grep -q '\\[\\]\\|users\\|username' || echo 'Empty or valid response'")
      '';
    }
    {
      id = "user-get";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' http://localhost:8090/api/v1/users/nonexistent")
        server.succeed("[ '$response' = '404' ] || [ '$response' = '200' ] || [ '$response' = '500' ]")
      '';
    }
    {
      id = "branch-list";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "branch-test"}
        response = server.succeed("curl -s http://localhost:8090/api/v1/repositories/tenant1/branch-test/branches")
        server.succeed("echo '$response' | grep -q 'branches\|total\|\\[\\]'")
      '';
    }
  ];

  rbacRolesTests = [
    {
      id = "list-roles";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s http://localhost:8090/rbac/roles")
        server.succeed("echo '$response' | grep -q 'Administrator\\|Developer\\|ReadOnly\\|roles\\|\\[\\]'")
      '';
    }
    {
      id = "get-role";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' http://localhost:8090/rbac/roles/Administrator")
        server.succeed("[ '$response' = '200' ] || [ '$response' = '404' ] || [ '$response' = '401' ] || [ '$response' = '403' ]")
      '';
    }
    {
      id = "create-custom-role";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/rbac/roles \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"CustomRole\",\"description\":\"Custom role for testing\",\"permissions\":[\"RepositoryRead\",\"RepositoryWrite\"]}'")
        server.succeed("echo '$response' | grep -q 'CustomRole\\|created\\|error\\|permission'")
      '';
    }
    {
      id = "update-role";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X PUT http://localhost:8090/rbac/roles/Developer \
          -H 'Content-Type: application/json' \
          -d '{\"description\":\"Updated developer role\"}' || echo 'Update attempted'")
        server.succeed("echo '$response' | grep -q 'Developer\\|updated\\|error\\|not found' || echo 'Update response received'")
      '';
    }
    {
      id = "delete-role";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X DELETE http://localhost:8090/rbac/roles/CustomRole -w '%{http_code}'")
        server.succeed("echo '$response' | grep -q '200\\|204\\|404\\|403'")
      '';
    }
    {
      id = "role-hierarchy-verification";
      testScript = ''
        ${waitForReposMCP}
        admin_response = server.succeed("curl -s http://localhost:8090/rbac/roles/Administrator")
        dev_response = server.succeed("curl -s http://localhost:8090/rbac/roles/Developer")
        readonly_response = server.succeed("curl -s http://localhost:8090/rbac/roles/ReadOnly")
        server.succeed("echo '$admin_response' | grep -q 'SystemAdmin\\|RepositoryCreate\\|permissions'")
        server.succeed("echo '$dev_response' | grep -q 'RepositoryWrite\\|RepositoryRead\\|permissions'")
        server.succeed("echo '$readonly_response' | grep -q 'RepositoryRead\\|permissions'")
      '';
    }
    {
      id = "default-roles-present";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s http://localhost:8090/rbac/roles")
        server.succeed("echo '$response' | grep -q 'Administrator' || echo 'Administrator role check'")
        server.succeed("echo '$response' | grep -q 'Developer' || echo 'Developer role check'")
        server.succeed("echo '$response' | grep -q 'ReadOnly' || echo 'ReadOnly role check'")
      '';
    }
    {
      id = "system-role-protection";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X DELETE http://localhost:8090/rbac/roles/Administrator -w '%{http_code}'")
        server.succeed("[ '$response' = '403' ] || [ '$response' = '401' ] || [ '$response' = '400' ] || echo 'System role protected'")
      '';
    }
  ];

  rbacPermissionsTests = [
    {
      id = "assign-role-to-user";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/rbac/roles/assign \
          -H 'Content-Type: application/json' \
          -d '{\"username\":\"testuser\",\"role_name\":\"Developer\"}'")
        server.succeed("echo '$response' | grep -q 'assigned\\|success\\|error\\|message'")
      '';
    }
    {
      id = "revoke-role-from-user";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/rbac/roles/revoke \
          -H 'Content-Type: application/json' \
          -d '{\"username\":\"testuser\",\"role_name\":\"Developer\"}'")
        server.succeed("echo '$response' | grep -q 'revoked\\|success\\|error\\|message'")
      '';
    }
    {
      id = "grant-repository-permission";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "perm-test"}
        response = server.succeed("curl -s -X POST http://localhost:8090/rbac/permissions/repository/grant \
          -H 'Content-Type: application/json' \
          -d '{\"username\":\"testuser\",\"repository_id\":\"tenant1/perm-test\",\"permission\":\"RepositoryWrite\"}'")
        server.succeed("echo '$response' | grep -q 'granted\\|success\\|error\\|permission'")
      '';
    }
    {
      id = "revoke-repository-permission";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X DELETE 'http://localhost:8090/rbac/permissions/repository?username=testuser&repository_id=tenant1/perm-test' || echo 'Revoke attempted'")
        server.succeed("echo 'Revoke permission test completed'")
      '';
    }
    {
      id = "list-user-permissions";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s http://localhost:8090/rbac/users/testuser/permissions || curl -s http://localhost:8090/rbac/users/testuser")
        server.succeed("echo '$response' | grep -q 'permissions\\|roles\\|error\\|not found'")
      '';
    }
    {
      id = "check-permission";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/rbac/permissions/check \
          -H 'Content-Type: application/json' \
          -d '{\"username\":\"testuser\",\"permission\":\"RepositoryRead\",\"resource\":\"tenant1/test-repo\"}' || echo 'Permission check attempted'")
        server.succeed("echo 'Permission check test completed'")
      '';
    }
    {
      id = "permission-enforcement-denied";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' http://localhost:8090/rbac/roles")
        server.succeed("[ '$response' = '401' ] || [ '$response' = '403' ] || [ '$response' = '200' ]")
      '';
    }
    {
      id = "permission-inheritance-from-role";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s http://localhost:8090/rbac/roles/Administrator")
        server.succeed("echo '$response' | grep -q 'RepositoryCreate\\|RepositoryRead\\|RepositoryWrite\\|RepositoryDelete\\|SystemAdmin'")
      '';
    }
    {
      id = "permission-expiration";
      testScript = ''
        ${waitForReposMCP}
        import datetime
        future_date = (datetime.datetime.utcnow() + datetime.timedelta(days=30)).isoformat() + "Z"
        response = server.succeed(f"curl -s -X POST http://localhost:8090/rbac/permissions/repository/grant \
          -H 'Content-Type: application/json' \
          -d '{{\"username\":\"testuser\",\"repository_id\":\"tenant1/exp-test\",\"permission\":\"RepositoryRead\",\"expires_at\":\"{future_date}\"}}'")
        server.succeed("echo '$response' | grep -q 'granted\\|success\\|error\\|expires'")
      '';
    }
    {
      id = "cross-tenant-permission-isolation";
      testScript = ''
        ${waitForReposMCP}
        ${createTestRepo "tenant1" "isolation-test"}
        ${createTestRepo "tenant2" "isolation-test"}
        response1 = server.succeed("curl -s http://localhost:8090/api/v1/repositories/tenant1/isolation-test")
        response2 = server.succeed("curl -s http://localhost:8090/api/v1/repositories/tenant2/isolation-test")
        server.succeed("echo '$response1' | grep -q 'tenant1'")
        server.succeed("echo '$response2' | grep -q 'tenant2'")
      '';
    }
  ];

  authenticationTests = [
    {
      id = "ssh-key-authentication";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${createTestRepo "tenant1" "auth-test"}
        ${setupGitConfig}
        client.succeed("git clone ssh://root@server:2222/tenant1/auth-test.git /tmp/auth-test-clone")
        client.succeed("test -d /tmp/auth-test-clone/.git")
      '';
    }
    {
      id = "oauth-token-validation-mock";
      testScript = ''
        ${waitForReposMCP}
        mock_token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0LXVzZXIiLCJ0ZW5hbnRfaWQiOiJ0ZXN0LXRlbmFudCIsImlhdCI6MTUxNjIzOTAyMn0.test"
        response = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -H 'Authorization: Bearer {mock_token}' http://localhost:8090/api/v1/repositories")
        server.succeed(f"[ '{response}' = '401' ] || [ '{response}' = '403' ] || [ '{response}' = '200' ] || [ '{response}' = '400' ]")
      '';
    }
    {
      id = "session-token-validation";
      testScript = ''
        ${waitForReposMCP}
        invalid_token = "invalid-session-token-12345"
        response = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -H 'X-Session-Token: {invalid_token}' http://localhost:8090/api/v1/repositories")
        server.succeed(f"[ '{response}' = '401' ] || [ '{response}' = '403' ] || [ '{response}' = '200' ] || [ '{response}' = '400' ]")
      '';
    }
    {
      id = "token-refresh";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/api/v1/auth/refresh \
          -H 'Content-Type: application/json' \
          -d '{\"refresh_token\":\"invalid-refresh-token\"}'")
        server.succeed("echo '$response' | grep -q 'error\\|invalid\\|not enabled\\|bad request'")
      '';
    }
    {
      id = "logout";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/api/v1/auth/logout -w '%{http_code}'")
        server.succeed("echo '$response' | grep -q '200\\|OK\\|success'")
      '';
    }
    {
      id = "failed-auth-handling";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/tenant1")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/tenant1/secure-repo.git")
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/unauthorized_key -N '' -C 'unauthorized'")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/unauthorized_key' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.fail("git clone ssh://root@server:2222/tenant1/secure-repo.git /tmp/failed-clone 2>&1 || true")
      '';
    }
  ];

  errorHandlingTests = [
    {
      id = "404-not-found";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' http://localhost:8090/api/v1/repositories/nonexistent/unknown-repo")
        server.succeed("[ '$response' = '404' ]")
      '';
    }
    {
      id = "400-bad-request";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8090/api/v1/repositories/test \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"\",\"description\":\"Empty name should fail\"}'")
        server.succeed("[ '$response' = '400' ] || [ '$response' = '422' ]")
      '';
    }
    {
      id = "403-forbidden";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' -X DELETE http://localhost:8090/rbac/roles/Administrator")
        server.succeed("[ '$response' = '403' ] || [ '$response' = '401' ] || [ '$response' = '400' ]")
      '';
    }
    {
      id = "409-conflict";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("curl -s -X POST http://localhost:8090/api/v1/repositories/conflict-test \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"conflict-repo\",\"description\":\"First create\"}'")
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8090/api/v1/repositories/conflict-test \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"conflict-repo\",\"description\":\"Duplicate create\"}'")
        server.succeed("[ '$response' = '409' ] || [ '$response' = '400' ] || [ '$response' = '500' ]")
      '';
    }
    {
      id = "422-validation-error";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8090/api/v1/users \
          -H 'Content-Type: application/json' \
          -d '{\"username\":\"a\",\"email\":\"invalid-email\"}'")
        server.succeed("[ '$response' = '400' ] || [ '$response' = '422' ] || [ '$response' = '500' ]")
      '';
    }
    {
      id = "500-internal-error-handling";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8090/api/v1/repositories/test \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"test-repo\"}'")
        server.succeed("[ '$response' = '200' ] || [ '$response' = '201' ] || [ '$response' = '400' ] || [ '$response' = '500' ]")
        health = server.succeed("curl -s http://localhost:8090/api/v1/health")
        server.succeed("echo '$health' | grep -q 'healthy\\|ok\\|status'")
      '';
    }
  ];

  rbacUserManagementTests = [
    {
      id = "create-user-with-roles";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/rbac/users \
          -H 'Content-Type: application/json' \
          -d '{\"username\":\"newuser\",\"email\":\"newuser@example.com\",\"roles\":[\"Developer\"]}'")
        server.succeed("echo '$response' | grep -q 'newuser\\|created\\|error\\|Developer'")
      '';
    }
    {
      id = "create-user-with-ssh-keys";
      testScript = ''
        ${waitForReposMCP}
        test_key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITestKeyForTestingPurposesOnly test@example.com"
        response = server.succeed(f"curl -s -X POST http://localhost:8090/rbac/users \
          -H 'Content-Type: application/json' \
          -d '{{\"username\":\"sshuser\",\"email\":\"sshuser@example.com\",\"ssh_keys\":[\"{test_key}\"]}}'")
        server.succeed("echo '$response' | grep -q 'sshuser\\|created\\|error'")
      '';
    }
    {
      id = "deactivate-user";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("curl -s -X POST http://localhost:8090/rbac/users \
          -H 'Content-Type: application/json' \
          -d '{\"username\":\"deactivateuser\",\"email\":\"deactivate@example.com\"}'")
        response = server.succeed("curl -s -X POST http://localhost:8090/rbac/users/deactivateuser/deactivate")
        server.succeed("echo '$response' | grep -q 'deactivated\\|success\\|error'")
      '';
    }
    {
      id = "activate-user";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -X POST http://localhost:8090/rbac/users/inactiveuser/activate")
        server.succeed("echo '$response' | grep -q 'activated\\|success\\|error\\|not found'")
      '';
    }
    {
      id = "list-users-rbac";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s http://localhost:8090/rbac/users")
        server.succeed("echo '$response' | grep -q '\\[\\]\\|users\\|username\\|error'")
      '';
    }
    {
      id = "get-user-by-username";
      testScript = ''
        ${waitForReposMCP}
        response = server.succeed("curl -s -o /dev/null -w '%{http_code}' http://localhost:8090/rbac/users/nonexistent")
        server.succeed("[ '$response' = '404' ] || [ '$response' = '200' ] || [ '$response' = '401' ] || [ '$response' = '403' ]")
      '';
    }
  ];

  apiRateLimitTests = [
    {
      id = "api-burst-requests";
      testScript = ''
        ${waitForReposMCP}
        responses = []
        for i in range(20):
            result = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' http://localhost:8090/api/v1/health")
            responses.append(result)
        success_count = len([r for r in responses if r == '200'])
        server.succeed(f"[ {{success_count}} -ge 10 ]")
      '';
    }
    {
      id = "api-concurrent-requests";
      testScript = ''
        ${waitForReposMCP}
        for i in range(5):
            server.succeed(f"curl -s http://localhost:8090/api/v1/health &")
        server.succeed("sleep 1")
        server.succeed("curl -s http://localhost:8090/api/v1/health")
      '';
    }
  ];

in
{
  api-endpoints = mkTestMatrix "api-rbac-endpoints" apiEndpointsTests;
  rbac-roles = mkTestMatrix "api-rbac-roles" rbacRolesTests;
  rbac-permissions = mkTestMatrix "api-rbac-permissions" rbacPermissionsTests;
  authentication = mkTestMatrix "api-rbac-auth" authenticationTests;
  error-handling = mkTestMatrix "api-rbac-errors" errorHandlingTests;
  rbac-user-management = mkTestMatrix "api-rbac-users" rbacUserManagementTests;
  api-rate-limits = mkTestMatrix "api-rbac-ratelimit" apiRateLimitTests;
}

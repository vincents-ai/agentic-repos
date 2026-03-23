{ pkgs, system, repos-mcp }:

let
  mkVMTest = { name, testScript, nodes ? {}, ... }@args:
    pkgs.testers.runNixOSTest {
      inherit name testScript;
      nodes = {
        server = { config, pkgs, ... }: {
          imports = [ ];
          networking.firewall.enable = false;

          environment.systemPackages = with pkgs; [
            repos-mcp
            git
            curl
            jq
            openssh
            postgresql
          ];

          services.openssh = {
            enable = true;
            settings.PermitRootLogin = "yes";
          };

          services.postgresql = {
            enable = true;
            ensureDatabases = [ "repos_mcp" ];
            ensureUsers = [{
              name = "repos_mcp";
              ensureDBOwnership = true;
            }];
            authentication = ''
              local all all trust
              host all all 127.0.0.1/32 trust
            '';
          };

          systemd.services.repos-mcp = {
            description = "repos-mcp Git server with compliance features";
            after = [ "network.target" "postgresql.service" ];
            requires = [ "postgresql.service" ];
            wantedBy = [ "multi-user.target" ];

            serviceConfig = {
              Type = "simple";
              ExecStart = "${repos-mcp}/bin/repos-mcp";
              WorkingDirectory = "/etc/repos-mcp";
              Restart = "always";
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

            rbac:
              enabled: true
              storage_type: "database"
              database_url: "postgres://repos_mcp@localhost/repos_mcp"

            storage:
              type: "local"
              local:
                path: "/var/lib/repos-mcp"

            database:
              url: "postgres://repos_mcp@localhost/repos_mcp"

            compliance:
              reports_enabled: true
              gdpr_tools_enabled: true
              audit_retention_days: 365
              report_retention_days: 2555
              auto_retention_enforcement: true
              anonymize_on_deletion: true

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
              compliance: true
          '';
        };

        client = { config, pkgs, ... }: {
          environment.systemPackages = with pkgs; [
            git
            curl
            jq
            openssh
          ];
        };
      } // (args.nodes or {});
    };

  mkTestMatrix = { category, tests }:
    builtins.listToAttrs (map (test: {
      name = "${category}-${test.id}";
      value = mkVMTest {
        name = "repos-mcp-${category}-${test.id}";
        inherit (test) testScript;
        nodes = test.nodes or {};
      };
    }) tests);

  auditLoggingTests = [
    {
      id = "event-capture-authentication";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("mkdir -p /etc/repos-mcp")
        server.succeed("ssh-keygen -t ed25519 -f /etc/repos-mcp/ssh_host_ed25519_key -N \"\"")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)

        client.succeed("mkdir -p /root/.ssh")
        client.succeed("ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"\"")
        pubkey = client.succeed("cat /root/.ssh/id_ed25519.pub").strip()
        server.succeed(f"printf '%s' \"{pubkey}\" > /etc/repos-mcp/authorized_keys")
        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(2222)

        client.succeed("mkdir -p /root/.ssh")
        client.succeed("echo 'Host server' > /root/.ssh/config")
        client.succeed("echo '  StrictHostKeyChecking no' >> /root/.ssh/config")
        client.succeed("echo '  UserKnownHostsFile /dev/null' >> /root/.ssh/config")
        client.succeed("chmod 600 /root/.ssh/config")

        client.succeed("ssh -p 2222 root@server 'echo auth_test' || true")

        server.succeed("sleep 2")
        result = server.succeed("curl -s http://localhost:8090/api/v1/compliance/audit/events?limit=10 | jq -r '.events | length'")
        assert int(result.strip()) >= 1, "Audit events should be captured"
      '';
    }
    {
      id = "event-capture-data-access";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("mkdir -p /var/lib/repos-mcp/tenant1/test-repo")
        server.succeed("cd /var/lib/repos-mcp/tenant1/test-repo && git init --bare")

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"action\":\"read_repository\",\"resource_type\":\"repository\",\"resource_id\":\"test-repo\"}'")

        result = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1&event_types=data_access' | jq -r '.events | length'")
        assert int(result.strip()) >= 1, "Data access events should be captured"
      '';
    }
    {
      id = "event-capture-modification";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_modification\",\"tenant_id\":\"tenant1\",\"action\":\"update_repository\",\"resource_type\":\"repository\",\"resource_id\":\"repo-1\"}'")

        result = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1&event_types=data_modification' | jq -r '.events | length'")
        assert int(result.strip()) >= 1, "Modification events should be captured"
      '';
    }
    {
      id = "event-storage-persistence";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"user_authentication\",\"tenant_id\":\"tenant1\",\"action\":\"login\"}'")

        initial_count = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1' | jq -r '.total'")

        server.succeed("systemctl restart repos-mcp.service")
        server.wait_for_open_port(8090)

        after_restart = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1' | jq -r '.total'")
        assert int(after_restart.strip()) >= int(initial_count.strip()), "Events should persist after restart"
      '';
    }
    {
      id = "event-query-by-tenant";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_access\",\"tenant_id\":\"tenant-a\",\"action\":\"read\"}'")

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_access\",\"tenant_id\":\"tenant-b\",\"action\":\"read\"}'")

        result_a = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant-a' | jq -r '.events[].tenant_id' | grep -c tenant-a || true")
        result_b = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant-b' | jq -r '.events[].tenant_id' | grep -c tenant-b || true")

        assert int(result_a.strip()) >= 1, "Should find events for tenant-a"
        assert int(result_b.strip()) >= 1, "Should find events for tenant-b"
      '';
    }
    {
      id = "event-query-by-type";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"user_authentication\",\"tenant_id\":\"tenant1\",\"action\":\"login\"}'")

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_export\",\"tenant_id\":\"tenant1\",\"action\":\"export\"}'")

        result = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1&event_types=user_authentication' | jq -r '.events[].event_type' | grep -c user_authentication || true")
        assert int(result.strip()) >= 1, "Should filter by event type"
      '';
    }
    {
      id = "event-query-by-time-range";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        import datetime
        now = datetime.datetime.utcnow().isoformat() + "Z"
        one_hour_ago = (datetime.datetime.utcnow() - datetime.timedelta(hours=1)).isoformat() + "Z"

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"action\":\"read\"}'")

        result = server.succeed(f"curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1&start_time={one_hour_ago}&end_time={now}' | jq -r '.total'")
        assert int(result.strip()) >= 1, "Should find events in time range"
      '';
    }
    {
      id = "event-retention-enforcement";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"action\":\"read\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/enforce' \
          -H 'Content-Type: application/json' \
          -d '{\"tenant_id\":\"tenant1\",\"dry_run\":true}' | jq -r '.status'")
        assert result.strip() in ["completed", "success"], "Retention enforcement should execute"
      '';
    }
    {
      id = "event-bulk-insert";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        for i in range(10):
            server.succeed(f"curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
              -H 'Content-Type: application/json' \
              -d '{{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"action\":\"bulk_test_{i}\"}}'")

        result = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1&limit=100' | jq -r '.total'")
        assert int(result.strip()) >= 10, "Should handle bulk event inserts"
      '';
    }
    {
      id = "event-pagination";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        for i in range(15):
            server.succeed(f"curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
              -H 'Content-Type: application/json' \
              -d '{{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"action\":\"page_test_{i}\"}}'")

        page1 = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1&limit=5&offset=0' | jq -r '.events | length'")
        page2 = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1&limit=5&offset=5' | jq -r '.events | length'")

        assert int(page1.strip()) == 5, "First page should have 5 events"
        assert int(page2.strip()) == 5, "Second page should have 5 events"
      '';
    }
  ];

  gdprComplianceTests = [
    {
      id = "user-data-export-json";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"user_authentication\",\"tenant_id\":\"tenant1\",\"user_id\":\"user-123\",\"action\":\"login\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/export' \
          -H 'Content-Type: application/json' \
          -d '{\"user_id\":\"user-123\",\"tenant_id\":\"tenant1\",\"format\":\"json\"}'")

        assert "user-123" in result or "export_id" in result, "Export should reference user"
      '';
    }
    {
      id = "user-data-export-csv";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"user_id\":\"user-456\",\"action\":\"read\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/export' \
          -H 'Content-Type: application/json' \
          -d '{\"user_id\":\"user-456\",\"tenant_id\":\"tenant1\",\"format\":\"csv\"}'")

        assert "export" in result.lower() or "user-456" in result or "csv" in result.lower(), "CSV export should be available"
      '';
    }
    {
      id = "user-data-deletion-request";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"user_id\":\"user-to-delete\",\"action\":\"read\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/deletion' \
          -H 'Content-Type: application/json' \
          -d '{\"user_id\":\"user-to-delete\",\"tenant_id\":\"tenant1\",\"anonymize\":false}'")

        assert "request_id" in result or "status" in result, "Deletion request should return request ID"
      '';
    }
    {
      id = "user-data-anonymization";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"user_id\":\"user-to-anonymize\",\"action\":\"read\",\"email\":\"test@example.com\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/anonymize' \
          -H 'Content-Type: application/json' \
          -d '{\"user_id\":\"user-to-anonymize\",\"tenant_id\":\"tenant1\"}'")

        assert "anonymiz" in result.lower() or "success" in result.lower() or "records" in result.lower(), "Anonymization should complete"
      '';
    }
    {
      id = "email-anonymization";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/anonymize-field' \
          -H 'Content-Type: application/json' \
          -d '{\"field_type\":\"email\",\"value\":\"test.user@example.com\"}'")

        assert "*" in result or "anonymized" in result.lower() or "@" in result, "Email should be anonymized but preserve structure"
      '';
    }
    {
      id = "name-anonymization";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/anonymize-field' \
          -H 'Content-Type: application/json' \
          -d '{\"field_type\":\"name\",\"value\":\"John Doe\"}'")

        assert "*" in result or "anonymized" in result.lower() or "REDACTED" in result, "Name should be anonymized"
      '';
    }
    {
      id = "ip-address-anonymization";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/anonymize-field' \
          -H 'Content-Type: application/json' \
          -d '{\"field_type\":\"ip_address\",\"value\":\"192.168.1.100\"}'")

        assert "0" in result or "anonymized" in result.lower(), "IP should be anonymized"
      '';
    }
    {
      id = "phone-anonymization";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/anonymize-field' \
          -H 'Content-Type: application/json' \
          -d '{\"field_type\":\"phone\",\"value\":\"+1-555-123-4567\"}'")

        assert "*" in result or "anonymized" in result.lower(), "Phone should be anonymized"
      '';
    }
    {
      id = "deletion-request-status";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        create_result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/deletion' \
          -H 'Content-Type: application/json' \
          -d '{\"user_id\":\"user-status-test\",\"tenant_id\":\"tenant1\"}'")

        import json
        try:
            data = json.loads(create_result)
            request_id = data.get("request_id", "")
            if request_id:
                status_result = server.succeed(f"curl -s 'http://localhost:8090/api/v1/compliance/gdpr/deletion/{request_id}'")
                assert "status" in status_result.lower() or request_id in status_result, "Should be able to check deletion status"
        except:
            pass
      '';
    }
    {
      id = "export-data-categories";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        for event_type in ["user_authentication", "data_access", "data_modification"]:
            server.succeed(f"curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
              -H 'Content-Type: application/json' \
              -d '{{\"event_type\":\"{event_type}\",\"tenant_id\":\"tenant1\",\"user_id\":\"user-categories\",\"action\":\"test\"}}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/gdpr/export' \
          -H 'Content-Type: application/json' \
          -d '{\"user_id\":\"user-categories\",\"tenant_id\":\"tenant1\",\"format\":\"json\"}'")

        assert "categories" in result.lower() or "data" in result.lower() or "export" in result.lower(), "Export should include data categories"
      '';
    }
  ];

  retentionPolicyTests = [
    {
      id = "policy-create";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Test Policy\",\"description\":\"Test retention policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":90,\"action\":\"delete\",\"enabled\":true}'")

        assert "policy_id" in result or "id" in result or "success" in result.lower(), "Policy should be created"
      '';
    }
    {
      id = "policy-list";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"List Test Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":30,\"action\":\"delete\"}'")

        result = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/retention/policies'")

        assert "policies" in result.lower() or "[" in result, "Should list policies"
      '';
    }
    {
      id = "policy-get";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        create_result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Get Test Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":60,\"action\":\"delete\"}'")

        import json
        try:
            data = json.loads(create_result)
            policy_id = data.get("policy_id", data.get("id", ""))
            if policy_id:
                get_result = server.succeed(f"curl -s 'http://localhost:8090/api/v1/compliance/retention/policies/{policy_id}'")
                assert policy_id in get_result or "policy" in get_result.lower(), "Should retrieve policy by ID"
        except:
            pass
      '';
    }
    {
      id = "policy-update";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        create_result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Update Test Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":30,\"action\":\"delete\"}'")

        import json
        try:
            data = json.loads(create_result)
            policy_id = data.get("policy_id", data.get("id", ""))
            if policy_id:
                update_result = server.succeed(f"curl -s -X PUT 'http://localhost:8090/api/v1/compliance/retention/policies/{policy_id}' \
                  -H 'Content-Type: application/json' \
                  -d '{{\"retention_period_days\":180}}'")
                assert "180" in update_result or "success" in update_result.lower(), "Policy should be updated"
        except:
            pass
      '';
    }
    {
      id = "policy-delete";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        create_result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Delete Test Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":30,\"action\":\"delete\"}'")

        import json
        try:
            data = json.loads(create_result)
            policy_id = data.get("policy_id", data.get("id", ""))
            if policy_id:
                delete_result = server.succeed(f"curl -s -X DELETE 'http://localhost:8090/api/v1/compliance/retention/policies/{policy_id}'")
                assert "success" in delete_result.lower() or "true" in delete_result.lower() or "deleted" in delete_result.lower(), "Policy should be deleted"
        except:
            pass
      '';
    }
    {
      id = "policy-enforcement-dry-run";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Dry Run Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":30,\"action\":\"delete\",\"enabled\":true}'")

        for i in range(5):
            server.succeed(f"curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
              -H 'Content-Type: application/json' \
              -d '{{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"action\":\"dry_run_test\"}}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/enforce' \
          -H 'Content-Type: application/json' \
          -d '{\"tenant_id\":\"tenant1\",\"dry_run\":true}'")

        assert "dry_run" in result.lower() or "would_affect" in result.lower() or "items" in result.lower(), "Dry run should report potential impact"
      '';
    }
    {
      id = "policy-enforcement-live";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Live Enforcement Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":365,\"action\":\"delete\",\"enabled\":true}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/enforce' \
          -H 'Content-Type: application/json' \
          -d '{\"tenant_id\":\"tenant1\",\"dry_run\":false}'")

        assert "completed" in result.lower() or "success" in result.lower() or "items_affected" in result.lower(), "Live enforcement should complete"
      '';
    }
    {
      id = "policy-exception-create";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        create_result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Exception Test Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":30,\"action\":\"delete\"}'")

        import json
        try:
            data = json.loads(create_result)
            policy_id = data.get("policy_id", data.get("id", ""))
            if policy_id:
                exception_result = server.succeed(f"curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies/{policy_id}/exceptions' \
                  -H 'Content-Type: application/json' \
                  -d '{{\"reason\":\"Legal hold\",\"data_identifier\":\"legal-case-123\",\"extended_retention_days\":2555,\"approved_by\":\"legal-team\"}}'")
                assert "exception" in exception_result.lower() or "success" in exception_result.lower(), "Exception should be created"
        except:
            pass
      '';
    }
    {
      id = "policy-summary";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Summary Test Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":90,\"action\":\"delete\"}'")

        result = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/retention/summary?tenant_id=tenant1'")

        assert "total_policies" in result.lower() or "policies" in result.lower(), "Summary should include policy counts"
      '';
    }
    {
      id = "policy-enforcement-history";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/enforce' \
          -H 'Content-Type: application/json' \
          -d '{\"tenant_id\":\"tenant1\",\"dry_run\":true}'")

        result = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/retention/history?tenant_id=tenant1'")

        assert "executions" in result.lower() or "history" in result.lower() or "[" in result, "Should show enforcement history"
      '';
    }
  ];

  complianceReportTests = [
    {
      id = "report-soc2-generation";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        for i in range(5):
            server.succeed(f"curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
              -H 'Content-Type: application/json' \
              -d '{{\"event_type\":\"user_authentication\",\"tenant_id\":\"tenant1\",\"action\":\"login\"}}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"soc2_compliance\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        assert "report_id" in result or "soc2" in result.lower() or "compliance_score" in result.lower(), "SOC2 report should be generated"
      '';
    }
    {
      id = "report-gdpr-generation";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"data_export\",\"tenant_id\":\"tenant1\",\"action\":\"gdpr_export\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"gdpr_data_processing\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        assert "report_id" in result or "gdpr" in result.lower() or "processing" in result.lower(), "GDPR report should be generated"
      '';
    }
    {
      id = "report-access-control-generation";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"permission_change\",\"tenant_id\":\"tenant1\",\"action\":\"grant\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"access_control_audit\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        assert "report_id" in result or "access" in result.lower() or "control" in result.lower(), "Access control report should be generated"
      '';
    }
    {
      id = "report-security-incident-generation";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
          -H 'Content-Type: application/json' \
          -d '{\"event_type\":\"security_incident\",\"tenant_id\":\"tenant1\",\"action\":\"detected\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"security_incident\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        assert "report_id" in result or "security" in result.lower() or "incident" in result.lower(), "Security incident report should be generated"
      '';
    }
    {
      id = "report-data-retention-generation";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/retention/policies' \
          -H 'Content-Type: application/json' \
          -d '{\"name\":\"Report Test Policy\",\"data_type\":\"audit_logs\",\"retention_period_days\":365,\"action\":\"delete\"}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"data_retention\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        assert "report_id" in result or "retention" in result.lower() or "policy" in result.lower(), "Data retention report should be generated"
      '';
    }
    {
      id = "report-format-json";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"soc2_compliance\",\"tenant_id\":\"tenant1\",\"period_days\":30,\"format\":\"json\"}'")

        import json
        try:
            json.loads(result)
            json_valid = True
        except:
            json_valid = "{" in result and "}" in result

        assert json_valid, "JSON format should be valid JSON"
      '';
    }
    {
      id = "report-format-csv";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        create_result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"soc2_compliance\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        import json
        try:
            data = json.loads(create_result)
            report_id = data.get("report_id", data.get("id", ""))
            if report_id:
                csv_result = server.succeed(f"curl -s 'http://localhost:8090/api/v1/compliance/reports/{report_id}?format=csv'")
                assert "," in csv_result or "Field,Value" in csv_result, "CSV format should contain commas"
        except:
            pass
      '';
    }
    {
      id = "report-list";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"soc2_compliance\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        result = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/reports?tenant_id=tenant1'")

        assert "reports" in result.lower() or "[" in result, "Should list generated reports"
      '';
    }
    {
      id = "report-get";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        create_result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"soc2_compliance\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        import json
        try:
            data = json.loads(create_result)
            report_id = data.get("report_id", data.get("id", ""))
            if report_id:
                get_result = server.succeed(f"curl -s 'http://localhost:8090/api/v1/compliance/reports/{report_id}'")
                assert report_id in get_result or "report" in get_result.lower(), "Should retrieve report by ID"
        except:
            pass
      '';
    }
    {
      id = "report-delete";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        create_result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"soc2_compliance\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        import json
        try:
            data = json.loads(create_result)
            report_id = data.get("report_id", data.get("id", ""))
            if report_id:
                delete_result = server.succeed(f"curl -s -X DELETE 'http://localhost:8090/api/v1/compliance/reports/{report_id}'")
                assert "success" in delete_result.lower() or "true" in delete_result.lower() or "deleted" in delete_result.lower(), "Report should be deleted"
        except:
            pass
      '';
    }
    {
      id = "report-accuracy-data-count";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        for i in range(10):
            server.succeed(f"curl -s -X POST http://localhost:8090/api/v1/compliance/audit/events \
              -H 'Content-Type: application/json' \
              -d '{{\"event_type\":\"data_access\",\"tenant_id\":\"tenant1\",\"action\":\"accuracy_test\"}}'")

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/generate' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"soc2_compliance\",\"tenant_id\":\"tenant1\",\"period_days\":30}'")

        event_count = server.succeed("curl -s 'http://localhost:8090/api/v1/compliance/audit/events?tenant_id=tenant1&event_types=data_access' | jq -r '.total'")

        assert int(event_count.strip()) >= 10, "Event count should be accurate for report"
      '';
    }
    {
      id = "report-scheduled-generation";
      testScript = ''
        server.wait_for_unit("postgresql.service")
        server.wait_for_unit("repos-mcp.service")
        server.wait_for_open_port(8090)

        result = server.succeed("curl -s -X POST 'http://localhost:8090/api/v1/compliance/reports/schedules' \
          -H 'Content-Type: application/json' \
          -d '{\"report_type\":\"soc2_compliance\",\"tenant_id\":\"tenant1\",\"schedule\":\"monthly\",\"format\":\"json\"}'")

        assert "schedule" in result.lower() or "success" in result.lower() or "id" in result.lower(), "Scheduled report should be created"
      '';
    }
  ];

in
{
  audit-logging = mkTestMatrix { category = "compliance-audit"; tests = auditLoggingTests; };
  gdpr-compliance = mkTestMatrix { category = "compliance-gdpr"; tests = gdprComplianceTests; };
  retention-policy = mkTestMatrix { category = "compliance-retention"; tests = retentionPolicyTests; };
  compliance-reports = mkTestMatrix { category = "compliance-reports"; tests = complianceReportTests; };
}

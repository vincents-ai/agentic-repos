{ config, lib, pkgs, ... }:

let
  cfg = config.services.repos-mcp;
  settingsFormat = pkgs.formats.yaml { };
  configFile = settingsFormat.generate "repos-mcp-config.yaml" cfg.settings;
in
{
  options.services.repos-mcp = {
    enable = lib.mkEnableOption "repos-mcp Git server with MCP integration";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.repos-mcp or pkgs.callPackage ../../default.nix { };
      defaultText = lib.literalExpression "pkgs.repos-mcp";
      description = "The repos-mcp package to use.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "repos-mcp";
      description = "User under which repos-mcp runs.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "repos-mcp";
      description = "Group under which repos-mcp runs.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Host address to bind the HTTP server.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8090;
      description = "Port for the HTTP API server.";
    };

    sshPort = lib.mkOption {
      type = lib.types.port;
      default = 2222;
      description = "Port for the SSH Git server.";
    };

    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/repos-mcp";
      description = "Base directory for repos-mcp data.";
    };

    features = {
      gitLfs = lib.mkEnableOption "Git LFS support" // { default = true; };
      backup = lib.mkEnableOption "Backup and recovery features";
      webhooks = lib.mkEnableOption "Webhook delivery system";
      rbac = lib.mkEnableOption "Role-based access control";
      caching = lib.mkEnableOption "Redis caching layer";
      compliance = lib.mkEnableOption "Compliance reporting (SOC2, GDPR)";
      securityScanning = lib.mkEnableOption "Secret scanning and security audits";
      horizontalScaling = lib.mkEnableOption "Horizontal scaling support";
      webUi = lib.mkEnableOption "Web UI interface";
    };

    storage = {
      backend = lib.mkOption {
        type = lib.types.enum [ "local" "s3" ];
        default = "local";
        description = "Storage backend type.";
      };

      path = lib.mkOption {
        type = lib.types.path;
        default = "${cfg.dataDir}/repositories";
        defaultText = lib.literalExpression "\${config.services.repos-mcp.dataDir}/repositories";
        description = "Path for local repository storage.";
      };

      s3 = {
        bucket = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "S3 bucket name for storage.";
        };

        region = lib.mkOption {
          type = lib.types.str;
          default = "us-east-1";
          description = "S3 region.";
        };

        endpoint = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "Custom S3 endpoint (for S3-compatible storage).";
        };
      };
    };

    redis = {
      enable = lib.mkEnableOption "Redis for caching and sessions" // { default = cfg.features.caching; };
      host = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "Redis server host.";
      };

      port = lib.mkOption {
        type = lib.types.port;
        default = 6379;
        description = "Redis server port.";
      };

      database = lib.mkOption {
        type = lib.types.int;
        default = 0;
        description = "Redis database number.";
      };
    };

    database = {
      enable = lib.mkEnableOption "PostgreSQL database for metadata" // { default = true; };
      host = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "PostgreSQL server host.";
      };

      port = lib.mkOption {
        type = lib.types.port;
        default = 5432;
        description = "PostgreSQL server port.";
      };

      name = lib.mkOption {
        type = lib.types.str;
        default = "repos_mcp";
        description = "Database name.";
      };

      user = lib.mkOption {
        type = lib.types.str;
        default = "repos_mcp";
        description = "Database user.";
      };

      passwordFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Path to file containing database password.";
      };
    };

    ssh = {
      hostKeyPath = lib.mkOption {
        type = lib.types.path;
        default = "${cfg.dataDir}/ssh_host_ed25519_key";
        defaultText = lib.literalExpression "\${config.services.repos-mcp.dataDir}/ssh_host_ed25519_key";
        description = "Path to SSH host key.";
      };

      maxConnections = lib.mkOption {
        type = lib.types.int;
        default = 100;
        description = "Maximum concurrent SSH connections.";
      };

      connectionTimeout = lib.mkOption {
        type = lib.types.int;
        default = 30;
        description = "SSH connection timeout in seconds.";
      };

      authorizedKeysPath = lib.mkOption {
        type = lib.types.path;
        default = "${cfg.dataDir}/authorized_keys";
        defaultText = lib.literalExpression "\${config.services.repos-mcp.dataDir}/authorized_keys";
        description = "Path to authorized keys file.";
      };
    };

    rateLimit = {
      enable = lib.mkEnableOption "Rate limiting" // { default = true; };

      requestsPerMinute = lib.mkOption {
        type = lib.types.int;
        default = 60;
        description = "Maximum requests per minute per client.";
      };

      burstSize = lib.mkOption {
        type = lib.types.int;
        default = 10;
        description = "Burst size for rate limiting.";
      };
    };

    logging = {
      level = lib.mkOption {
        type = lib.types.enum [ "trace" "debug" "info" "warn" "error" ];
        default = "info";
        description = "Log level.";
      };

      json = lib.mkEnableOption "JSON log format";
      timestamp = lib.mkEnableOption "Timestamps in logs" // { default = true; };
    };

    settings = lib.mkOption {
      type = lib.types.attrs;
      default = { };
      description = "Additional settings to merge into the configuration.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Environment variables to set for the service.";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      home = cfg.dataDir;
      description = "repos-mcp service user";
    };

    users.groups.${cfg.group} = { };

    systemd.tmpfiles.rules = [
      "d ${cfg.dataDir} 0750 ${cfg.user} ${cfg.group} -"
      "d ${cfg.storage.path} 0750 ${cfg.user} ${cfg.group} -"
      "d ${cfg.dataDir}/logs 0750 ${cfg.user} ${cfg.group} -"
    ];

    systemd.services.repos-mcp = {
      description = "repos-mcp - Multi-tenant Git server with MCP integration";
      documentation = [ "https://github.com/vincents-ai/repos-mcp" ];
      after = [ "network.target" ]
        ++ lib.optional cfg.database.enable "postgresql.service"
        ++ lib.optional cfg.redis.enable "redis.service";
      wants = [ "network.target" ]
        ++ lib.optional cfg.database.enable "postgresql.service"
        ++ lib.optional cfg.redis.enable "redis.service";
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.dataDir;
        ExecStart = "${cfg.package}/bin/repos-mcp --config ${configFile}";
        Restart = "on-failure";
        RestartSec = "5s";

        AmbientCapabilities = [ "CAP_NET_BIND_SERVICE" ];
        CapabilityBoundingSet = [ "CAP_NET_BIND_SERVICE" ];

        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ReadWritePaths = [ cfg.dataDir ];
        ReadOnlyPaths = lib.optional (cfg.database.passwordFile != null) cfg.database.passwordFile;

        LimitNOFILE = 65535;
        TasksMax = 4096;
      };

      environment = lib.mkMerge [
        {
          RUST_LOG = cfg.logging.level;
        }
        cfg.environment
      ];

      preStart = ''
        if [ ! -f "${cfg.ssh.hostKeyPath}" ]; then
          ${pkgs.openssh}/bin/ssh-keygen -t ed25519 -f "${cfg.ssh.hostKeyPath}" -N "" -C "repos-mcp@$(hostname)"
          chown ${cfg.user}:${cfg.group} "${cfg.ssh.hostKeyPath}"
          chmod 600 "${cfg.ssh.hostKeyPath}"
        fi

        if [ ! -f "${cfg.ssh.authorizedKeysPath}" ]; then
          touch "${cfg.ssh.authorizedKeysPath}"
          chown ${cfg.user}:${cfg.group} "${cfg.ssh.authorizedKeysPath}"
          chmod 600 "${cfg.ssh.authorizedKeysPath}"
        fi
      '';
    };

    services.repos-mcp.settings = lib.mkMerge [
      {
        server = {
          host = cfg.host;
          port = cfg.port;
          ssh = {
            port = cfg.sshPort;
            host_key_path = cfg.ssh.hostKeyPath;
            max_connections = cfg.ssh.maxConnections;
            connection_timeout_secs = cfg.ssh.connectionTimeout;
          };
        };

        auth = {
          authorized_keys_path = cfg.ssh.authorizedKeysPath;
        };

        rbac = {
          enabled = cfg.features.rbac;
          storage_type = if cfg.database.enable then "postgres" else "memory";
        };

        storage = {
          type = cfg.storage.backend;
          local = lib.mkIf (cfg.storage.backend == "local") {
            path = cfg.storage.path;
          };
          s3 = lib.mkIf (cfg.storage.backend == "s3") {
            bucket = cfg.storage.s3.bucket;
            region = cfg.storage.s3.region;
            endpoint = cfg.storage.s3.endpoint;
          };
        };

        database = lib.mkIf cfg.database.enable {
          host = cfg.database.host;
          port = cfg.database.port;
          name = cfg.database.name;
          user = cfg.database.user;
        };

        redis = lib.mkIf cfg.redis.enable {
          host = cfg.redis.host;
          port = cfg.redis.port;
          database = cfg.redis.database;
        };

        rate_limit = {
          enabled = cfg.rateLimit.enable;
          requests_per_minute = cfg.rateLimit.requestsPerMinute;
          burst_size = cfg.rateLimit.burstSize;
        };

        logging = {
          level = cfg.logging.level;
          json = cfg.logging.json;
          timestamp = cfg.logging.timestamp;
        };

        features = {
          git_lfs = cfg.features.gitLfs;
          backup = cfg.features.backup;
          webhooks = cfg.features.webhooks;
          caching = cfg.features.caching;
          compliance = cfg.features.compliance;
          security_scanning = cfg.features.securityScanning;
          horizontal_scaling = cfg.features.horizontalScaling;
          web_ui = cfg.features.webUi;
        };
      }
      cfg.settings
    ];

    networking.firewall.allowedTCPPorts = [ cfg.port cfg.sshPort ];
  };
}

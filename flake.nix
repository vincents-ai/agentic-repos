{
  description = "repos-mcp - Multi-tenant Git server with MCP integration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
            git
          ];

          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        repos-mcp-uncompressed = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          doCheck = false;
        });

        repos-mcp = pkgs.stdenv.mkDerivation {
          name = "repos-mcp-0.1.0";
          src = repos-mcp-uncompressed;
          
          nativeBuildInputs = [ pkgs.upx ];
          
          buildPhase = ''
            cp ${repos-mcp-uncompressed}/bin/repos-mcp repos-mcp-tmp
            chmod +w repos-mcp-tmp
            upx --best --lzma repos-mcp-tmp -o repos-mcp-compressed
            mkdir -p $out/bin
            cp repos-mcp-compressed $out/bin/repos-mcp
            chmod +x $out/bin/repos-mcp
            if [ -d ${repos-mcp-uncompressed}/share ]; then
              cp -r ${repos-mcp-uncompressed}/share $out/
            fi
          '';
          
          installPhase = "true";
        };

        containerImage = pkgs.dockerTools.buildLayeredImage {
          name = "repos-mcp";
          tag = "latest";
          
          contents = with pkgs; [
            repos-mcp
            bashInteractive
            coreutils
            git
            curl
            cacert
          ];
          
          config = {
            Cmd = [ "${repos-mcp}/bin/repos-mcp" "--config" "/app/config/config.yaml" ];
            ExposedPorts = {
              "2222/tcp" = {};
              "8090/tcp" = {};
            };
            WorkingDir = "/app";
            Env = [
              "PATH=${pkgs.lib.makeBinPath [ repos-mcp pkgs.git pkgs.curl pkgs.coreutils ]}"
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
            User = "1000:1000";
          };
        };

        # Make Garage available for S3-compatible storage tests
        garage = pkgs.garage;

        vmTests = import ./nix/vm-tests.nix { inherit pkgs system repos-mcp garage; };
        
        integrationTests = import ./nix/tests/default.nix { inherit pkgs system repos-mcp garage; };

      in
      {
        packages = {
          default = repos-mcp;
          inherit repos-mcp containerImage;
          dockerImage = containerImage;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            git
            cargo-audit
            cargo-license
            cargo-tarpaulin
          ];
          
          shellHook = ''
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
            export OPENSSL_DIR="${pkgs.openssl.dev}"
            export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
            export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
          '';
        };

        checks = {
          build = repos-mcp;
          container = containerImage;
        }
        // vmTests.auth
        // vmTests.git-protocol
        // vmTests.storage
        // vmTests.multi-tenancy
        // vmTests.performance
        // vmTests.network
        // vmTests.lfs
        // vmTests."error-handling"
        // vmTests.security
        // vmTests."edge-cases"
        // integrationTests.checks
        // integrationTests.tests;
      }
    )
    // {
      nixosModules = {
        repos-mcp = import ./nix/modules/repos-mcp.nix;
        default = self.nixosModules.repos-mcp;
      };

      nixosModule = self.nixosModules.default;

      overlays.default = final: prev: {
        repos-mcp = self.packages.${final.system}.default;
      };
    };
}

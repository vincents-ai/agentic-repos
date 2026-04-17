{
  description = "agentic-repos - Multi-tenant Git server with MCP integration";

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

        agentic-repos-uncompressed = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          doCheck = false;
        });

        agentic-repos = pkgs.stdenv.mkDerivation {
          name = "agentic-repos-0.1.0";
          src = agentic-repos-uncompressed;
          
          nativeBuildInputs = [ pkgs.upx ];
          
          buildPhase = ''
            cp ${agentic-repos-uncompressed}/bin/agentic-repos agentic-repos-tmp
            chmod +w agentic-repos-tmp
            upx --best --lzma agentic-repos-tmp -o agentic-repos-compressed
            mkdir -p $out/bin
            cp agentic-repos-compressed $out/bin/agentic-repos
            chmod +x $out/bin/agentic-repos
            if [ -d ${agentic-repos-uncompressed}/share ]; then
              cp -r ${agentic-repos-uncompressed}/share $out/
            fi
          '';
          
          installPhase = "true";
        };

        containerImage = pkgs.dockerTools.buildLayeredImage {
          name = "agentic-repos";
          tag = "latest";
          
          contents = with pkgs; [
            agentic-repos
            bashInteractive
            coreutils
            git
            curl
            cacert
          ];
          
          config = {
            Cmd = [ "${agentic-repos}/bin/agentic-repos" "--config" "/app/config/config.yaml" ];
            ExposedPorts = {
              "2222/tcp" = {};
              "8090/tcp" = {};
            };
            WorkingDir = "/app";
            Env = [
              "PATH=${pkgs.lib.makeBinPath [ agentic-repos pkgs.git pkgs.curl pkgs.coreutils ]}"
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
            User = "1000:1000";
          };
        };

        # Make Garage available for S3-compatible storage tests
        garage = pkgs.garage;

        vmTests = import ./nix/vm-tests.nix { inherit pkgs system agentic-repos garage; };
        
        integrationTests = import ./nix/tests/default.nix { inherit pkgs system agentic-repos garage; };

      in
      {
        packages = {
          default = agentic-repos;
          inherit agentic-repos containerImage;
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
          build = agentic-repos;
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
        agentic-repos = import ./nix/modules/agentic-repos.nix;
        default = self.nixosModules.agentic-repos;
      };

      nixosModule = self.nixosModules.default;

      overlays.default = final: prev: {
        agentic-repos = self.packages.${final.system}.default;
      };
    };
}

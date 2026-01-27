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

        # Configure crane to use our custom rust toolchain
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Common arguments for crane derivations
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

          # Environment variables for OpenSSL
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
        };

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix or CI) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the actual crate itself, reusing the dependency artifacts
        repos-mcp-uncompressed = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          doCheck = false;  # Skip tests during Nix build (tests require PostgreSQL)
        });

        # UPX-compressed version of the binary
        repos-mcp = pkgs.stdenv.mkDerivation {
          name = "repos-mcp-0.1.0";
          src = repos-mcp-uncompressed;
          
          nativeBuildInputs = [ pkgs.upx ];
          
          buildPhase = ''
            # Copy binary to temporary location and make writable
            cp ${repos-mcp-uncompressed}/bin/repos-mcp repos-mcp-tmp
            chmod +w repos-mcp-tmp
            
            # Compress with UPX to temporary output
            upx --best --lzma repos-mcp-tmp -o repos-mcp-compressed
            
            # Create output directory and install compressed binary
            mkdir -p $out/bin
            cp repos-mcp-compressed $out/bin/repos-mcp
            chmod +x $out/bin/repos-mcp
            
            # Copy other files from uncompressed build (if any)
            if [ -d ${repos-mcp-uncompressed}/share ]; then
              cp -r ${repos-mcp-uncompressed}/share $out/
            fi
          '';
          
          installPhase = "true";  # Already installed in buildPhase
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

      in
      {
        packages = {
          default = repos-mcp;
          inherit repos-mcp containerImage;
          dockerImage = containerImage;  # Alias for CI compatibility
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
        # Import and add all VM test categories to checks
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; }).auth
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; }).git-protocol
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; }).storage
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; }).multi-tenancy
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; }).performance
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; }).network
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; }).lfs
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; })."error-handling"
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; }).security
        // (import ./nix/vm-tests.nix { inherit pkgs system repos-mcp; })."edge-cases";
      }
    );
}

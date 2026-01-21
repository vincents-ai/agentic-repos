{
  description = "repos-mcp - Multi-tenant Git server with MCP integration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # Build just the binary with Nix
        repos-mcp-binary = pkgs.writeShellScriptBin "repos-mcp-builder" ''
          cd ${./.}
          export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"
          export OPENSSL_DIR="${pkgs.openssl.dev}"
          export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
          export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
          ${pkgs.cargo}/bin/cargo build --release
          cp target/release/repos-mcp $out/bin/
        '';

        # Create a proper Rust package build
        repos-mcp = pkgs.stdenv.mkDerivation {
          pname = "repos-mcp";
          version = "0.1.0";
          src = ./.;
          
          nativeBuildInputs = with pkgs; [
            rustToolchain
            pkg-config
          ];
          
          buildInputs = with pkgs; [
            openssl
            git
          ];
          
          buildPhase = ''
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"
            export OPENSSL_DIR="${pkgs.openssl.dev}"
            export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
            export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
            cargo build --release
          '';
          
          installPhase = ''
            mkdir -p $out/bin
            cp target/release/repos-mcp $out/bin/
          '';
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
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            git
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
        };
      }
    );
}

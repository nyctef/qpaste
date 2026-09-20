{
  description = "qpaste - a quick-and-dirty paste service";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self, nixpkgs, flake-utils }:
    (flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "qpaste";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rust-analyzer
            clippy
            rustfmt
          ];
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };
      }
    ))
    // {
      nixosModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.services.qpaste;
        in
        with lib;
        {
          options.services.qpaste = {
            enable = mkEnableOption "qpaste, a quick-and-dirty paste service";

            package = mkOption {
              type = types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              description = "The qpaste package to run.";
            };

            addr = mkOption {
              type = types.str;
              default = "127.0.0.1:3000";
              description = "Address (host:port) for qpaste to bind to.";
            };

            baseUrl = mkOption {
              type = types.nullOr types.str;
              default = null;
              example = "https://paste.example.com";
              description = ''
                Base URL handed back to uploaders. Set this if qpaste is behind
                a reverse proxy or bound to a non-loopback address, otherwise
                the returned links won't be reachable. Defaults to
                `http://''${addr}` if unset.
              '';
            };

            dataDir = mkOption {
              type = types.path;
              default = "/var/lib/qpaste";
              description = "Directory where uploaded pastes are stored.";
            };

            openFirewall = mkOption {
              type = types.bool;
              default = false;
              description = "Whether to open the firewall for the port in `addr`.";
            };
          };

          config = mkIf cfg.enable {
            systemd.services.qpaste = {
              description = "qpaste paste service";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];

              environment = {
                QPASTE_ADDR = cfg.addr;
                QPASTE_DATA_DIR = cfg.dataDir;
              }
              // optionalAttrs (cfg.baseUrl != null) {
                QPASTE_BASE_URL = cfg.baseUrl;
              };

              serviceConfig = {
                ExecStart = "${cfg.package}/bin/qpaste";
                DynamicUser = true;
                StateDirectory = "qpaste";
                ReadWritePaths = [ cfg.dataDir ];
                Restart = "on-failure";

                NoNewPrivileges = true;
                ProtectSystem = "strict";
                ProtectHome = true;
                PrivateTmp = true;
              };
            };

            networking.firewall.allowedTCPPorts = mkIf cfg.openFirewall [
              (toInt (lib.last (lib.splitString ":" cfg.addr)))
            ];
          };
        };
    };
}

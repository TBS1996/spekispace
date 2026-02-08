{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      systems = import inputs.systems;

      ######################################################################
      ## Home-Manager module (programs.speki)
      ######################################################################
      flake.homeManagerModules.speki = {
        lib,
        pkgs,
        config,
        ...
      }: let
        toml = pkgs.formats.toml {};
        cfg = config.programs.speki;
      in {
        options.programs.speki = {
          enable = lib.mkEnableOption "Speki app";
          package = lib.mkOption {
            type = lib.types.package;
            default = inputs.self.packages.${pkgs.system}.default;
            description = "Speki package to install.";
          };

          randomize = lib.mkOption {
            type = lib.types.nullOr lib.types.bool;
            default = null;
            description = "Shuffle/reorder behaviour (TOML: randomize).";
          };

          googleProjectId = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "TOML: google_project_id";
          };

          remoteGithubUsername = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "TOML: remote_github_username";
          };

          remoteGithubRepo = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "TOML: remote_github_repo";
          };

          storagePath = lib.mkOption {
            type = lib.types.nullOr lib.types.path;
            default = null;
            description = "TOML: storage_path";
          };

          recaller = lib.mkOption {
            type = lib.types.nullOr lib.types.str; # e.g. "fsrs"
            default = null;
            description = "TOML: recaller (string, e.g., \"fsrs\").";
          };

          backup = lib.mkOption {
            type = lib.types.submodule {
              options = {
                enable = lib.mkEnableOption "Emit [backup] table";
                username = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                };
                branch = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                };
                repo = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                };
                strategy = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                };
              };
            };
            default = {enable = false;};
            description = "Backup settings → [backup] table.";
          };

          # ---------- passthrough for arbitrary TOML ----------
          extraSettings = lib.mkOption {
            type = toml.type;
            default = {};
            description = "Arbitrary TOML keys merged into config.";
          };
        };

        config = lib.mkIf cfg.enable {
          home.packages = [cfg.package];

          # Only write non-null keys
          xdg.configFile."speki/config.toml".source = let
            base = lib.filterAttrs (_: v: v != null) {
              randomize = cfg.randomize;
              google_project_id = cfg.googleProjectId;
              remote_github_username = cfg.remoteGithubUsername;
              remote_github_repo = cfg.remoteGithubRepo;
              storage_path =
                if cfg.storagePath != null
                then toString cfg.storagePath
                else null;
              recaller = cfg.recaller;
            };

            backupTable = lib.optionalAttrs cfg.backup.enable {
              backup = lib.filterAttrs (_: v: v != null) {
                username = cfg.backup.username;
                branch = cfg.backup.branch;
                repo = cfg.backup.repo;
                strategy = cfg.backup.strategy;
              };
            };

            merged = lib.recursiveUpdate (lib.recursiveUpdate base backupTable) cfg.extraSettings;
          in
            toml.generate "speki-config.toml" merged;
        };
      };

      ######################################################################
      ## perSystem: package + devShells
      ######################################################################
      perSystem = {
        config,
        self',
        pkgs,
        lib,
        system,
        ...
      }: let
        # Use oxalica rust overlay
        _pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [inputs.rust-overlay.overlays.default];
        };

        rustToolchain = _pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rust-src" "rust-analyzer" "clippy"];
          targets = ["wasm32-unknown-unknown"];
        };

        nightlyToolchain = _pkgs.rust-bin.nightly.latest.default.override {
          extensions = ["rust-src" "rust-analyzer" "clippy"];
        };

        rustBuildInputs =
          [
            _pkgs.openssl
            _pkgs.libiconv
            _pkgs.pkg-config
          ]
          ++ lib.optionals _pkgs.stdenv.isLinux [
            _pkgs.glib
            _pkgs.gtk3
            _pkgs.libsoup_3
            _pkgs.webkitgtk_4_1
            _pkgs.xdotool
            _pkgs.mesa
            _pkgs.libGL
            _pkgs.egl-wayland
            _pkgs.alsa-lib
          ]
          ++ lib.optionals _pkgs.stdenv.isDarwin (with _pkgs.darwin.apple_sdk.frameworks; [
            IOKit
            Carbon
            WebKit
            Security
            Cocoa
          ]);
      in {
        # Make this overridable via flake-parts pkgs if you want; this keeps your overlay.
        _module.args.pkgs = _pkgs;

        packages.default = _pkgs.rustPlatform.buildRustPackage {
          pname = "speki";
          version = "0.1.0";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [_pkgs.pkg-config];
          buildInputs = rustBuildInputs;

          # Adjust if your binary name differs (e.g. "speki-app")
          meta = {
            description = "Ontological flashcard app";
            license = _pkgs.lib.licenses.mit;
            maintainers = with _pkgs.lib.maintainers; [];
            platforms = _pkgs.lib.platforms.all;
            mainProgram = "speki";
          };
        };

        devShells.default = _pkgs.mkShell {
          name = "dioxus-dev";
          buildInputs =
            rustBuildInputs
            ++ [
              _pkgs.wasm-pack
              _pkgs.wasm-bindgen-cli
              rustToolchain
              _pkgs.nodejs_20
              _pkgs.tailwindcss
              _pkgs.graphviz
              _pkgs.google-cloud-sdk
            ];
          nativeBuildInputs = [rustToolchain];
          shellHook = ''
            if ! command -v dx >/dev/null; then
              echo "Installing dioxus-cli with cargo..."
              cargo install dioxus-cli
            fi
            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library";
            echo "Node.js and TailwindCSS are now available!"
          '';
        };

        devShells.udeps = _pkgs.mkShell {
          name = "udeps-dev";
          buildInputs = rustBuildInputs ++ [nightlyToolchain _pkgs.cargo-udeps];
          shellHook = ''
            export CARGO_BIN="${nightlyToolchain}/bin"
            export PATH="$CARGO_BIN:$PATH"
            echo "Using nightly toolchain with cargo-udeps"
            echo "You can now run: cargo udeps"
          '';
        };
      };
    };
}

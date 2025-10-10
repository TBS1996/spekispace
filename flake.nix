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

      flake.homeManagerModules.speki = { lib, config, ... }:
      let
        cfg = config.programs.speki;
      in {
        options.programs.speki.testFlag = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "A simple test flag for Speki.";
        };

        config.home.file.".config/speki/nix_test".text =
          if cfg.testFlag then
            "Speki test flag is ENABLED ✅"
          else
            "Speki test flag is DISABLED ❌";
      };

      perSystem = {
        config,
        self',
        pkgs,
        lib,
        system,
        ...
      }: let
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
          ];
          targets = ["wasm32-unknown-unknown"];
        };

        nightlyToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = ["rust-src" "rust-analyzer" "clippy"];
        };

        rustBuildInputs =
          [
            pkgs.openssl
            pkgs.libiconv
            pkgs.pkg-config
          ]
          ++ lib.optionals pkgs.stdenv.isLinux [
            pkgs.glib
            pkgs.gtk3
            pkgs.libsoup_3
            pkgs.webkitgtk_4_1
            pkgs.xdotool

            pkgs.mesa
            pkgs.libGL
            pkgs.egl-wayland
          ]
          ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
            IOKit
            Carbon
            WebKit
            Security
            Cocoa
          ]);
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            inputs.rust-overlay.overlays.default
          ];
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "speki";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [pkgs.pkg-config];
          buildInputs = rustBuildInputs;

          meta = {
            description = "ontological flashcard app";
            license = pkgs.lib.licenses.mit;
            maintainers = with pkgs.lib.maintainers; [];
            platforms = pkgs.lib.platforms.all;
          };
        };

        devShells.udeps = pkgs.mkShell {
          name = "udeps-dev";

          buildInputs =
            rustBuildInputs
            ++ [
              nightlyToolchain
              pkgs.cargo-udeps
            ];

          shellHook = ''
            # Absolute path to nightly cargo binary
            export CARGO_BIN="${nightlyToolchain}/bin"

            # Optional: make it feel like cargo is just available
            export PATH="$CARGO_BIN:$PATH"

            echo "Using nightly toolchain with cargo-udeps"
            echo "You can now run: cargo udeps"
          '';
        };

        devShells.default = pkgs.mkShell {
          name = "dioxus-dev";
          buildInputs =
            rustBuildInputs
            ++ [
              pkgs.wasm-pack
              pkgs.wasm-bindgen-cli
              rustToolchain
              pkgs.nodejs_20
              pkgs.tailwindcss
              pkgs.graphviz
            ];

          nativeBuildInputs = [
            rustToolchain
          ];

          shellHook = ''
            if ! command -v dx >/dev/null; then
              echo "Installing dioxus-cli with cargo..."
              cargo install dioxus-cli
            fi

            # For rust-analyzer 'hover' tooltips to work.
            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library";
            echo "Node.js and TailwindCSS are now available!"
          '';
        };
      };
    };
}

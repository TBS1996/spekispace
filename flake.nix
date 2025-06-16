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

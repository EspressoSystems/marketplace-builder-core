{
  description = "Generic builder core for HotShot applications";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  inputs.flake-utils.url = "github:numtide/flake-utils";

  inputs.flake-compat.url = "github:edolstra/flake-compat";
  inputs.flake-compat.flake = false;

  inputs.rust-overlay.url = "github:oxalica/rust-overlay";

  inputs.pre-commit-hooks.url = "github:cachix/pre-commit-hooks.nix";
  inputs.pre-commit-hooks.inputs.nixpkgs.follows = "nixpkgs";

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      pre-commit-hooks,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.minimal.override {
          extensions = [
            "rustfmt"
            "clippy"
            "llvm-tools-preview"
            "rust-src"
          ];
        };
        rustDeps =
          with pkgs;
          [
            pkg-config
            openssl
            bash

            curl

            cargo-audit
            cargo-edit
            cargo-udeps
            cargo-sort
            cargo-nextest

            cmake
          ]
          ++ lib.optionals stdenv.isDarwin [
            darwin.apple_sdk.frameworks.Security
            darwin.apple_sdk.frameworks.CoreFoundation
            darwin.apple_sdk.frameworks.SystemConfiguration

            # https://github.com/NixOS/nixpkgs/issues/126182
            libiconv
          ];
        # nixWithFlakes allows pre v2.4 nix installations to use
        # flake commands (like `nix flake update`)
        nixWithFlakes = pkgs.writeShellScriptBin "nix" ''
          exec ${pkgs.nixFlakes}/bin/nix --experimental-features "nix-command flakes" "$@"
        '';

        shellHook = ''
          # Prevent cargo aliases from using programs in `~/.cargo` to avoid conflicts with rustup
          # installations.
          export CARGO_HOME=$HOME/.cargo-nix
          export PATH="$PWD/$CARGO_TARGET_DIR/release:$PATH"
        '';

        RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        RUST_BACKTRACE = 1;
        RUST_LOG = "info";
        RUSTFLAGS = " --cfg async_executor_impl=\"async-std\" --cfg async_channel_impl=\"async-std\" --cfg hotshot_example";
        RUSTDOCFLAGS = " --cfg async_executor_impl=\"async-std\" --cfg async_channel_impl=\"async-std\" --cfg hotshot_example";
      in
      {
        formatter = pkgs.nixfmt-rfc-style;

        checks = {
          pre-commit-check = pre-commit-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              nixfmt-rfc-style.enable = true;
              cargo-fmt = {
                enable = true;
                description = "Enforce rustfmt";
                entry = "cargo fmt --all -- --check";
                pass_filenames = false;
              };
              cargo-sort = {
                enable = true;
                description = "Ensure Cargo.toml are sorted";
                entry = "cargo sort -g -w";
                pass_filenames = false;
              };
              cargo-clippy = {
                enable = true;
                description = "Run clippy";
                entry = "cargo clippy --workspace --all-features --all-targets --tests -- -D clippy::dbg-macro";
                pass_filenames = false;
              };
              cargo-docs = {
                enable = true;
                description = "Run rustdoc";
                entry = "cargo doc --workspace --document-private-items --no-deps";
                pass_filenames = false;
              };
            };
          };
        };

        devShells = {
          default = pkgs.mkShell {
            shellHook =
              shellHook
              # install pre-commit hooks
              + self.checks.${system}.pre-commit-check.shellHook;
            buildInputs =
              with pkgs;
              [
                rust-bin.nightly.latest.rust-analyzer
                nixWithFlakes
                nixpkgs-fmt
                git
                mdbook # make-doc, documentation generation
                rustToolchain
              ]
              ++ rustDeps;

            inherit
              RUST_SRC_PATH
              RUST_BACKTRACE
              RUST_LOG
              RUSTFLAGS
              RUSTDOCFLAGS
              ;
          };
          perfShell = pkgs.mkShell {
            inherit shellHook;
            buildInputs = [
              nixWithFlakes
              (pkgs.cargo-llvm-cov.overrideAttrs (_: {
                # The package is marked as broken on Darwin because nixpkgs'
                # rustc comes without profiling on Darwin, but rustc fro
                # our toolchain does have profiling enabled, so we can just
                # set it as non-broken and disable tests, which would be run
                # with nixpkgs' rustc
                meta.broken = false;
                doCheck = false;
              }))
              rustToolchain
            ] ++ rustDeps;

            inherit
              RUST_SRC_PATH
              RUST_BACKTRACE
              RUST_LOG
              RUSTFLAGS
              RUSTDOCFLAGS
              ;
          };
        };
      }
    );
}

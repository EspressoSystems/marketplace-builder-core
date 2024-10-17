{
  description = "Generic builder core for HotShot applications";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };

    rust-overlay.url = "github:oxalica/rust-overlay";

    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      git-hooks,
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
        cargo-llvm-cov =
          if pkgs.stdenv.isDarwin then
            (pkgs.cargo-llvm-cov.overrideAttrs (_: {
              # The package is marked as broken on Darwin because nixpkgs'
              # rustc comes without profiling on Darwin, but rustc fro
              # our toolchain does have profiling enabled, so we can just
              # set it as non-broken and disable tests, which would be run
              # with nixpkgs' rustc
              meta.broken = false;
              doCheck = false;
            }))
          else
            pkgs.cargo-llvm-cov;
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
          ];

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
        RUST_MIN_STACK = 64000000;
      in
      {
        formatter = pkgs.nixfmt-rfc-style;

        checks = {
          pre-commit-check = git-hooks.lib.${system}.run {
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
              rustToolchain
              cargo-llvm-cov
            ] ++ rustDeps;

            inherit
              RUSTDOCFLAGS
              RUSTFLAGS
              RUST_BACKTRACE
              RUST_LOG
              RUST_MIN_STACK
              RUST_SRC_PATH
              ;
          };
        };
      }
    );
}

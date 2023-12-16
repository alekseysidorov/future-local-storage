{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };

    treefmt-nix.url = "github:numtide/treefmt-nix";
    flake-root.url = "github:srid/flake-root";
  };

  outputs = inputs@{ flake-parts, nixpkgs, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.treefmt-nix.flakeModule
        inputs.flake-root.flakeModule
      ];

      systems = nixpkgs.lib.systems.flakeExposed;

      flake = { };

      perSystem = { config, self', inputs', system, nixpkgs, pkgs, ... }: {
        # Setup nixpkgs with overlays.
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            inputs.rust-overlay.overlays.default
            (final: prev: {
              rustToolchains = {
                stable = pkgs.rust-bin.stable.latest.default.override {
                  extensions = [
                    "rust-src"
                    "rust-analyzer"
                  ];
                };
                nightly = pkgs.rust-bin.nightly.latest.complete;
              };
            })
          ];
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustToolchains.stable

            (writeShellApplication {
              name = "ci-run-tests";
              runtimeInputs = [
                rustToolchains.stable
              ];
              text = ''cargo test --all-features --all-targets'';
            })
            (writeShellApplication {
              name = "ci-run-lints";
              runtimeInputs = [
                rustToolchains.stable
              ];
              text = ''cargo clippy --all-features --all --all-targets'';
            })
            (writeShellApplication {
              name = "ci-run-miri";
              runtimeInputs = [
                rustToolchains.nightly
              ];
              text = ''cargo miri test --all-features --all --all-targets'';
            })
          ];
        };
        devShells.miri = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustToolchains.nightly
          ];
        };

        treefmt.config = {
          inherit (config.flake-root) projectRootFile;

          programs.nixpkgs-fmt.enable = true;
          programs.rustfmt = {
            enable = true;
            package = pkgs.rustToolchains.stable;
          };
          programs.beautysh.enable = true;
          programs.deno.enable = true;
          programs.taplo.enable = true;
        };

        formatter = config.treefmt.build.wrapper;
      };
    };
}

{
  description = "Init-once-per-future storage for Rust futures.";

  inputs = {
    # Nix and flake composition.
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    # Reusable Rust development tools and the rust-overlay capability.
    nix-devtools = {
      url = "github:alekseysidorov/nix-devtools";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-parts.follows = "flake-parts";
      inputs.treefmt-nix.follows = "treefmt-nix";
    };

    crane.url = "github:ipetkov/crane";
    rust-advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } (
      { ... }:
      let
        inherit (inputs.nixpkgs) lib;

        # Reuse nix-devtools' public overlay so its Rust toolchain capability
        # has one owner and is available to this flake's own package universe.
        defaultOverlay = inputs.nix-devtools.overlays.default;
      in
      {
        systems = lib.systems.flakeExposed;

        imports = [
          inputs.treefmt-nix.flakeModule
          inputs.nix-devtools.flakeModule
        ];

        perSystem =
          { system, ... }:
          let
            # Use one package universe, extended through the public overlay;
            # this keeps rust-bin and all check tooling on the same pkgs set.
            pkgs = inputs.nixpkgs.legacyPackages.${system}.extend defaultOverlay;

            # Keep the minimum supported compiler explicit and use a known
            # current stable compiler for normal development and packaging.
            rustVersions = {
              msrv = "1.93.0";
              stable = "1.98.0";
            };

            rustToolchains = {
              msrv = pkgs.rust-bin.stable.${rustVersions.msrv}.default;
              stable = pkgs.rust-bin.stable.${rustVersions.stable}.default.override {
                extensions = [
                  "clippy"
                  "rust-src"
                  "rustfmt"
                ];
              };
            };

            craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchains.stable;
            # Use nix-devtools' project source so Cargo, README documentation,
            # and other non-ignored project files share the repository boundary.
            src = pkgs.projectSource {
              projectRoot = ./.;
            };

            # Keep dependency compilation separate so build, test and clippy
            # checks reuse the same Cargo artifacts.
            commonArgs = {
              inherit src;
              strictDeps = true;
              # trybuild diagnostics vary between isolated Cargo environments;
              # the cases still have to fail, while local Cargo checks compare snapshots.
              preCheck = "export TRYBUILD=overwrite";
            };

            cargoArtifacts = craneLib.buildDepsOnly commonArgs;

            package = craneLib.buildPackage (
              commonArgs
              // {
                inherit cargoArtifacts;
              }
            );

            # Keep semver compatibility as an explicit runnable check. It is
            # intentionally a package rather than a default flake check because
            # the registry baseline exists only after the crate is published.
            semverCheck = pkgs.writeNushellApplication {
              name = "check-cargo-semver";
              runtimeInputs = [
                # cargo-semver-checks requires rustc >= 1.93; keep this check
                # on MSRV while regular project checks use current stable.
                rustToolchains.msrv
                pkgs.cargo-semver-checks
              ];
              text = ''
                def main [...args: string] {
                  # Remove rustdoc artifacts from previous toolchain/check runs.
                  ^cargo clean
                  ^cargo semver-checks --workspace ...$args
                }
              '';
            };

            publishCheck = pkgs.writeNushellApplication {
              name = "check-cargo-publish";
              runtimeInputs = [ rustToolchains.stable ];
              text = ''
                def main [...args: string] {
                  ^cargo publish --dry-run --allow-dirty ...$args
                }
              '';
            };
          in
          {
            treefmt = {
              projectRootFile = "flake.nix";

              programs = {
                nixfmt.enable = true;
                rustfmt = {
                  enable = true;
                  package = rustToolchains.stable;
                };
                taplo.enable = true;
              };
            };

            packages = {
              default = package;
              check-cargo-semver = semverCheck;
              check-cargo-publish = publishCheck;
            };

            checks = {
              build = package;

              test = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });

              clippy = craneLib.cargoClippy (
                commonArgs
                // {
                  inherit cargoArtifacts;
                  cargoClippyExtraArgs = "--all-targets --all-features -- -D warnings";
                }
              );

              audit = craneLib.cargoAudit {
                inherit src;
                advisory-db = inputs.rust-advisory-db;
              };
            };

            devShells.default = pkgs.mkShell {
              packages = [
                rustToolchains.stable
                pkgs.cargo-audit
                pkgs.cargo-nextest
                pkgs.cargo-semver-checks
                pkgs.rust-analyzer
              ];
            };
          };
      }
    );
}

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

      perSystem = { config, self', inputs', system, nixpkgs, pkgs, ... }:
        let
          rustToolchain = pkgs.rust-bin.stable.latest.complete;
        in
        {
          # Setup nixpkgs with overlays.
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [
              inputs.rust-overlay.overlays.default
            ];
            config = { };
          };

          devShells.default = pkgs.mkShell {
            buildInputs = [
              rustToolchain
            ];
          };

          treefmt.config = {
            inherit (config.flake-root) projectRootFile;

            programs.nixpkgs-fmt.enable = true;
            programs.rustfmt = {
              enable = true;
              package = rustToolchain;
            };
            programs.beautysh.enable = true;
            programs.deno.enable = true;
            programs.taplo.enable = true;
          };

          formatter = config.treefmt.build.wrapper;
        };
    };
}
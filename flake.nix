{
  description = "marklens — markdown ⇄ data via one compact template DSL";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      treefmt-nix,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        treefmt = treefmt-nix.lib.evalModule pkgs ./treefmt.nix;
      in
      {
        # `nix fmt` — format Rust, Nix, and markdown in one pass.
        formatter = treefmt.config.build.wrapper;

        # `nix flake check` — fail if anything is unformatted or fails to lint.
        checks.formatting = treefmt.config.build.check self;

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            mdbook # build the docs site: `mdbook serve` / `mdbook build`
            cargo-audit # security advisories: `cargo audit`
            markdownlint-cli2 # lint the docs: `markdownlint-cli2`
            treefmt.config.build.wrapper # the same formatter `nix fmt` runs
          ];
        };
      }
    );
}

{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
      in
      rec {
        packages.default = packages.stasher;

        devShell = with pkgs; mkShell {
          buildInputs = [ cargo rustc rustfmt pre-commit rustPackages.clippy rust-analyzer sqlx-cli bacon];
          RUST_SRC_PATH = rustPlatform.rustLibSrc;
        };

        packages.stasher = naersk-lib.buildPackage {
          src = ./.;
          pname = "stasher";
        };
      }
    );
}

{
    description = "Zellij, a terminal workspace with batteries included";

    inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";

    flake-utils.url = "github:numtide/flake-utils";
    flake-utils.inputs.nixpkgs.follows = "nixpkgs";
};

  outputs = { self, rust-overlay, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
  let
    overlays = [ (import rust-overlay) ];

    pkgs = import nixpkgs {
      inherit system overlays;
    };

    # The root directory of this project
    ZELLIJ_ROOT = toString ./.;

    rustToolchainToml = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;

      buildInputs = [
      rustToolchainToml
      pkgs.cargo-make
      pkgs.rust-analyzer
      pkgs.mkdocs

      # in order to run tests
      pkgs.openssl
      pkgs.pkg-config
      pkgs.binaryen
    ];

    in rec {

      packages.zellij = (pkgs.makeRustPlatform {
        cargo = rustToolchainToml;
        rustc = rustToolchainToml;
      }).buildRustPackage {
        pname = "zellij";
        name = "zellij";

        src = pkgs.nix-gitignore.gitignoreSource  [ ".git" "target" "/*.nix" ] ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
        };

        outputs = [ "bin" "out" "man" "info" ];

        inherit buildInputs;
        nativeBuildInputs = buildInputs;
      };

      defaultPackage = packages.zellij;

      devShell = pkgs.mkShell {
        name = "zellij-dev";
        inherit buildInputs;
      };

    }
    );
}

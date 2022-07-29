{
  description = "Zellij, a terminal workspace with batteries included";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.flake-utils.follows = "flake-utils";
    flake-compat.url = "github:edolstra/flake-compat";
    flake-compat.flake = false;
    crate2nix.url = "github:kolloch/crate2nix";
    crate2nix.flake = false;
  };

  outputs = {...} @ args: import .nix/zellij.nix args;
}

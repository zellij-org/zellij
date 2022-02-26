{
  description = "Zellij, a terminal workspace with batteries included";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    flake-utils.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = { self, rust-overlay, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [
      "aarch64-linux"
      "aarch64-darwin"
      "i686-linux"
      "x86_64-darwin"
      "x86_64-linux"
    ]
      (system:
        let
          overlays = [ (import rust-overlay) ];

          pkgs = import nixpkgs {
            inherit system overlays;
          };

          name = "zellij";
          pname = name;
          root = toString ./.;

          ignoreSource = [ ".git" "target" ];

          src = pkgs.nix-gitignore.gitignoreSource ignoreSource root;

          rustToolchainToml = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;
          cargoLock = { lockFile = ./Cargo.lock; };
          cargo = rustToolchainToml;
          rustc = rustToolchainToml;

          buildInputs = [
            rustToolchainToml

            # in order to run tests
            pkgs.openssl
          ];

          nativeBuildInputs = [
            pkgs.installShellFiles
            pkgs.copyDesktopItems

            pkgs.cargo-make
            # for openssl/openssl-sys
            pkgs.pkg-config
            # generates manpages
            pkgs.mandown
            # optimizing of wasm binaries
            pkgs.binaryen
          ];

          devInputs = [
            pkgs.rust-analyzer
          ];

        in
        rec {

          packages.zellij = (pkgs.makeRustPlatform {
            inherit cargo rustc;
          }).buildRustPackage {
            inherit src name cargoLock buildInputs nativeBuildInputs;

            preCheck = ''
              HOME=$TMPDIR
            '';

            postInstall = ''

              # explicit behavior
              $out/bin/zellij setup --generate-completion bash > ./completions.bash
              installShellCompletion --bash --name ${pname}.bash ./completions.bash
              $out/bin/zellij setup --generate-completion fish > ./completions.fish
              installShellCompletion --fish --name ${pname}.fish ./completions.fish
              $out/bin/zellij setup --generate-completion zsh > ./completions.zsh
              installShellCompletion --zsh --name _${pname} ./completions.zsh

              install -Dm644  ./assets/logo.png $out/share/icons/hicolor/scalable/apps/zellij.png

              copyDesktopItems
            '';

            desktopItems = [
              (pkgs.makeDesktopItem {
                type = "Application";
                inherit name;
                desktopName = "zellij";
                terminal = true;
                genericName = "Terminal multiplexer";
                comment = "Manage your terminal applications";
                exec = "zellij";
                icon = "zellij";
                categories = "ConsoleOnly;System";
                extraEntries = "Keywords=terminal;";
              })
            ];

            meta = with pkgs.lib; {
              homepage = "https://github.com/zellij-org/zellij/";
              description = "A terminal workspace with batteries included";
              license = [ licenses.mit ];
            };
          };

          defaultPackage = packages.zellij;

          devShell = pkgs.mkShell {
            name = "zellij-dev";
            inherit buildInputs;
            nativeBuildInputs = nativeBuildInputs ++ devInputs;
          };

        }
      );
}

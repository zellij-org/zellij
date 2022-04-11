{
  self,
  nixpkgs,
  rust-overlay,
  flake-utils,
  flake-compat,
  crate2nix,
}:
flake-utils.lib.eachSystem [
  "aarch64-linux"
  "aarch64-darwin"
  "i686-linux"
  "x86_64-darwin"
  "x86_64-linux"
]
(system: let
  overlays = [(import rust-overlay)];

  pkgs = import nixpkgs {inherit system overlays;};
  pkgsMusl = import nixpkgs {
    inherit system overlays;
    crossSystem = {config = "x86_64-unknown-linux-musl";};
  };

  crate2nixPkgs = import nixpkgs {
    inherit system;
    overlays = [
      (self: _: {
        rustc = rustToolchainToml;
        cargo = rustToolchainToml;
      })
    ];
  };

  name = "zellij";
  pname = name;
  root = self;

  ignoreSource = [".git" "target" "example"];

  src = pkgs.nix-gitignore.gitignoreSource ignoreSource root;

  rustToolchainToml = pkgs.rust-bin.fromRustupToolchainFile ../rust-toolchain;
  cargoLock = {
    lockFile = builtins.path {
      path = ../Cargo.lock;
      name = "Cargo.lock";
    };
  };
  cargo = rustToolchainToml;
  rustc = rustToolchainToml;

  buildInputs = [
    # in order to run tests
    pkgs.openssl
  ];

  nativeBuildInputs = [
    rustToolchainToml
    # for openssl/openssl-sys
    pkgs.pkg-config

    # generates manpages
    pkgs.mandown

    pkgs.installShellFiles
    pkgs.copyDesktopItems
  ];

  devInputs = [
    pkgs.cargo-make
    pkgs.rust-analyzer

    # optimizes wasm binaries
    pkgs.binaryen

    # used for snapshotting the e2e tests
    pkgs.cargo-insta
  ];

  fmtInputs = [
    pkgs.alejandra
    pkgs.treefmt
  ];

  postInstall = ''
    mandown ./docs/MANPAGE.md > ./zellij.1
    installManPage ./zellij.1

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
      categories = ["ConsoleOnly"];
    })
  ];
  meta = with pkgs.lib; {
    homepage = "https://github.com/zellij-org/zellij/";
    description = "A terminal workspace with batteries included";
    license = [licenses.mit];
  };
in rec {
  # crate2nix - better incremental builds, but uses ifd
  packages.zellij = crate2nixPkgs.callPackage ./crate2nix.nix {
    inherit
      name
      src
      nativeBuildInputs
      crate2nix
      desktopItems
      postInstall
      meta
      ;
  };

  # native nixpkgs support - keep supported
  packages.zellij-native = (pkgs.makeRustPlatform {inherit cargo rustc;}).buildRustPackage {
    inherit
      src
      name
      cargoLock
      buildInputs
      nativeBuildInputs
      postInstall
      desktopItems
      meta
      ;
  };
  # musl cross-compilation - static binary
  packages.zellij-musl = (pkgsMusl.makeRustPlatform {inherit rustc cargo;}).buildRustPackage {
    inherit
      src
      name
      cargoLock
      postInstall
      buildInputs
      nativeBuildInputs
      desktopItems
      meta
      ;
  };

  defaultPackage = packages.zellij;

  # nix run
  apps.zellij = flake-utils.lib.mkApp {drv = packages.zellij;};
  defaultApp = apps.zellij;

  devShells = {
    zellij = pkgs.callPackage ./devShell.nix {
      inherit buildInputs;
      nativeBuildInputs = nativeBuildInputs ++ devInputs ++ fmtInputs;
    };
    fmtShell = pkgs.mkShell {
      name = "fmt-shell";
      nativeBuildInputs = fmtInputs;
    };
    e2eShell = pkgs.pkgsMusl.mkShell {
      name = "e2e-shell";
      nativeBuildInputs = [
        pkgs.cargo-make
        pkgs.pkgsMusl.cargo
      ];
    };
  };

  devShell = devShells.zellij;
})
// rec {
  overlays = {
    default = final: prev: rec {
      zellij = self.packages.${prev.system}.zellij;
    };
    nightly = final: prev: rec {
      zellij-nightly = self.packages.${prev.system}.zellij;
    };
  };
}

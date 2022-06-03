{
  pkgs,
  root,
  cargo,
  rustc,
  cargoLock,
  nativeBuildInputs,
  buildInputs,
}: let
  ignoreSource = [
    ".git"
    ".github"
    "assets"
    "docs"
    "example"
    "target"
    ".editorconfig"
    ".envrc"
    ".git-blame-ignore-revs"
    "CHANGELOG.md"
    "CODE_OF_CONDUCT.md"
    "CONTRIBUTING.md"
    "GOVERNANCE.md"
    "LICENSE.md"
    "docker-compose.yml"
  ];
  src = pkgs.nix-gitignore.gitignoreSource ignoreSource root;

  makeDefaultPlugin = name:
    (pkgs.makeRustPlatform {inherit cargo rustc;}).buildRustPackage {
      inherit
        src
        name
        cargoLock
        buildInputs
        nativeBuildInputs
        ;
      buildPhase = ''
        cargo build --package ${name} --release --target=wasm32-wasi
        mkdir -p $out/bin;
        #cp target/wasm32-wasi/release/${name}.wasm $out/bin/${name}.wasm
        wasm-opt \
        -O target/wasm32-wasi/release/${name}.wasm \
        -o $out/bin/${name}.wasm
      '';
      installPhase = ":";
      checkPhase = ":";
    };
in {
  status-bar = makeDefaultPlugin "status-bar";
  tab-bar = makeDefaultPlugin "tab-bar";
  strider = makeDefaultPlugin "strider";
  compact-bar = makeDefaultPlugin "compact-bar";
}

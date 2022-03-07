{ pkgs
, crate2nix
, name
, src
, postInstall
, desktopItems
, nativeBuildInputs
, buildInputs
, meta
}:

let
  inherit (import "${crate2nix}/tools.nix" { inherit pkgs; })
    generatedCargoNix;

  project = import
    (generatedCargoNix {
        inherit name src;
    })
    {
      inherit pkgs;
      buildRustCrateForPkgs = pkgs:
        pkgs.buildRustCrate.override {
          defaultCrateOverrides = pkgs.defaultCrateOverrides // {
            # Crate dependency overrides go here
            zellij = attrs : {
              inherit postInstall desktopItems meta buildInputs nativeBuildInputs;
            };
          };
        };
    };

in
project.workspaceMembers.zellij.build

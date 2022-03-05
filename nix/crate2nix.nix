{ pkgs
, crate2nix
, name
, src
, postInstall
, desktopItems
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
              inherit postInstall desktopItems meta;
            # Crate dependency overrides go here
          };
        };
    };

in
project.workspaceMembers.zellij.build

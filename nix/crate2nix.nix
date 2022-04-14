{
  pkgs,
  crate2nix,
  name,
  src,
  patchPhase,
  postInstall,
  nativeBuildInputs,
  desktopItems,
  meta,
}: let
  inherit
    (import "${crate2nix}/tools.nix" {inherit pkgs;})
    generatedCargoNix
    ;

  project =
    import
    (generatedCargoNix {
      inherit name src;
    })
    {
      inherit pkgs;
      buildRustCrateForPkgs = pkgs:
        pkgs.buildRustCrate.override {
          defaultCrateOverrides =
            pkgs.defaultCrateOverrides
            // {
              # Crate dependency overrides go here
              zellij = attrs: {
                inherit postInstall desktopItems meta name nativeBuildInputs patchPhase;
              };
            };
        };
    };
in
  project.workspaceMembers.zellij.build

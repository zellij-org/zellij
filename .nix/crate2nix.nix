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
  darwinBuildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
      pkgs.darwin.apple_sdk.frameworks.DiskArbitration
      pkgs.darwin.apple_sdk.frameworks.Foundation
  ];

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
                buildInputs = darwinBuildInputs;
              };
              sysinfo = attrs: {
                buildInputs = darwinBuildInputs;
              };
            };
        };
    };
in
  project.workspaceMembers.zellij.build

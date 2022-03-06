{ mkShell
, buildInputs
, nativeBuildInputs
}:

mkShell {
  name = "zj-dev-env";
  inherit buildInputs nativeBuildInputs;
  ### Environment Variables
  RUST_BACKTRACE = 1;
}

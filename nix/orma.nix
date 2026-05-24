{
  pkgs ? import <nixpkgs> { },
}:
let
  orma = pkgs.pkgsStatic.rustPlatform.buildRustPackage {
    pname = "orma";
    version = "0.1.0";

    src = ../crate;
    cargoLock.lockFile = ../crate/Cargo.lock;

    nativeCheckInputs = [ pkgs.mkpasswd ];
    meta.mainProgram = "orma";
  };

  clippy = orma.overrideAttrs (previous: {
    pname = "${previous.pname}-clippy";
    nativeCheckInputs = previous.nativeCheckInputs ++ [ pkgs.clippy ];
    dontBuild = true;
    checkPhase = ''
      runHook preCheck
      cargo clippy --all-targets -- --deny warnings
      runHook postCheck
    '';
    installPhase = "touch $out";
  });

  fmt = orma.overrideAttrs (previous: {
    pname = "${previous.pname}-fmt";
    nativeCheckInputs = previous.nativeCheckInputs ++ [ pkgs.rustfmt ];
    dontBuild = true;
    checkPhase = ''
      runHook preCheck
      cargo fmt --check
      runHook postCheck
    '';
    installPhase = "touch $out";
  });

in
{
  inherit orma clippy fmt;
}

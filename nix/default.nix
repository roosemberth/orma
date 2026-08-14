{
  pkgs ? import <nixpkgs> { },
}:
pkgs.pkgsStatic.rustPlatform.buildRustPackage {
  pname = "orma";
  version = "0.1.0";

  src = ../crate;
  cargoLock.lockFile = ../crate/Cargo.lock;

  nativeCheckInputs = [ pkgs.mkpasswd ];
  meta.mainProgram = "orma";
}

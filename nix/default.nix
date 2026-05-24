{
  pkgs ? import <nixpkgs> { },
}:
pkgs.pkgsStatic.rustPlatform.buildRustPackage {
  pname = "orma";
  version = "0.1.0";

  src = ../crates/orma;
  cargoLock.lockFile = ../crates/orma/Cargo.lock;

  nativeCheckInputs = [ pkgs.mkpasswd ];

  meta.mainProgram = "orma";
}

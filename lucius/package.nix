{ pkgs ? import <nixpkgs> { } }:

pkgs.rustPlatform.buildRustPackage {
  pname = "lucius";
  version = "0.1.0";

  src = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      let
        rel = pkgs.lib.removePrefix ((toString ./.) + "/") (toString path);
      in
      rel == "Cargo.toml"
      || rel == "Cargo.lock"
      || pkgs.lib.hasPrefix "src/" rel
      || type == "directory";
  };

  cargoLock.lockFile = ./Cargo.lock;

  meta = {
    description = "Runtime for ides devshell services";
    mainProgram = "ides";
  };
}

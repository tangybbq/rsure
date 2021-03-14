# Shell configuration to build rsure.
{ pkgs ? import <unstable> {} }:
pkgs.mkShell {
  nativeBuildInputs = [
    pkgs.openssl.dev
    pkgs.pkgconfig
    pkgs.sqlite.dev

    pkgs.cargo
    pkgs.clippy
    pkgs.rustfmt
  ];
}

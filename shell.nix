# Shell configuration to build rsure.
{ pkgs ? import <nixos> {} }:
let
  lib = pkgs.lib;
  stdenv = pkgs.stdenv;

  # SCCS isn't particularly useful, but the file used by weave is
  # derived from what SCCS uses.  If this program is in the path, then
  # weave has additional tests that it can run.
  cssc = stdenv.mkDerivation rec {
    name = "cssc-1.4.1";

    src = pkgs.fetchurl {
      url = "mirror://gnu/cssc/CSSC-1.4.1.tar.gz";
      sha256 = "1vsisqq573xjr2qpn19iwmpqgl3mq03m790akpa4rvj60b4d1gni";
    };

    meta = with lib; {
      homepage = "https://www.gnu.org/software/cssc/";
      description = "GNU replacement for SCCS";
      license = licenses.gpl3;
    };
  };
in
pkgs.mkShell {
  nativeBuildInputs = [
    pkgs.openssl.dev
    pkgs.pkgconfig
    pkgs.sqlite.dev

    # pkgs.cargo
    # pkgs.clippy
    # pkgs.rustfmt
    # pkgs.cargo-bloat

    cssc
  ];
}

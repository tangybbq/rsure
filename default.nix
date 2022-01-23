/* { stdenv, pkgs, fetchFromGitHub, rustPlatform }: */

with import <nixpkgs> {};
rustPlatform.buildRustPackage rec {
  pname = "rsure";
  version = "0.9.3";

  src = fetchFromGitHub {
    owner = "tangybbq";
    repo = pname;
    rev = "v0.9.3";
    sha256 = "0crmds1qqmnx1pkfib2gj9l51g7iw2vy6p3z2jffgcc5s26fv5mb";
  };

  cargoSha256 = "0mnljanhf530zlf5x8l93rm2pzssjfgwkm741wih10h4d4admdmr";

  nativeBuildInputs = [
    pkgs.pkgconfig
  ];
  buildInputs = [ pkgs.openssl.dev pkgs.sqlite.dev ];

  meta = with stdenv.lib; {
    description = "A utility for ensuring file integrity";
    homepage = "https://github.com/tangybbq/rsure";
    license = with licenses; [ mit ];
    maintainers = with maintainers; [ d3zd3z ];
  };
}

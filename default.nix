/* { stdenv, pkgs, fetchFromGitHub, rustPlatform }: */

with import <nixpkgs> {};
rustPlatform.buildRustPackage rec {
  pname = "rsure";
  version = "0.9.1";

  src = fetchFromGitHub {
    owner = "tangybbq";
    repo = pname;
    rev = "728bbe7e7d9e385d7ba33ea3f43bf5d60d50b0f0";
    sha256 = "1ni9wk4qr42b5nzqik755n07334lj8by42gyg8gqnmgb812zazy2";
  };

  cargoSha256 = "14hdcca8axnzy59qq224j1z99ygvdbbxwfywhciq08rhip7lnmn8";

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

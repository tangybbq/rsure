/* { stdenv, pkgs, fetchFromGitHub, rustPlatform }: */

with import <nixpkgs> {};
rustPlatform.buildRustPackage rec {
  pname = "rsure";
  version = "0.9.4";

  src = fetchFromGitHub {
    owner = "tangybbq";
    repo = pname;
    rev = "v0.9.4";
    sha256 = "sha256:0bx0l2q64ma057l2wwvsnbgl8jr6szanfwr5311lqqzvp4r4kaqy";
  };

  cargoSha256 = "sha256:1bym7z2b3sw9g2hvixagir4bqh0389v9f2r66x2nf871683vc34y";

  nativeBuildInputs = [
    pkgs.pkgconfig
  ];
  buildInputs = [ pkgs.openssl.dev pkgs.sqlite.dev ];

  meta = with lib; {
    description = "A utility for ensuring file integrity";
    homepage = "https://github.com/tangybbq/rsure";
    license = with licenses; [ mit ];
    maintainers = with maintainers; [ d3zd3z ];
  };
}

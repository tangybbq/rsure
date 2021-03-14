/* { stdenv, pkgs, fetchFromGitHub, rustPlatform }: */

with import <nixpkgs> {};
rustPlatform.buildRustPackage rec {
  pname = "rsure";
  version = "0.9.2";

  src = fetchFromGitHub {
    owner = "tangybbq";
    repo = pname;
    rev = "v0.9.2";
    sha256 = "0l7hjm5dbq2ylpqlaj360ci7598x9766d1av6lrwcq0lkc3f9jac";
  };

  cargoSha256 = "0wjchzhgnynpl6j33b2kd4h7f1qj0bp00fn5wxfag4nnf5dn7s01";

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

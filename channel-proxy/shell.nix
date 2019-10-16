with import <nixpkgs> {};

mkShell {
  buildInputs = [
    cargo
    openssl.dev
    rustfmt
  ];
}

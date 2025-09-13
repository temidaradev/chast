{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc
    cargo
    pkg-config
    gtk4
  ];

  shellHook = ''
    export PKG_CONFIG_PATH="${pkgs.gtk4.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
  '';
}

{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = [
    pkgs.pkg-config
    pkgs.gobject-introspection
    pkgs.glib
    pkgs.gdk-pixbuf
    pkgs.gtk3
    pkgs.cairo
    pkgs.pango
    pkgs.atk
    pkgs.gdk-pixbuf
    pkgs.zlib
    pkgs.qt6.full
  ];
}

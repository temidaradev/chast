{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ] (
          system: f nixpkgs.legacyPackages.${system}
        );
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.cargo
            pkgs.rustc
            pkgs.rust-analyzer
            pkgs.rustfmt
            pkgs.clippy
          ];

          buildInputs = [
            pkgs.gobject-introspection
            pkgs.glib
            pkgs.gdk-pixbuf
            pkgs.gtk3
            pkgs.cairo
            pkgs.pango
            pkgs.atk
            pkgs.zlib
            pkgs.fontconfig
            pkgs.libxkbcommon
            pkgs.wayland
            pkgs.vulkan-loader
            pkgs.qt6.qtbase
          ];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.fontconfig
            pkgs.libxkbcommon
            pkgs.wayland
            pkgs.vulkan-loader
          ];
        };
      });
    };
}

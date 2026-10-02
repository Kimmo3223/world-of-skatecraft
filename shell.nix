{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell rec {
  nativeBuildInputs = with pkgs; [ rustup pkg-config clang ];
  buildInputs = with pkgs; [
    alsa-lib udev vulkan-loader libxkbcommon wayland
    libx11 libxcursor libxi libxrandr
  ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
}

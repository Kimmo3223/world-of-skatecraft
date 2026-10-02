# Build and run environment for NixOS (setup/setup.sh and play.sh enter it on their own).
{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell rec {
  nativeBuildInputs = with pkgs; [ rustup pkg-config clang git curl python3 ];
  buildInputs = with pkgs; [
    alsa-lib udev vulkan-loader libxkbcommon wayland
    libx11 libxcursor libxi libxrandr
    # pip-installed numpy wheels (the Skate 3 converter) link libstdc++ from the system path.
    stdenv.cc.cc.lib zlib
  ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
}

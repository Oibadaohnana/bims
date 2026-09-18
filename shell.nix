{ pkgs ? import <nixpkgs> { } }:

let
  # What winit and wgpu dlopen at runtime: the window system and the GPU
  # loader. None of it is linked at build time, so a `cargo build` outside
  # this shell still works — it is `cargo run` that wants LD_LIBRARY_PATH.
  runtimeLibs = with pkgs; [
    vulkan-loader
    libxkbcommon
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
  ];
in
pkgs.mkShell {
  name = "bims";
  packages = with pkgs; [
    rustc
    cargo
    rustfmt
    clippy
    pkg-config
  ];

  buildInputs = runtimeLibs;

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
}

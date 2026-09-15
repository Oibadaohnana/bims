{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  name = "bims";
  packages = with pkgs; [
    rustc      # ships wasm32-unknown-unknown std
    cargo
    lld        # nixpkgs rustc links wasm through the system lld
    rustfmt
    python3    # only used by ./serve.sh
  ];
}

#!/usr/bin/env bash
# Build the wasm modules and drop them next to the pages in web/.
#
# There are two cdylibs now: the room (`bims`) and the ship designer (`ship`).
# `cargo build` at the workspace root builds every member, so one command does
# both — but each one has to be *copied*, and a front end whose wasm was never
# copied is a page that fetches a 404 and shows nothing.
set -euo pipefail
cd "$(dirname "$0")"

build() {
  cargo build --release
  for name in bims ship; do
    cp "target/wasm32-unknown-unknown/release/$name.wasm" "web/$name.wasm"
    echo "built web/$name.wasm ($(wc -c < "web/$name.wasm") bytes)"
  done
}

# Use the pinned nix shell unless the toolchain is already on PATH.
if command -v cargo >/dev/null && command -v lld >/dev/null; then
  build
else
  export -f build
  exec nix-shell shell.nix --run build
fi

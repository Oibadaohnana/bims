#!/usr/bin/env bash
# Build the wasm module and drop it next to the page in web/.
set -euo pipefail
cd "$(dirname "$0")"

build() {
  cargo build --release
  cp target/wasm32-unknown-unknown/release/bims.wasm web/bims.wasm
  echo "built web/bims.wasm ($(wc -c < web/bims.wasm) bytes)"
}

# Use the pinned nix shell unless the toolchain is already on PATH.
if command -v cargo >/dev/null && command -v lld >/dev/null; then
  build
else
  export -f build
  exec nix-shell shell.nix --run build
fi

#!/usr/bin/env bash
# Build, then serve. A plain file:// open will not work: the page fetches
# bims.wasm, and fetch is blocked on file URLs.
#
# Arguments are passed through to dev-server.py — see `./serve.sh --help`.
set -euo pipefail
cd "$(dirname "$0")"

./build.sh

if command -v python3 >/dev/null; then
  exec python3 dev-server.py "$@"
else
  exec nix-shell -p python3 --run "python3 dev-server.py $*"
fi

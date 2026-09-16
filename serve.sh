#!/usr/bin/env bash
# `./run game` without the dispatcher, kept because it is in muscle memory.
# It used to serve the room on 8080; the room is `./run room` now, on 8084,
# and 8080 is the whole game. Arguments are passed through.
set -euo pipefail
cd "$(dirname "$0")"
exec ./run game "$@"

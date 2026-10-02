#!/usr/bin/env bash
# Starts the local server (if it is not running) and the game. Run setup/setup.sh first.
# In game: J hops on/off the skateboard (Xbox-style controller), K swaps the skate camera.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$ROOT"
if [ -e /etc/NIXOS ] && [ -z "${IN_NIX_SHELL:-}" ]; then
    exec nix-shell "$ROOT/shell.nix" --run "$(printf '%q ' "$0" "$@")"
fi
[ -x target/release/benilla ] || { echo "Not built yet: run setup/setup.sh first."; exit 1; }
docker compose -p "${SKATECRAFT_SERVER_PROJECT:-world-of-skatecraft}" --project-directory server -f server/compose.yaml up -d
export WOW_HOST=localhost
export WOW_SKATE_ASSETS="$ROOT/skate-data/assets"
export WOW_SKATE_AUDIO="$ROOT/skate-audio"
exec target/release/benilla "$@"

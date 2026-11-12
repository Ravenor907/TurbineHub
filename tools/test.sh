#!/bin/sh
# The engine's and helper's tests and clippy on the pinned toolchain, the same checks CI runs. On SteamOS the host
# has no C linker, so it runs in the "dev" distrobox when there is one.
set -eu
cd "$(dirname "$0")/.."
run='env -u XDG_CONFIG_HOME cargo +1.97.0 test -q && cargo +1.97.0 clippy -q --all-targets -- -D warnings && sh tools/house-rules.sh'
if command -v cc >/dev/null 2>&1; then sh -c "$run"; else distrobox enter dev -- sh -c "$run"; fi

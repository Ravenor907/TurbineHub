#!/bin/sh
# The engine's and helper's tests and clippy on the pinned toolchain, the house and release rules, the stand-in
# server and the quick panel's typecheck, tests and build: the same checks CI runs. On SteamOS the host has no C
# linker, so it runs in the "dev" distrobox when there is one.
set -eu
cd "$(dirname "$0")/.."
run='env -u XDG_CONFIG_HOME cargo +1.97.0 test -q && cargo +1.97.0 clippy -q --all-targets -- -D warnings && sh tools/house-rules.sh'
run="$run && python3 tools/release-check.py && python3 tools/stand-in.py --self-test"
run="$run && (cd decky && pnpm install --frozen-lockfile --silent && pnpm run -s typecheck && pnpm run -s test && pnpm run -s build)"
if command -v cc >/dev/null 2>&1; then sh -c "$run"; else distrobox enter dev -- sh -c "$run"; fi

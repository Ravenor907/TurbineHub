#!/bin/sh
# The whole picture run (tools/ci-shots.sh all) on every core at once: each size is cut into shards that run side by
# side, the way CI runs them on separate machines. Same checks, same pictures, a fraction of the time.
#   tools/shots-fast.sh [out dir, default shots] [shards per size, default a third of the cores]
# Each shard's own output is kept beside the pictures (<out>/run.<size>.<shard>.log). Fails when any shard fails.
set -u
cd "$(dirname "$0")/.."
out=${1:-shots}
cores=$(nproc 2> /dev/null || echo 4)
shards=${2:-$((cores / 3))}
[ "$shards" -lt 1 ] && shards=1
mkdir -p "$out" || exit 1
out=$(cd "$out" && pwd)
started=$(date +%s)
pids=""
for size in 1280x800 1920x1080 1600x900; do
  shard=0
  while [ "$shard" -lt "$shards" ]; do
    SHOTS_OUT="$out" sh tools/ci-shots.sh "$size" "$shard" "$shards" > "$out/run.$size.$shard.log" 2>&1 &
    pids="$pids $!"
    shard=$((shard + 1))
  done
done
bad=0
for pid in $pids; do
  wait "$pid" || bad=$((bad + 1))
done
# One line of totals from every shard's own last line ("Pictures: N taken ..., checks: A ok, B failed, ...").
sum() { cat "$out"/run.*.log | grep '^Pictures:' | grep -o "[0-9]* $1" | awk '{n += $1} END {print n + 0}'; }
echo "Pictures: $(cat "$out"/run.*.log | grep '^Pictures:' | sed 's/^Pictures: \([0-9]*\).*/\1/' | awk '{n += $1} END {print n + 0}') taken in $out with $((shards * 3)) runs side by side, checks: $(sum ok,) ok, $(sum failed) failed, $((($(date +%s) - started))) s"
grep -h '::error' "$out"/run.*.log | head -40
[ "$bad" -eq 0 ] || { echo "$bad of $((shards * 3)) runs failed: see $out/run.*.log"; exit 1; }

#!/bin/sh
# The repo's house rules, the same check CI runs: no em or en dashes and no attribution words anywhere tracked, and
# every commit is the owner's, with no trailer lines and none of those words in its message.
#   sh tools/house-rules.sh            the files, and the commits not on origin/main yet (the last one without it)
#   sh tools/house-rules.sh A..B       the files, and the commits in that range
set -u
cd "$(dirname "$0")/.."
fail=0
for d in '\342\200\224' '\342\200\223'; do
  hits=$(git grep -n -I -F "$(printf "$d")" -- . | head -n 20)
  if [ -n "$hits" ]; then echo "Dash found:"; echo "$hits"; fail=1; fi
done
# Kept encoded so this file doesn't contain the words either.
pattern=$(printf '%s' 'Y28tYXV0aG9yZWQtYnl8YW50aHJvcGljfGNsYXVkZXxjaGF0Z3B0fGNvcGlsb3Q=' | base64 -d)
hits=$(git grep -n -I -i -E "$pattern" -- . | head -n 20)
if [ -n "$hits" ]; then echo "Attribution word found:"; echo "$hits"; fail=1; fi

want=Ravenor907@users.noreply.github.com
if [ -n "${1:-}" ]; then
  range=$1
elif git rev-parse -q --verify origin/main >/dev/null 2>&1 && [ -n "$(git rev-list -n 1 origin/main..HEAD)" ]; then
  range=origin/main..HEAD
else
  range="HEAD -1"
fi
n=0
# $range is two words for a single commit ("HEAD -1").
for c in $(git rev-list $range); do
  n=$((n + 1))
  short=$(git log -1 --format=%h "$c")
  who=$(git log -1 --format='%ae %ce' "$c")
  if [ "$who" != "$want $want" ]; then echo "Commit $short: author or committer isn't $want ($who)"; fail=1; fi
  msg=$(git log -1 --format=%B "$c")
  if printf '%s\n' "$msg" | grep -q -i -E "$pattern"; then echo "Commit $short: attribution word in its message"; fail=1; fi
  if printf '%s\n' "$msg" | grep -q "$(printf '\342\200[\223\224]')"; then echo "Commit $short: dash in its message"; fail=1; fi
  # A trailer is a "Key: value" line in the last block of a message that has more than its first line.
  trailer=$(printf '%s\n' "$msg" | awk '
    NF { lines[++count] = $0; if (blank) start = count; blank = 0; next }
    { blank = 1 }
    END { if (count < 2) exit; if (start < 2) start = 2; for (i = start; i <= count; i++) if (lines[i] ~ /^[A-Za-z][A-Za-z-]*:/) print lines[i] }' | head -n 3)
  if [ -n "$trailer" ]; then echo "Commit $short: trailer line in its message:"; echo "$trailer"; fail=1; fi
done
[ $fail = 0 ] && echo "House rules: ok ($n commits checked)"
exit $fail

#!/bin/sh
# The repo's house rules, the same check CI runs: no em or en dashes and no attribution words anywhere tracked.
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
[ $fail = 0 ] && echo "House rules: ok"
exit $fail

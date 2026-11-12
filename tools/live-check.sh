#!/bin/sh
# Asks the real services Turbine Hub reads for the same things the app asks, and prints what came back, so the build
# log shows whether each still answers in the shape the engine reads. It only ever warns: a service being down, slow
# or changed must not fail a build whose checks all run against tools/stand-in.py.
set -u
ask() {   # ask <label> <url> <text the answer must contain>
  out=$(curl -sS -L --max-time 30 -w '\nHTTP %{http_code}' -A 'TurbineHub-build-check' "$2" 2>&1)
  code=$(printf '%s' "$out" | tail -n1)
  body=$(printf '%s' "$out" | sed '$d')
  case "$body" in
    *"$3"*) echo "::notice title=$1 answers::$code, $(printf '%s' "$body" | wc -c) bytes" ;;
    *) echo "::warning title=$1 didn't answer as expected::$code, wanted $3: $(printf '%s' "$body" | head -c 200 | tr '\n' ' ')" ;;
  esac
}
ask "Valve's rating" "https://store.steampowered.com/saleaction/ajaxgetdeckappcompatibilityreport?nAppID=1245620" '"resolved_category"'
ask "The anti-cheat list" "https://raw.githubusercontent.com/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json" '"storeIds"'
ask "The Decky plugin store" "https://plugins.deckbrew.xyz/plugins" '"name"'
ask "Flathub" "https://flathub.org/api/v2/appstream/com.heroicgameslauncher.hgl" '"name"'
ask "Turbine Hub's release feed" "https://github.com/Ravenor907/TurbineHub/releases.atom" '<feed'
exit 0

#!/bin/sh
# Tries the background helper from the installed Flatpak on made-up machines in the middle of a game. It runs once
# and prints what it would run instead of running it (turbinehub --agent --once --machine=<kind>), so nothing here
# touches this PC. Fails when it doesn't do what a game start asks for: Game Boost through the helper, a save
# snapshot, the lights, and on a docked handheld the sound outputs.
#   tools/agent-check.sh
set -u
APP=io.github.ravenor907.TurbineHub
# A build container has no session bus; a desktop has one already.
bus=""
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && command -v dbus-run-session > /dev/null; then bus="dbus-run-session --"; fi
fail=0
for kind in handheld desktop; do
  out=$(timeout 60 $bus flatpak run --command=turbinehub "$APP" --agent --once --machine=$kind < /dev/null 2>&1)
  code=$?
  need() {   # need <what it is> <text the output must have>
    if ! printf '%s\n' "$out" | grep -q -F -- "$2"; then
      echo "::error title=Background helper on a made-up $kind::$1 is missing"
      fail=1
    fi
  }
  if [ $code -ne 0 ]; then echo "::error title=Background helper on a made-up $kind::it ended with code $code"; fail=1; fi
  need "Game Boost through the helper" 'turbinehub-helper set-now {"values":[{"path":"/sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference","value":"performance"}'
  need "the save snapshot" 'would do: Snapshot { game: "steam:1145360"'
  need "the running game in its status" '"game":{"id":"steam:1145360","name":"Hades II"'
  if [ $kind = handheld ]; then
    need "the dock's look at the sound outputs" 'would do: Sinks'
    need "the lights" 'light: multicolor:rgb:joystick shows'
  fi
  # Whatever it would run on the host goes through the helper or flatpak-spawn, never sudo.
  if printf '%s\n' "$out" | grep -q -w sudo; then echo "::error title=Background helper on a made-up $kind::it would use sudo"; fail=1; fi
  [ $fail = 0 ] || printf '%s\n' "$out"
done
[ $fail = 0 ] && echo "Background helper: ok on a made-up handheld and desktop"
exit $fail

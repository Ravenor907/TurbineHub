#!/bin/sh
# A check of Turbine Hub's helper on a real machine, for the things a made-up machine can't show: the real kernel,
# the real udev, the real lock, and how long each job really takes. It makes small changes through the installed
# helper, looks at what the machine then says, and takes every one of them back. What was in the helper's record
# before it started is never touched: a setting the record already holds is skipped, never changed.
#   sh tools/live-helper-check.sh --yes-this-machine              run it
#   sh tools/live-helper-check.sh --yes-this-machine --save DIR   and keep the copies of the starting state in DIR
# It prints one line per check (ok, FAILED or skipped) and a count at the end, and ends with 1 when anything failed.
# It needs the helper already set up in Turbine Hub. It never asks for a password, never installs or updates the
# helper, never restarts or sleeps the machine, and never runs undo-all, uninstall, boot-next or decky-install.
# If a password box appears, cancel it: the helper isn't set up for this user.
# What it can't cover, and the checklist for that, is in docs/LIVE-CHECKS.md.
set -u

HELPER=/var/lib/turbinehub/bin/turbinehub-helper
STATE=/var/lib/turbinehub
LEDGER=$STATE/ledger.json
LOG=$STATE/helper.log
APP=io.github.ravenor907.TurbineHub
LIMIT=10       # seconds a helper job may take
APP_LIMIT=5    # seconds the app may take to answer a question
SYSCTL_FILE=/etc/sysctl.d/99-zz-turbinehub.conf
WAKE_FILE=/etc/udev/rules.d/71-turbinehub-wake.rules
ACCESS_FILE=/etc/udev/rules.d/70-turbinehub-access.rules
ZRAM_FILE=/etc/systemd/zram-generator.conf.d/turbinehub.conf
MODULES_FILE=/etc/modules-load.d/turbinehub.conf
UNIT_FILE=/etc/systemd/system/turbinehub-reapply.service
FILES="$SYSCTL_FILE $WAKE_FILE $ACCESS_FILE $ZRAM_FILE $MODULES_FILE $UNIT_FILE"

yes=0
save=
while [ $# -gt 0 ]; do
  case "$1" in
    --yes-this-machine) yes=1 ;;
    --save) save=${2:-}; shift ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown option $1. See: sh $0 --help"; exit 2 ;;
  esac
  shift
done
if ! grep -q '^ID=steamos$' /etc/os-release 2>/dev/null; then echo "This isn't SteamOS, so nothing was run."; exit 2; fi
if [ ! -x "$HELPER" ] || [ ! -r "$STATE/owner" ]; then echo "Turbine Hub's helper isn't set up on this machine (set it up in Turbine Hub first), so nothing was run."; exit 2; fi
if [ "$(id -u)" = 0 ]; then echo "Run this as the player, not as root: the helper is reached the way the app reaches it."; exit 2; fi
if [ "$(cat "$STATE/owner")" != "$(id -u)" ]; then echo "The helper on this machine belongs to another user, so nothing was run."; exit 2; fi
for tool in pkexec timeout date sed grep cmp stat; do
  command -v "$tool" >/dev/null 2>&1 || { echo "This needs $tool, which isn't here."; exit 2; }
done
if [ "$yes" != 1 ]; then
  echo "This changes real settings on this machine for a moment and puts each one back."
  echo "Run it again with --yes-this-machine to go ahead."
  exit 2
fi

T=$(mktemp -d "${XDG_RUNTIME_DIR:-/tmp}/turbinehub-live-check.XXXXXX") || exit 2
KEEP=${save:-$T/start}
mkdir -p "$KEEP" || exit 2
: > "$T/restore"
: > "$T/start-ids"
N_OK=0
N_FAIL=0
N_OLD=0
N_SKIP=0
OLD=0
DONE=0
RESTORED=0
RUNNER="pkexec --disable-internal-agent $HELPER"

# ---------------------------------------------------------------- the lines it prints
ok() { N_OK=$((N_OK + 1)); printf 'ok      %s\n' "$1"; }
bad() { N_FAIL=$((N_FAIL + 1)); printf 'FAILED  %s: %s\n' "$1" "$2"; }
skip() { N_SKIP=$((N_SKIP + 1)); printf 'skipped %s: %s\n' "$1" "$2"; }
# A check an older helper is known to fail: fixed in the source, on the machine once the helper is updated.
newer() {
  if [ "$OLD" = 1 ]; then
    N_OLD=$((N_OLD + 1))
    bad "$1" "$2 (fixed in a newer helper than the one installed here: update it in Turbine Hub, then run this again)"
  else
    bad "$1" "$2"
  fi
}
# is <what> <got> <wanted>
is() { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "wanted $3, found $2"; fi; }
# is_newer <what> <got> <wanted>: the same, for something an older helper gets wrong.
is_newer() { if [ "$2" = "$3" ]; then ok "$1"; else newer "$1" "wanted $3, found $2"; fi; }

# ---------------------------------------------------------------- running the helper
now() { date +%s%N; }

# call <slot> <job> [details]: runs one helper job the way the app does (pkexec, no password). Sets OUT (its
# answer), RC (its exit code, or "hung" when it didn't answer in $LIMIT seconds) and MS. The helper runs as root, so
# nothing here could stop a stuck one: the stopwatch stops waiting for it instead, and says so.
call() {
  slot=$1
  shift
  rm -f "$T/$slot.rc"
  s=$(now)
  # shellcheck disable=SC2086
  timeout "$LIMIT" sh -c 'rc=$1; shift; "$@"; echo $? > "$rc"' sh "$T/$slot.rc" $RUNNER "$@" > "$T/$slot.out" 2> "$T/$slot.err" < /dev/null
  t=$?
  MS=$((($(now) - s) / 1000000))
  if [ -r "$T/$slot.rc" ]; then RC=$(cat "$T/$slot.rc"); elif [ "$t" = 124 ]; then RC=hung; else RC=$t; fi
  OUT=$(cat "$T/$slot.out")
}

# job <what> <ok|refused> <job> [details]: one job and its line. A job that doesn't answer ends the run (after
# putting everything back): every job after it would only wait behind it.
job() {
  what=$1
  want=$2
  shift 2
  call main "$@"
  if [ "$RC" = hung ]; then
    bad "$what" "no answer in $LIMIT seconds"
    echo "A helper job is stuck, so the checks stop here."
    finish
  fi
  case "$want:$RC:$OUT" in
    ok:0:'{"ok":true'*) ok "$what ($MS ms)" ;;
    refused:1:'{"ok":false'*) ok "$what ($MS ms)" ;;
    *) bad "$what" "wanted $want, got exit $RC, $OUT $(head -c 200 "$T/main.err")"; return 1 ;;
  esac
}

# ---------------------------------------------------------------- reading the machine
# The value in a /sys file: the word in brackets for files like "always [madvise] never".
live() { sed -n '1{s/.*\[\(.*\)\].*/\1/;p;}' "$1" 2>/dev/null; }
ids() { sed -n 's/^{"id":\([0-9]*\),.*/\1/p' "$1" 2>/dev/null; }
entry() { grep "^{\"id\":$1," "$LEDGER" 2>/dev/null; }
# Is this text in the record as it was at the start? Then the setting is the owner's, and it is left alone.
recorded() { grep -qF -- "$1" "$KEEP/ledger.json" 2>/dev/null; }
# remember <path>: keeps a /sys value so it is put back whatever happens.
remember() { grep -q "^$1 " "$T/restore" || echo "$1 $(live "$1")" >> "$T/restore"; }
# waits up to three seconds for a /sys value (udev works after the helper has answered).
wait_live() {
  i=0
  while [ "$(live "$1")" != "$2" ] && [ $i -lt 30 ]; do sleep 0.1; i=$((i + 1)); done
  live "$1"
}
# How a file under /etc is now: its text, its permissions and its owner. "absent" when it isn't there.
etc_now() { if [ -e "$1" ]; then echo "$(cksum < "$1") $(stat -c '%a %U %G' "$1")"; else echo absent; fi; }
etc_start() { if [ -e "$KEEP$1" ]; then echo "$(cksum < "$KEEP$1") $(cat "$KEEP$1.mode")"; else echo absent; fi; }
etc_same() {
  for f in $FILES; do
    [ "$(etc_now "$f")" = "$(etc_start "$f")" ] || { bad "$1: $f is as it was at the start" "it differs"; return 1; }
  done
  ok "$1: every file under /etc is as it was at the start"
}

# ---------------------------------------------------------------- putting everything back
# Takes back every change that wasn't in the record at the start, and every /sys value that was set for a moment.
# Runs at the end, and also when a check fails half way or the run is stopped (Ctrl+C, a closed terminal).
restore() {
  i=0
  while pgrep -f "^$HELPER " > /dev/null 2>&1 && [ $i -lt 90 ]; do
    [ $i = 0 ] && echo "Waiting for a helper job that is still running..."
    sleep 1
    i=$((i + 1))
  done
  LIMIT=30
  for id in $(ids "$LEDGER"); do
    grep -qx "$id" "$T/start-ids" && continue
    call restore undo "{\"id\":$id}"
    echo "Put back: change $id taken out again, $OUT"
  done
  while read -r path value; do
    [ -n "$path" ] && [ "$(live "$path")" != "$value" ] || continue
    call restore set-now "{\"path\":\"$path\",\"value\":\"$value\"}"
    echo "Put back: $path set to $value again, $OUT"
  done < "$T/restore"
  LIMIT=10
  RESTORED=1
}

# The end: everything back, then the proof that the machine is as it was, then the count.
finish() {
  echo
  echo "== The starting state is back"
  restore
  sed '1s/"next":[0-9]*/"next":0/' "$KEEP/ledger.json" > "$T/a"
  sed '1s/"next":[0-9]*/"next":0/' "$LEDGER" > "$T/b"
  n=$(ids "$KEEP/ledger.json" | grep -c .)
  if cmp -s "$T/a" "$T/b"; then ok "the record holds the same $n changes, byte for byte"; else bad "the record holds the same $n changes, byte for byte" "it differs: compare $KEEP/ledger.json with $LEDGER"; fi
  if cmp -s "$KEEP/ledger.json" "$LEDGER"; then
    ok "the record's counter for the next change is the same too"
  else
    echo "note    the record's counter for the next change went from $(sed -n '1s/.*"next":\([0-9]*\).*/\1/p' "$KEEP/ledger.json") to $(sed -n '1s/.*"next":\([0-9]*\).*/\1/p' "$LEDGER"): a number is never used twice, so it only goes up"
  fi
  for f in $FILES; do
    if [ "$(etc_now "$f")" = "$(etc_start "$f")" ]; then
      if [ -e "$f" ]; then ok "$f is the same, byte for byte"; else ok "$f is still not there"; fi
    else
      bad "$f is as it was at the start" "it differs: compare with $KEEP$f"
    fi
  done
  while read -r path value; do
    [ -n "$path" ] && is "$path is $value again" "$(live "$path")" "$value"
  done < "$T/restore"
  while read -r key value; do
    [ -n "$key" ] && is "$key is $value again" "$(cat "/proc/sys/$(echo "$key" | tr . /)" 2>/dev/null)" "$value"
  done < "$KEEP/kernel.txt"
  call main verify
  is "the helper's own check finds what it found at the start" "$OUT" "$START_VERIFY"
  if pgrep -f "^$HELPER " > /dev/null 2>&1; then bad "no helper job is left running" "one still is"; else ok "no helper job is left running"; fi
  DONE=1
  echo
  if [ "$N_OLD" -gt 0 ]; then
    echo "$N_OK ok, $N_FAIL FAILED ($N_OLD of them fixed in a newer helper than the one installed here), $N_SKIP skipped"
  else
    echo "$N_OK ok, $N_FAIL FAILED, $N_SKIP skipped"
  fi
  [ -n "$save" ] && echo "The starting state is kept in $save"
  [ "$N_FAIL" = 0 ]
  exit $?
}

leave() {
  code=$?
  trap - EXIT INT TERM HUP
  if [ "$DONE" != 1 ] && [ "$RESTORED" != 1 ]; then
    echo
    echo "Stopped early: putting everything back."
    restore
    sed '1s/"next":[0-9]*/"next":0/' "$KEEP/ledger.json" > "$T/a"
    sed '1s/"next":[0-9]*/"next":0/' "$LEDGER" > "$T/b"
    if cmp -s "$T/a" "$T/b"; then echo "The record holds the same changes as at the start."; else echo "FAILED  the record differs from the start: compare $KEEP/ledger.json with $LEDGER"; fi
  fi
  rm -rf "$T"
  exit "$code"
}
trap leave EXIT
trap 'exit 130' INT TERM HUP

# ---------------------------------------------------------------- the starting state
echo "== The starting state"
cp "$LEDGER" "$KEEP/ledger.json" 2>/dev/null || : > "$KEEP/ledger.json"
ids "$KEEP/ledger.json" > "$T/start-ids"
for f in $FILES; do
  if [ -e "$f" ]; then
    mkdir -p "$KEEP$(dirname "$f")"
    cp "$f" "$KEEP$f"
    stat -c '%a %U %G' "$f" > "$KEEP$f.mode"
  fi
done
: > "$KEEP/kernel.txt"
echo "The record holds $(grep -c . "$T/start-ids") changes; they are left exactly as they are."

# The stopwatch itself, on a stand-in that takes three seconds with a one second limit.
RUNNER=sleep
LIMIT=1
call main 3
is "the stopwatch catches a job that doesn't answer in time" "$RC" hung
RUNNER="pkexec --disable-internal-agent $HELPER"
LIMIT=10

job "the helper answers without a password (status)" ok status || { echo "The helper can't be reached, so the checks stop here."; finish; }
HELPER_VERSION=$(printf '%s' "$OUT" | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')
# Helpers up to Turbine Hub 0.4 all answer 0.1.0, whatever release they came with.
[ "$HELPER_VERSION" = "0.1.0" ] && OLD=1
if [ "$OLD" = 1 ]; then
  newer "status says which Turbine Hub the helper came with" "it says $HELPER_VERSION for every release up to 0.4"
else
  ok "status says which Turbine Hub the helper came with ($HELPER_VERSION)"
fi
is "status lists the changes in the record" "$(printf '%s' "$OUT" | grep -o '"id":' | grep -c .)" "$(grep -c . "$T/start-ids")"
START_VERIFY=
job "the helper's own check answers (verify)" ok verify || true
START_VERIFY=$OUT
if [ "$OUT" = '{"ok":true,"drift":[]}' ]; then ok "verify finds every recorded change in effect"; else bad "verify finds every recorded change in effect" "$OUT"; fi
"$HELPER" status > "$T/direct.out" 2>&1
is "run by the player directly (no pkexec), the helper refuses" "$? $(cat "$T/direct.out")" '1 {"ok":false,"error":"The helper must run as root"}'

# ---------------------------------------------------------------- what the app asks the machine
echo
echo "== What the app itself asks (read only)"
field() { sed -n "s/.*\"$1\":\"\([^\"]*\)\".*/\1/p" "$T/app.out" | head -n 1; }
raw() { sed -n "s/.*\"$1\":\([^,}]*\).*/\1/p" "$T/app.out" | head -n 1; }
# ask <job> <exit code wanted>: one question to the installed app. Its line says whether the answer came in time
# and is real JSON.
ask() {
  s=$(now)
  timeout "$APP_LIMIT" flatpak run "$APP" --json "$1" > "$T/app.out" 2> "$T/app.err" < /dev/null
  rc=$?
  ms=$((($(now) - s) / 1000000))
  if [ "$rc" != "$2" ]; then bad "the app answers $1" "exit $rc after $ms ms: $(head -c 200 "$T/app.out" "$T/app.err")"; return 1; fi
  if command -v python3 > /dev/null 2>&1; then
    python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if isinstance(d, dict) and isinstance(d.get("ok"), bool) else 1)' "$T/app.out" 2> /dev/null || { bad "the app answers $1" "not valid JSON: $(head -c 200 "$T/app.out")"; return 1; }
  else
    grep -q '^{"ok":\(true\|false\),.*}$' "$T/app.out" || { bad "the app answers $1" "not JSON: $(head -c 200 "$T/app.out")"; return 1; }
  fi
  ok "the app answers $1 with valid JSON ($ms ms)"
}
system_battery() {
  for b in /sys/class/power_supply/*; do
    [ "$(cat "$b/type" 2> /dev/null)" = Battery ] && [ "$(cat "$b/scope" 2> /dev/null)" != Device ] && return 0
  done
  return 1
}
if ! command -v flatpak > /dev/null 2>&1 || ! flatpak info "$APP" > /dev/null 2>&1; then
  skip "what the app asks" "Turbine Hub's Flatpak isn't installed here"
else
  if ask version 0; then
    APP_VERSION=$(field version)
    if printf '%s' "$APP_VERSION" | grep -Eq '^[0-9]+\.[0-9]+$'; then ok "version: $APP_VERSION"; else bad "version is a one-dot version" "$APP_VERSION"; fi
  fi
  if ask machine 0; then
    if system_battery; then want_kind="handheld or laptop"; else want_kind=desktop; fi
    case "$want_kind" in *"$(field kind)"*) ok "machine: it is a $(field kind) ($(field name))" ;; *) bad "machine: the kind of machine" "wanted $want_kind, found $(field kind)" ;; esac
    is "machine: the processor is the one in /proc/cpuinfo ($(field cpu))" "$(field cpu)" "$(sed -n 's/^model name[^:]*: //p' /proc/cpuinfo | head -n 1)"
    cards=
    for c in /sys/class/drm/card*/device; do
      [ -r "$c/vendor" ] && cards="$cards $(sed 's/^0x//' "$c/vendor"):$(sed 's/^0x//' "$c/device")"
    done
    case " $cards " in *" $(field gpu) "*) ok "machine: the graphics card is one in this machine ($(field gpu))" ;; *) bad "machine: the graphics card" "found $(field gpu), this machine has$cards" ;; esac
    gb=$(($(sed -n 's/^MemTotal: *\([0-9]*\).*/\1/p' /proc/meminfo) / 1048576))
    got=$(raw memoryGb)
    if [ "${got:-0}" -ge $((gb - 1)) ] 2> /dev/null && [ "$got" -le $((gb + 2)) ]; then ok "machine: memory is right ($got GB)"; else bad "machine: memory" "found $got GB, the kernel says about $gb"; fi
    is "machine: the SteamOS version is the one in /etc/os-release" "$(field steamos)" "$(sed -n 's/^VERSION_ID="\{0,1\}\([^"]*\)"\{0,1\}$/\1/p' /etc/os-release)"
    # Windows is only there when a switched-on Windows Boot Manager entry starts from a partition this machine has.
    if boot=$(efibootmgr 2> /dev/null) && [ -n "$boot" ]; then
      win=false
      for u in $(printf '%s\n' "$boot" | sed -n 's/^Boot[0-9A-Fa-f]\{4\}\* *Windows Boot Manager.*GPT,\([0-9A-Fa-f-]\{36\}\).*/\1/p' | tr 'A-F' 'a-f'); do
        [ -e "/dev/disk/by-partuuid/$u" ] && win=true
      done
      is "machine: Windows is offered only when it is really on a drive here ($win)" "$(raw windows)" "$win"
    else
      skip "machine: Windows is offered only when it is really there" "efibootmgr didn't answer"
    fi
  fi
  if ask readings 0; then
    c=$(raw cpuC)
    if [ "${c:-0}" -ge 5 ] 2> /dev/null && [ "$c" -le 115 ]; then ok "readings: the processor's temperature is a real one ($c C)"; else bad "readings: the processor's temperature" "found $c"; fi
    if system_battery; then
      p=$(raw batteryPct)
      if [ "${p:-x}" -ge 0 ] 2> /dev/null && [ "$p" -le 100 ]; then ok "readings: the battery is at $p%"; else bad "readings: the battery" "found $p"; fi
    else
      is "readings: no battery, and none is made up" "$(raw batteryPct) $(raw batteryW)" "null null"
    fi
  fi
  if ask checkup 0; then
    if [ -n "$(field headline)" ] && grep -q '"items":\[' "$T/app.out"; then ok "checkup: $(field headline)"; else bad "checkup has a headline and a list" "$(head -c 200 "$T/app.out")"; fi
  fi
  if ask cards 0; then
    n=$(grep -o '"id":' "$T/app.out" | grep -c .)
    if [ "$n" -ge 3 ]; then ok "cards: $n cards for this machine"; else bad "cards: a card for each thing this machine has" "only $n"; fi
  fi
  if ask tuneup 0; then
    n=$(grep -o '"state":"\(on\|off\|steamos\)"' "$T/app.out" | grep -c .)
    if [ "$n" -ge 1 ] && [ "$n" = "$(grep -o '"id":' "$T/app.out" | grep -c .)" ]; then ok "tuneup: $n items, each on, off or done by SteamOS"; else bad "tuneup: every item has a state" "$(head -c 200 "$T/app.out")"; fi
  fi
  if ask changes 0; then
    is "changes: the helper is seen" "$(raw helper)" true
    is "changes: as many as the record holds" "$(grep -o '"id":' "$T/app.out" | grep -c .)" "$(grep -c . "$T/start-ids")"
    missing=
    while read -r line; do
      id=$(printf '%s' "$line" | sed -n 's/^{"id":\([0-9]*\),.*/\1/p')
      [ -n "$id" ] || continue
      words=$(printf '%s' "$line" | sed -n 's/^{"id":[0-9]*,"words":"\([^"]*\)".*/\1/p')
      grep -qF "{\"id\":$id,\"words\":\"$words\"" "$T/app.out" || missing="$missing $id"
    done < "$KEEP/ledger.json"
    is "changes: each with the record's number and words" "missing:$missing" "missing:"
  fi
  if ask glossary 0; then
    n=$(grep -o '"term":' "$T/app.out" | grep -c .)
    if [ "$n" -ge 10 ]; then ok "glossary: $n words explained"; else bad "glossary has its words" "only $n"; fi
  fi
  if ask panel 0; then
    is "panel: the same version as the app" "$(field version)" "${APP_VERSION:-}"
    is "panel: the helper is seen" "$(raw helper)" true
    if ls /sys/class/leds/*/multi_intensity > /dev/null 2>&1; then want=true; else want=false; fi
    is "panel: lights are offered only when the machine has them ($want)" "$(grep -o '"lighting":{"available":[a-z]*' "$T/app.out" | sed 's/.*://')" "$want"
  fi
  if ask no-such-job 2; then
    is "an unknown question is refused" "$(raw ok)" false
  fi
fi

# ---------------------------------------------------------------- each kind of setting, there and back
# applied <what> <setting>: applies one setting. Sets ID to its number in the record.
applied() {
  ID=
  job "$1: apply" ok apply "$2" || return 1
  ID=$(printf '%s' "$OUT" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  if [ -z "$ID" ] || grep -qx "$ID" "$T/start-ids"; then
    bad "$1: it is a new change in the record" "the helper answered $OUT"
    ID=
    return 1
  fi
  if [ -n "$(entry "$ID")" ]; then ok "$1: it is in the record as change $ID"; else bad "$1: it is in the record" "no change $ID"; fi
}
# checked <what>: the helper's own check still finds nothing new, and putting everything back in effect works.
checked() {
  job "$1: verify" ok verify && is "$1: verify finds it in effect" "$OUT" "$START_VERIFY"
  job "$1: reapply" ok reapply
}
# undone <what>: takes change $ID back.
undone() {
  job "$1: undo" ok undo "{\"id\":$ID}" || return 1
  is "$1: it is gone from the record" "$(entry "$ID")" ""
  etc_same "$1"
}

# A kernel setting that is 0 or 1: flipped through the helper, and back.
kernel_switch() {
  key=$1
  file=/proc/sys/$(echo "$key" | tr . /)
  if [ ! -r "$file" ]; then skip "$key" "this kernel doesn't have it"; return; fi
  if recorded "\"key\":\"$key\""; then skip "$key" "it is already in the record, which is left alone"; return; fi
  was=$(cat "$file")
  case "$was" in 0 | 1) ;; *) skip "$key" "it is $was, not 0 or 1"; return ;; esac
  grep -q "^$key " "$KEEP/kernel.txt" || echo "$key $was" >> "$KEEP/kernel.txt"
  new=$((1 - was))
  applied "$key = $new" "{\"kind\":\"sysctl\",\"key\":\"$key\",\"value\":$new}" || return
  is "$key: the kernel has $new" "$(cat "$file")" "$new"
  is "$key: $SYSCTL_FILE holds it" "$(grep -c -x "$key = $new" "$SYSCTL_FILE" 2> /dev/null)" 1
  is "$key: the record keeps what was there before ($was)" "$(entry "$ID" | grep -c "\"before\":{\"value\":\"$was\"}")" 1
  checked "$key"
  is "$key: still $new after reapply" "$(cat "$file")" "$new"
  undone "$key"
  is "$key: back to $was" "$(cat "$file")" "$was"
}

echo
echo "== Kernel settings (a file under /etc/sysctl.d and the running kernel)"
kernel_switch kernel.split_lock_mitigate
kernel_switch kernel.nmi_watchdog

echo
echo "== A recorded /sys value: the graphics card's power limit"
CAP=
for c in /sys/class/drm/card*/device/hwmon/hwmon*/power1_cap; do
  [ -r "$c" ] && [ -r "${c}_min" ] && [ -r "${c}_max" ] && CAP=$c && break
done
if [ -z "$CAP" ]; then
  skip "graphics card power limit" "this card has none to set"
elif recorded "power1_cap\""; then
  skip "graphics card power limit" "it is already in the record, which is left alone"
else
  was=$(cat "$CAP")
  lo=$(cat "${CAP}_min")
  hi=$(cat "${CAP}_max")
  new=$((was - 5000000))
  [ "$new" -ge "$lo" ] || new=$((was + 5000000))
  if [ "$new" -gt "$hi" ] || [ "$new" -lt "$lo" ]; then
    skip "graphics card power limit" "no room between $lo and $hi"
  else
    remember "$CAP"
    job "a limit above the card's own highest is refused" refused apply "{\"kind\":\"sysfs\",\"path\":\"$CAP\",\"value\":\"$((hi + 1000000))\"}"
    [ "$lo" -gt 1000000 ] && job "a limit below the card's own lowest is refused" refused set-now "{\"path\":\"$CAP\",\"value\":\"$((lo - 1000000))\"}"
    is "the limit is untouched by the refusals" "$(cat "$CAP")" "$was"
    if applied "power limit $((new / 1000000)) W" "{\"kind\":\"sysfs\",\"path\":\"$CAP\",\"value\":\"$new\"}"; then
      is "power limit: the card has it" "$(cat "$CAP")" "$new"
      is "power limit: the record keeps what was there before ($((was / 1000000)) W)" "$(entry "$ID" | grep -c "\"before\":{\"value\":\"$was\"}")" 1
      checked "power limit"
      is "power limit: still there after reapply" "$(cat "$CAP")" "$new"
      undone "power limit"
      is "power limit: back to $((was / 1000000)) W" "$(cat "$CAP")" "$was"
    fi
  fi
fi

echo
echo "== Values set for now and not recorded (what Game Boost does)"
cp "$LEDGER" "$T/ledger.before"
K=/sys/kernel/mm/transparent_hugepage/khugepaged/defrag
if [ ! -r "$K" ]; then
  skip "one value set for now" "this kernel has no $K"
elif recorded "\"path\":\"$K\""; then
  skip "one value set for now" "$K is already in the record, which is left alone"
else
  was=$(live "$K")
  case "$was" in
    0 | 1)
      remember "$K"
      new=$((1 - was))
      job "set-now: background huge page tidying to $new" ok set-now "{\"path\":\"$K\",\"value\":\"$new\"}" && is "set-now: the kernel has it" "$(live "$K")" "$new"
      job "set-now: and back to $was" ok set-now "{\"path\":\"$K\",\"value\":\"$was\"}" && is "set-now: the kernel has it back" "$(live "$K")" "$was"
      ;;
    *) skip "one value set for now" "$K is $was" ;;
  esac
fi
E0=/sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference
E1=/sys/devices/system/cpu/cpufreq/policy1/energy_performance_preference
if [ ! -r "$E0" ] || [ ! -r "$E1" ]; then
  skip "several values in one call" "this processor has no speed preference to set"
elif recorded "energy_performance_preference"; then
  skip "several values in one call" "a speed preference is already in the record, which is left alone"
elif [ "$(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_governor 2> /dev/null)" = performance ]; then
  skip "several values in one call" "the processor is held at full speed now, which fixes its preference"
else
  was0=$(live "$E0")
  was1=$(live "$E1")
  if [ "$was0" = performance ]; then new=balance_performance; else new=performance; fi
  remember "$E0"
  remember "$E1"
  if job "set-now: two cores' speed preference to $new in one call" ok set-now "{\"values\":[{\"path\":\"$E0\",\"value\":\"$new\"},{\"path\":\"$E1\",\"value\":\"$new\"}]}"; then
    is "set-now: both cores have it" "$(live "$E0") $(live "$E1")" "$new $new"
  fi
  if job "set-now: and back" ok set-now "{\"values\":[{\"path\":\"$E0\",\"value\":\"$was0\"},{\"path\":\"$E1\",\"value\":\"$was1\"}]}"; then
    is "set-now: both cores have theirs back" "$(live "$E0") $(live "$E1")" "$was0 $was1"
  fi
fi
if cmp -s "$LEDGER" "$T/ledger.before"; then ok "set-now left the record alone"; else bad "set-now left the record alone" "the record changed"; fi

echo
echo "== A wake rule for a USB device (a udev rule and the device's wake switch)"
WAKE_DEV=
for pass in enabled disabled; do
  for d in /sys/bus/usb/devices/*; do
    [ -z "$WAKE_DEV" ] && [ -r "$d/power/wakeup" ] && [ -r "$d/idVendor" ] && [ -r "$d/idProduct" ] || continue
    [ "$(cat "$d/bDeviceClass" 2> /dev/null)" = 09 ] && continue
    [ "$(cat "$d/power/wakeup")" = "$pass" ] || continue
    recorded "\"vendor\":\"$(cat "$d/idVendor")\",\"product\":\"$(cat "$d/idProduct")\"" && continue
    WAKE_DEV=$d
  done
done
if [ -z "$WAKE_DEV" ]; then
  skip "wake rule" "no USB device here that can wake the machine and isn't in the record"
else
  v=$(cat "$WAKE_DEV/idVendor")
  p=$(cat "$WAKE_DEV/idProduct")
  W=$WAKE_DEV/power/wakeup
  was=$(cat "$W")
  if [ "$was" = enabled ]; then new=disabled; flag=false; else new=enabled; flag=true; fi
  remember "$W"
  if applied "USB device $v:$p wake $new" "{\"kind\":\"wake\",\"vendor\":\"$v\",\"product\":\"$p\",\"wake\":$flag}"; then
    is "wake rule: $WAKE_FILE names the device" "$(grep -c "ATTRS{idVendor}==\"$v\", ATTRS{idProduct}==\"$p\", ATTR{power/wakeup}=\"$new\"" "$WAKE_FILE" 2> /dev/null)" 1
    is "wake rule: the device's wake switch is $new" "$(wait_live "$W" "$new")" "$new"
    is_newer "wake rule: the record keeps what was there before ($was)" "$(entry "$ID" | grep -c "\"before\":{\"value\":\"$was\"}")" 1
    checked "wake rule"
    undone "wake rule"
    is_newer "wake rule: after undo the device's wake switch is $was again" "$(wait_live "$W" "$was")" "$was"
    if [ "$(live "$W")" != "$was" ]; then
      job "wake rule: the wake switch put back by hand" ok set-now "{\"path\":\"$W\",\"value\":\"$was\"}"
    fi
  fi
fi

echo
echo "== Rules for devices and modules (files under /etc/udev and /etc/modules-load.d)"
if recorded '"what":"ntsync"'; then
  skip "NTSync access rule" "it is already in the record, which is left alone"
else
  mode_was=$(stat -c %a /dev/ntsync 2> /dev/null)
  if applied "NTSync access rule" '{"kind":"access","what":"ntsync"}'; then
    is "NTSync rule: $ACCESS_FILE holds exactly the one fixed line" "$(sed 1d "$ACCESS_FILE" 2> /dev/null)" 'KERNEL=="ntsync", MODE="0666"'
    if [ -e /dev/ntsync ]; then
      i=0
      while [ "$(stat -c %a /dev/ntsync)" != 666 ] && [ $i -lt 30 ]; do sleep 0.1; i=$((i + 1)); done
      is "NTSync rule: every program may use /dev/ntsync" "$(stat -c %a /dev/ntsync)" 666
    fi
    checked "NTSync rule"
    undone "NTSync rule"
    [ "${mode_was:-666}" = 666 ] || echo "note    /dev/ntsync was $mode_was before and stays open to every program until the next start"
  fi
fi
if recorded '"kind":"module"'; then
  skip "NTSync at every start" "it is already in the record, which is left alone"
elif [ ! -d /sys/module/ntsync ]; then
  skip "NTSync at every start" "the module isn't loaded now, and loading it can't be taken back without a restart"
else
  if applied "NTSync at every start" '{"kind":"module","name":"ntsync"}'; then
    is "module: $MODULES_FILE names it" "$(sed 1d "$MODULES_FILE" 2> /dev/null)" ntsync
    checked "module"
    undone "module"
    is "module: still loaded, as it was" "$([ -d /sys/module/ntsync ] && echo yes)" yes
  fi
fi
# The lighting rule only where it can't change anything: no lights and no ASUS controller. On a machine with either
# it opens files that stay open until the next start, and a helper from 0.3 or older can wait on itself there.
if recorded '"what":"lighting"'; then
  skip "lighting rule" "it is already in the record, which is left alone"
elif ls /sys/class/leds/*/multi_intensity > /dev/null 2>&1 || grep -qi '^HID_ID=[0-9A-F]*:00000B05:' /sys/class/hidraw/*/device/uevent 2> /dev/null; then
  skip "lighting rule" "this machine has lights or an ASUS controller: that is on the checklist in docs/LIVE-CHECKS.md"
else
  stat -c '%n %a %U %G' /dev/hidraw* > "$T/hid.before" 2> /dev/null
  if applied "lighting rule (no lights here, so only the file)" '{"kind":"access","what":"lighting"}'; then
    is "lighting rule: $ACCESS_FILE holds the two fixed lines" "$(sed 1d "$ACCESS_FILE" 2> /dev/null | grep -c -e '^SUBSYSTEM=="leds", TEST=="multi_intensity", RUN+="/usr/bin/chmod 0666 /sys%p/brightness /sys%p/multi_intensity"$' -e "^SUBSYSTEM==\"hidraw\", KERNEL==\"hidraw\\*\", ATTRS{idVendor}==\"0b05\", RUN+=\"$HELPER hid-access %k\"\$")" 2
    hid=$(ls /sys/class/hidraw 2> /dev/null | head -n 1)
    [ -n "$hid" ] && job "lighting rule: a device that isn't a light is looked at and left ($hid)" ok hid-access "$hid" && is "lighting rule: it wasn't opened" "$OUT" '{"ok":true,"opened":false}'
    checked "lighting rule"
    undone "lighting rule"
    sleep 0.5
    stat -c '%n %a %U %G' /dev/hidraw* > "$T/hid.after" 2> /dev/null
    if cmp -s "$T/hid.before" "$T/hid.after"; then ok "lighting rule: no device changed who may use it"; else bad "lighting rule: no device changed who may use it" "$(cat "$T/hid.after")"; fi
  fi
fi

echo
echo "== A change that fails leaves nothing behind"
if systemctl cat tailscaled.service > /dev/null 2>&1; then
  skip "a service that isn't on this machine" "Tailscale is installed here, and services that exist are never switched by this check"
else
  cp "$LEDGER" "$T/ledger.before"
  job "turning on a service that isn't installed is refused" refused apply '{"kind":"service","unit":"tailscaled.service","enabled":true}'
  if cmp -s "$LEDGER" "$T/ledger.before"; then ok "the failed change isn't in the record"; else bad "the failed change isn't in the record" "the record changed"; fi
  etc_same "the failed change"
fi

# ---------------------------------------------------------------- everything the helper must refuse
echo
echo "== Refusals"
cp "$LEDGER" "$T/ledger.before"
: > "$T/live.before"
for f in /proc/sys/kernel/core_pattern /proc/sys/kernel/nmi_watchdog /proc/sys/vm/max_map_count "$K" ${CAP:-}; do
  [ -r "$f" ] && echo "$f $(cat "$f")" >> "$T/live.before"
done
job "a kernel setting that isn't on the list" refused apply '{"kind":"sysctl","key":"kernel.core_pattern","value":1}'
job "a kernel setting on the list, out of its range" refused apply '{"kind":"sysctl","key":"kernel.nmi_watchdog","value":2}'
job "a kernel setting far below its range" refused apply '{"kind":"sysctl","key":"vm.max_map_count","value":1}'
job "a negative number" refused apply '{"kind":"sysctl","key":"kernel.nmi_watchdog","value":-1}'
job "a number written as words" refused apply '{"kind":"sysctl","key":"kernel.nmi_watchdog","value":"0"}'
job "a path outside /sys" refused set-now '{"path":"/etc/passwd","value":"x"}'
job "a path under /proc" refused set-now '{"path":"/proc/sys/kernel/core_pattern","value":"x"}'
job "a /sys path that isn't on the list" refused set-now '{"path":"/sys/kernel/mm/transparent_hugepage/hpage_pmd_size","value":"1"}'
job "a path that climbs out of /sys with .." refused set-now '{"path":"/sys/class/power_supply/../../../etc/passwd","value":"80"}'
job "a listed path reached through .." refused set-now '{"path":"/sys/kernel/mm/transparent_hugepage/khugepaged/../khugepaged/defrag","value":"0"}'
job "a listed path with a doubled slash" refused set-now '{"path":"/sys/kernel/mm//transparent_hugepage/khugepaged/defrag","value":"0"}'
job "a listed value out of its range" refused set-now '{"path":"/sys/kernel/mm/transparent_hugepage/khugepaged/defrag","value":"2"}'
job "a word that isn't one of the allowed ones" refused set-now '{"path":"/sys/kernel/mm/transparent_hugepage/khugepaged/defrag","value":"always\nnever"}'
job "a device that isn't on this machine" refused set-now '{"path":"/sys/class/power_supply/NOSUCH0/charge_control_end_threshold","value":"80"}'
job "an empty list of values" refused set-now '{"values":[]}'
job "a value without its path" refused set-now '{"values":[{"value":"1"}]}'
job "a job name the helper doesn't have" refused frobnicate
job "no job name at all" refused
job "a job without its details" refused apply
job "details cut off half way" refused apply '{"kind":"sysctl","key":"kernel.nmi_watchdog","value":'
job "details that aren't JSON" refused apply 'kind=sysctl key=kernel.nmi_watchdog value=0'
job "a kind of setting that doesn't exist" refused apply '{"kind":"file","path":"/etc/sudoers","text":"x"}'
job "a setting with a command in it" refused apply '{"kind":"zram","percent":100,"algorithm":"zstd\n[x]"}'
job "a module that isn't on the list" refused apply '{"kind":"module","name":"evil"}'
job "a service that isn't on the list" refused apply '{"kind":"service","unit":"getty@tty1.service","enabled":true}'
job "an access rule that doesn't exist" refused apply '{"kind":"access","what":"everything"}'
job "a USB id that isn't four hex digits" refused apply '{"kind":"wake","vendor":"046d\"","product":"c548","wake":false}'
job "more details than a job can take (70,000 characters)" refused apply "{\"kind\":\"sysctl\",\"key\":\"$(printf '%070000d' 0)\",\"value\":1}"
job "undo of a change that isn't in the record" refused undo '{"id":999999999}'
job "undo without a number" refused undo '{"id":"1"}'
last=$(ids "$LEDGER" | sort -n | tail -n 1)
call main undo "{\"id\":$((${last:-0} + 1000)).5}"
case "$RC:$OUT" in
  1:*"whole number"*) ok "undo of change number $((${last:-0} + 1000)).5 is refused as not a number of a change" ;;
  *) newer "undo of change number $((${last:-0} + 1000)).5 is refused as not a number of a change" "the helper cut it down to a whole number and looked for that change: $OUT" ;;
esac
job "a lights device with a path for a name" refused hid-access ../../etc/passwd
before=$(grep -c . "$LOG" 2> /dev/null)
call main "$(printf 'nothing ok\n0 0 live-helper-check: this line was made by a job name')"
after=$(grep -c . "$LOG" 2> /dev/null)
if [ -r "$LOG" ]; then
  is_newer "a job name can't write a line of its own into the helper's log" "$((after - before))" 1
else
  skip "a job name can't write a line of its own into the helper's log" "the log can't be read"
fi
echo "skipped a link that leads out of /sys: nothing but the kernel can make one in the real /sys (the helper's own tests cover it)"
N_SKIP=$((N_SKIP + 1))
if cmp -s "$LEDGER" "$T/ledger.before"; then ok "no refusal changed the record"; else bad "no refusal changed the record" "the record changed"; fi
etc_same "the refusals"
: > "$T/live.after"
for f in /proc/sys/kernel/core_pattern /proc/sys/kernel/nmi_watchdog /proc/sys/vm/max_map_count "$K" ${CAP:-}; do
  [ -r "$f" ] && echo "$f $(cat "$f")" >> "$T/live.after"
done
if cmp -s "$T/live.before" "$T/live.after"; then ok "no refusal changed a value on the machine"; else bad "no refusal changed a value on the machine" "$(cat "$T/live.after")"; fi

# ---------------------------------------------------------------- one job at a time
echo
echo "== Two jobs at once"
# together <slot> <job> [details]: a job in the background; its exit code and time land in a file.
together() {
  (
    call "$@"
    echo "$RC $MS" > "$T/$1.res"
  ) &
}
A=/proc/sys/kernel/split_lock_mitigate
B=/proc/sys/kernel/nmi_watchdog
if [ ! -r "$A" ] || [ ! -r "$B" ] || recorded '"key":"kernel.split_lock_mitigate"' || recorded '"key":"kernel.nmi_watchdog"'; then
  skip "two changes at the same moment" "the two kernel settings it uses aren't free on this machine"
else
  a_was=$(cat "$A")
  b_was=$(cat "$B")
  together one apply "{\"kind\":\"sysctl\",\"key\":\"kernel.split_lock_mitigate\",\"value\":$((1 - a_was))}"
  together two apply "{\"kind\":\"sysctl\",\"key\":\"kernel.nmi_watchdog\",\"value\":$((1 - b_was))}"
  together three verify
  wait
  for s in one two three; do
    read -r rc ms < "$T/$s.res"
    if [ "$rc" = 0 ]; then ok "job $s of three at once answered ($ms ms)"; else bad "job $s of three at once answered" "exit $rc, $(cat "$T/$s.out")"; fi
  done
  id1=$(sed -n 's/.*"id":\([0-9]*\).*/\1/p' "$T/one.out")
  id2=$(sed -n 's/.*"id":\([0-9]*\).*/\1/p' "$T/two.out")
  if [ -n "$id1" ] && [ -n "$id2" ] && [ "$id1" != "$id2" ] && [ -n "$(entry "$id1")" ] && [ -n "$(entry "$id2")" ]; then
    ok "both changes are in the record, each with its own number ($id1, $id2)"
  else
    bad "both changes are in the record, each with its own number" "$(cat "$T/one.out" "$T/two.out")"
  fi
  is "both are in $SYSCTL_FILE" "$(grep -c -x -e "kernel.split_lock_mitigate = $((1 - a_was))" -e "kernel.nmi_watchdog = $((1 - b_was))" "$SYSCTL_FILE" 2> /dev/null)" 2
  is "both are in effect" "$(cat "$A") $(cat "$B")" "$((1 - a_was)) $((1 - b_was))"
  if [ -n "$id1" ] && [ -n "$id2" ]; then
    together one undo "{\"id\":$id1}"
    together two undo "{\"id\":$id2}"
    wait
    for s in one two; do
      read -r rc ms < "$T/$s.res"
      if [ "$rc" = 0 ]; then ok "undo $s of two at once answered ($ms ms)"; else bad "undo $s of two at once answered" "exit $rc, $(cat "$T/$s.out")"; fi
    done
    is "both are gone from the record" "$(entry "$id1")$(entry "$id2")" ""
  fi
  is "both are back as they were" "$(cat "$A") $(cat "$B")" "$a_was $b_was"
  etc_same "two jobs at once"
fi
# The lock itself. A lock file every program can read is one every program can hold, and that stops every job.
if [ -r "$STATE/lock" ]; then
  newer "a player's program can't hold the helper's lock" "$STATE/lock can be read, so any program can hold it and make every job wait"
  if command -v flock > /dev/null 2>&1; then
    # While that is so, it shows a job really waits its turn: the lock is held here for two seconds.
    flock "$STATE/lock" sleep 2 &
    sleep 0.3
    call main verify
    wait
    if [ "$RC" = 0 ] && [ "$MS" -ge 1000 ]; then ok "a job waits for the one before it, then runs ($MS ms behind a lock held for 2 seconds)"; else bad "a job waits for the one before it, then runs" "exit $RC after $MS ms"; fi
  fi
else
  ok "a player's program can't hold the helper's lock"
fi

finish

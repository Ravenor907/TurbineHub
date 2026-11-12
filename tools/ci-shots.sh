#!/bin/sh
# Takes Turbine Hub's pictures with the installed Flatpak and runs the checks each picture carries.
#   tools/ci-shots.sh [1280x800|1920x1080|1600x900|all] [out dir, default shots]
#   tools/ci-shots.sh <size> <shard> <shards>      one shard of a size, into shots/ (CI)
# The app lists its pictures (turbinehub --shots: one "<name> <arguments>" line each) and draws them with fake data
# of its own, so nothing here depends on this PC. Each picture lands in <out>/<size>/<name>.png with its log beside
# it. 1600x900 is a Desktop Mode window (--window). Fails when a picture is missing or a check fails. The pictures
# named check-... are taken after the page was worked with real keys, a real mouse and a finger: taps never go
# through a dialog, a held key acts once, a resting mouse never takes the highlight (app/qml/SelfCheck.qml).
# In CI the bundle the build made (TurbineHub.flatpak in the working folder) is installed first; on a development
# PC tools/dev-build.sh has installed it.
set -u
APP=io.github.ravenor907.TurbineHub
part=${1:-all}
out=${2:-shots}
# CI splits each size over shards: tools/ci-shots.sh <size> <shard> <shards> takes every <shards>-th picture
# starting at <shard>, into shots/.
shard=0
shards=1
case "$out" in
  ''|*[!0-9]*) ;;
  *) shard=$out; shards=${3:-1}; out=shots ;;
esac
case "$part" in
  1280x800|1920x1080|1600x900) sizes=$part ;;
  all) sizes="1280x800 1920x1080 1600x900" ;;
  *) echo "usage: tools/ci-shots.sh [1280x800|1920x1080|1600x900|all] [out dir]"; exit 2 ;;
esac
if [ -n "${CI:-}" ] && [ -f TurbineHub.flatpak ]; then
  flatpak install --user -y --noninteractive --reinstall TurbineHub.flatpak > /dev/null || exit 1
fi
mkdir -p "$out" || exit 1
out=$(cd "$out" && pwd)
# A build container has no session bus; a desktop has one already.
bus=""
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && command -v dbus-run-session > /dev/null; then bus="dbus-run-session --"; fi
# No screen and no GPU needed: Qt draws offscreen with its software renderer.
app() {   # app <seconds> <arguments>
  t=$1
  shift
  timeout "$t" $bus flatpak run --env=QT_QPA_PLATFORM=offscreen --env=QT_QUICK_BACKEND=software \
    --filesystem="$out" "$APP" "$@" < /dev/null
}
list="$out/shots.list"
if ! app 60 --shots > "$list" 2> "$out/shots.list.log" || ! [ -s "$list" ]; then
  echo "::error title=No picture list::turbinehub --shots gave nothing"
  cat "$out/shots.list.log"
  exit 1
fi
total=0
missing=0
failed=0
passed=0
hung=0
inputs=0
for size in $sizes; do
  mkdir -p "$out/$size"
  extra=""
  [ "$size" = 1600x900 ] && extra="--window"
  index=-1
  while read -r name args; do
    case "$name" in ""|"#"*) continue ;; esac
    index=$((index + 1))
    [ $((index % shards)) -eq "$shard" ] || continue
    total=$((total + 1))
    png="$out/$size/$name.png"
    log="$out/$size/$name.log"
    rm -f "$png"
    # The list's arguments are plain words, split on spaces.
    # shellcheck disable=SC2086
    app 60 --screenshot="$png" --size="$size" $extra $args > "$log" 2>&1
    code=$?
    if [ $code -eq 124 ]; then
      echo "::error title=Timed out after 60 s::$size $name"
      hung=$((hung + 1))
    elif [ $code -ne 0 ]; then
      echo "::warning title=Turbine Hub stopped with code $code::$size $name"
    fi
    bad=0
    if [ ! -s "$png" ]; then
      echo "::error title=No screenshot::$size $name"
      tail -n 40 "$log"
      missing=$((missing + 1))
      bad=1
    fi
    n=$(grep -c '^SELFCHECK FAIL' "$log")
    if [ "$n" -gt 0 ]; then
      grep '^SELFCHECK FAIL' "$log" | sed "s|^|$size $name: |"
      failed=$((failed + n))
      bad=1
    fi
    passed=$((passed + $(grep -c '^SELFCHECK ok' "$log")))
    # A picture taken after the page was worked with real keys, a mouse and a finger (--check=...): the run must
    # say that every step held, so a check that never ran is a failure too.
    case " $args " in
      *" --check="*)
        if grep -q '^INPUTCHECK ok' "$log"; then
          inputs=$((inputs + 1))
        else
          echo "::error title=Input check failed::$size $name"
          failed=$((failed + 1))
          bad=1
        fi ;;
    esac
    # A script error in the page is a failure even when the picture came out.
    if grep -E "qrc:|\.qml:[0-9]+|TypeError|ReferenceError" "$log"; then
      echo "::error title=QML problem::$size $name"
      failed=$((failed + 1))
      bad=1
    fi
    # A picture that came out clean but left a failing exit code still fails.
    if [ $bad = 0 ] && [ $code -ne 0 ]; then failed=$((failed + 1)); fi
    # Three pictures that never came: stop here rather than wait a minute for every other one.
    if [ $hung -ge 3 ]; then echo "::error title=Turbine Hub hangs::three pictures timed out"; exit 1; fi
  done < "$list"
done
echo "Pictures: $total taken in $out ($sizes), $missing missing, checks: $passed ok, $failed failed, input checks: $inputs ok"
if [ $missing -gt 0 ] || [ $failed -gt 0 ] || [ $total -eq 0 ]; then exit 1; fi
exit 0

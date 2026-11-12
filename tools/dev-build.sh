#!/bin/sh
# Builds Turbine Hub on a development PC the same way the online build does and installs it for this user.
# Needs: flatpak, flatpak-builder, curl, git, and cargo (on SteamOS, where the host has no C linker, cargo runs in
# the "dev" distrobox). The first run downloads the KDE and Qt parts (a few GB); later runs take a minute or two.
#   sh tools/dev-build.sh          build and install
#   sh tools/dev-build.sh --pull   get the newest code from GitHub first
#   sh tools/dev-build.sh --run    start Turbine Hub after the install
set -eu
cd "$(dirname "$0")/.."
pull=0
run=0
for a in "$@"; do
  case "$a" in
    --pull) pull=1 ;;
    --run) run=1 ;;
    *) echo "unknown option $a"; exit 2 ;;
  esac
done
[ $pull = 1 ] && git pull --ff-only
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
# The Flatpak build is offline: the locked crates are vendored first, as the online build does.
mkdir -p .cargo
vendor='cargo +1.97.0 vendor --locked vendor > .cargo/config.toml.new && mv .cargo/config.toml.new .cargo/config.toml'
# An older config.toml would point cargo at a vendor folder that is being rewritten.
rm -f .cargo/config.toml
if command -v cc >/dev/null 2>&1 && command -v cargo >/dev/null 2>&1; then sh -c "$vendor"; else distrobox enter dev -- sh -c "$vendor"; fi
sh tools/get-fonts.sh
git rev-parse --short=9 HEAD > app/build-commit.txt
flatpak-builder --user --install --force-clean --ccache --install-deps-from=flathub --disable-rofiles-fuse \
  build-dir packaging/io.github.ravenor907.TurbineHub.json
if [ $run = 1 ]; then flatpak run io.github.ravenor907.TurbineHub; fi

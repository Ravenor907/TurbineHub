//! A made-up machine with a home folder, games, ratings, a Windows drive and a few recorded changes, so the pictures
//! and the tests show every screen with believable content and never touch the real system.
use super::Ui;
use crate::checkup::Facts;
use crate::cli::Ctx;
use crate::fake::{self, Preset};
use crate::helper::{Change, Ledger, Setting};
use crate::sensors::Readings;
use crate::settings::{Mode, Settings};
use std::fs;
use std::path::Path;

/// NonSteamLaunchers' newest release on the made-up machine.
pub const NSL_TAG: &str = "v4.2.94";

/// The second Steam library of `sample_with_unseen_library`: a folder no made-up or real machine has.
pub const UNSEEN_LIBRARY: &str = "/mnt/games-drive/SteamLibrary";

/// The made-up handheld of the pictures and tests.
pub const HANDHELD: Preset = Preset::Handheld;

/// The pictures of a screenshot run: (name, arguments).
pub const SHOTS: &[(&str, &str)] = &[
    ("pick", "--page=pick --machine=handheld"),
    ("setup-machine", "--page=setup --arg=machine --machine=handheld --mode=guided"),
    // The only place the password is asked: the dialog of the first system change. On a machine with a password,
    // on one without (it makes one), and with the helper set up already (nothing is asked).
    ("dialog-password", "--page=setup --arg=picks --act=quick-apply: --machine=handheld --mode=guided"),
    ("dialog-password-new", "--page=setup --arg=picks --act=quick-apply: --machine=handheld --mode=guided --no-password"),
    ("dialog-password-helper", "--page=setup --arg=picks --act=quick-apply: --machine=handheld --mode=guided --helper"),
    ("setup-picks-helper", "--page=setup --arg=picks --machine=handheld --mode=guided --helper"),
    ("setup-games", "--page=setup --arg=games --machine=handheld --mode=guided"),
    ("setup-bring", "--page=setup --arg=bring --machine=handheld --mode=guided"),
    ("setup-picks", "--page=setup --arg=picks --machine=handheld --mode=guided"),
    ("quick-setup", "--page=quick --machine=handheld --mode=veteran"),
    ("home-guided", "--page=home --machine=handheld --mode=guided"),
    ("home-veteran", "--page=home --machine=handheld --mode=veteran"),
    ("games", "--page=games --machine=handheld --mode=guided"),
    ("games-locked", "--page=games --arg=steam:1245620 --machine=handheld --mode=guided"),
    ("games-switches", "--page=games --arg=steam:1086940 --machine=handheld --mode=guided"),
    ("games-mods", "--page=games --arg=steam:367520 --machine=handheld --mode=guided"),
    ("games-search", "--page=games --act=search:games --value=el --machine=handheld --mode=guided"),
    ("games-search-none", "--page=games --act=search:games --value=zelda --machine=handheld --mode=guided"),
    ("tool", "--page=tool --machine=handheld --mode=guided"),
    ("tool-new", "--page=tool --machine=desktop --mode=guided --no-tool"),
    ("dialog-tool", "--page=tool --act=tool:install --machine=desktop --mode=guided --no-tool"),
    ("dialog-tool-how", "--page=games --arg=steam:1091500 --act=tool-how:steam:1091500 --machine=handheld --mode=guided"),
    ("dialog-tool-remove", "--page=tool --act=tool:remove --machine=handheld --mode=guided"),
    ("proton", "--page=proton --arg=steam:1086940 --machine=handheld --mode=guided"),
    ("dialog-proton", "--page=proton --arg=steam:1086940 --act=proton-pick:steam:1086940|GE-Proton10-4 --machine=handheld --mode=guided"),
    ("saves", "--page=saves --arg=steam:1086940 --machine=handheld --mode=guided --installed=com.github.mtkennerly.ludusavi"),
    ("saves-empty", "--page=saves --arg=steam:2379780 --machine=handheld --mode=guided"),
    ("dialog-restore", "--page=saves --arg=steam:1086940 --keys=down,a --machine=handheld --mode=guided --installed=com.github.mtkennerly.ludusavi"),
    ("library", "--page=library --machine=handheld --mode=guided"),
    ("library-offline", "--page=library --machine=handheld --mode=guided --offline"),
    ("store", "--page=store --arg=apps --machine=handheld --mode=guided"),
    ("store-offline", "--page=store --arg=apps --machine=handheld --mode=guided --offline"),
    ("store-decky", "--page=store --arg=decky --machine=handheld --mode=guided"),
    ("store-tv", "--page=store --arg=tv --machine=tv --mode=guided"),
    ("store-search", "--page=store --act=search:store --value=stream --machine=handheld --mode=guided"),
    ("store-item", "--page=store-item --arg=com.discordapp.Discord --machine=handheld --mode=guided"),
    ("store-item-new", "--page=store-item --arg=com.spotify.Client --machine=handheld --mode=guided"),
    ("store-item-update", "--page=store-item --arg=com.heroicgameslauncher.hgl --machine=handheld --mode=guided --offline"),
    ("store-item-tile", "--page=store-item --arg=tile:netflix --machine=tv --mode=guided"),
    ("store-item-decky", "--page=store-item --arg=decky --machine=handheld --mode=guided"),
    ("store-launchers", "--page=store --arg=launchers --machine=handheld --mode=guided"),
    ("store-launchers-nsl", "--page=store --arg=launchers --machine=handheld --mode=guided --nsl"),
    ("store-item-nsl", "--page=store-item --arg=nsl --machine=handheld --mode=guided"),
    ("store-item-nsl-desktop", "--page=store-item --arg=nsl --machine=desktop --mode=guided --window"),
    ("store-item-nsl-installed", "--page=store-item --arg=nsl --machine=desktop --mode=guided --window --nsl"),
    ("dialog-nsl", "--page=store-item --arg=nsl --act=nsl:install --machine=desktop --mode=guided --window"),
    ("dialog-nsl-game-mode", "--page=store-item --arg=nsl --act=nsl:install --machine=handheld --mode=guided"),
    ("dialog-nsl-plugin", "--page=store-item --arg=nsl --act=nsl-plugin: --machine=desktop --mode=guided --window --nsl"),
    ("machine", "--page=machine --machine=handheld --mode=guided"),
    ("lighting", "--page=module:lighting --machine=handheld --mode=guided"),
    ("lighting-veteran", "--page=module:lighting --machine=handheld --mode=veteran"),
    ("lighting-preset", "--page=module:lighting --act=light:preset --value=10 --machine=handheld --mode=guided"),
    ("lighting-spiral", "--page=module:lighting --act=light:effect --value=8 --machine=handheld --mode=veteran"),
    ("lighting-scrolled", "--page=module:lighting --keys=down,down,down,down,down,down,down,down,a --machine=handheld --mode=guided"),
    ("lighting-cards", "--page=module:lighting --keys=down,right,right,down --machine=handheld --mode=guided"),
    ("dialog-lighting", "--page=module:lighting --act=light-test: --machine=handheld --mode=guided"),
    ("lighting-other-app", "--page=module:lighting --machine=handheld --mode=guided --lights=allyhub"),
    ("lighting-other-huesync", "--page=module:lighting --machine=handheld --mode=guided --lights=huesync"),
    ("dialog-lighting-other", "--page=module:lighting --act=light-rival:allyhub --machine=handheld --mode=guided --lights=allyhub"),
    ("lighting-left-to-other", "--page=module:lighting --act=light:allyhub --machine=handheld --mode=guided --lights=allyhub"),
    ("lighting-closed", "--page=module:lighting --machine=handheld --mode=veteran --lights=closed"),
    ("dialog-lighting-open", "--page=module:lighting --act=light-test: --machine=handheld --mode=veteran --lights=closed"),
    ("lighting-stuck", "--page=module:lighting --machine=handheld --mode=guided --lights=stuck"),
    ("lighting-test", "--page=module:lighting --act=light-test: --machine=handheld --mode=guided --lights=open"),
    ("lighting-test-veteran", "--page=module:lighting --act=light-test: --machine=handheld --mode=veteran --lights=open"),
    ("dialog-lighting-check", "--page=module:lighting --act=light-report: --machine=handheld --mode=guided"),
    ("home-lights-other", "--page=home --machine=handheld --mode=guided --lights=allyhub"),
    ("home-lights-stuck", "--page=home --machine=handheld --mode=guided --lights=stuck"),
    ("power", "--page=module:power --machine=handheld --mode=veteran"),
    ("tuneup", "--page=module:tuneup --machine=handheld --mode=guided"),
    ("sleep", "--page=module:sleep --machine=handheld --mode=guided"),
    ("battery", "--page=module:battery --machine=handheld --mode=guided"),
    ("dock", "--page=module:dock --machine=handheld --mode=guided"),
    ("dialog", "--page=machine --act=tuneup:all --machine=handheld --mode=guided"),
    ("dialog-guide", "--page=home --act=checkup:help:hdmi --machine=tv --mode=guided"),
    ("menu", "--page=home --act=menu: --machine=handheld --mode=guided"),
    ("settings", "--page=settings --machine=handheld --mode=guided"),
    ("settings-pc", "--page=settings --machine=desktop --mode=guided"),
    ("dialog-library", "--page=settings --act=self-tile:add --machine=handheld --mode=guided"),
    ("dialog-library-steam", "--page=settings --act=self-tile:add --machine=handheld --mode=guided --steam-running"),
    ("dialog-library-unsure", "--page=settings --act=self-tile:add --machine=handheld --mode=guided --steam-unsure"),
    ("dialog-library-remove", "--page=settings --act=self-tile:remove --machine=desktop --mode=guided"),
    ("dialog-panel", "--page=store --arg=decky --act=panel-guide: --machine=handheld --mode=guided"),
    ("settings-look", "--page=settings --arg=look --machine=handheld --mode=guided"),
    ("settings-updates", "--page=settings --arg=updates --machine=handheld --mode=veteran --offline"),
    ("settings-updated", "--page=settings --arg=updates --machine=handheld --mode=guided --updated"),
    ("dialog-restart", "--page=settings --arg=updates --act=restart-app: --machine=desktop --mode=guided --window --updated"),
    ("settings-help", "--page=settings --arg=help --machine=handheld --mode=veteran"),
    ("home-unseen-library", "--page=home --machine=desktop --mode=guided --unseen-library"),
    ("games-unseen-library", "--page=games --machine=desktop --mode=guided --unseen-library"),
    ("dialog-allow-library", "--page=home --act=checkup:allow-library --machine=desktop --mode=veteran --unseen-library"),
    ("changes", "--page=changes --machine=handheld --mode=guided"),
    ("bring", "--page=bring --machine=handheld --mode=guided"),
    ("windows-drive", "--page=windrive --machine=handheld --mode=guided"),
    ("windows-apps", "--page=winapps --machine=handheld --mode=guided"),
    ("wheres-my", "--page=wheres --machine=handheld --mode=guided"),
    ("tv-home", "--page=home --machine=tv --mode=guided"),
    ("pc-home", "--page=home --machine=desktop --mode=guided"),
    ("pc-home-veteran", "--page=home --machine=desktop --mode=veteran"),
    ("pc-machine", "--page=machine --machine=desktop --mode=guided"),
    ("pc-drives", "--page=module:drives --machine=desktop --mode=guided"),
    ("pc-monitors", "--page=module:monitors --machine=desktop --mode=guided"),
    ("pc-sound", "--page=module:sound --machine=desktop --mode=guided"),
    ("pc-gpu", "--page=module:gpu --machine=desktop --mode=veteran"),
    // Undervolting (Veteran only, under Advanced): every state of the made-up RX 9070 XT with its fold open, Guided
    // (nothing of it), a handheld (nothing: its chip isn't offered it), and each of its questions.
    ("pc-gpu-uv-off", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=off"),
    ("pc-gpu-uv-pending", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=pending"),
    ("pc-gpu-uv-on", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=on"),
    ("pc-gpu-uv-checking", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=checking"),
    ("pc-gpu-uv-session", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=session"),
    ("pc-gpu-uv-deep", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=deep"),
    ("pc-gpu-uv-again", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=again"),
    ("pc-gpu-uv-tripped", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=tripped"),
    ("pc-gpu-uv-leaving", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=leaving"),
    ("pc-gpu-uv-unsupported", "--page=module:gpu --machine=desktop --mode=veteran --undervolt=unsupported"),
    ("pc-gpu-uv-guided", "--page=module:gpu --machine=desktop --mode=guided --undervolt=session"),
    ("pc-home-uv-checking", "--page=home --machine=desktop --mode=veteran --undervolt=checking"),
    ("tv-gpu-uv", "--page=module:gpu --machine=tv --mode=veteran --undervolt=checking"),
    ("tv-gpu-uv-guided", "--page=module:gpu --machine=tv --mode=guided --undervolt=tripped"),
    ("uv-handheld", "--page=module:power --machine=handheld --mode=veteran --undervolt=on"),
    ("uv-handheld-machine", "--page=machine --machine=handheld --mode=veteran --undervolt=on"),
    ("dialog-uv-on", "--page=module:gpu --act=uv-on: --machine=desktop --mode=veteran --undervolt=off --helper"),
    ("dialog-uv-restart", "--page=module:gpu --act=uv-restart:on --machine=desktop --mode=veteran --undervolt=pending --helper"),
    ("dialog-uv-try", "--page=module:gpu --act=uv-try: --value=-30 --machine=desktop --mode=veteran --undervolt=on --helper"),
    ("dialog-uv-try-deep", "--page=module:gpu --act=uv-try: --value=-120 --machine=desktop --mode=veteran --undervolt=on --helper"),
    ("dialog-uv-keep", "--page=module:gpu --act=uv-question: --machine=desktop --mode=veteran --undervolt=trying --helper"),
    ("dialog-uv-still", "--page=module:gpu --act=uv-question: --machine=desktop --mode=veteran --undervolt=asking --helper"),
    ("dialog-uv-again", "--page=module:gpu --act=uv-again: --machine=desktop --mode=veteran --undervolt=again --helper"),
    ("dialog-uv-off", "--page=module:gpu --act=uv-off: --machine=desktop --mode=veteran --undervolt=session --helper"),
    ("pc-home-uv-tripped", "--page=home --machine=desktop --mode=veteran --undervolt=tripped"),
    // My machine: every section of each made-up machine, in both modes (the ones above are the rest of them).
    ("machine-veteran", "--page=machine --machine=handheld --mode=veteran"),
    ("power-guided", "--page=module:power --machine=handheld --mode=guided"),
    ("screen", "--page=module:display --machine=handheld --mode=guided"),
    ("screen-veteran", "--page=module:display --machine=handheld --mode=veteran"),
    ("battery-veteran", "--page=module:battery --machine=handheld --mode=veteran"),
    ("sleep-veteran", "--page=module:sleep --machine=handheld --mode=veteran"),
    ("tuneup-veteran", "--page=module:tuneup --machine=handheld --mode=veteran"),
    ("dock-veteran", "--page=module:dock --machine=handheld --mode=veteran"),
    ("windows", "--page=module:windows --machine=handheld --mode=guided"),
    ("windows-veteran", "--page=module:windows --machine=handheld --mode=veteran"),
    ("pc-machine-veteran", "--page=machine --machine=desktop --mode=veteran"),
    ("pc-gpu-guided", "--page=module:gpu --machine=desktop --mode=guided"),
    ("pc-processor", "--page=module:cpu --machine=desktop --mode=guided"),
    ("pc-processor-veteran", "--page=module:cpu --machine=desktop --mode=veteran"),
    ("pc-monitors-veteran", "--page=module:monitors --machine=desktop --mode=veteran"),
    ("pc-drives-veteran", "--page=module:drives --machine=desktop --mode=veteran"),
    ("pc-sound-veteran", "--page=module:sound --machine=desktop --mode=veteran"),
    ("pc-sleep", "--page=module:sleep --machine=desktop --mode=guided"),
    ("pc-sleep-veteran", "--page=module:sleep --machine=desktop --mode=veteran"),
    ("pc-tuneup", "--page=module:tuneup --machine=desktop --mode=guided"),
    ("pc-tuneup-veteran", "--page=module:tuneup --machine=desktop --mode=veteran"),
    ("pc-windows", "--page=module:windows --machine=desktop --mode=guided"),
    ("pc-windows-veteran", "--page=module:windows --machine=desktop --mode=veteran"),
    ("tv-machine", "--page=machine --machine=tv --mode=guided"),
    ("tv-machine-veteran", "--page=machine --machine=tv --mode=veteran"),
    ("tv-gpu", "--page=module:gpu --machine=tv --mode=guided"),
    ("tv-gpu-veteran", "--page=module:gpu --machine=tv --mode=veteran"),
    ("tv-processor", "--page=module:cpu --machine=tv --mode=guided"),
    ("tv-processor-veteran", "--page=module:cpu --machine=tv --mode=veteran"),
    ("tv-screen", "--page=module:monitors --machine=tv --mode=guided"),
    ("tv-screen-veteran", "--page=module:monitors --machine=tv --mode=veteran"),
    ("tv-drives", "--page=module:drives --machine=tv --mode=guided"),
    ("tv-drives-veteran", "--page=module:drives --machine=tv --mode=veteran"),
    ("tv-sound", "--page=module:sound --machine=tv --mode=guided"),
    ("tv-sound-veteran", "--page=module:sound --machine=tv --mode=veteran"),
    ("tv-sleep", "--page=module:sleep --machine=tv --mode=guided"),
    ("tv-sleep-veteran", "--page=module:sleep --machine=tv --mode=veteran"),
    ("tv-tuneup", "--page=module:tuneup --machine=tv --mode=guided"),
    ("tv-tuneup-veteran", "--page=module:tuneup --machine=tv --mode=veteran"),
    // In a Desktop Mode window the screens take their choices; a large interface size and the light theme.
    ("screen-window", "--page=module:display --machine=handheld --mode=guided --window"),
    ("pc-monitors-window", "--page=module:monitors --machine=desktop --mode=veteran --window"),
    ("power-size-150", "--page=module:power --machine=handheld --mode=guided --scale=150"),
    ("battery-size-150", "--page=module:battery --machine=handheld --mode=guided --scale=150"),
    ("tv-machine-size-150", "--page=machine --machine=tv --mode=guided --scale=150"),
    ("machine-light", "--page=machine --machine=handheld --mode=veteran --theme=light"),
    ("battery-light", "--page=module:battery --machine=handheld --mode=guided --theme=light"),
    ("pc-monitors-light", "--page=module:monitors --machine=desktop --mode=guided --theme=light --accent=iron"),
    ("machine-helper", "--page=machine --machine=handheld --mode=guided --helper"),
    ("power-helper", "--page=module:power --machine=handheld --mode=veteran --helper --power-mode=quiet"),
    // The sections by the controls: RT twice, the highlight walked up into the chips and along them, B back to
    // Overview, a section's Options, its About, and a switch on an Overview card.
    ("machine-rt", "--page=machine --keys=rt,rt --machine=handheld --mode=guided"),
    ("machine-lt-keys", "--page=module:tuneup --keys=lt --machine=desktop --mode=guided --window"),
    ("machine-chips", "--page=module:power --keys=up,right,right,a --machine=handheld --mode=guided"),
    ("machine-b-overview", "--page=machine --keys=rt,rt,rt,b --machine=handheld --mode=guided"),
    ("options-machine-section", "--page=module:power --keys=down,x --machine=handheld --mode=guided"),
    ("dialog-about", "--page=module:battery --act=about:battery --machine=handheld --mode=guided"),
    ("dialog-card-switch", "--page=machine --keys=right,right,right,a --machine=handheld --mode=guided"),
    ("dialog-power-mode", "--page=module:power --act=power-mode: --value=0 --machine=handheld --mode=veteran"),
    ("motion-machine-section", "--page=machine --keys=rt,down,down --machine=handheld --mode=guided --motion=full"),
    // Home: every check, Guided's More, Decky Loader gone after a SteamOS update, Start's fix for all, a mode in
    // effect, and the quick switches at a large interface size and in the light theme.
    ("home-all-checks", "--page=home --act=home-all: --machine=handheld --mode=guided"),
    ("home-veteran-all-checks", "--page=home --act=home-all: --machine=handheld --mode=veteran"),
    ("home-more", "--page=home --act=home-more: --machine=handheld --mode=guided"),
    ("home-decky", "--page=home --machine=handheld --mode=guided --decky-stopped"),
    ("dialog-fix-all", "--page=home --keys=start --machine=handheld --mode=veteran --decky-stopped"),
    ("home-mode", "--page=home --machine=handheld --mode=guided --helper --power-mode=balanced"),
    ("dialog-home-mode", "--page=home --act=power-mode: --value=2 --machine=handheld --mode=guided"),
    ("home-veteran-size-150", "--page=home --machine=handheld --mode=veteran --scale=150"),
    ("pc-home-light", "--page=home --machine=desktop --mode=guided --theme=light"),
    ("tv-home-veteran", "--page=home --machine=tv --mode=veteran"),
    ("home-size-150", "--page=home --machine=handheld --mode=guided --scale=150"),
    ("machine-size-150", "--page=machine --machine=handheld --mode=guided --scale=150"),
    ("settings-size-50", "--page=settings --machine=handheld --mode=guided --scale=50"),
    ("home-light", "--page=home --machine=handheld --mode=veteran --theme=light"),
    ("games-contrast", "--page=games --machine=handheld --mode=guided --theme=contrast"),
    // Every theme and accent: the words of each picture are measured against what they are drawn on (4.5 to 1).
    ("home-dark-brass", "--page=home --machine=handheld --mode=guided --theme=dark --accent=brass"),
    ("library-dark-iron", "--page=library --machine=handheld --mode=guided --theme=dark --accent=iron"),
    ("machine-black-copper", "--page=machine --machine=handheld --mode=guided --accent=copper"),
    ("games-light-copper", "--page=games --arg=steam:1245620 --machine=handheld --mode=guided --theme=light --accent=copper"),
    ("library-light-iron", "--page=library --machine=handheld --mode=guided --theme=light --accent=iron"),
    ("settings-light-brass", "--page=settings --arg=look --machine=handheld --mode=guided --theme=light --accent=brass"),
    ("library-contrast", "--page=library --machine=handheld --mode=guided --theme=contrast"),
    ("dialog-light", "--page=machine --act=tuneup:all --machine=handheld --mode=guided --theme=light"),
    // Color vision: the three states side by side in each mode, and the row that picks it.
    ("library-vision-red", "--page=library --machine=handheld --mode=guided --vision=red"),
    ("library-vision-green", "--page=library --machine=handheld --mode=guided --vision=green --theme=light"),
    ("library-vision-blue", "--page=library --machine=handheld --mode=guided --vision=blue"),
    ("library-vision-none", "--page=library --machine=handheld --mode=guided --vision=none"),
    ("library-vision-none-light", "--page=library --machine=handheld --mode=guided --vision=none --theme=light"),
    ("home-vision-red-contrast", "--page=home --machine=handheld --mode=guided --vision=red --theme=contrast"),
    ("settings-look-vision-red", "--page=settings --arg=look --machine=handheld --mode=guided --vision=red"),
    ("settings-look-vision-green", "--page=settings --arg=look --machine=handheld --mode=guided --vision=green"),
    ("settings-look-vision-blue", "--page=settings --arg=look --machine=handheld --mode=guided --vision=blue"),
    ("settings-look-vision-none", "--page=settings --arg=look --machine=handheld --mode=guided --vision=none"),
    // The smallest interface size: words stay readable, buttons stay finger sized, and everything re-flows.
    ("home-size-50", "--page=home --machine=handheld --mode=guided --scale=50"),
    ("home-veteran-size-50", "--page=home --machine=handheld --mode=veteran --scale=50"),
    ("games-size-50", "--page=games --arg=steam:1086940 --machine=handheld --mode=guided --scale=50"),
    ("library-size-50", "--page=library --machine=handheld --mode=guided --scale=50"),
    ("store-size-50", "--page=store --arg=apps --machine=handheld --mode=guided --scale=50"),
    ("machine-size-50", "--page=machine --machine=handheld --mode=guided --scale=50"),
    ("lighting-size-50", "--page=module:lighting --machine=handheld --mode=guided --scale=50"),
    ("look-size-50", "--page=settings --arg=look --machine=handheld --mode=guided --scale=50"),
    ("dialog-size-50", "--page=machine --act=tuneup:all --machine=handheld --mode=guided --scale=50"),
    ("menu-size-50", "--page=home --act=menu: --machine=handheld --mode=guided --scale=50"),
    ("pick-size-50", "--page=pick --machine=handheld --scale=50"),
    ("setup-size-50", "--page=setup --arg=machine --machine=handheld --mode=guided --scale=50"),
    // A dense screen (a 7 inch 1080p handheld, 12 pixels to the millimetre): 6 mm for a finger is 74 pixels there.
    ("home-dense", "--page=home --machine=handheld --mode=veteran --finger=74"),
    ("games-dense", "--page=games --arg=steam:1086940 --machine=handheld --mode=guided --finger=74"),
    ("look-dense", "--page=settings --arg=look --machine=handheld --mode=guided --finger=74"),
    ("dialog-dense", "--page=module:tuneup --act=tuneup:all --machine=handheld --mode=guided --no-password --finger=74"),
    ("menu-dense", "--page=home --act=menu: --machine=handheld --mode=guided --finger=74"),
    // Animations at Full and Reduced (pictures are otherwise taken with them off): the page is worked with presses
    // while everything moves, then must come to rest on the very picture the same presses give with animations off.
    ("motion-home", "--page=home --keys=down,down --machine=handheld --mode=guided --motion=full"),
    ("motion-settings", "--page=settings --keys=right,down,down,down,a --machine=handheld --mode=guided --motion=full --installed=com.github.mtkennerly.ludusavi"),
    ("motion-games", "--page=games --keys=down,down,down,right,down,rt --machine=handheld --mode=guided --motion=full"),
    ("motion-machine", "--page=machine --keys=down,down,right --machine=handheld --mode=guided --motion=full"),
    ("motion-dialog", "--page=machine --act=tuneup:all --keys=up,down --machine=handheld --mode=guided --motion=full"),
    ("motion-menu", "--page=home --act=menu: --keys=down,down --machine=handheld --mode=guided --motion=full"),
    ("motion-library", "--page=library --keys=down,down,down,down,down,down --machine=handheld --mode=guided --motion=full"),
    ("motion-reduced-settings", "--page=settings --keys=right,down,down,down,a --machine=handheld --mode=guided --motion=reduced --installed=com.github.mtkennerly.ludusavi"),
    ("motion-reduced-dialog", "--page=machine --act=tuneup:all --keys=up,down --machine=handheld --mode=guided --motion=reduced"),
    // The page worked with real keys, a real mouse and a finger before its picture (app/qml/SelfCheck.qml).
    ("check-taps", "--page=settings --check=taps --machine=handheld --mode=guided"),
    ("check-hold-switch", "--page=settings --check=hold --focus=snapshots --machine=handheld --mode=guided --installed=com.github.mtkennerly.ludusavi"),
    ("check-hold-dialog", "--page=settings --arg=about --check=hold --focus=remove-app --machine=handheld --mode=guided"),
    ("check-hover", "--page=settings --check=hover --machine=handheld --mode=guided"),
    ("check-mouse", "--page=settings --check=mouse --machine=handheld --mode=guided"),
    ("check-typing", "--page=games --check=typing --gamemode --machine=handheld --mode=guided"),
    ("check-pointer", "--page=settings --check=pointer --focus=snapshots --machine=handheld --mode=guided --installed=com.github.mtkennerly.ludusavi"),
    ("check-drag", "--page=module:lighting --check=drag --machine=handheld --mode=guided"),
    ("check-options", "--page=games --arg=steam:1086940 --check=options --machine=desktop --mode=guided"),
    ("check-sections", "--page=machine --check=sections --machine=handheld --mode=guided"),
    // Options (X, R, a right-click, R3 held): a menu for one thing, with its own press first.
    ("options-game", "--page=games --arg=steam:1086940 --keys=x --machine=handheld --mode=guided"),
    ("options-store-item", "--page=store --arg=apps --keys=x --machine=handheld --mode=guided"),
    ("options-store-update", "--page=store --arg=installed --keys=down,down,x --machine=handheld --mode=guided"),
    ("options-machine-card", "--page=machine --keys=x --machine=handheld --mode=guided"),
    ("options-page", "--page=windrive --keys=x --machine=handheld --mode=guided"),
    ("options-keys", "--page=games --arg=steam:1086940 --keys=x --machine=desktop --mode=veteran --window"),
    ("options-size-150", "--page=machine --keys=x --machine=handheld --mode=guided --scale=150"),
    // The controller's pointer (the right stick) over a page and over a dialog; Find open; Help's controls.
    ("pointer", "--page=machine --pointer=0.36,0.47 --machine=handheld --mode=guided"),
    ("pointer-dialog", "--page=machine --act=tuneup:all --pointer=0.62,0.66 --machine=handheld --mode=guided"),
    ("find-games", "--page=games --keys=y --gamemode --machine=handheld --mode=guided"),
    ("find-store", "--page=store --keys=y --machine=desktop --mode=guided"),
    ("help-controls", "--page=help --arg=games@steam:1086940 --machine=handheld --mode=guided"),
    ("help-controls-quick", "--page=help --arg=quick@ --machine=desktop --mode=veteran"),
    ("motion-options", "--page=machine --keys=down,x,down --machine=handheld --mode=guided --motion=full"),
    // Files, the explorer. Each is taken at 1280x800 (a handheld), 1920x1080 and as a 1600x900 Desktop Mode window
    // (Explorer's strip of tabs, lines for a mouse). The page's argument sets it up: the place, the view, what is
    // highlighted (engine/src/ui/files.rs, `apply`).
    ("files-home", "--page=files --machine=handheld --mode=guided"),
    ("files-this-pc", "--page=files --arg=at=pc --machine=handheld --mode=guided"),
    ("files-downloads", "--page=files --arg=at=/home/deck/Downloads --machine=handheld --mode=guided"),
    ("files-downloads-linux", "--page=files --arg=view=linux;at=/home/deck/Downloads --machine=handheld --mode=guided"),
    ("files-veteran", "--page=files --machine=handheld --mode=veteran"),
    ("files-linux-root", "--page=files --arg=view=linux;at=/ --machine=handheld --mode=veteran"),
    ("files-c-drive", "--page=files --arg=at=/ --machine=handheld --mode=guided"),
    ("files-games", "--page=files --arg=at=games --machine=handheld --mode=guided"),
    ("files-game", "--page=files --arg=at=game:steam:1086940 --machine=handheld --mode=guided"),
    ("files-game-windows-drive", "--page=files --arg=at=/home/deck/.local/share/Steam/steamapps/compatdata/1086940/pfx/drive_c --machine=handheld --mode=guided"),
    ("files-program-files", "--page=files --arg=at=programs --machine=handheld --mode=guided"),
    ("files-appdata", "--page=files --arg=at=appdata --machine=handheld --mode=guided"),
    ("files-windows-locked", "--page=files --arg=at=windows --machine=handheld --mode=guided"),
    ("files-sd-card", "--page=files --arg=at=/run/media/deck/sdcard --machine=handheld --mode=guided"),
    ("files-recycle-bin", "--page=files --arg=at=trash --machine=handheld --mode=guided"),
    ("dialog-files-empty-bin", "--page=files --arg=at=trash --act=files-empty: --machine=handheld --mode=guided"),
    ("files-menu", "--page=files --arg=at=/home/deck/Downloads;sel=wallpaper-jade.png --keys=x --machine=handheld --mode=guided"),
    ("files-menu-folder", "--page=files --arg=at=/home/deck/Pictures --keys=x --machine=handheld --mode=guided"),
    ("files-menu-page", "--page=files --arg=at=/home/deck/Music --keys=x --machine=handheld --mode=guided"),
    ("files-menu-new", "--page=files --arg=at=/home/deck/Documents --act=files-menu:new --machine=handheld --mode=guided"),
    ("files-menu-view", "--page=files --arg=at=/home/deck/Documents --act=files-menu:view --machine=handheld --mode=guided"),
    ("files-rename", "--page=files --arg=at=/home/deck/Downloads --act=files-rename-start:/home/deck/Downloads/wallpaper-jade.png --machine=handheld --mode=guided"),
    ("dialog-files-conflict", "--page=files --arg=at=/home/deck/Pictures --act=files-drop:copy|/home/deck/Pictures --value=[\"/home/deck/Downloads/wallpaper-jade.png\"] --machine=handheld --mode=guided"),
    ("files-copying", "--page=files --arg=at=/home/deck/Downloads --act=files-demo:copy --machine=handheld --mode=guided"),
    ("dialog-files-properties", "--page=files --arg=at=/home/deck/Downloads --act=files-props:/home/deck/Downloads/wallpaper-jade.png --machine=handheld --mode=guided"),
    ("dialog-files-properties-veteran", "--page=files --arg=at=/home/deck --act=files-props:/home/deck/Downloads --machine=handheld --mode=veteran"),
    ("files-pane-picture", "--page=files --arg=at=/home/deck/Downloads;pane=1;sel=wallpaper-jade.png --machine=handheld --mode=guided"),
    ("files-pane-text", "--page=files --arg=at=/home/deck/Downloads;pane=1;sel=ProtonGE-notes.txt --machine=handheld --mode=guided"),
    ("files-pane-folder", "--page=files --arg=at=/home/deck;pane=1;sel=Pictures --machine=handheld --mode=guided"),
    ("files-search", "--page=files --arg=at=/home/deck/Downloads;search=zip --machine=handheld --mode=guided"),
    ("files-search-below", "--page=files --arg=at=/home/deck;search=wallpaper;deep=1 --machine=handheld --mode=guided"),
    ("files-search-none", "--page=files --arg=at=/home/deck/Downloads;search=zelda --machine=handheld --mode=guided"),
    ("files-large-icons", "--page=files --arg=at=/home/deck/Downloads;layout=icons --machine=handheld --mode=guided"),
    ("files-list-view", "--page=files --arg=at=/home/deck/Downloads;layout=list;sort=name --machine=handheld --mode=guided"),
    ("files-hidden", "--page=files --arg=at=/home/deck;hidden=1 --machine=handheld --mode=guided"),
    ("files-narrow", "--page=files --arg=at=/home/deck/Downloads --half --machine=handheld --mode=guided"),
    ("files-narrow-this-pc", "--page=files --arg=at=pc --half --machine=desktop --mode=guided"),
    ("files-tabs", "--page=files --arg=at=/home/deck/Downloads;tab=pc;tab=games --machine=handheld --mode=guided"),
    ("files-ticked", "--page=files --arg=at=/home/deck/Downloads --keys=start,down,start,down --machine=handheld --mode=guided"),
    ("files-view-text", "--page=files --arg=at=/home/deck/Downloads --act=files-open:/home/deck/Downloads/ProtonGE-notes.txt --machine=handheld --mode=guided"),
    ("files-view-picture", "--page=files --arg=at=/home/deck/Downloads --act=files-open:/home/deck/Downloads/wallpaper-jade.png --machine=handheld --mode=guided"),
    ("dialog-files-desktop-mode", "--page=files --arg=at=/home/deck/Downloads --act=files-open:/home/deck/Downloads/trailer-capture.mp4 --machine=handheld --mode=guided"),
    ("dialog-files-shortcut", "--page=files --arg=at=/home/deck/Desktop --act=files-new:shortcut --machine=handheld --mode=guided"),
    ("files-pointer", "--page=files --arg=at=/home/deck/Downloads --pointer=0.62,0.52 --machine=handheld --mode=guided"),
    // The same on a TV PC (125% from the couch) and on a desktop PC.
    ("files-tv-home", "--page=files --machine=tv --mode=guided"),
    ("files-tv-this-pc", "--page=files --arg=at=pc --machine=tv --mode=guided"),
    ("files-tv-downloads", "--page=files --arg=at=/home/deck/Downloads --machine=tv --mode=guided"),
    ("files-tv-downloads-linux", "--page=files --arg=view=linux;at=/home/deck/Downloads --machine=tv --mode=guided"),
    ("files-tv-game", "--page=files --arg=at=game:steam:1086940 --machine=tv --mode=guided"),
    ("files-tv-windows-locked", "--page=files --arg=at=windows --machine=tv --mode=guided"),
    ("files-tv-recycle-bin", "--page=files --arg=at=trash --machine=tv --mode=guided"),
    ("files-tv-menu", "--page=files --arg=at=/home/deck/Downloads;sel=wallpaper-jade.png --keys=x --machine=tv --mode=guided"),
    ("files-tv-pane-picture", "--page=files --arg=at=/home/deck/Downloads;pane=1;sel=wallpaper-jade.png --machine=tv --mode=guided"),
    ("files-tv-large-icons", "--page=files --arg=at=/home/deck/Downloads;layout=icons --machine=tv --mode=guided"),
    ("files-tv-tabs", "--page=files --arg=at=/home/deck/Downloads;tab=pc --machine=tv --mode=guided"),
    ("dialog-files-tv-conflict", "--page=files --arg=at=/home/deck/Pictures --act=files-drop:copy|/home/deck/Pictures --value=[\"/home/deck/Downloads/wallpaper-jade.png\"] --machine=tv --mode=guided"),
    ("files-pc-this-pc", "--page=files --arg=at=pc --machine=desktop --mode=guided"),
    ("files-pc-downloads", "--page=files --arg=at=/home/deck/Downloads --machine=desktop --mode=veteran"),
    ("files-pc-sd-card", "--page=files --arg=at=/run/media/deck/sdcard;pane=1 --machine=desktop --mode=guided"),
    // Every look, the smallest and the largest interface size, a dense screen.
    ("files-light", "--page=files --arg=at=/home/deck/Downloads;pane=1;sel=wallpaper-jade.png --machine=handheld --mode=guided --theme=light"),
    ("files-contrast", "--page=files --arg=at=pc --machine=handheld --mode=guided --theme=contrast"),
    ("files-dark-brass", "--page=files --arg=at=/home/deck/Downloads --keys=start --machine=handheld --mode=guided --theme=dark --accent=brass"),
    ("files-vision-none", "--page=files --arg=at=windows --machine=handheld --mode=guided --vision=none"),
    ("files-size-50", "--page=files --arg=at=/home/deck/Downloads --machine=handheld --mode=guided --scale=50"),
    ("files-this-pc-size-50", "--page=files --arg=at=pc --machine=handheld --mode=guided --scale=50"),
    ("files-size-150", "--page=files --arg=at=/home/deck/Downloads --machine=handheld --mode=guided --scale=150"),
    ("files-menu-size-150", "--page=files --arg=at=/home/deck/Downloads --keys=x --machine=handheld --mode=guided --scale=150"),
    ("files-dense", "--page=files --arg=at=/home/deck/Downloads --machine=handheld --mode=guided --finger=74"),
    ("motion-files", "--page=files --arg=at=/home/deck --keys=down,down,a,down --machine=handheld --mode=guided --motion=full"),
    ("motion-files-menu", "--page=files --arg=at=/home/deck/Downloads --keys=down,x,down --machine=handheld --mode=guided --motion=full"),
    ("help-files", "--page=help --arg=files@ --machine=handheld --mode=guided"),
    // Files worked with a real mouse, real keys and the controller (app/qml/SelfCheck.qml).
    ("check-files-mouse", "--page=files --arg=at=/home/deck/Downloads;sort=name --check=files-mouse --machine=desktop --mode=guided"),
    ("check-files-keys", "--page=files --arg=at=/home/deck/Downloads;sort=name --check=files-keys --machine=desktop --mode=guided"),
    ("check-files-pad", "--page=files --arg=at=/home/deck --check=files-pad --machine=handheld --mode=guided"),
    // The Store: every row with its state and its button, each section, and the line of work that runs while the
    // player browses (one being worked on, two waiting; the note when one ends; the question at closing).
    ("store-essentials", "--page=store --machine=handheld --mode=guided"),
    ("store-installed", "--page=store --arg=installed --machine=handheld --mode=guided"),
    ("store-emulation", "--page=store --arg=emulation --machine=desktop --mode=veteran"),
    ("store-working", "--page=store --arg=apps --machine=handheld --mode=guided --queue=0"),
    ("store-queue", "--page=store --arg=apps --machine=handheld --mode=guided --queue=2"),
    ("store-queue-installed", "--page=store --arg=installed --machine=handheld --mode=guided --queue=2"),
    ("store-done", "--page=store --arg=apps --act=queue-finish: --machine=handheld --mode=guided --queue=2"),
    ("store-failed", "--page=store --arg=apps --act=queue-finish: --value=You're offline. This needs the internet. --machine=handheld --mode=guided --queue=0"),
    ("store-refused", "--page=store --arg=apps --act=store:com.spotify.Client --machine=handheld --mode=guided --queue=2"),
    ("dialog-store-install", "--page=store --arg=apps --keys=right,a --machine=handheld --mode=guided"),
    ("dialog-store-update-all", "--page=store --arg=installed --keys=start --machine=handheld --mode=guided"),
    ("dialog-close-working", "--page=home --act=quit: --machine=handheld --mode=guided --queue=2"),
    ("store-tv-queue", "--page=store --arg=tv --machine=tv --mode=guided --queue=1"),
    ("store-pc-queue", "--page=store --arg=apps --machine=desktop --mode=veteran --queue=2"),
    ("store-window-queue", "--page=store --arg=apps --machine=desktop --mode=guided --window --queue=2"),
    ("store-size-150", "--page=store --arg=apps --machine=handheld --mode=guided --scale=150 --queue=2"),
    ("store-narrow", "--page=store --arg=apps --half --machine=handheld --mode=guided --queue=2"),
    ("store-light", "--page=store --arg=installed --machine=handheld --mode=guided --theme=light --queue=1"),
    ("store-contrast", "--page=store --arg=apps --machine=handheld --mode=guided --theme=contrast --queue=1"),
    ("store-vision-none", "--page=store --arg=installed --machine=handheld --mode=guided --vision=none --queue=1"),
    ("store-dense", "--page=store --arg=apps --machine=handheld --mode=guided --finger=74"),
    ("motion-store", "--page=store --keys=rt,down,right,rt --machine=handheld --mode=guided --motion=full"),
    // The top bar's status: the battery (only where there is one), the clock, the work and the updates, on each
    // kind of machine and where there is little room (the clock goes first, then the percent).
    ("topbar-handheld", "--page=home --machine=handheld --mode=veteran --queue=2"),
    ("topbar-tv", "--page=home --machine=tv --mode=guided --queue=2"),
    ("topbar-pc", "--page=home --machine=desktop --mode=guided --queue=2"),
    ("topbar-window", "--page=machine --machine=desktop --mode=guided --window --queue=2"),
    ("topbar-narrow", "--page=home --half --machine=handheld --mode=guided --queue=2"),
    ("topbar-size-150", "--page=machine --machine=handheld --mode=guided --scale=150 --queue=2"),
    ("topbar-size-50", "--page=home --machine=handheld --mode=guided --scale=50 --queue=2"),
    // What's new after an update, and the way to the other session in the menu.
    ("whats-new", "--page=home --act=whats-new-due: --machine=handheld --mode=guided --whats-new"),
    ("settings-whats-new", "--page=settings --arg=updates --machine=handheld --mode=guided --whats-new"),
    ("dialog-session-desktop", "--page=home --act=session:desktop --machine=handheld --mode=guided"),
    ("dialog-session-gamemode", "--page=home --act=session:gamemode --machine=desktop --mode=guided --window"),
    ("menu-window", "--page=home --act=menu: --machine=desktop --mode=guided --window"),
    ("dialog-validate", "--page=games --arg=steam:1086940 --act=game:validate|steam:1086940 --machine=handheld --mode=guided"),
    ("help-store", "--page=help --arg=store@apps --machine=handheld --mode=guided"),
    // The Store worked with real keys and the controller: a row's button, the line of work, the sections.
    ("check-store-row", "--page=store --arg=apps --check=store-row --machine=handheld --mode=guided"),
    ("check-queue", "--page=store --arg=apps --check=queue --machine=desktop --mode=guided --queue=2"),
    // The storage saver's section of My machine: the made-up handheld with leftovers of removed games and an SD
    // card that is nearly full, the same on the desktop and the TV PC, a Check first row ticked, the plan, after
    // a cleaning (with its Undo), while a look runs, with nothing to clear, and where the fixes are offered.
    ("storage", "--page=module:storage --machine=handheld --mode=guided --storage=full"),
    ("storage-veteran", "--page=module:storage --machine=handheld --mode=veteran --storage=full"),
    ("storage-plain", "--page=module:storage --machine=handheld --mode=guided"),
    ("storage-pc", "--page=module:storage --machine=desktop --mode=guided --storage=full"),
    ("storage-tv", "--page=module:storage --machine=tv --mode=guided --storage=full"),
    ("storage-ticked", "--page=module:storage --keys=down,down,down,down,down,down,down,a --machine=handheld --mode=guided --storage=full"),
    ("storage-unticked", "--page=module:storage --keys=a,down,a --machine=handheld --mode=guided --storage=full"),
    ("storage-steam-running", "--page=module:storage --machine=handheld --mode=guided --storage=full --steam-running"),
    ("dialog-storage", "--page=module:storage --act=storage-free: --machine=handheld --mode=guided --storage=full"),
    ("dialog-storage-steam-running", "--page=module:storage --act=storage-free: --machine=handheld --mode=veteran --storage=full --steam-running"),
    ("dialog-storage-danger", "--page=module:storage --keys=down,down,down,down,down,down,down,down,down,down,down,down,down,a,start --machine=desktop --mode=guided --storage=full"),
    ("storage-cleaned", "--page=module:storage --machine=handheld --mode=guided --storage=cleaned"),
    ("dialog-storage-undo", "--page=module:storage --act=storage-undo: --machine=handheld --mode=guided --storage=cleaned"),
    ("storage-looking", "--page=module:storage --machine=handheld --mode=guided --storage=looking"),
    ("storage-nothing", "--page=module:storage --machine=handheld --mode=guided --storage=empty"),
    ("storage-size-150", "--page=module:storage --machine=handheld --mode=guided --storage=full --scale=150"),
    ("storage-size-50", "--page=module:storage --machine=handheld --mode=guided --storage=full --scale=50"),
    ("storage-light", "--page=module:storage --machine=handheld --mode=guided --storage=full --theme=light"),
    ("storage-contrast", "--page=module:storage --machine=tv --mode=veteran --storage=full --theme=contrast"),
    ("storage-window", "--page=module:storage --machine=desktop --mode=veteran --storage=full --window"),
    ("storage-narrow", "--page=module:storage --half --machine=handheld --mode=guided --storage=full"),
    ("dialog-storage-light", "--page=module:storage --act=storage-free: --machine=handheld --mode=guided --storage=full --theme=light"),
    ("dialog-storage-size-150", "--page=module:storage --act=storage-free: --machine=tv --mode=guided --storage=full --scale=150"),
    ("home-storage-full", "--page=home --machine=handheld --mode=guided --storage=full"),
    ("home-veteran-storage-full", "--page=home --machine=handheld --mode=veteran --storage=full"),
    ("machine-storage-full", "--page=machine --machine=handheld --mode=guided --storage=full"),
    ("games-size-veteran", "--page=games --arg=steam:1086940 --machine=handheld --mode=veteran --storage=full"),
    ("help-storage", "--page=help --arg=module:storage@ --machine=handheld --mode=guided --storage=full"),
    ("dialog-about-storage", "--page=module:storage --act=about:storage --machine=handheld --mode=guided --storage=full"),
    ("motion-storage", "--page=module:storage --keys=down,down,a,down --machine=handheld --mode=guided --storage=full --motion=full"),
    // Pictures drawn for blank library tiles, and their Undo where the fix was offered and in Review changes.
    ("home-tile-art", "--page=home --act=home-all: --machine=desktop --mode=guided --tile-art=fix"),
    ("home-tile-art-drawn", "--page=home --act=home-all: --machine=desktop --mode=guided --tile-art=drawn"),
    ("dialog-tile-art-undo", "--page=home --act=checkup:tile-art-undo --machine=desktop --mode=guided --tile-art=drawn"),
    ("changes-own-undo", "--page=changes --machine=desktop --mode=guided --tile-art=drawn --storage=cleaned"),
    // Storage worked with the pointer and Start: a Check first row ticked, the dialog lists it.
    ("check-storage", "--page=module:storage --check=storage --machine=handheld --mode=guided --storage=full"),
];

fn w(base: &Path, rel: &str, text: &str) {
    let f = base.join(rel.trim_start_matches('/'));
    if let Some(d) = f.parent() {
        let _ = fs::create_dir_all(d);
    }
    let _ = fs::write(f, text);
}

/// (app id, name, folder, Valve's category or 0 for none).
const GAMES: &[(&str, &str, &str, u32)] = &[
    ("1086940", "Baldur's Gate 3", "Baldurs Gate 3", 3),
    ("1245620", "Elden Ring", "ELDEN RING", 3),
    ("1091500", "Cyberpunk 2077", "Cyberpunk 2077", 3),
    ("553850", "Helldivers 2", "Helldivers 2", 2),
    ("252950", "Rocket League", "rocketleague", 2),
    ("1172470", "Apex Legends", "Apex Legends", 1),
    ("367520", "Hollow Knight", "Hollow Knight", 3),
    ("2379780", "Balatro", "Balatro", 0),
];

const ANTICHEAT: &str = r#"[{"name":"Elden Ring","status":"Supported","storeIds":{"steam":"1245620"}},{"name":"Helldivers 2","status":"Supported","storeIds":{"steam":"553850"}},{"name":"Rocket League","status":"Running","storeIds":{"steam":"252950"}},{"name":"Apex Legends","status":"Denied","storeIds":{"steam":"1172470"}}]"#;
const FIXES: &str = r#"{"1091500":{"kind":"switch","value":"fsr4","words":"One tap: turn on the FSR 4 upgrade for a sharper picture."}}"#;

/// Steam's own settings on the made-up machine: its Proton for all games, one game on Turbine Hub's tool, one on
/// Turbine Mod Manager's, and whatever else a sample state sets (app id, tool).
pub(super) fn steam_config(steam: &Path, more: &[(&str, &str)]) {
    let mut map = format!("\"0\"\n{{\n\"name\" \"proton_11\"\n}}\n\"1086940\"\n{{\n\"name\" \"{}\"\n}}\n\"367520\"\n{{\n\"name\" \"{}\"\n}}\n", crate::compat_tool::TOOL_NAME, crate::compat_tool::MOD_TOOL_NAME);
    for (id, tool) in more {
        map.push_str(&format!("\"{id}\"\n{{\n\"name\" \"{tool}\"\n}}\n"));
    }
    w(steam, "config/config.vdf", &format!("\"InstallConfigStore\"\n{{\n\"Software\"\n{{\n\"Valve\"\n{{\n\"Steam\"\n{{\n\"CompatToolMapping\"\n{{\n{map}}}\n}}\n}}\n}}\n}}\n"));
}

/// What each made-up game takes on its drive (its main file is only said to be this big; nothing is written).
const GAME_BYTES: &[(&str, u64)] = &[("Baldurs Gate 3", 122_400_000_000), ("ELDEN RING", 48_700_000_000), ("Cyberpunk 2077", 68_300_000_000), ("Helldivers 2", 71_900_000_000), ("rocketleague", 26_100_000_000), ("Apex Legends", 74_500_000_000), ("Hollow Knight", 9_200_000_000), ("Balatro", 412_000_000)];

fn home(home: &Path) {
    let steam = home.join(".local/share/Steam");
    w(&steam, "steamapps/libraryfolders.vdf", &format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n}}\n", steam.display()));
    for (id, name, dir, _) in GAMES {
        w(&steam, &format!("steamapps/appmanifest_{id}.acf"), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{dir}\"\n\t\"buildid\"\t\t\"1\"\n}}\n"));
        w(&steam, &format!("steamapps/common/{dir}/game.exe"), "MZ");
        let _ = fs::create_dir_all(steam.join(format!("steamapps/compatdata/{id}/pfx")));
    }
    // Three Protons: two of Steam's own (one needs a runtime, which is installed) and one the player added.
    let manifest = |need: &str| format!("\"manifest\"\n{{\n  \"version\" \"2\"\n  \"commandline\" \"/proton %verb%\"\n{need}}}\n");
    for name in ["Proton 11.0", "Proton - Experimental"] {
        w(&steam, &format!("steamapps/common/{name}/proton"), "");
        w(&steam, &format!("steamapps/common/{name}/toolmanifest.vdf"), &manifest("  \"require_tool_appid\" \"4183110\"\n"));
    }
    w(&steam, "steamapps/appmanifest_4183110.acf", "\"AppState\"\n{\n\t\"appid\"\t\t\"4183110\"\n\t\"name\"\t\t\"Steam Linux Runtime 4.0\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"SteamLinuxRuntime_4\"\n}\n");
    w(&steam, "steamapps/common/SteamLinuxRuntime_4/toolmanifest.vdf", "\"manifest\"\n{\n  \"commandline\" \"/_v2-entry-point --verb=%verb% --\"\n}\n");
    w(&steam, "compatibilitytools.d/GE-Proton10-4/proton", "");
    w(&steam, "compatibilitytools.d/GE-Proton10-4/toolmanifest.vdf", &manifest(""));
    w(&steam, "compatibilitytools.d/GE-Proton10-4/compatibilitytool.vdf", "\"compatibilitytools\"\n{\n  \"compat_tools\"\n  {\n    \"GE-Proton10-4\"\n    {\n      \"install_path\" \".\"\n    }\n  }\n}\n");
    steam_config(&steam, &[]);
    // A signed-in Steam account, with no non-Steam games yet.
    let _ = fs::create_dir_all(steam.join("userdata/1001/config"));
    w(home, "homebrew/services/PluginLoader", "");
    w(home, "homebrew/plugins/decky-css/plugin.json", "{\"name\":\"CSS Loader\"}");
    w(home, "homebrew/plugins/protondb/plugin.json", "{\"name\":\"ProtonDB Badges\"}");
}

/// What the file explorer shows on the made-up machine: the explorer's own files (`fake::files`: the player's
/// folders, the card with a Steam library on it, apps, both Recycle Bins) and a Downloads folder filled over the
/// last weeks, so its list has dates to group by.
fn explorer_files(root: &Path, home: &Path, now: u64) {
    fake::files(root, home);
    // SteamOS itself, as much of it as a list needs.
    for f in ["usr/bin/bash", "usr/bin/steam", "usr/lib/os-release", "usr/lib/libc.so.6", "usr/share/applications/steam.desktop", "usr/share/fonts/readme.txt", "etc/hostname", "etc/os-release"] {
        w(root, f, if f.ends_with("os-release") { "NAME=\"SteamOS\"\nVERSION_ID=3.9.5\n" } else { "steamos\n" });
    }
    let downloads = home.join("Downloads");
    w(&downloads, "heroic-save-backup.zip", &format!("PK\x03\x04{}", "0".repeat(9000)));
    w(&downloads, "ProtonGE-notes.txt", "GE-Proton10-4 fixes the launcher of two games.\nKeep Proton 11 for the rest.\n");
    w(&downloads, "old-controller-profile.vdf", "\"controller_mappings\"\n{\n\t\"title\"\t\"Back paddles\"\n}\n");
    w(&downloads, "trailer-capture.mp4", &format!("\0\0\0\x18ftypmp42{}", "0".repeat(20_000)));
    w(&downloads, "Handheld-manual.pdf", &format!("%PDF-1.7\n{}", "0".repeat(6000)));
    let day = 86_400;
    // (name, how long ago it arrived, its size: the big ones are only said to be big, nothing is written for them.)
    let files: [(&str, u64, u64); 9] = [
        ("TurbineHub.flatpak", 1500, 3_355_443),
        ("NonSteamLaunchers-install.log", 6000, 0),
        ("heroic-save-backup.zip", 7200, 13_002_342),
        ("bg3-mod-fixer.zip", 3 * day, 50_960_793),
        ("wallpaper-jade.png", 4 * day, 0),
        ("ProtonGE-notes.txt", 4 * day + 3000, 0),
        ("old-controller-profile.vdf", 15 * day, 6144),
        ("trailer-capture.mp4", 22 * day, 192_937_984),
        ("Handheld-manual.pdf", 29 * day, 5_976_883),
    ];
    for (name, ago, size) in files {
        if let Ok(f) = fs::File::options().write(true).open(downloads.join(name)) {
            if size > 0 {
                let _ = f.set_len(size);
            }
            let _ = f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(now.saturating_sub(ago)));
        }
    }
}

fn cache(cache: &Path, now: u64, offline: bool) {
    w(cache, "anticheat.json", ANTICHEAT);
    w(cache, "fixes.json", FIXES);
    for (id, cat) in GAMES.iter().filter(|g| g.3 > 0).map(|g| (g.0, g.3)).chain([("632470", 3), ("1145360", 3), ("892970", 3)]) {
        w(cache, &format!("valve/{id}.json"), &format!("{{\"success\":1,\"results\":{{\"appid\":{id},\"resolved_category\":{cat}}}}}"));
    }
    w(cache, "checked", &(now - if offline { 3 * 86_400 } else { 600 }).to_string());
    // What the host's flatpak said last about versions and sizes.
    super::store::remember_installed(cache, "com.heroicgameslauncher.hgl\t2.17.2\t412.6\u{a0}MB\ncom.vysp3r.ProtonPlus\t0.5.9\t9.8\u{a0}MB\ncom.discordapp.Discord\t0.0.98\t318.4\u{a0}MB\n");
    super::store::remember_remote(cache, "com.heroicgameslauncher.hgl\t2.18.0\t148.3\u{a0}MB\t415.0\u{a0}MB\ncom.spotify.Client\t1.2.60\t182.7\u{a0}MB\t511.9\u{a0}MB\ncom.discordapp.Discord\t0.0.98\t101.5\u{a0}MB\t318.4\u{a0}MB\n", now - if offline { 3 * 86_400 } else { 600 });
}

fn sensors(root: &Path, preset: Preset) -> Readings {
    match preset {
        HANDHELD => Readings { cpu_c: Some(58.0), gpu_c: Some(55.0), gpu_w: Some(12.0), battery_pct: Some(78), battery_w: Some(14.2), battery_wh: Some(46.0), charging: false, fan_rpm: Some(3100) },
        Preset::Desktop => Readings { cpu_c: Some(41.0), gpu_c: Some(38.0), gpu_w: Some(34.0), battery_pct: None, battery_w: None, battery_wh: None, charging: false, fan_rpm: Some(880) },
        Preset::TvPc => {
            let _ = root;
            Readings { cpu_c: Some(44.0), gpu_c: Some(48.0), gpu_w: Some(62.0), battery_pct: None, battery_w: None, battery_wh: None, charging: false, fan_rpm: Some(940) }
        }
    }
}

fn ledger(root: &Path, preset: Preset, now: u64) {
    let mut l = Ledger::default();
    let set = |l: &mut Ledger, st: Setting, before: &str, ago: u64| {
        l.record(Change::Set(st), vec![("value".into(), before.into())], now - ago, "3.9.5");
    };
    set(&mut l, Setting::Sysctl { key: "vm.swappiness".into(), value: 100 }, "60", 86_400 * 2);
    set(&mut l, Setting::Sysctl { key: "vm.page-cluster".into(), value: 0 }, "3", 86_400 * 2);
    set(&mut l, Setting::Sysctl { key: "vm.max_map_count".into(), value: 2_147_483_642 }, "1048576", 86_400 * 2);
    if preset == HANDHELD {
        set(&mut l, Setting::Sysfs { path: "/sys/class/power_supply/BAT0/charge_control_end_threshold".into(), value: "80".into() }, "100", 3600 * 5);
        w(root, "sys/class/power_supply/BAT0/charge_control_end_threshold", "80\n");
    }
    w(root, crate::helper::LEDGER, &l.to_text());
}

/// What the background helper would have written on this machine over the last days: it is on and running, with
/// battery readings, sleeps and game sessions on the handheld.
fn agent_files(ui: &Ui, preset: Preset, now: u64) {
    use crate::history::{Sample, Session, Sleep, add_sample, add_session, add_sleep};
    let report = crate::agent::Report { at: now, version: ui.ctx.version.clone(), game: None, update: Some(String::new()), updates: ui.updates.clone(), plugins: Vec::new(), checked: now - 3 * 3600 };
    report.save(&ui.data);
    if preset != HANDHELD {
        return;
    }
    // The day before the last half hour, a reading every ten minutes: charged in the evening, played, asleep from
    // 16 to 9 hours ago (nothing was read then), charged again in the morning and on battery since.
    for i in (4..=144u64).rev() {
        let h = i as f64 / 6.0;
        let (pct, charging, temp) = if h > 22.0 {
            (60.0 + (24.0 - h) * 20.0, true, 46.0)
        } else if h > 19.0 {
            (100.0 - (22.0 - h) * 1.0, false, 44.0)
        } else if h > 16.0 {
            (97.0 - (19.0 - h) * 1.7, false, 68.0 + (i % 4) as f64)
        } else if h > 9.0 {
            continue;
        } else if h > 6.5 {
            (50.0 + (9.0 - h) * 20.0, true, 47.0)
        } else {
            (100.0 - (6.5 - h) * 3.1, false, if h < 3.0 { 62.0 + (i % 3) as f64 } else { 45.0 })
        };
        add_sample(&ui.data, &Sample { at: now - i * 600, pct: pct.round().clamp(0.0, 100.0) as u32, watts: (!charging).then_some(9.5), charging, game: String::new(), temp: Some(temp) });
    }
    for i in (0..=30u64).rev() {
        add_sample(&ui.data, &Sample { at: now - i * 60, pct: 78 + (i / 8) as u32, watts: Some(13.6 + (i % 3) as f64 * 0.6), charging: false, game: String::new(), temp: Some(58.0 - (i % 2) as f64) });
    }
    add_sleep(&ui.data, &Sleep { at: now - 3 * 86_400, secs: 9 * 3600, before: 71, after: 69, charging: false, wakers: Vec::new() });
    add_sleep(&ui.data, &Sleep { at: now - 2 * 86_400, secs: 6 * 3600 + 1200, before: 40, after: 100, charging: true, wakers: Vec::new() });
    add_sleep(&ui.data, &Sleep { at: now - 9 * 3600, secs: 7 * 3600, before: 92, after: 50, charging: false, wakers: vec!["Built-in controller".into()] });
    let play = |game: &str, name: &str, ago: u64, mins: u64, before: u32, after: u32| add_session(&ui.data, &Session { game: game.into(), name: name.into(), start: now - ago, end: now - ago + mins * 60, asleep: 0, before: Some(before), after: Some(after), on_battery: true });
    play("steam:1091500", "Cyberpunk 2077", 4 * 86_400, 75, 96, 41);
    play("steam:1086940", "Baldur's Gate 3", 3 * 86_400 + 40_000, 110, 100, 38);
    play("steam:1086940", "Baldur's Gate 3", 2 * 86_400 + 30_000, 64, 88, 50);
    play("steam:367520", "Hollow Knight", 86_400, 95, 90, 62);
    play("steam:2379780", "Balatro", 20_000, 130, 97, 70);
}

pub(crate) fn build(dir: &Path, preset: Preset, mode: Option<Mode>) -> Ui {
    let _ = fs::remove_dir_all(dir);
    let root = dir.join("root");
    fake::machine(&root, preset);
    if preset == HANDHELD {
        // The sample handheld is no particular device: its firmware just says what it is.
        w(&root, "sys/class/dmi/id/product_name", "Handheld\n");
        w(&root, "sys/bus/usb/devices/3-1/product", "Built-in controller\n");
    }
    let now = crate::now();
    // The home folder is where a real one is, under the folder that stands for "/", so the file explorer finds it
    // by its real path.
    let ctx = Ctx { root: root.clone(), home: root.join(fake::FILES_HOME.trim_start_matches('/')), config_home: dir.join("config"), efibootmgr: if preset == Preset::TvPc { String::new() } else { fake::EFI_WITH_WINDOWS.into() }, version: "0.1".into() };
    home(&ctx.home);
    explorer_files(&root, &ctx.home, now);
    for (dir, bytes) in GAME_BYTES {
        if let Ok(f) = fs::File::options().write(true).open(ctx.home.join(".local/share/Steam/steamapps/common").join(dir).join("game.exe")) {
            let _ = f.set_len(*bytes);
        }
    }
    ledger(&root, preset, now);
    let st = Settings { mode, setup_done: vec!["machine".into(), "games".into()], ..Settings::default() };
    let _ = st.save(&ctx.config_home);
    let mut ui = Ui::new(ctx, dir.join("cache"));
    cache(&ui.cache, now, false);
    ui.sample = true;
    ui.readings = Some(sensors(&root, preset));
    ui.installed = vec![super::store::HEROIC.into(), "com.vysp3r.ProtonPlus".into(), "com.discordapp.Discord".into()];
    // The launch tool is set up and Steam has started since; one game has a switch on and a few snapshots.
    let steam = ui.ctx.home.join(".local/share/Steam");
    let _ = crate::compat_tool::install(&steam, &ui.folder());
    ui.steam_started = Some(now + 60);
    let mut switches = super::Switches::default();
    switches.set("steam:1086940", crate::switches::Switch::Fsr4, true);
    switches.save(&ui.folder());
    for (ago, kind, bytes) in [(1800, "launch", 3_400_000), (86_400 * 2, "launch", 3_300_000), (86_400 * 9, "manual", 2_900_000)] {
        let d = crate::snapshots::game_dir(&ui.data, "steam:1086940").join((now - ago).to_string());
        let _ = fs::create_dir_all(&d);
        let _ = fs::write(d.join("snapshot.json"), format!("{{\"game\":\"steam:1086940\",\"name\":\"Baldur's Gate 3\",\"at\":{},\"kind\":\"{kind}\",\"bytes\":{bytes},\"files\":12}}\n", now - ago));
    }
    ui.updates = vec![super::store::HEROIC.into()];
    ui.decky_tag = Some("v3.1.10".into());
    ui.nsl_tag = Some(NSL_TAG.into());
    ui.free_bytes = Some(640_000_000_000);
    ui.sound = vec![("speakers".into(), "Speakers".into()), ("headset".into(), "Headset".into()), ("hdmi".into(), "TV over HDMI".into())];
    ui.sound_now = "headset".into();
    ui.facts = match preset {
        HANDHELD => Facts { updates: vec!["Heroic Games Launcher".into()], has_password: Some(true), windows_not_brought: true, ..Facts::default() },
        Preset::Desktop => Facts { current_hz: Some(60), screens: vec![crate::checkup::ScreenNow { name: "DP-4".into(), hz: Some(60), vrr: "automatic".into(), hdr: Some(false) }], updates: vec!["Heroic Games Launcher".into()], has_password: Some(true), windows_not_brought: true, ..Facts::default() },
        Preset::TvPc => Facts { updates: vec!["Heroic Games Launcher".into()], has_password: Some(true), ..Facts::default() },
    };
    if preset == Preset::Desktop {
        // On the PC Turbine Hub is in the Steam library already, added by itself; its pictures aren't there yet.
        let _ = crate::shortcuts::add_shortcut(&ui.ctx.home.join(".local/share/Steam/userdata/1001/config/shortcuts.vdf"), &super::selftile::tile(), super::selftile::MARKER, crate::shortcuts::Steam::Closed);
    }
    if preset != Preset::TvPc {
        let mount = dir.join("windows");
        fake::windows_drive(&mount);
        ui.windows_mount = Some(mount);
    }
    ui.agent_on = Some(true);
    agent_files(&ui, preset, now);
    ui.absorb();
    for (ago, text) in [(86_400 * 2, "Tune-up: 3 settings turned on"), (3600 * 5, if preset == HANDHELD { "Charge limit set to 80%" } else { "Sound goes to Headset" }), (3600 * 2, "Heroic Games Launcher installed"), (1500, "Checked 11 games against the ratings"), (240, "Xbox Cloud added to the library")] {
        super::note_activity(&ui.folder(), now - ago, text);
    }
    ui
}

impl Ui {
    /// A sample run on a machine where the launch tool was never set up.
    pub fn sample_without_tool(&mut self) {
        let _ = fs::remove_dir_all(crate::compat_tool::tool_dir(&self.ctx.home.join(".local/share/Steam")));
    }
    /// A sample run where NonSteamLaunchers was started by Turbine Hub at the newest release, with Battle.net in its
    /// shared Windows files and the EA app in its own (the places NonSteamLaunchers.sh looks).
    pub fn sample_with_nsl(&mut self) {
        let cd = self.ctx.home.join(".local/share/Steam/steamapps/compatdata");
        w(&cd, "NonSteamLaunchers/pfx/drive_c/Program Files (x86)/Battle.net/Battle.net Launcher.exe", "MZ");
        let _ = fs::create_dir_all(cd.join("TheEAappLauncher/pfx/drive_c/Program Files/Electronic Arts/EA Desktop/13.1"));
        let _ = crate::nsl::remember(&self.folder(), NSL_TAG);
    }
    /// A sample run where Steam lists a second library on a drive Turbine Hub isn't allowed to read.
    pub fn sample_with_unseen_library(&mut self) {
        let steam = self.ctx.home.join(".local/share/Steam");
        // (The card's library stays in the list: only the third one is out of reach.)
        let card = self.ctx.root.join(fake::FILES_DRIVE.trim_start_matches('/'));
        w(&steam, "steamapps/libraryfolders.vdf", &format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n\"1\"\n{{\n\"path\" \"{}\"\n}}\n\"2\"\n{{\n\"path\" \"{UNSEEN_LIBRARY}\"\n}}\n}}\n", steam.display(), card.display()));
    }
    /// A sample run where Turbine Hub's helper is set up already: no change asks for the password.
    pub fn sample_with_helper(&mut self) {
        w(&self.ctx.root, crate::helper::BIN.trim_start_matches('/'), "");
    }
    /// A sample run where the host couldn't say whether Steam is running.
    pub fn sample_steam_unsure(&mut self) {
        self.steam_running = true;
        self.steam_unsure = true;
    }
    /// A sample run right after a newer Turbine Hub was installed.
    pub fn sample_updated(&mut self) {
        self.update = Some("0.9".into());
        self.update_installed = Some("0.9".into());
    }
    /// A sample run with the lights in one of their states: "allyhub" (Ally Hub's agent is setting them too),
    /// "huesync" (the plugin is installed and Decky Loader runs), "closed" (set up before, closed again), "stuck"
    /// (opening them again didn't hold), "open" (set up and open, ready for the light test).
    pub fn sample_lights(&mut self, state: &str) {
        use super::lighting::{AllyHub, HostLights, Lighting};
        use std::os::unix::fs::PermissionsExt;
        let m = self.machine();
        let folder = self.folder();
        Lighting::load(&folder).save(&folder);
        let rule = turbinehub_helper::policy::Access::Lighting.rule().to_string();
        // (Decky Loader is on the made-up machines and runs.)
        let mut host = HostLights { asked: true, decky_running: true, rule: Some(rule), reapply: "enabled".into(), ..HostLights::default() };
        let setup = |ui: &Ui| {
            let mut l = ui.ledger();
            l.record(Change::Set(crate::modules::lighting_access()), Vec::new(), crate::now() - 3600, "3.9.5");
            w(&ui.ctx.root, crate::helper::LEDGER, &l.to_text());
        };
        let files = |ui: &Ui, mode: u32| {
            for led in &m.rgb_leds {
                for f in ["multi_intensity", "brightness"] {
                    let _ = fs::set_permissions(ui.ctx.root.join("sys/class/leds").join(led).join(f), fs::Permissions::from_mode(mode));
                }
            }
        };
        match state {
            "allyhub" => {
                host.allyhub = Some(AllyHub { controller: "allyhub".into(), hid_method: "m6".into(), encoding: "hid".into() });
                host.allyhub_agent = true;
                setup(self);
                files(self, 0o666);
            }
            "huesync" => {
                w(&self.ctx.home, "homebrew/plugins/HueSync/plugin.json", "{\"name\":\"HueSync\"}");
                host.decky_running = true;
                setup(self);
                files(self, 0o666);
            }
            "closed" | "stuck" => {
                setup(self);
                files(self, 0o644);
                if state == "stuck" {
                    let need: Vec<String> = crate::lights::driven(&self.ctx.root, &m.rgb_leds, "").iter().flat_map(crate::lights::needs).collect();
                    let _ = super::lighting::check_open(&self.ctx.root, &folder, &need, false);
                }
            }
            "open" => {
                setup(self);
                files(self, 0o666);
            }
            _ => {}
        }
        self.host_lights = host;
    }
    /// A sample run after a SteamOS update took Decky Loader's service away: its files are there, it doesn't run.
    pub fn sample_decky_stopped(&mut self) {
        self.host_lights.asked = true;
        self.host_lights.decky_running = false;
    }
    /// A sample run with the machine in one of the power modes ("quiet", "balanced", "boost"): the kernel holds
    /// that mode's values.
    pub fn sample_power_mode(&mut self, mode: &str) {
        use crate::modules::{PowerMode, power_mode};
        let Some(mode) = PowerMode::ALL.into_iter().find(|x| x.title().eq_ignore_ascii_case(mode)) else { return };
        for x in power_mode(&self.ctx.root, &self.machine(), mode) {
            if let Setting::Sysfs { path, value } = x {
                w(&self.ctx.root, path.trim_start_matches('/'), &format!("{value}\n"));
            }
        }
    }
    /// A sample run with non-Steam games that show as blank tiles ("fix"), or with their pictures drawn by Turbine
    /// Hub already ("drawn"), so there is something to undo.
    pub fn sample_tile_art(&mut self, state: &str) {
        let file = self.ctx.home.join(".local/share/Steam/userdata/1001/config/shortcuts.vdf");
        for (name, exe) in [("Heroic Games Launcher", "/usr/bin/flatpak"), ("Old Tower Defense", "/home/deck/Games/tower/tower.x86_64")] {
            let entry = crate::shortcuts::NewShortcut { name: name.into(), exe: format!("\"{exe}\""), start_dir: "\"/home/deck/\"".into(), launch_options: format!("--sample-entry={}", name.replace(' ', "-")), flatpak_id: String::new(), icon: String::new() };
            let _ = crate::shortcuts::add_shortcut(&file, &entry, &entry.launch_options, crate::shortcuts::Steam::Closed);
        }
        if state == "drawn" {
            let _ = crate::tile_art::fix(&self.ctx.home, &self.folder(), crate::shortcuts::Steam::Closed);
        }
    }
    /// A sample run with the made-up desktop's RX 9070 XT in one of undervolting's states: "pending" (tuning turned
    /// on, the restart not made yet), "on" (on after the restart, the card at its own voltage), "trying" (-30 mV
    /// just tried, the 10 second question), "checking" (-30 mV in its 5 minutes, 4:12 left), "asking" (the 5
    /// minutes over, the 60 second question), "session" (-30 mV passed and is on until the restart), "deep" (-110
    /// mV on, both slider warnings showing), "again" (after a restart: -30 mV passed before on this kernel, the card
    /// at 0, "Use -30 mV again"), "tripped" (-60 mV was on when the machine stopped), "leaving" (on, but turned off
    /// and waiting for the restart), "unsupported" (an RX 6800 in its place). The Advanced fold opens, and the
    /// checks' clock stands still so the pictures look the same every time.
    pub fn sample_undervolt(&mut self, state: &str) {
        use crate::uvcheck::{Stage, Underway};
        let root = self.ctx.root.clone();
        let mut l = self.ledger();
        let m = self.machine();
        let now = crate::now();
        self.uv.clock.set(1_000_000);
        if state == "unsupported" {
            let dev = format!("sys/devices/pci0000:00/{}", fake::GPU_SLOT);
            w(&root, &format!("{dev}/device"), "0x73bf\n");
            w(&root, &format!("{dev}/ip_discovery/die/0/MP1/0/major"), "11\n");
            w(&root, &format!("{dev}/ip_discovery/die/0/MP1/0/revision"), "7\n");
            self.uv_open = true;
            return;
        }
        if matches!(state, "pending" | "on" | "trying" | "checking" | "asking" | "session" | "deep" | "again" | "tripped")
            && let Some(st) = crate::modules::gpu_overdrive(&m)
        {
            l.record(Change::Set(st), Vec::new(), now - 86_400, "3.9.5");
        }
        let live = match state {
            "trying" | "checking" | "asking" | "session" => ("kept", -30),
            "deep" => ("kept", -110),
            "on" | "leaving" | "again" | "tripped" => ("on", 0),
            _ => ("", 0),
        };
        fake::undervolt(&root, live.0, live.1);
        self.uv.slot = fake::GPU_SLOT.into();
        let kernel = self.machine().kernel;
        let at = self.uv.clock.now();
        let underway = |stage, left_ms| Some(Underway { stage, offset: -30, back: 0, until: at + left_ms, short: false });
        match state {
            "trying" => self.uv.session.underway = underway(Stage::Ask, crate::uvcheck::ASK_MS),
            "checking" => self.uv.session.underway = underway(Stage::Check, 252_000),
            "asking" => self.uv.session.underway = underway(Stage::Again, crate::uvcheck::AGAIN_MS),
            "session" | "deep" => {
                self.uv.session.on = Some(live.1);
                self.uv.note = Some(super::undervolt::Note { offset: live.1, kernel });
            }
            "again" => self.uv.note = Some(super::undervolt::Note { offset: -30, kernel }),
            "tripped" => {
                w(&root, turbinehub_helper::gpu::TRIPPED.trim_start_matches('/'), &turbinehub_helper::gpu::tripped_text(-60, now - 600));
                self.uv.note = Some(super::undervolt::Note { offset: -40, kernel });
            }
            _ => {}
        }
        w(&root, crate::helper::LEDGER.trim_start_matches('/'), &l.to_text());
        self.uv_open = true;
    }
    /// A sample run as if the internet were gone: the lists were fetched three days ago.
    pub fn sample_offline(&mut self) {
        self.offline = true;
        cache(&self.cache, crate::now(), true);
    }
}

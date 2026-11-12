# Turbine Hub design

Turbine Hub is the first stop after installing SteamOS: everything the "first things to do" guides tell people to paste into Konsole, done from the couch with a controller (or at a desk with a mouse), explained in Windows terms, checked before it runs, and undoable. It is a standalone SteamOS app and covers three kinds of machine: handhelds, desktop PCs, and media-center PCs used in Game Mode on a TV.

The app name is two words, **Turbine Hub**. The repository, Flatpak ID and code use **TurbineHub** / `turbinehub`.

Turbine Mod Manager (github.com/Ravenor907/Turbine-Mod-Manager) is the reference for every engineering choice. Where this document is silent, do what Turbine does.

## 1. Who it is for

- **Guided mode**: people new to SteamOS, often coming from Windows. Walks them through setup, explains things in Windows terms, hides expert controls.
- **Veteran mode** ("I know more than you."): people who know what they want. One-screen setup, a dashboard Home, every control visible.
- Both modes have the same features. The first-run screen asks which one; Settings switches any time without losing anything.

Machines, all first-class:
- Handhelds of any make, detected by what they have (battery, built-in screen, case type), never from a list of devices.
- Desktop PCs in Game Mode and in Desktop Mode with mouse and keyboard.
- TV PCs in Game Mode with a controller only.

## 2. Product

### Tabs
Home, Games, Store, My machine, Settings. LB/RB (Q/E on a keyboard) switch tabs.

### First run
1. Mode picker (Guided or Veteran).
2. Guided: Your machine (detected), Password, Will my games run?, Bring it over, Picks.
3. Veteran: Quick setup, one screen of checkboxes, one password, one go. "Load My setup file" applies a saved setup.

### Home
- Guided: what needs attention first ("Two things could use a look"), the checkup, health stats, a "Finish setting up" card until setup is done.
- Veteran: dashboard (temps, power, battery with time left, fans), quick switches, compact checkup, activity log. No fps tile: nothing in Turbine Hub measures it (MangoHud's overlay does, per game).
- The battery tile says how long the charge lasts: the energy left over the average draw of the last ten minutes.
- The checkup lists only problems, each with its fix. It includes "known limits" for the hardware in plain words (Nvidia drivers, 4K 120 Hz over HDMI, one screen in Game Mode) and PC checks (monitor below its refresh rate, unused drive).

### Will my games run? (library check)
Every owned game (Steam, plus Epic, GOG and Amazon through Heroic, plus Game Pass titles) gets Works, Needs a fix (one tap), or Won't run here with the best alternative (GeForce NOW, Xbox Cloud, or Restart to Windows). Data: Valve's SteamOS ratings, anti-cheat lists, Turbine Hub's own fixes. Fetched fresh, never pinned in the app.

### Bring it over from Windows
- Other stores: Heroic (Epic, GOG, Amazon) and NonSteamLaunchers (Battle.net, EA, Ubisoft, Rockstar and more), each launcher leading to NonSteamLaunchers' page in the Store.
- Game Pass: Xbox Cloud as a tile. Local Game Pass (Xodus Nexus) only under Veteran, labeled Experimental.
- Your Windows drive: found automatically, read only, never written. Copies Steam games into a SteamOS library (no re-download) and saves into the right Proton prefixes. Saves use Ludusavi's CLI with the Windows drive added as an "other Windows" root.
- Your Windows apps: pick the Windows apps you use, get the SteamOS version of each in one install (Discord, MangoHud and MangoJuice for MSI Afterburner, Steam Input for DS4Windows, Xbox Cloud for the Xbox app, and so on).
- TV and streaming: Netflix, YouTube, Disney+, Twitch, Plex as controller-ready tiles.
- "Where's my...?": Windows things and where they are on SteamOS, and the glossary. VIEW (Help) on any page first explains that page and its buttons, with "Where's my...?" one press further.

### Games
Per-game switches (FSR 4 upgrade, frame generation, Handheld mode, Proton log, Proton choice), "Won't start?" help (another Proton, reset Windows files with a backup, write a report that stays on the machine), the save time machine (snapshot at every launch, restore, undo). Mods open Turbine Mod Manager. Games whose anti-cheat runs online are locked: Turbine Hub never changes them.

### Store
Essentials, Apps (Flathub), Decky plugins (with turn-off and "Turn all off" for troubleshooting), Launchers, TV and streaming, Emulation (EmuDeck), Installed. Update all. Everything installed keeps itself current.

NonSteamLaunchers (github.com/moraroy/NonSteamLaunchers-On-Steam-Deck, `engine/src/nsl.rs`) has its own page under Launchers. Install, Open and Update fetch the project's own `NonSteamLaunchers.sh` at its newest release tag (never `main`), check it names that version, and start it on the host with `flatpak-spawn --host bash`, as the player, without the helper or a password. Its windows need Desktop Mode, so in Game Mode the page says how to get there. With Decky Loader there, the page also offers its Decky plugin through the project's `NSLPluginInstaller.sh` (which asks for the password in its own window). Installed is read from its Windows files in Steam's compatdata folder; the version is the release Turbine Hub last started.

### My machine
Capability modules, shown only when the hardware has them:
- Handheld: Power (TDP, Game Boost), Display (the built-in screen's refresh range, from its own EDID), Lighting (any RGB lights the kernel exposes), Battery (charge limit, history), Sleep (drain per sleep, wake sources), Dock, Tune-up, Restart to Windows.
- PC: Graphics card (fan curve, power limit, safe reset), Processor (Game Boost), Monitors (refresh, VRR, HDR, which screen Game Mode uses), Drives (set up a drive as a game library, Windows drives read only), Sound (output), Sleep and wake (Wake on LAN, controller wake, USB wake), Tune-up, Restart to Windows.
- TDP is shown only where the kernel exposes power limits, and hidden while Steam's own slider or a TDP plugin manages it.

### Settings
Mode, interface size (50 to 150%, re-flows), animations, the background helper (on or off) and the save time machine, theme and accent (Turbine's design system), updates, My setup (save and apply), help and reports (Labs mode as in Turbine), Bring it over again, "Review changes" (undo anything, one step at a time), remove Turbine Hub completely.

### Deferred
Phone remote and Wake-on-LAN web page: after the core is solid.

### Wording rules
- Player-facing words are short and plain, with one glossary used everywhere.
- No past tense about the player ("Your Windows apps", not "I used to use").
- No greetings or time-of-day remarks.
- No placeholder art: game and app art only where real art exists (Steam's art, Flathub icons), otherwise the name alone.
- No em or en dashes.

### Controls and layout (from Turbine)
- One footer hint bar with a fixed priority order. A and B are never dropped, VIEW and START have short labels, LB/RB show tab names. With a keyboard the hints switch to keys (Enter, Esc, Q/E, F1).
- One dialog template: sizes to its content, buttons pinned, controller glyph inside each button.
- Distinct focus, selected and primary styles. Every action has its own icon.
- Scroll fades, the focused item always fully in view, toasts inside the content area, top insets, dark scrims behind overlays.
- Body text at least 14 px at 1280x800. Interface size 50 to 150% re-flows instead of truncating.
- One motion system with an Animations on/off setting.
- In Game Mode everything opens inside the app, never in a new window.
- Screens are checked at 1280x800, 1920x1080 and a 1600x900 Desktop Mode window.

## 3. Architecture

```
TurbineHub/
  Cargo.toml          workspace: engine, app, helper (edition 2024, toolchain 1.97.0 in CI)
  VERSION             one-dot version, e.g. 0.1
  engine/             all logic, std only, unit tested
  app/                Qt 6 QML UI through cxx-qt (+ a little C++ glue)
    qml/              UI glue only; SelfCheck.qml loads only in --screenshot mode
  helper/             turbinehub-helper: the privileged helper, std only, tiny
  decky/              optional quick panel: TypeScript frontend, Python backend that only calls --json
  packaging/          Flatpak manifest, desktop file, metainfo, icons, Steam art
  tools/              ci-shots.sh, qml-check.js, release-check.py (from Turbine)
  .github/workflows/  build.yml (from Turbine)
```

### engine (std only)
Copied from Turbine where they fit: `json`, `vdf`, `sha256`, `zip`, `net` (curl with fixed arguments), `steam`, `shortcuts`, `steam_art`, `compat_tool`, `selfupdate`, `report`, `anticheat` (the online anti-cheat list only).

New:
- `hardware`: reads DMI, sysfs, DRM and PCI once into a `Machine` (kind, CPU, GPU, panel, battery, lighting, power limits, drives, monitors). Each module asks it for a `Capability`.
- `modules/*`: one file per capability (power, display, lighting, battery, sleep, dock, gpu, cpu, monitors, drives, sound, wake, tuneup, windows_boot). Each answers: applies? current state? what change would X make? what does undo look like?
- `checkup`: rules that turn state into "needs a look" items with a fix.
- `library`: owned games from Steam and Heroic, Valve ratings, anti-cheat status, the three labels and the alternative.
- `bring`: stores, Game Pass, Windows drive scan (games and Ludusavi save plan), Windows app map.
- `store`: Flathub (via `flatpak` on the host), Decky store, launchers, streaming tiles.
- `saves`: the Windows drive's saves on top of the Ludusavi CLI: read with settings of Turbine Hub's own (the drive as an "other Windows" root, its Steam folders as Steam roots) and restored per game with every place on the drive sent to a folder on this machine, so nothing is ever written to the drive.
- `snapshots`: the save time machine's snapshots on top of the Ludusavi CLI (one folder per snapshot, the newest few kept).
- `agent`: every decision of the background agent (see below). `history`: its battery, sleep and session files and what the screens make of them. `lights`: lighting effects as frames for any RGB light.
- `sources`: everything fetched online, each with a saved copy, a second host where one exists, and a built-in copy; `refresh` is the one place the app goes online for the anti-cheat list, the fixes and Valve's ratings.
- `ledger`: the change record shared with the helper (see 4).
- `settings`, `glossary`, `mode`.
- `cli`: `--json` verbs for the Decky backend and scripts.
- `ui`: what every screen shows and what every press does. Each page is one JSON object; each press answers with a note, a page to open, or the one dialog with its exact steps (`Plan`, `Step`). It also holds the per-game switches file, the lighting file, the Store's list, the setup file and the made-up machine used for pictures and tests.

Every decision (filtering, sorting, what to warn about, what a check means) is Rust with tests. QML only shows what the engine says.

### app
One cxx-qt bridge object, `Hub`, hands QML the engine's pages (`Hub.page`) and takes presses (`Hub.act`, `Hub.confirm`). The steps of a confirmed dialog and the questions to the host and the internet run on their own threads and come back through `Hub.poll`, together with the controller (SDL). `app/qml/COMPONENTS.md` lists the parts.

`turbinehub --screenshot=<png> --size=<W>x<H> --page=<page> [--arg=<the page's own>] [--act=<action>:<argument> [--value=<typed text>]] [--machine=handheld|desktop|tv] [--mode=guided|veteran] [--window] [--keys=down,a] [--installed=<app id>] [--no-tool]` renders a page on a made-up machine and loads SelfCheck.qml, which checks cut-off text, overflow, text size, the footer's hints and the highlight. `turbinehub --shots` lists the pictures the build takes.

### Background agent
The same Flatpak run as `turbinehub --agent`, started by a systemd **user** unit, `~/.config/systemd/user/turbinehub-agent.service` (user-level, no password, survives SteamOS updates). Settings, Background helper writes the unit through `flatpak-spawn --host` and enables it; turning it off disables and removes it. Players see it as "the background helper".

What it does, all decided in `engine/src/agent.rs` (the app's `agent.rs` only looks, runs the commands it is handed and brings the answers back):

- **Games.** Steam starts every game under its `reaper` program with `SteamLaunch AppId=<id>` in the arguments. The agent reads the process list every few seconds (through `pgrep` on the host, because the sandbox has its own /proc) and changes nothing about Steam.
- **Game Boost.** With the setting on and the privileged helper installed: `set-now` for each CPU policy's energy preference at a game's start, and the values from before at its end (kept in its state file, so a restart of the agent still puts them back). It is a processor setting, not a change to a game, so it applies to every game, including those with online anti-cheat.
- **Save snapshots.** With the save time machine on and Ludusavi installed: a snapshot of the game's saves at each start (see `engine/src/snapshots.rs` for the layout), at most one per game in ten minutes, the newest five kept, unchanged saves skipped. Reading save files touches no game.
- **Battery.** A reading a minute into `history/battery.jsonl` (a week of them), each game session into `history/sessions.jsonl`. From these: time left, and battery use per game (sessions that ran on battery from start to end).
- **Sleep.** A wall clock that jumped ahead of the steady clock is a sleep, confirmed by the kernel's count in `/sys/power/suspend_stats/success`. The charge before and after and the USB devices whose wake count rose go into `history/sleep.jsonl`; the checkup's sleep item and the Sleep page read it.
- **Lighting.** Frames for every `*:rgb:*` light in `/sys/class/leds` (static, breathe, heartbeat, color cycle, wave, spiral, candle, twinkle; the charge as a color; a color per game), 20 a second on wall power and 8 on battery, only the files that changed. Only once the player has saved lighting choices and the helper's lighting access rule has opened the files. Inside the sandbox /sys is read only, so each file gets one long-lived `tee` on the host.
- **Dock.** On a handheld, an outside screen connecting or going away: lights off while docked and sound to the TV's output and back, as the Dock page says.
- **Updates.** Once a day, never while a game runs: a newer Turbine Hub, Flathub apps (`flatpak remote-ls --updates`), Decky plugins against the plugin store's list. It only reports; installing is the player's press in the app.
- **After a SteamOS update.** See 4.6.

It talks to the app through files in the app's data folder, no socket: `agent/status.json` (still running, the game, updates found), `agent/drift.json`, the history files, and lines in the activity list. One agent at a time (a lock file). `turbinehub --agent --once --machine=handheld` runs it once on a made-up machine and prints what it would run; the build's checks do that.

### Steam integration
- Adds itself to the Steam library with its own art (`ui/selftile.rs` on Turbine's shortcuts and steam_art), so it opens from Game Mode: offered in the first-run picks, in Settings (add, fix art, remove) and as a checkup tip. Steam's list of non-Steam games is only written while Steam is closed, with a backup first; while Steam runs the player gets the steps. The art in `packaging/steam-art` is drawn by `tools/steam-art.py` and never replaces a picture the player picked.
- The quick panel for Decky Loader is not installed by the app: Store, Decky plugins shows whether it is there (by its package name, read only) and how Decky Loader installs it from the release's `TurbineHub-Decky.zip`.
- Per-game switches reach a game through Turbine Hub's own compatibility tool, "Turbine Hub" (`engine/src/compat_tool.rs`, built the way Turbine Mod Manager builds its tool): three files in Steam's `compatibilitytools.d`, a folder Steam only reads. The player picks it once per game in the game's Properties, Compatibility; Steam lists a new tool after its next start, which the player does once. Turbine Hub never writes Steam's own files and never restarts Steam: it only reads `config.vdf` to see which tool a game uses.
- The tool's script builds the line Steam builds itself (the Steam Linux Runtime the Proton asks for, that Proton, the game), then reads the game's line in Turbine Hub's games file (`games.json`, one game per line) and sets what each switch stands for: the FSR 4 upgrade, Handheld mode and the Proton log are environment variables, frame generation wraps the game's command in the Lossless Scaling plugin's launcher when that is installed. Which Proton: the player's pick for the game (a file in Turbine Hub's `launch` folder, with Undo), else Steam's choice for all games, else the newest installed. With Turbine Hub's files gone the game still starts.
- Steam allows one tool per game. A game that uses Turbine Mod Manager's tool gets its switches from Turbine Mod Manager, and Turbine Hub says so instead of showing switches.
- Native Linux games, games that won't run here and games with online anti-cheat are never offered the tool or any switch. The games with online anti-cheat are also listed for the script, which starts them as they are even if someone picks the tool for one.

### Decky quick panel
Optional. TypeScript frontend, Python backend that only runs `flatpak run io.github.ravenor907.TurbineHub --json <verb>`. No logic of its own.

### Flatpak
org.kde.Platform/Sdk 6.11 with rust-stable, offline build, vendored crates, committed Cargo.lock, built in ghcr.io/flathub-infra/flatpak-github-actions:kde-6.11. Permissions as Turbine's, plus what the modules need (read-only `/sys` and `/run/udev` data, `/run/media`, Ludusavi's and Heroic's folders). Host actions use `flatpak-spawn --host`.

ID: `io.github.ravenor907.TurbineHub`.

## 4. The privileged helper

### 4.1 Goals
- Small and auditable: one std-only Rust binary, a fixed list of verbs, no shell, no free-form commands, no arbitrary paths.
- Every change recorded, backed up first, undoable.
- Survives SteamOS updates, or is put back automatically after one.
- Never touches the read-only system: no pacman, no `steamos-readonly disable`, nothing under `/usr`.
- One password, once.

### 4.2 Where things live
SteamOS keeps `/etc` (an overlay) and `/var` across updates; `/usr` is replaced. So:

| What | Where | Owner |
|---|---|---|
| Helper binary | `/var/lib/turbinehub/bin/turbinehub-helper` | root, 0755 |
| Change ledger | `/var/lib/turbinehub/ledger.json` | root, 0644 |
| Backups of every file it changes | `/var/lib/turbinehub/backup/<change id>` | root |
| Helper log | `/var/lib/turbinehub/helper.log` | root, 0644 |
| Permission rule | `/etc/polkit-1/rules.d/49-turbinehub.rules` | root |
| Boot re-apply unit | `/etc/systemd/system/turbinehub-reapply.service` | root |

The helper files are root-owned and not writable by the user, so the user account can't swap the binary that the rule allows.

### 4.3 How the app calls it
1. **First install (the one password):** the in-app password dialog (works in Game Mode, where the system has no password dialog of its own) runs `flatpak-spawn --host sudo -S -k <helper inside the Flatpak> install` with the password on stdin. The helper copies itself to `/var/lib/turbinehub/bin`, writes the polkit rule and the boot unit, and records which user it serves. The password is never stored.
2. **After that:** `flatpak-spawn --host pkexec /var/lib/turbinehub/bin/turbinehub-helper <job> <json>`. The polkit rule says YES only for that exact program, for the active local session of its owner. No password, works in Game Mode.
3. **Helper updates ask for the password again** (step 1). They are rare, and a password-free self-update would let any program running as the player replace the helper.
4. The helper refuses to run unless its folder and file are root's and writable by nobody else, and refuses callers other than its owner.

### 4.4 Jobs (the whole list)
Config files are **never** taken from the caller: a hand-written sysctl, udev, modprobe, tmpfiles or systemd file can run any command as root. The caller sends a typed **setting**; the helper checks it against fixed lists (`helper/src/policy.rs`) and writes the files itself.

| Job | Does |
|---|---|
| `apply <setting>` | Checks, records "before", applies, writes the files, records the change |
| `undo {id}`, `undo-all` | Puts things back from the record, newest first |
| `verify` | Lists what differs between the record and the machine (read only) |
| `reapply` | Puts the record back in effect (boot unit, after SteamOS updates) |
| `set-now {path,value}` | A listed /sys value right now, not recorded (Game Boost while a game runs; gone at reboot) |
| `decky-install {tag}` | Downloads Decky Loader's official release itself and installs it only if it matches the SHA-256 GitHub publishes |
| `boot-next {entry}` | Next boot only: an entry efibootmgr lists as Windows Boot Manager |
| `status` | Version and the record |
| `install` | Through sudo only: installs or updates the helper |
| `uninstall` | `undo-all`, then removes the rule, unit and helper |

Settings:

| Setting | Allowed | Lives in |
|---|---|---|
| `sysctl` | Listed keys (swappiness, page-cluster, dirty bytes, max_map_count, split-lock...) with ranges | `/etc/sysctl.d/99-zz-turbinehub.conf` + applied now |
| `sysfs` | Listed /sys patterns (charge limit, EPP, governor, CPU boost, THP, wake flags, GPU power cap, firmware TDP) with value checks; the path must exist and stay inside /sys with links followed; limits read from the kernel's own min/max files where they exist | runtime, re-applied at boot |
| `wake` | One USB device (vendor:product hex) may or may not wake the machine | `/etc/udev/rules.d/71-turbinehub-wake.rules` |
| `access` | Fixed rules: `lighting` (the kernel's RGB LED files, for the agent's animations), `ntsync` | `/etc/udev/rules.d/70-turbinehub-access.rules` |
| `zram` | 25 to 200% of memory; zstd, lz4 or lzo-rle | `/etc/systemd/zram-generator.conf.d/turbinehub.conf` (next boot) |
| `module` | `ntsync` | `/etc/modules-load.d/turbinehub.conf` + loaded now |
| `service` | `sshd`, `plugin_loader` (Decky), `tailscaled`: on or off | systemd |

Drive formatting is **not** a helper job. The Drives module hands off to SteamOS's own formatting in Steam's Storage settings.

### 4.5 The ledger
One JSON file, one entry per setting (a second change to the same setting updates it and keeps the first "before"):

```json
{"id":7,"words":"Kernel setting vm.swappiness set to 100",
 "change":{"kind":"sysctl","key":"vm.swappiness","value":100},
 "before":{"value":"60"},"applied":1794455000,"steamos":"3.9.5"}
```

- The files under /etc are always written from the ledger, so the ledger and the files can't disagree, and a tampered ledger with an unlisted setting is refused when read.
- Decky's entry keeps backups of any loader and unit that were there before, in `/var/lib/turbinehub/backup/`.
- The app shows the ledger as **Settings, Review changes**: every change in plain words, with Undo. Under it, the changes made in the player's own folders without the password (the launch tool, Proton picks, game switches, library tiles, the background helper's start-up entry, apps the Store installed), each with its Undo or its page.

### 4.6 SteamOS updates
- Files in `/etc` and everything in `/var` survive.
- Runtime values are put back at every boot by `turbinehub-reapply.service`.
- After an update (the agent sees the SteamOS version change), the agent runs `verify` and writes down what differs. The app runs `verify` at its own start too. Anything that drifted shows in the checkup with one tap to put it back (`reapply`), and the activity list says so in one line. Anything SteamOS now does itself (for example a tune-up item that became default) shows as "SteamOS does it" in the tune-up.

### 4.7 Safety rules
- Check before applying: every module states the exact change and the undo before asking.
- Back up before any change.
- Never touch a game whose anti-cheat runs online.
- Never write to a Windows (NTFS) drive.
- The helper refuses to run if its own file or folder is writable by anyone but root.

## 5. Updates, reports, CI

- Updates: GitHub releases checked at startup and once a day by the background agent, installed only when the player agrees, bundle checked by SHA-256 (Turbine's selfupdate). Test builds: opt-in pre-releases.
- Reports: written to Turbine Hub's own folder only, never sent anywhere. Labs mode: as in Turbine.
- A release only ships a bundle from a green CI run of the same commit (Turbine's reuse and release jobs).
- CI follows Turbine's build.yml: house rules check, engine tests and `clippy -D warnings` on 1.97.0, QML syntax check, release rules, Flatpak build, screenshot shards at 1280x800, 1920x1080 and 1600x900 with SelfCheck, stand-in servers and fake home folders (a live check may warn, never fail), layout checks and before/after screenshot comparison.
- The repo variable `TURBINE_RUNNER` picks GitHub's runners or the owner's PC. The PC gets its own runner set with the label `turbinehub`, and a matching switch next to `~/dev-setup/turbine-ci`.

## 6. Build order

1. Feature inventory: done (this document, sections 1 and 2).
2. Architecture and helper design: this document, sections 3 and 4.
3. Engine with tests.
4. App UI (from the reviewed mockups).
5. CI and runners.
6. Decky plugin.
7. Screenshot review of every screen at every size.
8. A hands-on checklist for the owner to try in Game Mode.

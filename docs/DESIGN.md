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
Home, Games, Store, My machine, Settings. LB/RB (Q/E on a keyboard) switch tabs. The controls are the same on every page (see Controls below).

### First run
1. Mode picker (Guided or Veteran).
2. Guided: Your machine (detected), Will my games run?, Bring it over, Picks. Four steps.
3. Veteran: Quick setup, one screen of checkboxes, one go. "Load My setup file" applies a saved setup.
4. No step asks for the password, and nothing reminds the player to set one (no setup step, no checkup item). A password is invasive, so a new program doesn't open with it: it is asked only inside the dialog of the first system change (see 4.3). While the helper isn't set up (or is an older one), the picks that are system changes carry a "Needs your password" mark, and the Apply dialog starts with how many of the ticked changes it is for. Picks that change nothing of the system (Flathub apps, the library entry, Game Boost's own switch) apply without it. A settings file that still lists the old "password" step reads fine: the name is passed over.

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
- "Where's my...?": Windows things and where they are on SteamOS, and the glossary. Help (in every Options menu and in the menu, F1 on a keyboard) first explains the page it was asked on and every control, with "Where's my...?" one press further.

### Games
Per-game switches (FSR 4 upgrade, frame generation, Handheld mode, Proton log, Proton choice), "Won't start?" help (another Proton, reset Windows files with a backup, write a report that stays on the machine), the save time machine (snapshot at every launch, restore, undo). Mods open Turbine Mod Manager. Games whose anti-cheat runs online are locked: Turbine Hub never changes them.

### Store
Essentials, Apps (Flathub), Decky plugins (with turn-off and "Turn all off" for troubleshooting), Launchers, TV and streaming, Emulation (EmuDeck), Installed. Update all. Everything installed keeps itself current.

NonSteamLaunchers (github.com/moraroy/NonSteamLaunchers-On-Steam-Deck, `engine/src/nsl.rs`) has its own page under Launchers. Install, Open and Update fetch the project's own `NonSteamLaunchers.sh` at its newest release tag (never `main`), check it names that version, and start it on the host with `flatpak-spawn --host bash`, as the player, without the helper or a password. Its windows need Desktop Mode, so in Game Mode the page says how to get there. With Decky Loader there, the page also offers its Decky plugin through the project's `NSLPluginInstaller.sh` (which asks for the password in its own window). Installed is read from its Windows files in Steam's compatdata folder; the version is the release Turbine Hub last started.

### My machine
Capability modules, shown only when the hardware has them:
- Handheld: Power (TDP, Game Boost), Display (the built-in screen's refresh range, from its own EDID), Lighting (any multicolor light the kernel exposes: a live preview, Ally Hub's presets as cards, brightness first; HueSync can be left to run them), Battery (charge limit, history), Sleep (drain per sleep, wake sources), Dock, Tune-up, Restart to Windows.
- PC: Graphics card (fan curve, power limit, safe reset), Processor (Game Boost), Monitors (refresh, VRR, HDR, which screen Game Mode uses), Drives (set up a drive as a game library, Windows drives read only), Sound (output), Sleep and wake (Wake on LAN, controller wake, USB wake), Tune-up, Restart to Windows.
- TDP is shown only where the kernel exposes power limits, and hidden while Steam's own slider or a TDP plugin manages it.

### Settings
Mode, interface size (50 to 150%, re-flows), the background helper (on or off) and the save time machine, Look (theme and accent from Turbine's design system, color vision: typical, red-weak, green-weak, blue-weak or no color, and animations: full, reduced or off), updates, My setup (save and apply), help and reports (Labs mode as in Turbine), Bring it over again, "Review changes" (undo anything, one step at a time), remove Turbine Hub completely.

### Files
A file explorer laid out like the one on Windows, in Turbine Hub's own look, over the folders that are really there. All of it is decided in `engine/src/files` (the pages come with 0.5's interface). Two views of one disk, switched at any time: **Windows-like** (Guided starts here) names and groups places the way Windows does, **True Linux** (Veteran starts here) shows every real name, path, hidden file, link, permission and owner. The real path is never hidden: every entry carries it, and "Copy as path" hands out both forms.

Where each Windows place really goes:

| Windows-like | Real folder | Notes |
|---|---|---|
| Home | (made up) | The pinned Quick access folders and the files opened lately through Turbine Hub |
| This PC | (made up) | The player's six folders, every drive with free and total room, the Recycle Bin |
| SteamOS (C:) | `/` | Shows Games, Program Files, Users and Windows; the rest of the top of the disk counts as hidden. Its room is the home partition's, where the player's files are |
| C:\Users, C:\Users\\<user> | the folder the home folder is in, the home folder | Desktop, Documents, Downloads, Music, Pictures and Videos are the folders `~/.config/user-dirs.dirs` names (the standard names where it says nothing), shown under their Windows names |
| C:\Users\\<user>\AppData | Roaming = `~/.config`, Local = `~/.local/share`, Apps from the Store = `~/.var/app` | A Store app's folder shows under the app's name when its `.desktop` file gives one |
| C:\Games | every Steam library's `steamapps/common`, merged, and Heroic's games | Each game by name. What else Steam keeps there (Proton, its runtimes) counts as hidden |
| C:\Games\\<game> | (made up) | Game files (its folder), Its Windows C: drive (`steamapps/compatdata/<id>/pfx/drive_c`, or the Heroic prefix), Saves for a game that runs without Windows when one of the usual places is there (`~/.config/unity3d/<maker>/<game>`, `~/.local/share/<game>`, `~/.config/<game>`), Save snapshots when Turbine Hub kept some |
| C:\Program Files | `/var/lib/flatpak/app` and `~/.local/share/flatpak/app` | Each app by name, opening on its files (`<id>/current/active/files`) |
| C:\Windows | `/usr`, with `etc` = `/etc` | Locked |
| D:, E: ... | folders under `/run/media/<user>/`, then every other real partition that isn't the system's | "SD card" for a card, else the drive's label, else the folder's name. A Windows (NTFS) drive is read only |
| Recycle Bin | `~/.local/share/Trash` and each other drive's `.Trash-<uid>` (or `.Trash/<uid>`), as one list | |

Paths map both ways (`Places::windows_path`, `Places::resolve`). The most specific place wins when showing a path; a path nothing else claims reads as it is under C: (`/opt/x` is `C:\opt\x`), so every real path has a Windows-style one and comes back from it. The address bar takes either style: upper or lower case, either slash, quotes, `..`, `%AppData%`, `~`, `file://`. A link keeps the name it was reached by. Drive letters go by order (cards and plugged-in drives by name, then other partitions) and may change when a drive comes or goes, as on Windows.

Lists: name, real name, real path, kind, a plain type word (from the ending, or the first bytes for a name without one), size, date, hidden, writable, locked, and in True Linux the permission text, owner and group. Sorted by name (numbers as numbers), date, size or type, folders first; grouped as Explorer groups (by date: Today, Yesterday, Earlier this week, Last week, Earlier this month, Last month, Earlier this year, A long time ago, with weeks starting on Monday; by type, size or name). Hidden files are off in Windows-like and on in True Linux unless asked. Search is "name contains" in the folder, or below it within limits (depth, hits, names looked at), never through a link and never into `/proc`, `/sys` or `/dev`. A folder that can't be read is an empty list with the reason, never an error.

Safety rules, each refused with a plain sentence before anything is touched (`engine/src/files/guard.rs`):
- No password is ever involved: every file is read and changed as the player, and what the player may not write is refused.
- SteamOS itself (`/usr`, `/etc` and the system's other folders), Turbine Hub's own folders and Windows drives are shown and never changed.
- Never renamed, moved or deleted: a drive's top folder or anything above it, the home folder or anything above it, Steam's own folders (each library, its `steamapps`, `common` and `compatdata`, Steam's `userdata` and the account folders in it, `~/.steam`), `~/.config`, `~/.local`, `~/.local/share`, `~/.var` and `~/.var/app`. What is inside them (a game's folder, its Windows C: drive, a screenshot) is the player's to manage.
- Every check looks at the path as given and at where its folders really lead, so a link into a locked place opens nothing. A link itself is the player's: deleting one removes the link and never what it points to.
- Nothing is ever written over. A name that is taken asks: skip, replace (the old one goes to the Recycle Bin first) or keep both ("name (2)"). A folder never goes into itself.
- A copy checks the room first, reports as it goes and can be stopped (what was half written is removed). A move on one drive is a rename; across drives it is a copy, then the removal of the original.
- Every operation is a plan first (what it does, in plain words) and hands back its undo: the last operation of each kind can be taken back. An undo never erases: what has to go, goes to the Recycle Bin.

Recycle Bin rules (the freedesktop Trash specification 1.0, so Dolphin sees the same bin):
- Delete moves the item to `files/` and writes `info/<name>.trashinfo` first (`[Trash Info]`, `Path=` escaped byte by byte, `DeletionDate=` on the player's clock), made only if that name is free, which keeps two deletes of the same name apart ("name (2)"). A deleted folder's size goes into `directorysizes`.
- The home folder's drive uses `~/.local/share/Trash` (never `$XDG_DATA_HOME`, which inside the Flatpak is the app's own folder). A file on another drive goes to the bin at the top of that drive, so deleting never copies between drives: `.Trash/<uid>` when the drive has a real `.Trash` folder with the sticky bit, else `.Trash-<uid>`, with the path written from the top of the drive.
- A bin, or its `files` or `info`, that is a link is never used, to put in or to empty. A note in a drive's bin that points out of the drive is not believed.
- Restore puts an item back where it was (its folders are made again), asking when the name is taken. What is in the bin can only be restored or emptied.
- Emptying the Recycle Bin is the only thing in the explorer that erases for good, and its dialog has Cancel highlighted.

Opening: text (UTF-8, UTF-16 with its mark, old Western files; the first 256 KB) and common pictures preview inside the app; everything else opens with its own app through `xdg-open` on the host in Desktop Mode.

Sandbox: every access goes through one trait (`Fs`) with real paths, and a map says where the app finds each one. Inside the Flatpak the host's `/usr` and `/etc` are under `/run/host`; the home folder, other apps' data (`~/.var/app`), drives under `/run/media` and installed apps (`/var/lib/flatpak`, read only) are at their own paths. The Flatpak asks for exactly that (`home`, `~/.var/app`, `host-os:ro`, `host-etc:ro`, `/var/lib/flatpak:ro`), not for the whole machine, and no file action ever asks for a password. Links are followed in real paths, so a link to `/usr/share` leads to the machine's folder and not the Flatpak runtime's.

### Deferred
Phone remote and Wake-on-LAN web page: after the core is solid.

### Wording rules
- Player-facing words are short and plain, with one glossary used everywhere.
- No past tense about the player ("Your Windows apps", not "I used to use").
- No greetings or time-of-day remarks.
- No placeholder art: game and app art only where real art exists (Steam's art, Flathub icons), otherwise the name alone.
- No em or en dashes.

### Controls
One control scheme, shared with Turbine Mod Manager: whoever knows one app knows the other. It holds on every page and in every dialog, and each line has a way on the controller, with a keyboard alone and with a mouse alone. `engine/src/ui/controls.rs` holds it (`SCHEME`); Help shows it from there, and a test holds this table and the README's to it.

| Do | Controller | Keyboard | Mouse |
|---|---|---|---|
| Move | D-pad or left stick | Arrows or W A S D | Move the mouse |
| Select | A | Enter or Space | Click |
| Back | B | Esc or Backspace | Back side button |
| Options | X | R | Right-click |
| Find | Y | F, Ctrl+F or / | Click the search box |
| Tabs | LB / RB | Q / E, or Page Up / Page Down | Click a tab |
| Menu | View | Tab | Click Menu in the footer |
| Main button | Start | Ctrl+Enter | Click the button |
| Play | Hold Start | Hold Space | Click Play |
| Jump | LT / RT | Home / End | Wheel |
| Pointer | Right stick |  |  |
| Click | R3 |  | Click |
| Pointer options | Hold R3 |  | Right-click |
| Scroll | Sweep the left stick round |  | Wheel |
| Help | X, then Help | F1 | Right-click, then Help |
| Size |  | Ctrl+= / Ctrl+- | Ctrl and the wheel |

- **Options (X)** is a menu for the highlighted thing: its own press first (what A does there), then what else it offers, then what the page offers (its own actions, its main button, Find), then Help. A game lists Play and everything its panel holds, a Store row what its own page does (Install, Open, Update, Remove), a card of My machine the switches and buttons of its page. Each row shows the button that does it directly where it has one. With nothing under a right-click it is the page's menu.
- **Find (Y)** is on every page with a list (Games, Store, Where's my...?): the search box takes the highlight and typing starts, with Steam's keyboard in Game Mode. At a keyboard a letter or digit that is no key of its own starts it too.
- **No page owns a button.** A page says what it offers (`options`, `primary`, `search`, `hold`) and `controls::finish` turns that into the Options menu's rows and the footer's hints for every page alike. What used to sit on X and Y (Library check, Update all, Undo all, Tick all, Load setup, Show all, Continue setup, Refresh) is in Options, and on the page as a button.
- **The pointer.** The right stick moves a pointer drawn by Turbine Hub (Game Mode has no cursor), as in Turbine Mod Manager (`app/src/pointer.rs`: a dead zone, slow near the middle and across the screen in under a second at the edge, hidden after three seconds without it and at once when a button, the D-pad, the left stick, a key or the real mouse is used). Its moves take the highlight as a mouse's do and its clicks are real mouse events, so they pass the same gate as a tap and work over lists and dialogs. R3 clicks; R3 held for 450 ms (a ring fills) is the right button: Options for what is under it. With the pointer hidden R3 is A and R3 held is Options for the highlight. The left stick pushed one way is the D-pad (one direction per push); swept round the rim it is a scroll wheel.

Decided where Turbine Hub has something Turbine Mod Manager doesn't, or the other way round:
- **View** opens the quick wheel in Turbine Mod Manager; here it opens the menu, which is the same thing as a list (Help for this page, Where's my...?, Bring it over, Review changes, Settings, Close). Tab opens it at a keyboard (a tap, not a hold: a list has nothing to point at while held).
- **Start** is the main button of the page it is on, as on Turbine Mod Manager's sheets (Continue, Apply, Copy, Fix all), and Play when held for a second on a game, as there. A tap with no main button opens Settings, as there.
- **Help** had View to itself. It is now the last row of every Options menu and the first row of the menu, and F1 at a keyboard.
- **LT and RT** were "list or details" and "search". Search is Find (Y); the two sides of a page are Left, Right and A, as before. LT and RT jump a screenful, as in Turbine Mod Manager's lists, and while the pointer shows RT holds the left button (drag a slider) and LT is the right button, as there.
- **R3 held** is the right button here (the owner's choice); in Turbine Mod Manager R3 held drags. RT drags in both.
- Start is called START on every pad's hints: "MENU", its name on newer pads, would stand beside the menu that View opens.

### Layout (from Turbine)
- One footer hint bar with a fixed priority order. A and B are never dropped; Find, Menu, Options and the main button give way in that order when the bar is too narrow, and LB/RB sit beside the tabs. The hints follow the last input: keys (Enter, Esc, R, F, Tab) after a key or the mouse in a window, the controller's after a press on it (the pointer counts as the controller), and always the controller's in Game Mode.
- One dialog template: sizes to its content, buttons pinned, controller glyph inside each button.
- Input that can't surprise: the controller only counts while Turbine Hub is in front (not behind a game, and not for a moment after coming back); a tap or click (the controller pointer's too) obeys what a press obeys and never reaches what lies under a dialog or the busy cover; a held key or button acts once (only directions repeat); the mouse moves the highlight only when it really moves; the left stick gives one direction per push; one window at a time (a second start brings the open one forward). The build tries these with real keys, a real mouse, a finger and sticks fed in as the real ones are read (the check-... pictures).
- Distinct focus, selected and primary styles. Every action has its own icon.
- Scroll fades, the focused item always fully in view, toasts inside the content area, top insets, dark scrims behind overlays.
- Body text at least 14 px at 1280x800. Interface size 50 to 150% re-flows instead of truncating: text is never under 12 px nor under 90% of its design size, and anything to press is never under 44 px or 6 mm, at any interface size and in any window.
- Words read at 4.5 to 1 or better on what they are drawn on, in every theme, accent and color vision mode. A state (fine, needs a look, won't work) always has its own mark and word beside its color; the Color vision setting picks colors that red-weak, green-weak or blue-weak eyes tell apart, or none at all.
- One motion system with an Animations setting: Full, Reduced (fades only, nothing travels or grows) or Off. An older settings file's true or false reads as Full or Off.
- A screen reader (Orca in Desktop Mode) is told the kind, the name and the state of whatever the highlight is on, the tabs, the footer's hints, the dialog and the notes.
- The build holds all of it: every picture measures its own words, sizes, chips and names (and all 80 combinations of theme, accent and color vision), and pages shot with animations on must come to rest on the picture they give with animations off.
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
- `applog`: Turbine Hub's own log and crash log (see 5): what is hidden in every line, the size caps, how a run that never closed is told from Game Mode ending. `drawing`: software drawing where there is no graphics card to draw with.
- `files`: the file explorer (see 2, Files): `fs` (the one door to the disk and the sandbox map), `places` (the Windows-like places and the two-way path mapping), `list`, `guard` (what is refused and why), `trash`, `ops` (plans, progress, undo), `preview`, `quick` (pinned folders and recent files, in `files.json` in Turbine Hub's folder). `fake::files` gives the made-up machines a home folder, games, apps, a card and a Recycle Bin for it.
- `cli`: `--json` verbs for the Decky backend and scripts.
- `ui`: what every screen shows and what every press does. Each page is one JSON object; each press answers with a note, a page to open, or the one dialog with its exact steps (`Plan`, `Step`). `ui/controls.rs` gives every page its Options menu and its footer hints from what the page offers. It also holds the per-game switches file, the lighting file, the Store's list, the setup file and the made-up machine used for pictures and tests.

Every decision (filtering, sorting, what to warn about, what a check means) is Rust with tests. QML only shows what the engine says.

### app
One cxx-qt bridge object, `Hub`, hands QML the engine's pages (`Hub.page`) and takes presses (`Hub.act`, `Hub.confirm`). The steps of a confirmed dialog and the questions to the host and the internet run on their own threads and come back through `Hub.poll`, together with the controller (SDL): its buttons, and where the sticks have moved the pointer (`pointer.rs`, Turbine Mod Manager's). `app/qml/COMPONENTS.md` lists the parts.

`turbinehub --screenshot=<png> --size=<W>x<H> --page=<page> [--arg=<the page's own>] [--act=<action>:<argument> [--value=<typed text>]] [--machine=handheld|desktop|tv] [--mode=guided|veteran] [--window] [--keys=down,a] [--pointer=<x>,<y> as parts of the window] [--check=<input check>] [--installed=<app id>] [--no-tool] [--theme=...] [--accent=...] [--vision=...] [--scale=50..150] [--motion=full|reduced]` renders a page on a made-up machine and loads SelfCheck.qml, which checks cut-off text, overflow, text size, contrast, finger sizes, what a screen reader is told, the footer's hints and the highlight. `turbinehub --shots` lists the pictures the build takes.

### Background agent
The same Flatpak run as `turbinehub --agent`, started by a systemd **user** unit, `~/.config/systemd/user/turbinehub-agent.service` (user-level, no password, survives SteamOS updates). Settings, Background helper writes the unit through `flatpak-spawn --host` and enables it; turning it off disables and removes it. Players see it as "the background helper".

What it does, all decided in `engine/src/agent.rs` (the app's `agent.rs` only looks, runs the commands it is handed and brings the answers back):

- **Games.** Steam starts every game under its `reaper` program with `SteamLaunch AppId=<id>` in the arguments. The agent reads the process list every few seconds (through `pgrep` on the host, because the sandbox has its own /proc) and changes nothing about Steam.
- **Game Boost.** With the setting on and the privileged helper installed: `set-now` for each CPU policy's energy preference at a game's start, and the values from before at its end (kept in its state file, so a restart of the agent still puts them back). It is a processor setting, not a change to a game, so it applies to every game, including those with online anti-cheat.
- **Save snapshots.** With the save time machine on and Ludusavi installed: a snapshot of the game's saves at each start (see `engine/src/snapshots.rs` for the layout), at most one per game in ten minutes, the newest five kept, unchanged saves skipped. Reading save files touches no game.
- **Battery.** A reading a minute into `history/battery.jsonl` (a week of them), each game session into `history/sessions.jsonl`. From these: time left, and battery use per game (sessions that ran on battery from start to end).
- **Sleep.** A wall clock that jumped ahead of the steady clock is a sleep, confirmed by the kernel's count in `/sys/power/suspend_stats/success`. The charge before and after and the USB devices whose wake count rose go into `history/sleep.jsonl`; the checkup's sleep item and the Sleep page read it.
- **Lighting.** Frames for every multicolor light in `/sys/class/leds` (any LED with a `multi_intensity` file: red, green and blue per part, or one packed color per part as `multi_index` says), with Ally Hub's effects (static, breathe, heartbeat, color cycle, wave, strobe, candle, twinkle, spiral) and its twelve presets, the low battery flash, a look per game and the charge as a color (breathing while it charges), 20 a second on wall power and 8 on battery, only the files that changed. Colors are corrected for how LEDs give out light and the brightness follows how eyes see it. Only once the player has saved lighting choices and the helper's lighting access rule has opened what the way in use needs, each light by itself (one that stays closed never keeps the others dark); never while the player left the lights to HueSync or Ally Hub. Packed colors are written the way the guided test found right (a plain number, blue first, or hex text, Ally Hub's encodings), and a light left in an effect of its own is put back to plain colors first. Inside the sandbox /sys is read only, so each file gets one long-lived `tee` on the host.
- **The ASUS lighting chip.** Where a kernel caps the packed colors of the ROG Ally's rings at 255 (the files then show blue only, seen from `multi_max_intensity` or by reading back the first frame), `lights_hid` drives the controller's lighting chip instead, on the one hidraw interface with ASUS's vendor id and HID usage 0xFF31/0x0080 that belongs to the rings' own USB device. No one sequence works on every firmware, so `lights_hid::METHODS` holds Ally Hub's eight routes (the kernel's own light switched off once, every time or never; the "ASUS Tech.Inc." handshake or not; the four parts one by one or all at once; output or feature reports; set and apply every time or only at first; the pause between packets; the sequence sent twice), byte for byte. Windows Dynamic Lighting (usage 0x0059/0x0001) is switched off as often as the route says, best effort: the colors go out even when the chip refuses that report. On the routes that can be animated a frame is one packet when every part has the same color and only the changed parts otherwise; on the others the chip runs one of its own modes (solid, pulse, rainbow, spiral), sent once per change. The spiral presets can use the chip's own rainbow spiral at full brightness. The chip is also used when the guided test, or Ally Hub's saved choice on the same machine, says it is the way that works. The sandbox sees /dev, so the app sends the reports itself, on a node opened for writing only (a write for an output report, an ioctl for a feature report).
- **Test lights.** A guided test, as in Ally Hub: the page asks the background helper (a request file) to light the rings red one way after another, the light's own files in each encoding first and then the chip routes m6, m7, m8, m2, m3, and asks "Did both rings turn red?" (Yes, No, Only partly). The first Yes is saved as `route` in lighting.json and used from then on; the look stays off the lights while a way is shown.
- **Ally Hub and HueSync on the same machine.** The app reads Ally Hub's `~/.config/allyhub/config.json` through the host (`cat`, never written) and starts from its saved way (`lighting.encoding`, `lighting.hid_method`) until the player's own test says otherwise. When Ally Hub's agent runs with its lighting on, or HueSync is installed with Decky Loader running, two apps would paint over each other many times a second: the Lighting page and the checkup say who is setting the lights, with "Let Turbine Hub drive them" (the exact place to turn the other's lighting off; Turbine Hub never changes another app's settings) and "Leave them to ..." (the background helper keeps its hands off).
- **Closed lights.** The checkup looks at exactly what the way in use needs for the main light (its color and brightness files, or the chip's commands interface) and at nothing else. After the helper opened them the app looks again: what is still closed is said with the reason, remembered, and the checkup then offers the lighting check in place of the same fix. A helper older than the app (its file differs from the one in the Flatpak) is updated first, with the password.
- **Lighting check.** One press writes `reports/TurbineHub-lighting-report.txt`: every LED folder with its files, modes and contents, every ASUS HID interface with its usages and mode, the way in use and where it came from, who else is setting the lights, the helper's rule and its unit, what is closed, what was written and what reads back, the background helper's own notes and the last light test.
- **Dock.** On a handheld, an outside screen connecting or going away: lights off while docked and sound to the TV's output and back, as the Dock page says.
- **Updates.** Once a day, never while a game runs: a newer Turbine Hub, Flathub apps (`flatpak remote-ls --updates`), Decky plugins against the plugin store's list. It only reports; installing is the player's press in the app.
- **After a SteamOS update.** See 4.6.

It talks to the app through files in the app's data folder, no socket: `agent/status.json` (still running, the game, updates found), `agent/drift.json`, the history files, and lines in the activity list. One agent at a time (a lock file). `turbinehub --agent --once --machine=handheld` runs it once on a made-up machine and prints what it would run; the build's checks do that.

### Steam integration
- Adds itself to the Steam library with its own art (`ui/selftile.rs` on Turbine's shortcuts and steam_art), so it opens from Game Mode: offered in the first-run picks, in Settings (add, fix art, remove) and as a checkup tip. Steam's list of non-Steam games is only written while Steam is closed, with a backup first; while Steam runs the player gets the steps. "Is Steam running?" has three answers (yes, no, couldn't be asked) and is asked again on the host right before every write (`shortcuts::Steam`): anything but a sure "no" writes nothing and says why in plain words. At every start, where the entry exists and Turbine Hub placed its pictures before, the pictures are brought up to date, so new art arrives with an update. The art in `packaging/steam-art` is drawn by `tools/steam-art.py` and never replaces a picture the player picked.
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
- One password, once, and never before the first system change needs it.

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
1. **First install (the one password):** every system change is built by `ui/plan.rs` (`with_helper`), the only place a password is asked. While the helper is missing, that dialog starts with why the password is asked now ("This changes a system setting, so SteamOS asks for your password, like the admin prompt on Windows. Turbine Hub uses it once to set up its helper and never stores it."), lists the helper's setup first, and on a machine without a password has two boxes to create one (`passwd`), saying it can't be recovered if forgotten. The in-app password dialog (works in Game Mode, where the system has no password dialog of its own) runs `flatpak-spawn --host sudo -S -k <helper inside the Flatpak> install` with the password on stdin. The helper copies itself to `/var/lib/turbinehub/bin`, writes the polkit rule and the boot unit, and records which user it serves. The password is never stored.
2. **After that:** `flatpak-spawn --host pkexec /var/lib/turbinehub/bin/turbinehub-helper <job> <json>`. The polkit rule says YES only for that exact program, for the active local session of its owner. No password, works in Game Mode.
3. **Helper updates ask for the password again** (step 1). They are rare, and a password-free self-update would let any program running as the player replace the helper. An update of the app never replaces the installed helper by itself: the next change that goes through the helper sees that its file differs from the one in the Flatpak and installs the new one first.
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
| `access` | Fixed rules: `lighting` (every multicolor LED's color and brightness files, and on ASUS devices the helper's own `hid-access` check, which adds only the write bit to the hidraw interfaces with ASUS's lighting or Dynamic Lighting usage and leaves every other bit as it was, so another lighting app's access stays; also run at boot and after every wake by `reapply`, which opens the lights' files too, with their own `effect` and `mode` files), `ntsync` | `/etc/udev/rules.d/70-turbinehub-access.rules` |
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

- Updates: GitHub releases checked at startup and once a day by the background agent, installed only when the player agrees, bundle checked by SHA-256 (Turbine's selfupdate). The checks the app makes by itself (its start, after every finished job) ask GitHub at most once in six hours and use the kept answer in between; Check now always asks. A failed install is said in words the player can act on (needs the internet, no space, a system install needs Desktop Mode's password). Afterwards the running version keeps going: in Desktop Mode Settings offers Restart, in Game Mode it says to close Turbine Hub and start it again from Steam. Test builds: opt-in pre-releases.
- Reports: written to Turbine Hub's own folder only, never sent anywhere. Labs mode: as in Turbine.
- Logs: `app.log` (presses by their action, each step of a confirmed dialog and how it ended, what the host answered, Qt's warnings) and `crash.log` (panics, Qt's fatal errors, runs that ended without closing) in `logs` in the data folder, capped at 1 MB (one older file kept) and 200 KB. The home folder, the user name, keys, passwords and addresses are replaced before a line is written. The display going away (Game Mode ending) is not a crash. Their last lines go into a report the player writes (Settings, Help and reports, or a game's page); nothing is ever sent anywhere.
- A Steam library in a folder the Flatpak may not read is never silently missing: the checkup and the Games tab name it with Allow, which shows the folder, runs `flatpak override --user --filesystem=<folder>` for Turbine Hub on the host, and says to start Turbine Hub again.
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

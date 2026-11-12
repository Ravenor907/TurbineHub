//! Bring it over from Windows: the six cards, the Windows drive's games to copy and its saves to bring into the
//! games here, and the Windows apps with what each becomes on SteamOS. The Windows drive is only ever read.
use super::plan::{Plan, Step, dialog, refresh, toast};
use super::store::{self, CHROME, HEROIC};
use super::{Ui, arr, b, hint, n, obj, row, s, section, size_words, with};
use crate::bring::{self, Becomes, WinGame};
use crate::hardware::DriveRole;
use crate::json::Json;
use crate::library::{AntiCheat, parse_anticheat};
use crate::saves::{self, Job, Target, WindowsRun};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

impl Ui {
    /// Where the Windows drive is mounted: the first mounted NTFS partition of a drive that holds Windows.
    pub fn windows_drive(&self) -> Option<PathBuf> {
        if self.windows_mount.is_some() {
            return self.windows_mount.clone();
        }
        self.machine().drives.iter().filter(|d| d.role == DriveRole::Windows).flat_map(|d| d.partitions.iter()).filter(|p| p.fs == "ntfs" || p.fs == "ntfs3").find_map(|p| p.mount.as_ref().map(PathBuf::from))
    }
    /// The Windows partition to open when none is mounted yet ("/dev/sda3").
    fn windows_partition(&self) -> Option<String> {
        self.machine().drives.iter().filter(|d| d.role == DriveRole::Windows).flat_map(|d| d.partitions.iter()).filter(|p| p.fs.starts_with("ntfs")).max_by_key(|p| p.label == "Windows").map(|p| format!("/dev/{}", p.name))
    }
    fn brought(&self, step: &str) -> bool {
        self.settings().brought.iter().any(|x| x == step)
    }
}

/// One card: `status` is shown until the step is done, `press` is the button's words, action and argument.
fn card(id: &str, icon: &str, title: &str, line: &str, status: &str, done: bool, press: (&str, &str, &str)) -> Json {
    let (button, act, arg) = press;
    obj! {"id": s(id), "icon": s(icon), "title": s(title), "value": s(""), "line": s(line), "status": s(if done { "Done" } else { status }), "done": b(done), "badge": s(""), "button": s(button), "act": s(act), "arg": s(arg)}
}

/// The six Bring it over cards.
pub(crate) fn hub(ui: &mut Ui) -> Json {
    let heroic = store::is_installed(ui, HEROIC);
    let xcloud = store::tile("xcloud").is_some_and(|t| store::tile_added(ui, t)) || ui.brought("gamepass");
    let mount = ui.windows_drive();
    let (win_line, win_status, win_button, win_act, win_arg) = match (&mount, ui.windows_partition()) {
        (Some(m), _) => {
            let games = bring::steam_games(m).len();
            (match games { 0 => "Windows found on this drive. No Steam games on it.".to_string(), 1 => "Windows found on this drive: 1 Steam game. Copy it over, nothing downloads again.".into(), c => format!("Windows found on this drive: {c} Steam games. Copy them over, nothing downloads again.") }, "Not done yet", if games > 0 { "Copy games" } else { "" }, "open", "windrive")
        }
        (None, Some(_)) => ("A Windows drive is here. Turbine Hub opens it read only to look for games.".to_string(), "Not opened yet", "Read the drive", "win-mount", ""),
        (None, None) => ("No Windows drive on this machine. Plug one in and it shows up here.".to_string(), "Nothing to copy", "", "none", ""),
    };
    let pass_line = if ui.veteran() { "Xbox Cloud as a library tile. Installing Game Pass games on the machine is experimental and not in this build." } else { "Xbox Cloud as a library tile: Game Pass games, streamed." };
    obj! {
        "title": s("Bring it over from Windows"),
        "line": s("Your games and apps, set up the SteamOS way. Do any of these now or later."),
        "note": s(""),
        "columns": n(3.0),
        "cards": arr([
            card("stores", "bag", "Other stores", "Epic, GOG and Amazon through Heroic. Battle.net, EA, Ubisoft and more through NonSteamLaunchers.", if heroic { "Heroic installed" } else { "Not done yet" }, ui.brought("stores"), ("Add stores", "open", "store|launchers")),
            card("gamepass", "cloud", "Game Pass", pass_line, "Not done yet", xcloud, (if xcloud { "" } else { "Add Xbox Cloud" }, "tile", "xcloud")),
            card("windows", "drive", "Your Windows drive", &win_line, win_status, ui.brought("windows"), (win_button, win_act, win_arg)),
            card("apps", "swap", "Your Windows apps", "Pick the Windows apps you use. Get the SteamOS version of each in one install.", "Not done yet", ui.brought("apps"), ("Pick apps", "open", "winapps")),
            card("tv", "tv", "TV and streaming", "Netflix, YouTube, Disney+, Twitch and Plex as controller-ready tiles.", "Not done yet", ui.brought("tv"), ("Pick services", "open", "store|tv")),
            card("wheres", "help", "Where's my...?", "Task Manager, the C: drive, installing programs: where they are on SteamOS.", "Always in Help", false, ("Open", "open", "wheres")),
        ]),
        "hints": arr([]),
    }
}

/// App ids the anti-cheat list marks as blocked on SteamOS.
fn blocked_ids(cache: &Path) -> BTreeSet<String> {
    parse_anticheat(&std::fs::read_to_string(cache.join("anticheat.json")).unwrap_or_default()).into_iter().filter(|(_, a)| matches!(a, AntiCheat::Blocked | AntiCheat::Planned)).map(|(id, _)| id).collect()
}

struct DriveGames {
    games: Vec<WinGame>,
    blocked: BTreeSet<String>,
    here: BTreeSet<String>,
}

impl DriveGames {
    fn can_copy(&self, g: &WinGame) -> bool {
        !self.blocked.contains(&g.app_id) && !self.here.contains(&g.app_id)
    }
}

fn drive_games(ui: &Ui, mount: &Path) -> DriveGames {
    let here = crate::scan_games(&ui.ctx.home).games.iter().filter(|g| g.store == crate::Store::Steam).map(|g| g.store_id.clone()).collect();
    DriveGames { games: bring::steam_games(mount), blocked: blocked_ids(&ui.cache), here }
}

/// The picks of a picking screen; the first visit ticks `defaults`.
fn picks<'a>(ui: &'a mut Ui, page: &str, defaults: impl FnOnce() -> BTreeSet<String>) -> &'a mut BTreeSet<String> {
    ui.picked.entry(page.to_string()).or_insert_with(defaults)
}

/// The games here that saves from the Windows drive can go into: Windows games Steam starts, never one whose
/// anti-cheat runs online. Each with its folder on the drive when it is installed there too.
fn save_targets(ui: &Ui, mount: &Path) -> Vec<Target> {
    let on_drive = bring::steam_games(mount);
    super::games::listed(ui)
        .into_iter()
        .filter(|l| l.game.store == crate::Store::Steam && l.game.runtime != crate::Runtime::Native && !l.locked())
        .map(|l| Target { windows_dir: on_drive.iter().find(|w| w.app_id == l.game.store_id).map(|w| w.dir.clone()), name: l.game.name, prefix: l.game.prefix, install_dir: l.game.install_dir })
        .collect()
}

fn names(list: &[String]) -> String {
    match list.len() {
        0..=3 => list.join(", "),
        c => format!("{} and {} more", list[..3].join(", "), c - 3),
    }
}

/// The Saves part of the Windows drive's page: the one button, what the last run did, and its Undo.
fn saves_rows(ui: &Ui) -> Vec<Json> {
    let line = if store::is_installed(ui, saves::LUDUSAVI) { "Ludusavi finds the saves on this drive and puts them into the games installed here. Steam Cloud brings most by itself." } else { "Ludusavi is installed first. It finds the saves on this drive and puts them into the games installed here." };
    let mut rows = vec![with(with(row("win-saves", "", "button", "Bring saves over", line), "icon", s("save")), "button", s("Bring over"))];
    let Some(last) = WindowsRun::load(&ui.folder()) else { return rows };
    let when = super::saves::when_words(crate::now(), last.at);
    let title = match (last.undone, last.brought.len()) {
        (true, _) => "Last time: undone".to_string(),
        (_, 0) => "Last time: no saves had a place here yet".into(),
        (_, 1) => format!("Last time: saves of {} brought over", last.brought[0]),
        (_, _) => format!("Last time: saves of {} brought over", names(&last.brought)),
    };
    let waiting = if last.waiting.is_empty() { String::new() } else { format!(" Kept for later: {}. Install those games and start each once, then bring saves over again.", names(&last.waiting)) };
    rows.push(row("none", "", "info", &title, format!("{when}.{waiting}").trim()));
    if last.kept.is_some() && !last.undone {
        rows.push(with(with(row("win-saves-undo", "", "button", "Undo", "Puts back the saves these games had here before"), "icon", s("restore")), "button", s("Undo")));
    }
    rows
}

/// Your Windows drive: the Steam games on it, ticked to copy.
pub(crate) fn drive(ui: &mut Ui) -> Json {
    let Some(mount) = ui.windows_drive() else {
        return obj! {"crumb": s("Bring it over"), "title": s("Your Windows drive"), "intro": s(""), "sections": arr([with(section("", vec![]), "empty", s("No Windows drive is open. Go back and pick Read the drive."))]), "hints": arr([])};
    };
    let dg = drive_games(ui, &mount);
    let free = ui.free_bytes;
    let picked = picks(ui, "windrive", || dg.games.iter().filter(|g| dg.can_copy(g)).map(|g| g.app_id.clone()).collect()).clone();
    let chosen: Vec<&WinGame> = dg.games.iter().filter(|g| picked.contains(&g.app_id) && dg.can_copy(g)).collect();
    let bytes: u64 = chosen.iter().map(|g| g.bytes).sum();
    let rows: Vec<Json> = dg
        .games
        .iter()
        .map(|g| {
            let size = size_words(g.bytes);
            if dg.here.contains(&g.app_id) {
                with(with(with(row("none", "", "info", &g.name, "Already on this machine"), "value", s(size)), "badge", s("Here already")), "tone", s("ok"))
            } else if dg.blocked.contains(&g.app_id) {
                with(with(with(row("none", "", "info", &g.name, "Its anti-cheat blocks SteamOS, so copying it wouldn't help"), "value", s(size)), "badge", s("Won't run here")), "tone", s("stop"))
            } else {
                with(with(row("tick", &format!("windrive|{}", g.app_id), "check", &g.name, ""), "on", b(picked.contains(&g.app_id))), "value", s(size))
            }
        })
        .collect();
    let room = match free {
        Some(f) if f < bytes => format!("Not enough room: {} picked, {} free. Untick some games.", size_words(bytes), size_words(f)),
        Some(f) => format!("Plenty of room: {} free. You can keep playing while it copies.", size_words(f)),
        None => "You can keep playing while it copies.".into(),
    };
    let count = chosen.len();
    obj! {
        "crumb": s("Bring it over"),
        "title": s("Your Windows drive"),
        "intro": s("Read only. Nothing on Windows changes."),
        "sections": arr([
            with(section(&if count == 0 { format!("Steam games \u{b7} 0 of {} picked", dg.games.len()) } else { format!("Steam games \u{b7} {count} of {} picked \u{b7} {}", dg.games.len(), size_words(bytes)) }, rows), "empty", s(if dg.games.is_empty() { "No Steam games found on this drive." } else { "" })),
            section("Saves", saves_rows(ui)),
        ]),
        "note": s(room),
        "primary": if count > 0 { obj! {"label": s(if count == 1 { "Copy 1 game".to_string() } else { format!("Copy {count} games") }), "k": s("start"), "act": s("copy"), "arg": s("")} } else { Json::Null },
        "hints": arr([hint("x", "Tick all")]),
        "x": obj! {"act": s("tick-all"), "arg": s("windrive")},
    }
}

/// The app ids a pick on the apps screen can install: a Flathub app or the Xbox Cloud tile.
fn installable(a: &bring::AppMap) -> bool {
    match a.becomes {
        Becomes::Flatpak(_) => true,
        Becomes::Tile(t) => store::tile(t).is_some(),
        Becomes::BuiltIn | Becomes::Nsl(_) => false,
    }
}

fn app_here(ui: &Ui, a: &bring::AppMap) -> bool {
    match a.becomes {
        Becomes::Flatpak(id) => store::is_installed(ui, id),
        Becomes::Nsl(id) => crate::nsl::launcher(id).is_some_and(|x| crate::nsl::launcher_here(&ui.ctx.home, x)),
        _ => false,
    }
}

/// Your Windows apps: each with what it becomes here, ticked when it was found on the Windows drive.
pub(crate) fn apps(ui: &mut Ui) -> Json {
    let found: Vec<&str> = ui.windows_drive().map(|m| bring::apps_found(&m)).unwrap_or_default();
    let here: Vec<bool> = bring::APPS.iter().map(|a| app_here(ui, a)).collect();
    let picked = picks(ui, "winapps", || bring::APPS.iter().zip(&here).filter(|(a, h)| installable(a) && !**h && found.contains(&a.id)).map(|(a, _)| a.id.to_string()).collect()).clone();
    let rows: Vec<Json> = bring::APPS
        .iter()
        .zip(&here)
        .map(|(a, here)| {
            let line = format!("{} \u{b7} {}", a.steamos, a.note);
            let on_drive = found.contains(&a.id);
            if *here {
                with(with(row("none", "", "info", a.windows, &line), "badge", s("Installed")), "tone", s("ok"))
            } else if installable(a) {
                with(with(with(row("tick", &format!("winapps|{}", a.id), "check", a.windows, &line), "on", b(picked.contains(a.id))), "badge", s(if on_drive { "On your Windows drive" } else { "" })), "tone", s("muted"))
            } else if let Becomes::Nsl(id) = a.becomes {
                // Its page in the Store says how, and what to tick once NonSteamLaunchers is there.
                let nsl = super::nsl::state(ui);
                let line = match crate::nsl::launcher(id) {
                    Some(x) if nsl.installed => format!("{} \u{b7} Open NonSteamLaunchers and tick {}", a.steamos, x.tick),
                    _ => line,
                };
                with(with(with(row("open", "store-item|nsl", "button", a.windows, &line), "button", s("How")), "badge", s(if on_drive { "On your Windows drive" } else { "" })), "tone", s("muted"))
            } else {
                with(with(row("none", "", "info", a.windows, &line), "badge", s("Built in")), "tone", s("muted"))
            }
        })
        .collect();
    let count = bring::APPS.iter().zip(&here).filter(|(a, h)| installable(a) && !**h && picked.contains(a.id)).count();
    obj! {
        "crumb": s("Bring it over"),
        "title": s("Your Windows apps"),
        "intro": s("Pick the apps you use on Windows. Turbine Hub installs the SteamOS version."),
        "sections": arr([with(section("", rows), "columns", n(2.0))]),
        "primary": if count > 0 { obj! {"label": s(if count == 1 { "Install 1 app".to_string() } else { format!("Install {count} apps") }), "k": s("start"), "act": s("install-apps"), "arg": s("")} } else { Json::Null },
        "hints": arr([]),
    }
}

/// Copies a folder and everything in it. Nothing at the source is changed; links are skipped. Answers the bytes
/// copied.
pub fn copy_tree(from: &Path, to: &Path) -> std::io::Result<u64> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        return Ok(0);
    }
    if meta.is_file() {
        if let Some(p) = to.parent() {
            std::fs::create_dir_all(p)?;
        }
        return std::fs::copy(from, to);
    }
    std::fs::create_dir_all(to)?;
    let mut total = 0;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        total += copy_tree(&e.path(), &to.join(e.file_name()))?;
    }
    Ok(total)
}

/// The steamapps folder games are copied into: Steam's main library on this machine.
pub fn copy_target(home: &Path) -> Option<PathBuf> {
    crate::steam::steam_roots(home).into_iter().next().map(|r| r.join("steamapps"))
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        "tick" => {
            let (page, id) = arg.split_once('|')?;
            let set = ui.picked.entry(page.to_string()).or_default();
            if !set.remove(id) {
                set.insert(id.to_string());
            }
            Some(refresh())
        }
        "tick-all" => {
            let all: BTreeSet<String> = match arg {
                "windrive" => {
                    let mount = ui.windows_drive()?;
                    let dg = drive_games(ui, &mount);
                    dg.games.iter().filter(|g| dg.can_copy(g)).map(|g| g.app_id.clone()).collect()
                }
                "quick" => super::setup::all_ids(ui),
                _ => return None,
            };
            let set = ui.picked.entry(arg.to_string()).or_default();
            *set = if *set == all { BTreeSet::new() } else { all };
            Some(refresh())
        }
        "win-mount" => {
            let part = ui.windows_partition()?;
            let mut p = Plan::new("Read your Windows drive?", "Turbine Hub opens the drive read only to look for Steam games and apps. Nothing on it changes.", "Read it");
            p.items.push((part.clone(), "Opened read only".into()));
            p.steps.push(Step::Host(["flatpak-spawn", "--host", "udisksctl", "mount", "-b", &part, "-o", "ro"].iter().map(|x| x.to_string()).collect()));
            p.done = "Windows drive opened".into();
            Some(dialog(ui, p))
        }
        "copy" => {
            let mount = ui.windows_drive()?;
            let Some(target) = copy_target(&ui.ctx.home) else { return Some(toast("Steam's library wasn't found on this machine.")) };
            let dg = drive_games(ui, &mount);
            let picked = ui.picked.get("windrive").cloned().unwrap_or_default();
            let chosen: Vec<&WinGame> = dg.games.iter().filter(|g| picked.contains(&g.app_id) && dg.can_copy(g)).collect();
            let bytes: u64 = chosen.iter().map(|g| g.bytes).sum();
            if chosen.is_empty() {
                return Some(toast("Tick a game first."));
            }
            if ui.free_bytes.is_some_and(|f| f < bytes) {
                return Some(toast("Not enough room for those. Untick some games."));
            }
            let mut p = Plan::new(&if chosen.len() == 1 { "Copy 1 game?".to_string() } else { format!("Copy {} games?", chosen.len()) }, &format!("{} from your Windows drive into Steam's library here. The Windows drive is only read. Steam checks the files at its next start instead of downloading them.", size_words(bytes)), "Copy");
            p.items = chosen.iter().map(|g| (g.name.clone(), size_words(g.bytes))).collect();
            p.steps = chosen.iter().map(|g| Step::CopyGame { name: g.name.clone(), pairs: bring::copy_plan(g, &target) }).collect();
            p.steps.push(Step::Done { setup: None, brought: Some("windows") });
            p.done = "Games copied. Steam checks them at its next start.".into();
            Some(dialog(ui, p))
        }
        "win-saves" => {
            let Some(mount) = ui.windows_drive() else { return Some(toast("No Windows drive is open.")) };
            let targets = save_targets(ui, &mount);
            let ready = targets.iter().filter(|t| t.prefix.is_some()).count();
            if ready == 0 {
                return Some(toast("No game here can take saves yet. Install a game and start it once first."));
            }
            let mut p = Plan::new("Bring saves over?", "Ludusavi looks for saves on your Windows drive and puts them into the games installed here. The Windows drive is only read. Saves these games have here now are kept first, so Undo brings them back.", "Bring over");
            if !store::is_installed(ui, saves::LUDUSAVI) {
                p.items.push(("Ludusavi".into(), "It does the copying. Installed from Flathub first.".into()));
                p.steps.push(store::install_step(saves::LUDUSAVI));
            }
            p.items.push(("Close your games first".into(), "A game can't be running while its saves change.".into()));
            p.items.push((if ready == 1 { "1 game here can take saves".to_string() } else { format!("{ready} games here can take saves") }, "Windows games from Steam that have been started here once. Saves of other games are kept for later.".into()));
            p.items.push(("Games with online anti-cheat are left alone".into(), String::new()));
            p.steps.push(Step::Saves(Job::Windows { mount, targets }));
            p.done = "Saves brought over. The page says for which games.".into();
            Some(dialog(ui, p))
        }
        "win-saves-undo" => {
            let Some(last) = WindowsRun::load(&ui.folder()).filter(|l| l.kept.is_some() && !l.undone) else { return Some(with(toast("There is nothing to undo."), "refresh", b(true))) };
            let mut p = Plan::new("Undo bringing saves over?", &format!("The saves these games had here before come back: {}.", names(&last.brought)), "Undo");
            p.danger = true;
            p.items.push(("Close your games first".into(), "A game can't be running while its saves change.".into()));
            p.steps.push(Step::Saves(Job::WindowsUndo));
            p.done = "The saves from before are back".into();
            Some(dialog(ui, p))
        }
        "install-apps" => {
            let picked = ui.picked.get("winapps").cloned().unwrap_or_default();
            let chosen: Vec<&bring::AppMap> = bring::APPS.iter().filter(|a| installable(a) && !app_here(ui, a) && picked.contains(a.id)).collect();
            if chosen.is_empty() {
                return Some(toast("Tick an app first."));
            }
            let mut p = Plan::new(&if chosen.len() == 1 { "Install 1 app?".to_string() } else { format!("Install {} apps?", chosen.len()) }, "They come from Flathub and keep themselves up to date. No password needed.", "Install");
            let mut ids: Vec<&str> = Vec::new();
            for a in &chosen {
                p.items.push((a.steamos.to_string(), format!("For {}", a.windows)));
                match a.becomes {
                    Becomes::Flatpak(id) if !ids.contains(&id) => ids.push(id),
                    Becomes::Tile(t) => {
                        if let Some(t) = store::tile(t) {
                            if !ids.contains(&CHROME) && !store::is_installed(ui, CHROME) {
                                ids.push(CHROME);
                            }
                            p.steps.extend(store::tile_steps(ui, t).into_iter().filter(|x| !matches!(x, Step::Host(c) if c.iter().any(|w| w == "install"))));
                        }
                    }
                    _ => {}
                }
            }
            let mut steps: Vec<Step> = ids.iter().map(|id| store::install_step(id)).collect();
            steps.append(&mut p.steps);
            steps.push(Step::Done { setup: None, brought: Some("apps") });
            p.steps = steps;
            p.done = "Apps installed".into();
            Some(dialog(ui, p))
        }
        _ => None,
    }
}

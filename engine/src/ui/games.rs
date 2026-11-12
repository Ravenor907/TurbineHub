//! Games: the list with each game's label, one game's panel (switches, help when it won't start, saves, mods), and
//! the library check. Games whose anti-cheat runs online are locked: they get no switch and no change at all.
use super::launch::{self, Uses};
use super::plan::{Plan, Step, dialog, guide, refresh, toast};
use super::{Ui, arr, b, hint, n, obj, row, s, section, with};
use crate::compat_tool as tool;
use crate::games::{Game, Runtime, Store};
use crate::json::Json;
use crate::library::{self, AntiCheat, Fix, Label, Options, ValveRating, Verdict};
use crate::switches::Switch;
use std::path::{Path, PathBuf};

/// The player's game switches: one file in Turbine Hub's folder, read by the launch tool at each start of a game.
pub use crate::switches::GameSwitches as Switches;

/// What Turbine Hub knows about how games run, from the files in the cache folder: the anti-cheat list, Turbine
/// Hub's fixes and Valve's report per game. `crate::sources::refresh` keeps them current, each with a saved copy
/// and, for the two lists, a built-in one, so they are there offline too.
#[derive(Debug, Default)]
pub struct Ratings {
    anticheat: Vec<(String, AntiCheat)>,
    fixes: Vec<(String, Fix)>,
    cache: PathBuf,
    /// The FSR 4 upgrade does something on this machine (see `super::fsr4_works`): a fix that is only that switch
    /// counts for nothing elsewhere.
    pub fsr4: bool,
}

impl Ratings {
    pub fn load(cache: &Path) -> Ratings {
        let text = |f: &str| std::fs::read_to_string(cache.join(f)).unwrap_or_default();
        Ratings { anticheat: library::parse_anticheat(&text("anticheat.json")), fixes: library::parse_fixes(&text("fixes.json")), cache: cache.to_path_buf(), fsr4: true }
    }
    /// Where Valve's report for one Steam game is kept.
    pub fn valve_file(cache: &Path, app_id: &str) -> PathBuf {
        cache.join("valve").join(format!("{app_id}.json"))
    }
    pub fn verdict(&self, g: &Game, o: Options) -> Verdict {
        if g.store != Store::Steam {
            return library::verdict(ValveRating::Unknown, None, None, o);
        }
        let valve = std::fs::read_to_string(Self::valve_file(&self.cache, &g.store_id)).map(|t| library::parse_valve_report(&t)).unwrap_or(ValveRating::Unknown);
        let ac = self.anticheat.iter().find(|(id, _)| *id == g.store_id).map(|(_, a)| *a);
        let fix = self.fixes.iter().find(|(id, _)| *id == g.store_id).map(|(_, f)| f).filter(|f| self.fsr4 || !(f.kind == "switch" && f.value == "fsr4"));
        library::verdict(valve, ac, fix, o)
    }
}

pub(crate) fn game_id(g: &Game) -> String {
    format!("{}:{}", g.store.key(), g.store_id)
}

fn tone(v: &Verdict) -> &'static str {
    match v.label {
        Label::Works => "ok",
        Label::NeedsFix => "warn",
        Label::WontRun => "stop",
        Label::NotRated => "muted",
    }
}

/// The short label beside a game in the list.
fn chip(v: &Verdict) -> &'static str {
    match v.label {
        Label::Works => "Works",
        Label::NeedsFix => "Fix",
        Label::WontRun => "Blocked",
        Label::NotRated => "Not rated",
    }
}

pub(crate) struct Listed {
    pub game: Game,
    pub verdict: Verdict,
}

impl Listed {
    /// Its anti-cheat runs online: Turbine Hub never changes how it starts.
    pub fn locked(&self) -> bool {
        self.verdict.online_anticheat
    }
    /// A Windows game Steam starts with Proton: the only kind the launch tool and the switches are for.
    pub fn takes_switches(&self) -> bool {
        self.game.store == Store::Steam && self.game.runtime != Runtime::Native && !self.locked() && self.verdict.label != Label::WontRun
    }
}

pub(crate) fn listed(ui: &Ui) -> Vec<Listed> {
    let m = ui.machine();
    let ratings = Ratings { fsr4: super::fsr4_works(&m), ..Ratings::load(&ui.cache) };
    let o = Options { windows_installed: m.windows_boot.is_some() };
    crate::scan_games(&ui.ctx.home).games.into_iter().map(|g| Listed { verdict: ratings.verdict(&g, o), game: g }).collect()
}

pub(crate) fn find(ui: &Ui, id: &str) -> Option<Listed> {
    listed(ui).into_iter().find(|l| game_id(&l.game) == id)
}

fn runs_with(g: &Game) -> &'static str {
    match g.runtime {
        Runtime::Windows => "Proton",
        Runtime::Native => "Linux version",
        Runtime::Unknown => "Not started yet",
    }
}

/// Does `text` hold every word of what the player typed? Case and marks like the apostrophe in "Baldur's" don't
/// matter, and neither do the spaces inside a name ("eldenring" finds "Elden Ring"). An empty search matches
/// everything.
pub fn matches(query: &str, text: &str) -> bool {
    let plain = |t: &str| -> String { t.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).flat_map(char::to_lowercase).collect() };
    let hay = plain(text);
    let tight: String = hay.split_whitespace().collect();
    plain(query).split_whitespace().all(|w| hay.contains(w) || tight.contains(w))
}

/// What the player typed into a page's search box ("" when nothing).
pub(crate) fn query(ui: &Ui, page: &str) -> String {
    ui.filter.get(&format!("search:{page}")).cloned().unwrap_or_default()
}

/// The search box of a side page: what it holds and the words shown while it is empty.
pub(crate) fn search_box(ui: &Ui, page: &str, hint: &str) -> Json {
    obj! {"arg": s(page), "value": s(query(ui, page)), "hint": s(hint)}
}

/// Why the FSR 4 upgrade isn't offered on this machine.
const FSR4_ELSEWHERE: &str = "The FSR 4 upgrade needs a Radeon RX 9000 graphics card. On this one it would change nothing.";

/// The words under a switch, as short as the row allows.
fn switch_line(ui: &Ui, sw: Switch) -> &'static str {
    match sw {
        Switch::Fsr4 if !super::fsr4_works(&ui.machine()) => "Needs a Radeon RX 9000 card. It changes nothing on this one",
        Switch::Fsr4 => "Sharper upscaling, costs a few frames",
        Switch::FrameGen if tool::framegen_wrapper(&ui.ctx.home).is_file() => "Lossless Scaling, set the multiplier in Decky",
        Switch::FrameGen => "Needs the Lossless Scaling plugin from Decky's store first",
        Switch::Handheld => "The game's handheld presets and on-screen keyboard",
        Switch::ProtonLog => "Writes a log of the next launch into your home folder",
    }
}

/// The ways to play a game that won't run here.
/// The cloud services come first: Restart to Windows is a bigger step, so it is never where the highlight lands. It
/// is offered only when Windows is really on this machine (`windows_boot` needs its partition to be there).
fn alternatives(ui: &Ui) -> Vec<Json> {
    let mut out = Vec::new();
    for id in ["geforcenow", "xcloud"] {
        let Some(t) = super::store::tile(id) else { continue };
        let line = if id == "geforcenow" { "Streams the game from a PC in the cloud, when the game is offered there" } else { "Streams it when it is part of Game Pass" };
        out.push(if super::store::tile_added(ui, t) {
            with(with(with(row("none", "", "info", t.name, line), "icon", s("cloud")), "badge", s("In your library")), "tone", s("ok"))
        } else {
            with(with(row("tile", t.id, "button", t.name, line), "icon", s("cloud")), "button", s("Add tile"))
        });
    }
    if ui.machine().windows_boot.is_some() {
        out.push(with(with(row("open", "module:windows", "button", "Restart to Windows", "It runs there. The next start only, then SteamOS again."), "icon", s("reboot")), "button", s("Open")));
    }
    out
}

/// The first row of a game that runs: Play, where the highlight lands when the game is opened.
fn play_row(l: &Listed) -> Json {
    let line = if l.game.store == Store::Steam { "Steam starts it" } else { "It starts from Heroic, or from its tile in your Steam library" };
    with(with(with(row("game", &format!("play|{}", game_id(&l.game)), "button", "Play", line), "icon", s("play")), "button", s("Play")), "primary", b(true))
}

fn panel(ui: &Ui, l: &Listed, tv: &launch::ToolView) -> Vec<Json> {
    let id = game_id(&l.game);
    let mut out = Vec::new();
    if l.verdict.label == Label::WontRun {
        // Nothing Turbine Hub can switch makes it run: only the other ways to play it.
        out.push(section("Play it another way", alternatives(ui)));
        return out;
    }
    out.push(section("", vec![play_row(l)]));
    if l.locked() {
        // It plays fine: only Turbine Hub keeps its hands off it.
        let r = row("none", "", "info", "Plays as it is", "Its anti-cheat runs online, so Turbine Hub never changes how this game starts. No switches, nothing to set up.");
        out.push(section("Online anti-cheat", vec![with(r, "icon", s("shield"))]));
    } else {
        let uses = tv.uses(&l.game);
        if l.game.runtime == Runtime::Native {
            out.push(section("Switches", vec![row("none", "", "info", "Linux version", "It runs without Proton, so the switches don't apply.")]));
        } else if l.game.store != Store::Steam {
            out.push(section("Switches", vec![row("none", "", "info", "Set in Heroic", "The switches reach games that Steam starts. For this one, open its settings in Heroic.")]));
        } else if uses == Uses::Mods {
            let mut rows = vec![with(with(with(row("none", "", "info", "Uses Turbine Mod Manager in Steam", "Steam allows one tool per game, so this game's switches come with Turbine Mod Manager."), "icon", s("external")), "badge", s("Mods")), "tone", s("muted"))];
            rows.push(if mod_manager(ui) {
                with(row("game", &format!("mods|{id}"), "button", "Open Turbine Mod Manager", "Its settings for this game"), "icon", s("external"))
            } else {
                // Its tool is still picked in Steam, but the app itself isn't here: say how to switch back.
                with(row("none", "", "info", "Turbine Mod Manager isn't installed", "To use Turbine Hub's switches instead: in Steam, the game's Properties, Compatibility, pick \"Turbine Hub\"."), "icon", s("info"))
            });
            out.push(section("Switches", rows));
        } else {
            let sw = Switches::load(&ui.folder());
            let mut rows = vec![launch::game_row(tv, &l.game, uses)];
            let fsr4 = super::fsr4_works(&ui.machine());
            rows.extend(Switch::ALL.iter().filter(|k| fsr4 || **k != Switch::Fsr4 || sw.is_on(&id, Switch::Fsr4) || sw.fsr4_all).map(|k| with(row("switch", &format!("{id}|{}", k.key()), "toggle", k.title(), switch_line(ui, *k)), "on", b(sw.is_on(&id, *k) || (*k == Switch::Fsr4 && sw.fsr4_all)))));
            out.push(section("Switches", rows));
        }
        let mut help = Vec::new();
        if l.game.runtime != Runtime::Native {
            let line = if l.game.store == Store::Steam { launch::proton_line(ui, tv, &l.game) } else { "Heroic keeps this choice".to_string() };
            help.push(with(row("open", &format!("proton|{id}"), "button", "Try another Proton", &line), "icon", s("swap")));
        }
        if l.game.prefix.is_some() {
            help.push(with(row("game", &format!("reset|{id}"), "button", "Reset Windows files", "A fresh start, the old ones kept as a backup"), "icon", s("restore")));
        }
        if let Some(pfx) = pfx_path(&l.game)
            && reset_backup(&ui.folder(), &l.game.store_id, &pfx).is_some()
        {
            help.push(with(with(row("game", &format!("reset-undo|{id}"), "button", "Undo the reset", "Puts the Windows files from before the reset back"), "icon", s("restore")), "button", s("Undo")));
        }
        help.push(with(row("game", &format!("report|{id}"), "button", "Write a report", "What a helper needs to see, with private details removed"), "icon", s("log")));
        out.push(section("Won't start?", help));
    }
    let line = match crate::snapshots::list(&ui.data, &id).len() {
        0 => "Back up now, restore a snapshot, undo".to_string(),
        1 => "1 snapshot. Restoring keeps the current saves first".into(),
        c => format!("{c} snapshots. Restoring one keeps the current saves first"),
    };
    out.push(section("Saves", vec![with(row("open", &format!("saves|{id}"), "button", "Saves and snapshots", &line), "icon", s("save"))]));
    // Mods live in Turbine Mod Manager: the row is there only when that app is (the switches above say the rest
    // for a game that uses its tool).
    if !l.locked() && mod_manager(ui) && tv.uses(&l.game) != Uses::Mods {
        out.push(section("Mods", vec![with(row("game", &format!("mods|{id}"), "button", "Mods: open Turbine Mod Manager", "Turbine Mod Manager handles mods"), "icon", s("external"))]));
    }
    out
}

/// Is Turbine Mod Manager installed?
fn mod_manager(ui: &Ui) -> bool {
    ui.installed.iter().any(|a| a == super::store::TURBINE)
}

/// The order of the list: games that run first, the ones that won't run here at the end; by name inside each.
fn play_order(l: &Listed) -> (bool, String) {
    (l.verdict.label == Label::WontRun, l.game.name.to_lowercase())
}

/// The Games tab: every installed game on the left, the picked one's panel on the right. `arg` is the picked game.
/// What was typed into the search box narrows the list.
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    let everything = listed(ui);
    let total = everything.len();
    let q = query(ui, "games");
    let tv = launch::tool_view(ui);
    // Not at every letter of a search: the files only follow what Steam and the ratings say.
    if q.is_empty() {
        launch::keep_files(ui, &tv, &everything);
    }
    let mut all: Vec<Listed> = everything.into_iter().filter(|l| matches(&q, &l.game.name)).collect();
    all.sort_by_key(play_order);
    let search = search_box(ui, "games", "Search games");
    let picked = all.iter().position(|l| game_id(&l.game) == arg).unwrap_or(0);
    let side = arr(all.iter().map(|l| obj! {"id": s(game_id(&l.game)), "title": s(&l.game.name), "chip": s(chip(&l.verdict)), "tone": s(tone(&l.verdict)), "lock": b(false)}));
    let Some(l) = all.get(picked) else {
        if total == 0 {
            return obj! {"side": arr([]), "sideTitle": s("No games yet"), "index": n(0.0), "empty": s("No games found. Install one in Steam or Heroic and it shows up here."), "sections": arr([]), "hints": arr([])};
        }
        return obj! {"side": arr([]), "sideTitle": s(format!("0 of {total} games")), "index": n(0.0), "search": search, "empty": s(format!("No game matches \"{}\".", q.trim())), "sections": arr([]), "hints": arr([hint("rt", "Search")])};
    };
    let side_title = match (q.trim().is_empty(), total) {
        (true, 1) => "Installed \u{b7} 1 game".to_string(),
        (true, c) => format!("Installed \u{b7} {c} games"),
        (false, c) => format!("{} of {c} games", all.len()),
    };
    let mut chips = vec![obj! {"text": s(l.verdict.label.words()), "tone": s(tone(&l.verdict))}];
    if l.takes_switches() && tv.uses(&l.game) == Uses::Hub && tv.state != tool::State::Missing {
        chips.push(obj! {"text": s("Uses Turbine Hub"), "tone": s("ok")});
    }
    let playable = l.verdict.label != Label::WontRun;
    let mut hints = vec![hint("rt", "Search"), hint("y", "Library check")];
    if playable {
        hints.insert(0, hint("x", "Play"));
    }
    obj! {
        "side": side,
        "sideTitle": s(side_title),
        "index": n(picked as f64),
        "search": search,
        "header": obj! {
            "title": s(&l.game.name),
            "chips": arr(chips),
            "line": s(format!("{} \u{b7} {} \u{b7} {}", l.game.store.label(), runs_with(&l.game), l.verdict.note)),
        },
        "sections": arr(panel(ui, l, &tv)),
        "hints": arr(hints),
        "x": if playable { obj! {"act": s("game"), "arg": s(format!("play|{}", game_id(&l.game)))} } else { Json::Null },
        "y": obj! {"act": s("open"), "arg": s("library")},
    }
}

/// Works first, then not rated, needs a fix, won't run; by name inside each.
fn order(l: Label) -> u8 {
    match l {
        Label::Works => 0,
        Label::NotRated => 1,
        Label::NeedsFix => 2,
        Label::WontRun => 3,
    }
}

const FILTERS: &[(&str, &str)] = &[("all", "All"), ("fix", "Need a fix"), ("no", "Won't run here")];

/// Will my games run? Counts, a filter and one row per game.
pub(crate) fn library(ui: &mut Ui) -> Json {
    let mut all = listed(ui);
    all.sort_by_key(|l| (order(l.verdict.label), l.game.name.to_lowercase()));
    let verdicts: Vec<Verdict> = all.iter().map(|l| l.verdict.clone()).collect();
    let (works, fix, no, unrated) = library::counts(&verdicts);
    let filter = ui.filter.get("library").cloned().unwrap_or_else(|| "all".into());
    let never_checked = super::checked_at(&ui.cache).is_none();
    let rows: Vec<Json> = all
        .iter()
        .filter(|l| filter == "all" || l.verdict.label.key() == filter)
        .map(|l| {
            // Before the lists were ever fetched, "not rated" only means "not looked up yet".
            let note = if l.verdict.label == Label::NotRated && never_checked { "Waiting for the ratings. They arrive once the internet is there." } else { l.verdict.note.as_str() };
            let mut r = row("fix", &game_id(&l.game), if l.verdict.fix.is_some() { "button" } else { "info" }, &l.game.name, &format!("{} \u{b7} {note}", l.game.store.label()));
            r = with(r, "badge", s(l.verdict.label.words()));
            r = with(r, "tone", s(tone(&l.verdict)));
            if l.verdict.fix.is_some() {
                r = with(r, "button", s("Fix"));
            }
            r
        })
        .collect();
    let mut stores: Vec<&str> = all.iter().map(|l| l.game.store.label()).collect();
    stores.sort();
    stores.dedup();
    let idx = FILTERS.iter().position(|(k, _)| *k == filter).unwrap_or(0);
    let pick = with(with(row("filter", "library", "choice", "Show", ""), "options", arr(FILTERS.iter().map(|(_, t)| s(*t)))), "index", n(idx as f64));
    let empty = if all.is_empty() { "No games found yet. Install one in Steam or Heroic first." } else if rows.is_empty() { "Nothing here." } else { "" };
    obj! {
        "title": s("Will my games run?"),
        "intro": s("Checked against Valve's ratings, the anti-cheat lists and Turbine Hub's own fixes."),
        "tiles": arr([
            obj! {"value": s(works.to_string()), "label": s(if works == 1 { "Works" } else { "Work" }), "tone": s("ok")},
            obj! {"value": s(fix.to_string()), "label": s(if fix == 1 { "Needs a fix" } else { "Need a fix" }), "tone": s("warn")},
            obj! {"value": s(no.to_string()), "label": s("Won't run here"), "tone": s("stop")},
            obj! {"value": s(unrated.to_string()), "label": s("Not rated yet"), "tone": s("muted")},
        ]),
        "note": s(match all.len() { 0 => String::new(), 1 => format!("1 game \u{b7} {}", stores.join(", ")), c => format!("{c} games \u{b7} {}", stores.join(", ")) }),
        "fresh": s(super::checked_words(crate::now(), super::checked_at(&ui.cache), ui.offline)),
        "stale": b(ui.offline),
        "sections": arr([section("", vec![pick]), with(section("", rows), "empty", s(empty))]),
        "primary": if fix > 0 { obj! {"label": s(format!("Fix all {fix}")), "k": s("x"), "act": s("fix-all"), "arg": s("")} } else { Json::Null },
        "hints": arr(if fix > 0 { vec![hint("x", "Fix all")] } else { vec![] }),
    }
}

/// Turns a game's Windows files folder into a backup beside it, so the next start makes fresh ones. Answers the
/// backup's name.
pub fn reset_windows_files(pfx: &Path, now: u64) -> Result<String, String> {
    if pfx.file_name().and_then(|n| n.to_str()) != Some("pfx") || !pfx.is_dir() {
        return Err("This game has no Windows files yet.".into());
    }
    let name = format!("pfx.turbinehub-backup-{now}");
    std::fs::rename(pfx, pfx.with_file_name(&name)).map_err(|e| format!("Couldn't move the Windows files: {e}"))?;
    Ok(name)
}

/// Where reports are written: a folder in Turbine Hub's own folder.
pub fn reports_dir(ui: &Ui) -> PathBuf {
    ui.folder().join("reports")
}

/// Where a Steam game's Windows files are, or go once it starts: also while a reset has moved them aside.
fn pfx_path(g: &Game) -> Option<PathBuf> {
    if let Some(p) = &g.prefix {
        return Some(p.clone());
    }
    if g.store != Store::Steam {
        return None;
    }
    let steamapps = g.install_dir.parent()?.parent()?;
    Some(steamapps.join("compatdata").join(&g.store_id).join("pfx"))
}

/// Where the last reset of a game's Windows files is recorded for Undo: beside its Proton pick's Undo, in Turbine
/// Hub's launch folder.
pub fn reset_record(folder: &Path, app_id: &str) -> PathBuf {
    tool::launch_dir(folder).join(format!("{}.reset", app_id.replace(['/', '.'], "")))
}

/// The backup the last reset of this game made, when it is still there.
pub fn reset_backup(folder: &Path, app_id: &str, pfx: &Path) -> Option<PathBuf> {
    let name = std::fs::read_to_string(reset_record(folder, app_id)).ok()?.trim().to_string();
    let ok = name.strip_prefix("pfx.turbinehub-backup-").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    let path = pfx.with_file_name(&name);
    (ok && path.is_dir()).then_some(path)
}

/// Puts the backup back as the game's Windows files. Fresh ones the game made since are kept beside them under a
/// name of their own, never deleted. Answers that name, when there were fresh ones.
pub fn undo_reset(pfx: &Path, backup: &Path, now: u64) -> Result<Option<String>, String> {
    if !backup.is_dir() {
        return Err("The backup isn't there any more.".into());
    }
    let mut kept = None;
    if pfx.exists() {
        let name = format!("pfx.turbinehub-fresh-{now}");
        std::fs::rename(pfx, pfx.with_file_name(&name)).map_err(|e| format!("Couldn't move the fresh Windows files aside: {e}"))?;
        kept = Some(name);
    }
    std::fs::rename(backup, pfx).map_err(|e| format!("Couldn't put the Windows files back: {e}"))?;
    Ok(kept)
}

/// The reset's own steps of a confirmed dialog. None for any other step.
pub fn reset_step(step: &Step, folder: &Path, now: u64) -> Option<Result<(), String>> {
    Some(match step {
        Step::ResetPrefix { pfx, app_id } => reset_windows_files(pfx, now).and_then(|name| {
            let record = reset_record(folder, app_id);
            let _ = std::fs::create_dir_all(tool::launch_dir(folder));
            crate::write_atomic(&record, format!("{name}\n").as_bytes()).map_err(|e| format!("The files were moved, but Undo couldn't be recorded: {e}"))
        }),
        Step::ResetUndo { pfx, app_id } => match reset_backup(folder, app_id, pfx) {
            None => Err("The backup isn't there any more.".into()),
            Some(backup) => undo_reset(pfx, &backup, now).map(|_| {
                let _ = std::fs::remove_file(reset_record(folder, app_id));
            }),
        },
        _ => return None,
    })
}

/// The report for one game: what Turbine Hub knows about it and the machine, private details removed.
pub fn report_text(ui: &Ui, id: &str, when: &str) -> String {
    let m = ui.machine();
    let st = ui.settings();
    let sw = Switches::load(&ui.folder());
    let games = find(ui, id).map(|l| vec![format!("{} ({id}): {}, {}, switches on: {}", l.game.name, runs_with(&l.game), l.verdict.label.words(), Some(sw.applied(id).iter().map(|x| x.key()).collect::<Vec<_>>().join(", ")).filter(|x| !x.is_empty()).unwrap_or_else(|| "none".into()))]).unwrap_or_default();
    crate::report::build(&crate::report::ProblemReport {
        version: ui.ctx.version.clone(),
        when: when.into(),
        device: vec![format!("Machine: {}", super::top_name(&m)), format!("Processor: {}", m.cpu), format!("Memory: {} GB", m.memory_gb), format!("SteamOS: {}", m.steamos)],
        screen: "Games".into(),
        settings: vec![format!("mode: {:?}", st.mode), format!("game boost: {}", st.game_boost)],
        games,
        log: ui.ledger().entries.iter().map(|e| e.change.words()).collect(),
        launch: Vec::new(),
    })
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    match action {
        // What was typed into a page's search box.
        "search" => {
            if !["games", "store"].contains(&arg) {
                return None;
            }
            let text: String = value.chars().filter(|c| !c.is_control()).take(60).collect();
            ui.filter.insert(format!("search:{arg}"), text);
            Some(refresh())
        }
        "fsr4all" => {
            let mut sw = Switches::load(&ui.folder());
            if !sw.fsr4_all && !super::fsr4_works(&ui.machine()) {
                return Some(toast(FSR4_ELSEWHERE));
            }
            sw.fsr4_all = !sw.fsr4_all;
            sw.save(&ui.folder());
            Some(refresh())
        }
        "switch" => {
            let (id, k) = arg.split_once('|')?;
            let Some(l) = find(ui, id).filter(|l| !l.locked()) else {
                return Some(toast("Its anti-cheat runs online, so Turbine Hub leaves this game alone."));
            };
            if !l.takes_switches() {
                return Some(toast("The switches are for Windows games that Steam starts."));
            }
            let uses = launch::tool_view(ui).uses(&l.game);
            if uses == Uses::Mods {
                return Some(toast("This game uses Turbine Mod Manager in Steam. Its switches come with Turbine Mod Manager."));
            }
            if k == Switch::Fsr4.key() && !super::fsr4_works(&ui.machine()) && !Switches::load(&ui.folder()).on(id, k) {
                return Some(toast(FSR4_ELSEWHERE));
            }
            let mut sw = Switches::load(&ui.folder());
            Some(match sw.toggle(id, k, false) {
                Ok(_) => {
                    sw.save(&ui.folder());
                    with(refresh(), "toast", s(if uses == Uses::Hub { "Applies the next time the game starts." } else { "Saved. It applies once this game uses Turbine Hub in Steam." }))
                }
                Err(e) => toast(e),
            })
        }
        "game" => {
            let (what, id) = arg.split_once('|')?;
            let l = find(ui, id)?;
            let name = l.game.name.clone();
            let locked = l.locked();
            Some(match what {
                "play" if l.verdict.label == Label::WontRun => toast("It won't run on SteamOS. Its page lists the other ways to play it."),
                "play" if l.game.store == Store::Steam => {
                    let mut p = Plan::new(&format!("Play {name}?"), "Steam starts the game.", "Play");
                    p.steps.push(Step::Host(["flatpak-spawn", "--host", "xdg-open", &format!("steam://rungameid/{}", l.game.store_id)].iter().map(|x| x.to_string()).collect()));
                    p.done = format!("Starting {name}");
                    dialog(ui, p)
                }
                "play" => guide("Start it from Heroic", "Games from Epic, GOG and Amazon start from Heroic, or from their tile in your Steam library.", &[]),
                "reset" | "reset-undo" if locked => toast("Its anti-cheat runs online, so Turbine Hub leaves this game alone."),
                "reset" => {
                    let pfx = l.game.prefix.clone()?;
                    let backup = "pfx.turbinehub-backup-<time of the reset>";
                    let veteran = ui.veteran();
                    let ludusavi = super::store::is_installed(ui, crate::saves::LUDUSAVI);
                    let mut p = Plan::new("Reset Windows files?", &format!("{name} gets fresh Windows files the next time it starts. The old ones are moved into a backup folder right beside them, nothing is deleted, and Undo on this page puts them back."), "Reset");
                    p.danger = true;
                    p.items.push(("Close the game first".into(), "It can't be running while this happens.".into()));
                    p.items.push(("Its Windows files move into a backup".into(), if veteran { format!("{} \u{2192} {}", pfx.display(), pfx.with_file_name(backup).display()) } else { "Same folder, under a new name.".into() }));
                    if ludusavi {
                        p.items.push(("A snapshot of its saves first".into(), "Saves the game keeps inside its Windows files would otherwise start empty. Its Saves page restores them after its next start.".into()));
                        p.steps.push(Step::Saves(crate::saves::Job::Backup { game: super::saves::game_ref(&l) }));
                    } else {
                        p.items.push(("Saves kept inside them go into the backup too".into(), "The game starts without them until Undo puts the old files back. Most Steam games keep their saves in Steam Cloud as well.".into()));
                    }
                    p.steps.push(Step::ResetPrefix { pfx, app_id: l.game.store_id.clone() });
                    p.done = "Windows files reset. Undo is on this game's page.".into();
                    dialog(ui, p)
                }
                "reset-undo" => {
                    let pfx = pfx_path(&l.game)?;
                    let Some(backup) = reset_backup(&ui.folder(), &l.game.store_id, &pfx) else { return Some(with(toast("The backup isn't there any more."), "refresh", b(true))) };
                    let mut p = Plan::new("Put the old Windows files back?", &format!("The Windows files of {name} from before the reset come back. Fresh ones made since are kept beside them under a new name, nothing is deleted."), "Put back");
                    p.items.push(("Close the game first".into(), "It can't be running while this happens.".into()));
                    if ui.veteran() {
                        p.items.push((format!("{} \u{2192} {}", backup.display(), pfx.display()), String::new()));
                    }
                    p.steps.push(Step::ResetUndo { pfx, app_id: l.game.store_id.clone() });
                    p.done = "The old Windows files are back".into();
                    dialog(ui, p)
                }
                "report" => {
                    let mut p = Plan::new("Write a report?", &format!("Turbine Hub writes what it knows about {name} and this machine into a text file. Names, addresses and keys are removed. Nothing is sent anywhere."), "Write it");
                    let at = reports_dir(ui);
                    p.items.push(("Where it goes".into(), at.to_string_lossy().into_owned()));
                    p.steps.push(Step::Report { game: id.to_string() });
                    p.done = format!("Report written. It's in {}", at.display());
                    dialog(ui, p)
                }
                "mods" if locked => toast("Its anti-cheat runs online, so mods stay off."),
                "mods" => {
                    if !mod_manager(ui) {
                        return Some(toast("Turbine Mod Manager isn't installed here. It comes from github.com/Ravenor907/Turbine-Mod-Manager."));
                    }
                    let mut p = Plan::new("Open Turbine Mod Manager?", "Mods live in Turbine Mod Manager. It opens on top of Turbine Hub.", "Open");
                    p.steps.push(Step::Host(["flatpak-spawn", "--host", "flatpak", "run", super::store::TURBINE].iter().map(|x| x.to_string()).collect()));
                    p.done = "Opening Turbine Mod Manager".into();
                    dialog(ui, p)
                }
                _ => return None,
            })
        }
        "fix" | "fix-all" => {
            let all = listed(ui);
            let mut sw = Switches::load(&ui.folder());
            let (mut done, mut notes) = (0, Vec::new());
            for l in all.iter().filter(|l| action == "fix-all" || game_id(&l.game) == arg) {
                let Some(f) = &l.verdict.fix else { continue };
                let id = game_id(&l.game);
                if f.kind == "switch" && l.takes_switches() && !sw.on(&id, &f.value) && sw.toggle(&id, &f.value, false).is_ok() {
                    done += 1;
                } else if f.kind != "switch" {
                    notes.push((l.game.name.clone(), if f.kind == "proton" { format!("Use {} for this game: Games, the game, Try another Proton.", f.value) } else { f.words.clone() }));
                }
            }
            sw.save(&ui.folder());
            Some(if notes.is_empty() {
                with(refresh(), "toast", s(match done { 0 => "Nothing to fix.".to_string(), 1 => "Fix on. It applies the next time the game starts.".into(), c => format!("{c} fixes on. They apply the next time each game starts.") }))
            } else {
                let items: Vec<(&str, &str)> = notes.iter().map(|(a, t)| (a.as_str(), t.as_str())).collect();
                with(guide("How to fix", "These need a step of yours.", &items), "refresh", b(true))
            })
        }
        _ => None,
    }
}

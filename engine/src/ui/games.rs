//! Games: the list with each game's label, one game's panel (switches, help when it won't start, saves, mods), and
//! the library check. Games whose anti-cheat runs online are locked: they get no switch and no change at all.
use super::plan::{Plan, Step, dialog, guide, refresh, toast};
use super::{Ui, arr, b, hint, n, obj, row, s, section, with};
use crate::games::{Game, Runtime, Store};
use crate::json::{self, Json};
use crate::library::{self, AntiCheat, Fix, Label, Options, ValveRating, Verdict};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The per-game switches, in the order shown: (id, name, what it does).
pub const SWITCHES: &[(&str, &str, &str)] = &[
    ("fsr4", "FSR 4 upgrade", "Sharper upscaling, costs a few frames"),
    ("framegen", "Frame generation", "Lossless Scaling, set the multiplier in Decky"),
    ("handheld", "Handheld mode", "The game's handheld presets and on-screen keyboard"),
    ("protonlog", "Proton log", "Writes a log of the next launch"),
];

/// The player's game switches: one file in Turbine Hub's folder, read by the launch tool at each start of a game.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Switches {
    /// FSR 4 for every Proton game that isn't locked.
    pub fsr4_all: bool,
    /// Game id ("steam:1086940") to the switches that are on.
    pub games: BTreeMap<String, BTreeSet<String>>,
}

impl Switches {
    fn file(folder: &Path) -> PathBuf {
        folder.join("games.json")
    }
    pub fn parse(text: &str) -> Switches {
        let Ok(j) = json::parse(text) else { return Switches::default() };
        let mut games = BTreeMap::new();
        for (id, v) in j.get("games").map(Json::entries).unwrap_or(&[]) {
            let on: BTreeSet<String> = v.as_array().iter().filter_map(Json::as_str).filter(|x| SWITCHES.iter().any(|(k, _, _)| k == x)).map(String::from).collect();
            if !on.is_empty() {
                games.insert(id.clone(), on);
            }
        }
        Switches { fsr4_all: j.get("fsr4All").and_then(Json::as_bool).unwrap_or(false), games }
    }
    pub fn to_json(&self) -> String {
        let games = Json::Obj(self.games.iter().map(|(id, on)| (id.clone(), arr(on.iter().map(s)))).collect());
        obj! {"fsr4All": b(self.fsr4_all), "games": games}.to_text() + "\n"
    }
    pub fn load(folder: &Path) -> Switches {
        std::fs::read_to_string(Self::file(folder)).map(|t| Switches::parse(&t)).unwrap_or_default()
    }
    pub fn save(&self, folder: &Path) {
        let _ = crate::write_atomic(&Self::file(folder), self.to_json().as_bytes());
    }
    pub fn on(&self, game: &str, switch: &str) -> bool {
        self.games.get(game).is_some_and(|x| x.contains(switch))
    }
    /// Flips one switch. A locked game is refused and nothing changes.
    pub fn toggle(&mut self, game: &str, switch: &str, locked: bool) -> Result<bool, &'static str> {
        if locked {
            return Err("Its anti-cheat runs online, so Turbine Hub leaves this game alone.");
        }
        if !SWITCHES.iter().any(|(k, _, _)| *k == switch) {
            return Err("Unknown switch.");
        }
        let set = self.games.entry(game.to_string()).or_default();
        let now = if set.remove(switch) { false } else { set.insert(switch.to_string()) };
        if set.is_empty() {
            self.games.remove(game);
        }
        Ok(now)
    }
}

/// What Turbine Hub knows about how games run, from the files the app fetched into the cache folder: the anti-cheat
/// list, Turbine Hub's fixes and Valve's report per game.
#[derive(Debug, Default)]
pub struct Ratings {
    anticheat: Vec<(String, AntiCheat)>,
    fixes: Vec<(String, Fix)>,
    cache: PathBuf,
}

pub const ANTICHEAT_URL: &str = "https://raw.githubusercontent.com/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json";
pub const FIXES_URL: &str = "https://raw.githubusercontent.com/Ravenor907/TurbineHub/main/data/fixes.json";

impl Ratings {
    pub fn load(cache: &Path) -> Ratings {
        let text = |f: &str| std::fs::read_to_string(cache.join(f)).unwrap_or_default();
        Ratings { anticheat: library::parse_anticheat(&text("anticheat.json")), fixes: library::parse_fixes(&text("fixes.json")), cache: cache.to_path_buf() }
    }
    /// Where Valve's report for one Steam game is kept.
    pub fn valve_file(cache: &Path, app_id: &str) -> PathBuf {
        cache.join("valve").join(format!("{app_id}.json"))
    }
    pub fn valve_url(app_id: &str) -> String {
        format!("https://store.steampowered.com/saleaction/ajaxgetdeckappcompatibilityreport?nAppID={app_id}")
    }
    pub fn verdict(&self, g: &Game, o: Options) -> Verdict {
        if g.store != Store::Steam {
            return library::verdict(ValveRating::Unknown, None, None, o);
        }
        let valve = std::fs::read_to_string(Self::valve_file(&self.cache, &g.store_id)).map(|t| library::parse_valve_report(&t)).unwrap_or(ValveRating::Unknown);
        let ac = self.anticheat.iter().find(|(id, _)| *id == g.store_id).map(|(_, a)| *a);
        let fix = self.fixes.iter().find(|(id, _)| *id == g.store_id).map(|(_, f)| f);
        library::verdict(valve, ac, fix, o)
    }
}

/// What the app fetches to keep the ratings fresh: (address, file in the cache folder). Valve's reports only for
/// games without one yet.
pub fn rating_downloads(ui: &Ui) -> Vec<(String, PathBuf)> {
    let mut out = vec![(ANTICHEAT_URL.to_string(), ui.cache.join("anticheat.json")), (FIXES_URL.to_string(), ui.cache.join("fixes.json"))];
    for g in crate::scan_games(&ui.ctx.home).games.iter().filter(|g| g.store == Store::Steam) {
        let f = Ratings::valve_file(&ui.cache, &g.store_id);
        if !f.exists() {
            out.push((Ratings::valve_url(&g.store_id), f));
        }
    }
    out
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
        _ if v.online_anticheat => "Online anti-cheat",
        Label::Works => "Works",
        Label::NeedsFix => "Fix",
        Label::WontRun => "Blocked",
        Label::NotRated => "Not rated",
    }
}

struct Listed {
    game: Game,
    verdict: Verdict,
}

fn listed(ui: &Ui) -> Vec<Listed> {
    let ratings = Ratings::load(&ui.cache);
    let o = Options { windows_installed: ui.machine().windows_boot.is_some() };
    crate::scan_games(&ui.ctx.home).games.into_iter().map(|g| Listed { verdict: ratings.verdict(&g, o), game: g }).collect()
}

fn find(ui: &Ui, id: &str) -> Option<Listed> {
    listed(ui).into_iter().find(|l| game_id(&l.game) == id)
}

fn runs_with(g: &Game) -> &'static str {
    match g.runtime {
        Runtime::Windows => "Proton",
        Runtime::Native => "Linux version",
        Runtime::Unknown => "Not started yet",
    }
}

fn panel(ui: &Ui, l: &Listed) -> Vec<Json> {
    let id = game_id(&l.game);
    let mut out = Vec::new();
    if l.verdict.online_anticheat {
        let mut r = row("none", "", "info", "Locked", "Its anti-cheat runs online. Turbine Hub never changes how this game starts.");
        r = with(r, "icon", s("lock"));
        out.push(section("Online anti-cheat", vec![r]));
    } else {
        let sw = Switches::load(&ui.folder());
        if l.game.runtime == Runtime::Native {
            out.push(section("Switches", vec![row("none", "", "info", "Linux version", "It runs without Proton, so the switches don't apply.")]));
        } else {
            out.push(section("Switches", SWITCHES.iter().map(|(k, title, line)| with(row("switch", &format!("{id}|{k}"), "toggle", title, line), "on", b(sw.on(&id, k) || (*k == "fsr4" && sw.fsr4_all)))).collect()));
        }
        let mut help = vec![with(row("game", &format!("proton|{id}"), "button", "Try another Proton", "Where Steam keeps the choice"), "icon", s("swap"))];
        if l.game.prefix.is_some() {
            help.push(with(row("game", &format!("reset|{id}"), "button", "Reset Windows files", "A fresh start, the old ones kept as a backup"), "icon", s("restore")));
        }
        help.push(with(row("game", &format!("report|{id}"), "button", "Write a report", "What a helper needs to see, with private details removed"), "icon", s("log")));
        out.push(section("Won't start?", help));
    }
    out.push(section("Saves", vec![with(row("game", &format!("backup|{id}"), "button", "Back up saves now", "With Ludusavi"), "icon", s("save")), with(row("game", &format!("restore|{id}"), "button", "Restore a save", "Puts the newest backup back"), "icon", s("restore"))]));
    if !l.verdict.online_anticheat {
        out.push(section("Mods", vec![with(row("game", &format!("mods|{id}"), "button", "Mods: open Turbine", "Turbine Mod Manager handles mods"), "icon", s("external"))]));
    }
    out
}

/// The Games tab: every installed game on the left, the picked one's panel on the right. `arg` is the picked game.
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    let all = listed(ui);
    let picked = all.iter().position(|l| game_id(&l.game) == arg).unwrap_or(0);
    let side = arr(all.iter().map(|l| obj! {"id": s(game_id(&l.game)), "title": s(&l.game.name), "chip": s(chip(&l.verdict)), "tone": s(if l.verdict.online_anticheat { "muted" } else { tone(&l.verdict) }), "lock": b(l.verdict.online_anticheat)}));
    let Some(l) = all.get(picked) else {
        return obj! {"side": arr([]), "sideTitle": s("No games yet"), "index": n(0.0), "empty": s("No games found. Install one in Steam or Heroic and it shows up here."), "sections": arr([]), "hints": arr([])};
    };
    obj! {
        "side": side,
        "sideTitle": s(match all.len() { 1 => "Installed \u{b7} 1 game".to_string(), c => format!("Installed \u{b7} {c} games") }),
        "index": n(picked as f64),
        "header": obj! {
            "title": s(&l.game.name),
            "chips": arr([obj! {"text": s(l.verdict.label.words()), "tone": s(tone(&l.verdict))}]),
            "line": s(format!("{} \u{b7} {} \u{b7} {}", l.game.store.label(), runs_with(&l.game), l.verdict.note)),
        },
        "sections": arr(panel(ui, l)),
        "hints": arr([hint("x", "Play"), hint("y", "Library check")]),
        "x": obj! {"act": s("game"), "arg": s(format!("play|{}", game_id(&l.game)))},
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

/// The report for one game: what Turbine Hub knows about it and the machine, private details removed.
pub fn report_text(ui: &Ui, id: &str, when: &str) -> String {
    let m = ui.machine();
    let st = ui.settings();
    let sw = Switches::load(&ui.folder());
    let games = find(ui, id).map(|l| vec![format!("{} ({id}): {}, {}, switches on: {}", l.game.name, runs_with(&l.game), l.verdict.label.words(), sw.games.get(id).map(|x| x.iter().cloned().collect::<Vec<_>>().join(", ")).filter(|x| !x.is_empty()).unwrap_or_else(|| "none".into()))]).unwrap_or_default();
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

fn ludusavi(verb: &str, name: &str) -> Vec<String> {
    ["flatpak-spawn", "--host", "flatpak", "run", super::store::LUDUSAVI, verb, "--force", name].iter().map(|x| x.to_string()).collect()
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        "fsr4all" => {
            let mut sw = Switches::load(&ui.folder());
            sw.fsr4_all = !sw.fsr4_all;
            sw.save(&ui.folder());
            Some(refresh())
        }
        "switch" => {
            let (id, k) = arg.split_once('|')?;
            let locked = find(ui, id).is_none_or(|l| l.verdict.online_anticheat);
            let mut sw = Switches::load(&ui.folder());
            Some(match sw.toggle(id, k, locked) {
                Ok(_) => {
                    sw.save(&ui.folder());
                    with(refresh(), "toast", s("Applies the next time the game starts."))
                }
                Err(e) => toast(e),
            })
        }
        "game" => {
            let (what, id) = arg.split_once('|')?;
            let l = find(ui, id)?;
            let name = l.game.name.clone();
            let locked = l.verdict.online_anticheat;
            Some(match what {
                "play" if l.game.store == Store::Steam => {
                    let mut p = Plan::new(&format!("Play {name}?"), "Steam starts the game.", "Play");
                    p.steps.push(Step::Host(["flatpak-spawn", "--host", "xdg-open", &format!("steam://rungameid/{}", l.game.store_id)].iter().map(|x| x.to_string()).collect()));
                    p.done = format!("Starting {name}");
                    dialog(ui, p)
                }
                "play" => guide("Start it from Heroic", "Games from Epic, GOG and Amazon start from Heroic, or from their tile in your Steam library.", &[]),
                "proton" => guide(
                    "Try another Proton",
                    "Steam keeps this choice per game.",
                    &[("1", "In Steam, open the game's page and press the gear."), ("2", "Properties, then Compatibility."), ("3", "Tick \"Force the use of a specific Steam Play compatibility tool\" and pick another Proton."), ("Tip", "Proton Experimental and the newest GE-Proton fix the most games. ProtonPlus in the Store tab installs more.")],
                ),
                "reset" if locked => toast("Its anti-cheat runs online, so Turbine Hub leaves this game alone."),
                "reset" => {
                    let pfx = l.game.prefix.clone()?;
                    let mut p = Plan::new("Reset Windows files?", &format!("{name} gets fresh Windows files the next time it starts. The old ones are kept beside them as a backup. Saves kept only inside them are in the backup too."), "Reset");
                    p.items.push(("Close the game first".into(), "It can't be running while this happens.".into()));
                    p.steps.push(Step::ResetPrefix { pfx });
                    p.done = "Windows files reset. The backup is beside them.".into();
                    dialog(ui, p)
                }
                "report" => {
                    let mut p = Plan::new("Write a report?", &format!("Turbine Hub writes what it knows about {name} and this machine into a text file. Names, addresses and keys are removed. Nothing is sent anywhere."), "Write it");
                    p.steps.push(Step::Report { game: id.to_string() });
                    p.done = "Report written".into();
                    dialog(ui, p)
                }
                "backup" | "restore" => {
                    if !ui.installed.iter().any(|a| a == super::store::LUDUSAVI) {
                        return Some(with(toast("Saves need Ludusavi. Install it from Store, Essentials."), "open", s("store")));
                    }
                    let back = what == "backup";
                    let mut p = Plan::new(if back { "Back up saves now?" } else { "Restore a save?" }, &if back { format!("Ludusavi copies the saves of {name} into its backup folder.") } else { format!("Ludusavi puts the newest backup of {name} back. Saves made since then are replaced.") }, if back { "Back up" } else { "Restore" });
                    p.danger = !back;
                    p.steps.push(Step::Host(ludusavi(what, &name)));
                    p.done = if back { "Saves backed up".into() } else { "Save restored".into() };
                    dialog(ui, p)
                }
                "mods" if locked => toast("Its anti-cheat runs online, so mods stay off."),
                "mods" => {
                    if !ui.installed.iter().any(|a| a == super::store::TURBINE) {
                        return Some(toast("Turbine Mod Manager isn't installed."));
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
                if f.kind == "switch" && !l.verdict.online_anticheat && !sw.on(&id, &f.value) && sw.toggle(&id, &f.value, false).is_ok() {
                    done += 1;
                } else if f.kind != "switch" {
                    notes.push((l.game.name.clone(), if f.kind == "proton" { format!("Use {} for this game: its page in Steam, the gear, Properties, Compatibility.", f.value) } else { f.words.clone() }));
                }
            }
            sw.save(&ui.folder());
            Some(if notes.is_empty() {
                with(refresh(), "toast", s(match done { 0 => "Nothing to fix.".to_string(), 1 => "Fix on. It applies the next time the game starts.".into(), c => format!("{c} fixes on. They apply the next time each game starts.") }))
            } else {
                let items: Vec<(&str, &str)> = notes.iter().map(|(a, t)| (a.as_str(), t.as_str())).collect();
                with(guide("How to fix", "These need a step in Steam.", &items), "refresh", b(true))
            })
        }
        _ => None,
    }
}

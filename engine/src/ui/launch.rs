//! The launch tool's pages: "Turbine Hub in Steam" (set up, update, remove, which games use it) and one game's
//! Proton (the installed ones, the player's pick, Undo). The tool itself is `compat_tool`; this decides what the
//! pages say about it and what each press asks for.
use super::games::{self, Listed};
use super::plan::{Plan, Step, dialog, guide, toast};
use super::{Ui, arr, obj, row, s, section, with};
use crate::compat_tool::{self as tool, Proton, State};
use crate::games::{Game, Store};
use crate::json::Json;
use std::path::{Path, PathBuf};

/// Which compatibility tool Steam has for a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uses {
    /// Turbine Hub's: the game's switches and Proton pick apply.
    Hub,
    /// Turbine Mod Manager's: its switches come with that app.
    Mods,
    /// Some other tool (a Proton forced in Steam).
    Other,
    /// Steam's own choice.
    None,
}

/// The launch tool as it stands on this machine, read once per page.
pub struct ToolView {
    pub steam_root: Option<PathBuf>,
    pub state: State,
    /// Steam has started since the tool was set up, so the tool is in Steam's list.
    pub seen: bool,
    /// Steam's settings, as text (only ever read).
    config: String,
}

impl ToolView {
    pub fn uses(&self, g: &Game) -> Uses {
        if g.store != Store::Steam {
            return Uses::None;
        }
        match tool::chosen_in(&self.config, &g.store_id).as_deref() {
            Some(tool::TOOL_NAME) => Uses::Hub,
            Some(tool::MOD_TOOL_NAME) => Uses::Mods,
            Some(_) => Uses::Other,
            None => Uses::None,
        }
    }
    fn protons(&self) -> Vec<Proton> {
        self.steam_root.as_deref().map(tool::installed_protons).unwrap_or_default()
    }
}

pub fn tool_view(ui: &Ui) -> ToolView {
    let steam_root = crate::steam::steam_roots(&ui.ctx.home).into_iter().next();
    let state = steam_root.as_deref().map(|r| tool::state(r, &ui.folder())).unwrap_or(State::Missing);
    let seen = state != State::Missing && steam_root.as_deref().is_some_and(|r| tool::seen_by_steam(r, ui.steam_started));
    let config = steam_root.as_deref().map(tool::config_text).unwrap_or_default();
    ToolView { steam_root, state, seen, config }
}

/// Keeps what the tool's script reads besides the games file: the games it must leave alone, and Steam's own choice
/// of Proton for all games. Only while the tool is set up; both files are written only when they change.
pub(crate) fn keep_files(ui: &Ui, tv: &ToolView, all: &[Listed]) {
    let Some(root) = tv.steam_root.as_deref().filter(|_| tv.state != State::Missing) else { return };
    let locked: Vec<String> = all.iter().filter(|l| l.game.store == Store::Steam && l.locked()).map(|l| l.game.store_id.clone()).collect();
    let _ = tool::write_locked(&ui.folder(), &locked);
    let _ = tool::write_default_proton(root, &ui.folder());
}

/// The first row of a game's switches: where the launch tool stands for this game, and the one press that helps.
pub(crate) fn game_row(tv: &ToolView, g: &Game, uses: Uses) -> Json {
    let id = games::game_id(g);
    match (tv.state, uses) {
        (State::Missing, _) => with(with(row("open", "tool", "button", "Set up Turbine Hub in Steam", "The switches reach a game through Turbine Hub's launch tool. Set it up once."), "icon", s("bolt")), "button", s("Set up")),
        (State::Outdated, _) => with(with(with(with(row("open", "tool", "button", "Update the launch tool", "This Turbine Hub brings a newer one."), "icon", s("bolt")), "button", s("Update")), "badge", s("Update ready")), "tone", s("warn")),
        (State::Current, Uses::Hub) => with(with(with(row("none", "", "info", "On for this game", "It starts through Turbine Hub in Steam. The switches apply at every start."), "icon", s("bolt")), "badge", s("On")), "tone", s("ok")),
        (State::Current, _) => {
            let line = if tv.seen { "In Steam: the game's Properties, Compatibility, pick \"Turbine Hub\"." } else { "Restart Steam once, then pick \"Turbine Hub\" in the game's Properties, Compatibility." };
            with(with(row("tool-how", &id, "button", "Turn the switches on for this game", line), "icon", s("bolt")), "button", s("How"))
        }
    }
}

fn name_of(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The Proton row's line on a game's panel: the pick, or what Automatic comes to.
pub(crate) fn proton_line(ui: &Ui, tv: &ToolView, g: &Game) -> String {
    if let Some(d) = tool::proton_pick(&ui.folder(), &g.store_id).filter(|d| tool::is_proton(d)) {
        return format!("Your pick: {}", name_of(&d));
    }
    let list = tv.protons();
    match tool::automatic(&list, &tv.config) {
        Some(p) => format!("Automatic: {}", p.name),
        None => "Pick one for this game, with Undo".into(),
    }
}

fn not_found() -> Json {
    obj! {"crumb": s("Games"), "title": s("Not found"), "intro": s(""), "sections": arr([with(section("", vec![]), "empty", s("That game isn't installed any more."))]), "hints": arr([])}
}

/// Turbine Hub in Steam: the launch tool's state, how to turn it on for a game, the games that use it, and Remove.
pub(crate) fn tool_page(ui: &mut Ui) -> Json {
    let tv = tool_view(ui);
    let all = games::listed(ui);
    keep_files(ui, &tv, &all);
    let intro = "The launch tool carries each game's switches and Proton pick into the game. It lives in a folder Steam only reads. Steam's own files stay as they are.";
    if tv.steam_root.is_none() {
        return obj! {"crumb": s("Games"), "title": s("Turbine Hub in Steam"), "intro": s(intro), "sections": arr([with(section("", vec![]), "empty", s("Steam wasn't found on this machine. Start Steam once and sign in."))]), "hints": arr([])};
    }
    let state_row = match tv.state {
        State::Missing => with(with(row("tool", "install", "button", "Not set up yet", "One press. Nothing in Steam changes until you pick it for a game."), "icon", s("bolt")), "button", s("Set up")),
        State::Outdated => with(with(with(with(row("tool", "update", "button", "An update is ready", "This Turbine Hub brings a newer launch tool. Games keep starting meanwhile."), "icon", s("bolt")), "button", s("Update")), "badge", s("Update ready")), "tone", s("warn")),
        State::Current => with(with(with(row("none", "", "info", "Set up", if tv.seen { "Steam lists it as \"Turbine Hub\" in every game's Properties, Compatibility." } else { "Steam lists it as \"Turbine Hub\" after its next start." }), "icon", s("bolt")), "badge", s("Ready")), "tone", s("ok")),
    };
    let mut first = vec![state_row];
    if tv.state != State::Missing && !tv.seen {
        first.push(with(with(with(row("none", "", "info", "Restart Steam once", "Steam finds new tools when it starts: Steam button, Power, Restart Steam."), "icon", s("refresh")), "badge", s("Needed once")), "tone", s("warn")));
    }
    let how: Vec<Json> = tool::steps("the game", tv.seen || tv.state == State::Missing).into_iter().map(|(n, t)| row("none", "", "info", &format!("Step {n}"), &t)).collect();
    let steam_games: Vec<&Listed> = all.iter().filter(|l| l.game.store == Store::Steam).collect();
    // A game that names the tool while the tool is gone can't start: say so before anything else about it.
    let mut using: Vec<Json> = steam_games
        .iter()
        .filter(|l| tv.uses(&l.game) == Uses::Hub)
        .map(|l| {
            let r = row("open", &format!("games|{}", games::game_id(&l.game)), "button", &l.game.name, if tv.state == State::Missing { "Steam names the tool for this game, so it can't start until the tool is set up" } else { "Its switches and Proton pick apply at every start" });
            if tv.state == State::Missing { with(with(r, "badge", s("Needs the tool")), "tone", s("warn")) } else { with(with(r, "badge", s("On")), "tone", s("ok")) }
        })
        .collect();
    using.extend(steam_games.iter().filter(|l| tv.uses(&l.game) == Uses::Mods).map(|l| with(with(row("none", "", "info", &l.game.name, "Uses Turbine Mod Manager's tool. Its switches come with Turbine Mod Manager."), "badge", s("Mods")), "tone", s("muted"))));
    let none_yet = if using.is_empty() && tv.state != State::Missing { "None yet." } else { "" };
    let mut sections = vec![section("", first), section("Turn it on for a game", how), with(section("Games using it", using), "empty", s(none_yet))];
    if tv.state != State::Missing {
        sections.push(section("Remove", vec![with(with(row("tool", "remove", "button", "Remove the launch tool", "Games that use it need another tool picked in Steam first"), "icon", s("remove")), "button", s("Remove"))]));
    }
    obj! {"crumb": s("Games"), "title": s("Turbine Hub in Steam"), "intro": s(intro), "sections": arr(sections), "hints": arr([])}
}

/// One game's Proton: what it starts with now, every Proton installed, and Undo. `arg` is the game.
pub(crate) fn proton_page(ui: &mut Ui, arg: &str) -> Json {
    let Some(l) = games::find(ui, arg) else { return not_found() };
    let title = format!("Proton for {}", l.game.name);
    let plain = |intro: &str, rows: Vec<Json>| obj! {"crumb": s("Games"), "title": s(&title), "intro": s(intro), "sections": arr([section("", rows)]), "hints": arr([])};
    if l.locked() {
        return plain("", vec![with(row("none", "", "info", "Plays as it is", "Its anti-cheat runs online, so Turbine Hub never changes how this game starts, Proton included."), "icon", s("shield"))]);
    }
    if l.game.store != Store::Steam {
        return plain("Heroic keeps this choice.", vec![row("none", "", "info", "In Heroic", "Open the game's settings there and pick another Wine or Proton version under Wine."), row("none", "", "info", "Tip", "Heroic's own Wine Manager installs more versions.")]);
    }
    if l.game.runtime == crate::Runtime::Native {
        return plain("", vec![row("none", "", "info", "Linux version", "It runs without Proton.")]);
    }
    let tv = tool_view(ui);
    let list = tv.protons();
    let app = &l.game.store_id;
    let pick = tool::proton_pick(&ui.folder(), app).filter(|d| tool::is_proton(d));
    let auto = tool::automatic(&list, &tv.config);
    let auto_words = match (tool::steam_default(&list, &tv.config), auto) {
        (Some(p), _) => format!("Steam's choice for all games: {}", p.name),
        (None, Some(p)) => format!("The newest one installed: {}", p.name),
        (None, None) => "No Proton is installed yet. Start any Windows game in Steam once.".to_string(),
    };
    let uses = tv.uses(&l.game);
    let mut first = Vec::new();
    let now_name = pick.as_deref().map(name_of).or_else(|| auto.map(|p| p.name.clone())).unwrap_or_else(|| "None yet".into());
    let last = tool::last_launch_words(&ui.folder(), app).unwrap_or_default();
    first.push(with(row("none", "", "info", &format!("Starts with {now_name}"), format!("{} {last}", if pick.is_some() { "Your pick." } else { "Automatic." }).trim()), "icon", s("swap")));
    if uses != Uses::Hub || tv.state != State::Current {
        if uses == Uses::Mods {
            first.push(with(row("none", "", "info", "Uses Turbine Mod Manager in Steam", "Its Proton is set in Turbine Mod Manager. A pick here waits until the game uses Turbine Hub."), "icon", s("external")));
        } else {
            first.push(game_row(&tv, &l.game, uses));
        }
    }
    if let Some(before) = tool::proton_before(&ui.folder(), app) {
        let back = before.as_deref().map(name_of).unwrap_or_else(|| "Automatic".into());
        first.push(with(with(row("proton-undo", arg, "button", "Undo the last change", &format!("Back to {back}")), "icon", s("restore")), "button", s("Undo")));
    }
    // The one in use is only named; every other one can be picked.
    let choice = |on: bool, name: &str, title: &str, line: &str| if on { with(with(row("none", "", "info", title, line), "badge", s("In use")), "tone", s("ok")) } else { with(row("proton-pick", &format!("{arg}|{name}"), "button", title, line), "button", s("Use")) };
    let mut rows = vec![choice(pick.is_none(), "", "Automatic", &auto_words)];
    for p in &list {
        rows.push(if p.usable {
            choice(pick.as_deref() == Some(p.dir.as_path()), &p.name, &p.name, p.kind.words())
        } else {
            with(with(row("none", "", "info", &p.name, "The Steam Linux Runtime it needs isn't installed. Start a game with it in Steam once."), "badge", s("Can't start")), "tone", s("muted"))
        });
    }
    let more = with(with(row("open", "store-item|com.vysp3r.ProtonPlus", "button", "More versions", "ProtonPlus installs GE-Proton and others. They show up here."), "icon", s("download")), "button", s("Open"));
    obj! {
        "crumb": s("Games"),
        "title": s(title),
        "intro": s("Another Proton often fixes a game that won't start. The pick is Turbine Hub's own: Steam's settings stay as they are."),
        "sections": arr([section("", first), section("Installed", rows), section("", vec![more])]),
        "hints": arr([]),
    }
}

/// The launch tool's own steps of a confirmed dialog. None for any other step.
pub fn run_step(step: &Step, home: &Path, folder: &Path) -> Option<Result<(), String>> {
    let root = || crate::steam::steam_roots(home).into_iter().next().ok_or_else(|| "Steam wasn't found on this machine.".to_string());
    Some(match step {
        Step::Tool { install: true } => root().and_then(|r| {
            tool::install(&r, folder).map_err(|e| format!("Couldn't write the launch tool: {e}"))?;
            let _ = tool::write_default_proton(&r, folder);
            Ok(())
        }),
        Step::Tool { install: false } => root().and_then(|r| tool::remove(&r).map(|_| ()).map_err(|ids| format!("{} still use it in Steam. Pick another tool for them first.", if ids.len() == 1 { "1 game".to_string() } else { format!("{} games", ids.len()) }))),
        Step::ProtonPick { app_id, dir } => tool::set_proton_pick(folder, app_id, dir.as_deref()).map_err(|e| format!("Couldn't keep that pick: {e}")),
        Step::ProtonUndo { app_id } => tool::undo_proton_pick(folder, app_id).map(|_| ()).map_err(|e| format!("Couldn't undo: {e}")),
        _ => return None,
    })
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        "tool" => {
            let tv = tool_view(ui);
            let Some(root) = tv.steam_root.clone() else { return Some(toast("Steam wasn't found on this machine.")) };
            let dir = tool::tool_dir(&root);
            match arg {
                "install" | "update" => {
                    if tv.state == State::Current {
                        return Some(toast("The launch tool is set up already."));
                    }
                    let update = tv.state == State::Outdated;
                    let mut p = Plan::new(if update { "Update the launch tool?" } else { "Set up Turbine Hub in Steam?" }, "Turbine Hub writes its launch tool into Steam's folder for tools. Steam only reads that folder, and none of Steam's own files change.", if update { "Update" } else { "Set up" });
                    p.items.push(("Three small files".into(), if ui.veteran() { dir.to_string_lossy().into_owned() } else { "In Steam's folder for tools".into() }));
                    if !update {
                        p.items.push(("Restart Steam once afterwards".into(), "Steam finds new tools when it starts: Steam button, Power, Restart Steam.".into()));
                        p.items.push(("Then pick it per game".into(), "In a game's Properties, Compatibility, pick \"Turbine Hub\". Games you don't pick it for start as before.".into()));
                    }
                    p.steps.push(Step::Tool { install: true });
                    p.done = if update { "Launch tool updated".into() } else { "Launch tool set up. Restart Steam once so it shows in Steam's list.".into() };
                    Some(dialog(ui, p))
                }
                "remove" => {
                    if tv.state == State::Missing {
                        return Some(toast("The launch tool isn't set up."));
                    }
                    let using = tool::games_using_in(&tv.config, tool::TOOL_NAME);
                    if !using.is_empty() {
                        let all = games::listed(ui);
                        let names: Vec<(String, String)> = using.iter().map(|id| (all.iter().find(|l| l.game.store == Store::Steam && l.game.store_id == *id).map(|l| l.game.name.clone()).unwrap_or_else(|| format!("Steam app {id}")), "Properties, Compatibility: pick another tool, or untick the box.".to_string())).collect();
                        let items: Vec<(&str, &str)> = names.iter().map(|(a, t)| (a.as_str(), t.as_str())).collect();
                        return Some(guide("Switch these games back first", "They use Turbine Hub in Steam. Without the tool Steam couldn't start them, so it stays until they use another one. Steam keeps the change when it closes.", &items));
                    }
                    let mut p = Plan::new("Remove the launch tool?", "Turbine Hub takes its three files out of Steam's folder for tools. No game uses it. The switches and Proton picks stay saved, in case it comes back.", "Remove");
                    p.danger = true;
                    p.items.push(("Removed".into(), if ui.veteran() { dir.to_string_lossy().into_owned() } else { "Its three files in Steam's folder for tools".into() }));
                    p.steps.push(Step::Tool { install: false });
                    p.done = "Launch tool removed".into();
                    Some(dialog(ui, p))
                }
                _ => None,
            }
        }
        "tool-how" => {
            let l = games::find(ui, arg)?;
            if !l.takes_switches() {
                return Some(toast("The switches are for Windows games that Steam starts."));
            }
            let tv = tool_view(ui);
            if tv.state == State::Missing {
                return Some(super::plan::open("tool"));
            }
            let steps = tool::steps(&l.game.name, tv.seen);
            let mut items: Vec<(&str, &str)> = steps.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
            items.push(("After that", "Start the game from Steam as always. Its switches here apply at every start."));
            Some(guide(&format!("Turn the switches on for {}", l.game.name), "Steam keeps one tool per game. Picking Turbine Hub's is three presses in Steam, once.", &items))
        }
        "proton-pick" => {
            let (id, name) = arg.split_once('|')?;
            let l = games::find(ui, id)?;
            if l.locked() {
                return Some(toast("Its anti-cheat runs online, so Turbine Hub leaves this game alone."));
            }
            if !l.takes_switches() {
                return Some(toast("This is for Windows games that Steam starts."));
            }
            let tv = tool_view(ui);
            let pick = tool::proton_pick(&ui.folder(), &l.game.store_id);
            let dir = if name.is_empty() { None } else { Some(tv.protons().into_iter().find(|p| p.name == name && p.usable)?.dir) };
            if dir == pick {
                return Some(toast("That one is in use already."));
            }
            let uses = tv.uses(&l.game) == Uses::Hub && tv.state != State::Missing;
            let what = if name.is_empty() { "Automatic".to_string() } else { name.to_string() };
            let mut p = Plan::new(&format!("Use {what} for {}?", l.game.name), if name.is_empty() { "The game starts with Steam's choice for all games again, or the newest Proton installed." } else { "The game starts with this Proton from now on. Undo puts the last one back." }, "Use it");
            p.items.push((l.game.name.clone(), what.clone()));
            if !uses {
                p.items.push(("Applies once the game uses Turbine Hub in Steam".into(), "The pick is kept until then. The page shows how to turn it on.".into()));
            }
            p.steps.push(Step::ProtonPick { app_id: l.game.store_id.clone(), dir });
            p.done = format!("{} starts with {what}", l.game.name);
            Some(dialog(ui, p))
        }
        "proton-undo" => {
            let l = games::find(ui, arg)?;
            if l.locked() || l.game.store != Store::Steam {
                return Some(toast("There is nothing to undo."));
            }
            let Some(before) = tool::proton_before(&ui.folder(), &l.game.store_id) else { return Some(toast("There is nothing to undo.")) };
            let back = before.as_deref().map(name_of).unwrap_or_else(|| "Automatic".into());
            let mut p = Plan::new("Undo the Proton change?", &format!("{} goes back to {back}.", l.game.name), "Undo");
            p.items.push((l.game.name.clone(), back.clone()));
            p.steps.push(Step::ProtonUndo { app_id: l.game.store_id.clone() });
            p.done = format!("{} is back on {back}", l.game.name);
            Some(dialog(ui, p))
        }
        _ => None,
    }
}

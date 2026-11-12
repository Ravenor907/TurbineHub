//! Settings: the sections and their rows, Review changes, the player's setup file, and the menu.
use super::plan::{Plan, Step, dialog, helper_plan, refresh, state, toast, with_helper};
use super::{Ui, ago, arr, b, checked_at, checked_words, n, obj, row, s, section, with};
use crate::helper::{self, Change, Setting};
use crate::json::{self, Json};
use crate::settings::{ACCENTS, Mode, THEMES};
use std::path::PathBuf;

const SECTIONS: &[(&str, &str)] = &[("general", "General"), ("look", "Look"), ("updates", "Updates"), ("setup", "My setup"), ("help", "Help and reports"), ("about", "About")];
const THEME_WORDS: &[&str] = &["Dark", "Black", "Light", "High contrast"];
const ACCENT_WORDS: &[&str] = &["Brass", "Iron", "Copper", "Jade"];
const ACCENT_HEX: &[&str] = &["#C99E52", "#A9B4BE", "#B07A5A", "#6FAE8C"];
const FLATPAK_ID: &str = "io.github.ravenor907.TurbineHub";

fn choice(act: &str, title: &str, line: &str, options: &[&str], index: usize) -> Json {
    with(with(row(act, "", "choice", title, line), "options", arr(options.iter().map(|o| s(*o)))), "index", n(index as f64))
}

fn open_row(page: &str, title: &str, line: &str, button: &str) -> Json {
    with(row("open", page, "button", title, line), "button", s(button))
}

pub fn setup_file(ui: &Ui) -> PathBuf {
    ui.folder().join("my-setup.json")
}

/// The player's setup as one file: the look and mode, the system changes Turbine Hub made, and the Store apps
/// installed. Another machine applies it in one go.
pub fn setup_json(ui: &Ui) -> String {
    let st = ui.settings();
    let apps = super::store::APPS.iter().filter(|a| super::store::is_installed(ui, a.id)).map(|a| s(a.id));
    let changes = ui.ledger().settings().iter().map(|x| json::parse(&x.to_json()).unwrap_or(Json::Null)).collect::<Vec<_>>();
    obj! {
        "turbinehub": s(&ui.ctx.version),
        "mode": s(if st.mode == Some(Mode::Veteran) { "veteran" } else { "guided" }),
        "theme": s(&st.theme),
        "accent": s(&st.accent),
        "uiScale": n(st.ui_scale as f64),
        "gameBoost": b(st.game_boost),
        "changes": arr(changes),
        "apps": arr(apps),
    }
    .to_text()
        + "\n"
}

/// What a setup file asks for on this machine: the settings the helper accepts and the Store apps not here yet.
/// Anything the helper's lists don't allow is dropped, so a hand-edited file can't ask for more than the app can.
pub fn setup_wants(ui: &Ui, text: &str) -> Result<(Vec<Setting>, Vec<&'static super::store::App>), String> {
    let j = json::parse(text).map_err(|_| "The setup file isn't readable.".to_string())?;
    if j.str_at("turbinehub").is_none() {
        return Err("That isn't a Turbine Hub setup file.".into());
    }
    let ledger = ui.ledger();
    let settings: Vec<Setting> = j.get("changes").map(Json::as_array).unwrap_or(&[]).iter().filter_map(|c| Setting::from_json(&turbinehub_helper::json::parse(&c.to_text()).ok()?).ok()).filter(|x| x.check().is_ok()).filter(|x| ledger.find_key(&x.key()).is_none_or(|e| e.change != Change::Set(x.clone()))).collect();
    let apps = j.get("apps").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).filter_map(|id| super::store::APPS.iter().find(|a| a.id == id)).filter(|a| !super::store::is_installed(ui, a.id)).collect();
    Ok((settings, apps))
}

/// The Settings tab. `arg` is the section.
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    let st = ui.settings();
    let index = SECTIONS.iter().position(|(id, _)| *id == arg).unwrap_or(0);
    let (id, title) = SECTIONS[index];
    let changes = ui.ledger().entries.len();
    let rows: Vec<Json> = match id {
        "general" => {
            let mut scale = with(row("scale", "", "slider", "Interface size", "50 to 150%. Layouts re-flow, nothing gets cut off."), "value", n(st.ui_scale as f64));
            for (k, v) in [("min", 50.0), ("max", 150.0), ("step", 10.0)] {
                scale = with(scale, k, n(v));
            }
            scale = with(with(scale, "unit", s("%")), "live", b(true));
            vec![
                choice("mode", "Mode", "Guided explains things. Veteran shows every control.", &["Guided", "Veteran"], if st.mode == Some(Mode::Veteran) { 1 } else { 0 }),
                scale,
                with(row("animations", "", "toggle", "Animations", "Fades and slides between screens"), "on", b(st.animations)),
                agent_row(ui),
                with(with(with(row("snapshots", "", "toggle", "Save time machine", &format!("A snapshot of a game's saves each time it starts. The newest {} are kept.", crate::snapshots::KEEP)), "on", b(st.save_snapshots)), "badge", s(if st.save_snapshots && !ui.agent_running() { "Needs the background helper" } else { "" })), "tone", s("warn")),
                open_row("bring", "Bring it over", "Run the Windows steps again", "Open"),
                open_row("changes", "Review changes", &match changes { 0 => "Nothing changed yet".to_string(), 1 => "1 system change, with Undo".into(), c => format!("{c} system changes, each with Undo") }, "Review"),
            ]
        }
        "look" => vec![
            choice("theme", "Theme", "", THEME_WORDS, THEMES.iter().position(|t| *t == st.theme).unwrap_or(0)),
            with(choice("accent", "Accent", "", ACCENT_WORDS, ACCENTS.iter().position(|a| *a == st.accent).unwrap_or(0)), "swatches", arr(ACCENT_HEX.iter().map(|h| s(*h)))),
        ],
        "updates" => {
            let fresh = checked_words(crate::now(), checked_at(&ui.cache), ui.offline);
            let mut rows = vec![match &ui.update {
                Some(v) => with(with(with(row("update-app", "", "button", &format!("Turbine Hub {v} is ready"), &fresh), "button", s("Update")), "badge", s("Update ready")), "tone", s("warn")),
                None => with(with(row("update-check", "", "button", &format!("Turbine Hub {}", ui.ctx.version), &fresh), "button", s("Check now")), "icon", s("refresh")),
            }];
            if ui.veteran() {
                rows.push(with(row("testbuilds", "", "toggle", "Test builds", "Get builds before they are released. They can have rough edges."), "on", b(st.test_builds)));
            }
            rows.push(row("none", "", "info", "Store apps", "Apps from the Store tab keep themselves current through Flathub."));
            if ui.agent_running() {
                rows.push(row("none", "", "info", "Checked every day", "The background helper looks once a day and says so here and on Home. Nothing installs by itself."));
            }
            rows
        }
        "setup" => {
            let has = setup_file(ui).is_file();
            vec![
                with(with(row("setup-save", "", "button", "Save my setup", "The look, the system changes and the Store apps, in one file"), "button", s("Save")), "icon", s("save")),
                with(with(with(row("setup-load", "", if has { "button" } else { "info" }, "Apply my setup file", if has { "Makes this machine match the saved file" } else { "No setup file yet. Save one here, or copy one into Turbine Hub's folder." }), "button", s(if has { "Apply" } else { "" })), "icon", s("restore")), "value", s("")),
                row("none", "", "info", "Where it is", &setup_file(ui).to_string_lossy()),
            ]
        }
        "help" => vec![
            open_row("wheres", "Where's my...?", "Windows things and where they are on SteamOS", "Open"),
            with(row("reports", "", "toggle", "Send problem reports", "Off unless you turn it on. Reports never hold names, addresses or keys."), "on", b(st.reports)),
            open_row("pick", "Pick a mode again", "Shows the first screen", "Open"),
        ],
        _ => vec![
            with(row("none", "", "info", "Turbine Hub", "The first stop after installing SteamOS"), "value", s(format!("Version {}", ui.ctx.version))),
            row("none", "", "info", "License", "GPL 3.0 or later. Fonts: Cinzel and Barlow, SIL Open Font License."),
            row("none", "", "info", "Source", "github.com/Ravenor907/TurbineHub"),
            with(with(row("remove-app", "", "button", "Remove Turbine Hub completely", "Undoes every system change, removes the helper, then the app"), "button", s("Remove")), "icon", s("remove")),
        ],
    };
    obj! {
        "side": arr(SECTIONS.iter().map(|(id, title)| obj! {"id": s(*id), "title": s(*title), "chip": s(""), "tone": s(""), "lock": b(false)})),
        "sideTitle": s(""),
        "index": n(index as f64),
        "header": obj! {"title": s(title), "chips": arr([]), "line": s("")},
        "sections": arr([section("", rows)]),
        "hints": arr([]),
    }
}

/// The background helper's switch, with whether it is running now.
fn agent_row(ui: &Ui) -> Json {
    let on = ui.agent_running();
    let (badge, tone) = match (on, ui.agent().alive(crate::now())) {
        (true, true) => ("Running", "ok"),
        (true, false) => ("Not running", "warn"),
        _ => ("", ""),
    };
    with(with(with(row("agent", "", "toggle", "Background helper", "Game Boost, moving lights, battery and sleep history and save snapshots while you play"), "on", b(on)), "badge", s(badge)), "tone", s(tone))
}

/// A recorded change in the words a player uses: the settings Turbine Hub's own controls make are named after the
/// control, everything else keeps the helper's words.
pub fn change_words(c: &Change) -> String {
    if let Change::Set(Setting::Sysfs { path, value }) = c {
        let file = path.rsplit('/').next().unwrap_or("");
        if file == "charge_control_end_threshold" {
            return format!("Battery stops charging at {value}%");
        }
        if path.contains("/ppt_pl") {
            return format!("Power limit set to {value} W");
        }
        if file == "power1_cap" {
            return format!("Graphics power limit set to {} W", value.parse::<u64>().map(|v| (v + 500_000) / 1_000_000).unwrap_or(0));
        }
    }
    c.words()
}

/// Review changes: every system change, newest first, each with Undo.
pub(crate) fn changes(ui: &mut Ui) -> Json {
    let ledger = ui.ledger();
    let now = crate::now();
    let rows: Vec<Json> = ledger.entries.iter().rev().map(|e| with(with(row("undo", &e.id.to_string(), "button", &change_words(&e.change), &format!("{} ago, on SteamOS {}", ago(now, e.applied), e.steamos)), "button", s("Undo")), "icon", s("restore"))).collect();
    let any = !rows.is_empty();
    obj! {
        "crumb": s("Settings"),
        "title": s("Review changes"),
        "intro": s("Every system change Turbine Hub made, newest first. Undo puts back what was there before."),
        "sections": arr([with(section("", rows), "empty", s(if any { "" } else { "Turbine Hub hasn't changed anything on this machine." }))]),
        "hints": arr(if any { vec![super::hint("y", "Undo all")] } else { vec![] }),
        "y": if any { obj! {"act": s("undo-all"), "arg": s("")} } else { Json::Null },
    }
}

fn host(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host"].iter().chain(args).map(|x| x.to_string()).collect()
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    let idx = value.parse::<f64>().ok().map(|v| v.round().max(0.0) as usize);
    let mut st = ui.settings();
    let out = match action {
        "mode" => {
            st.mode = Some(if idx? == 1 { Mode::Veteran } else { Mode::Guided });
            state()
        }
        "scale" => {
            st.ui_scale = (idx? as u32).clamp(50, 150);
            state()
        }
        "animations" => {
            st.animations = !st.animations;
            state()
        }
        "theme" => {
            st.theme = THEMES.get(idx?)?.to_string();
            state()
        }
        "accent" => {
            st.accent = ACCENTS.get(idx?)?.to_string();
            state()
        }
        "testbuilds" => {
            st.test_builds = !st.test_builds;
            refresh()
        }
        "reports" => {
            st.reports = !st.reports;
            refresh()
        }
        "agent" => {
            let home = ui.ctx.home.clone();
            let p = if ui.agent_running() {
                let mut p = Plan::new("Turn the background helper off?", "Game Boost, moving lights, battery and sleep history and save snapshots stop. What it wrote down so far stays.", "Turn off");
                p.items.push(("Stop it and remove its start-up entry".into(), crate::agent::unit_file(&home).to_string_lossy().into_owned()));
                p.steps = crate::agent::turn_off(&home);
                p.done = "Background helper off".into();
                p
            } else {
                let mut p = Plan::new("Turn the background helper on?", "Turbine Hub keeps running without a window, from the moment this machine starts. It needs no password and changes nothing by itself.", "Turn on");
                p.items.push(("Add a start-up entry in your own folder".into(), crate::agent::unit_file(&home).to_string_lossy().into_owned()));
                p.items.push(("Start it now and with every start".into(), "Turn it off here at any time.".into()));
                p.steps = crate::agent::turn_on(&home);
                p.done = "Background helper on".into();
                p
            };
            return Some(dialog(ui, p));
        }
        "snapshots" => {
            if !st.save_snapshots && !ui.installed.iter().any(|a| a == super::store::LUDUSAVI) {
                return Some(with(toast("Saves need Ludusavi. Install it from Store, Essentials."), "open", s("store")));
            }
            st.save_snapshots = !st.save_snapshots;
            if st.save_snapshots && !ui.agent_running() { with(refresh(), "toast", s("Snapshots start once the background helper is on.")) } else { refresh() }
        }
        "home-all" => {
            ui.home_all = !ui.home_all;
            return Some(refresh());
        }
        "filter" => {
            let keys = ["all", "fix", "no"];
            ui.filter.insert(arg.to_string(), keys.get(idx?)?.to_string());
            return Some(refresh());
        }
        "undo" => {
            let id: u64 = arg.parse().ok()?;
            let ledger = ui.ledger();
            let e = ledger.entries.iter().find(|e| e.id == id)?;
            let mut p = Plan::new("Undo this change?", "Turbine Hub puts back what was there before.", "Undo");
            p.items.push((change_words(&e.change), String::new()));
            p.steps.push(Step::Host(helper::undo(id)));
            p.done = "Change undone".into();
            return Some(with_helper(ui, p));
        }
        "undo-all" => {
            let ledger = ui.ledger();
            if ledger.entries.is_empty() {
                return Some(toast("Nothing to undo."));
            }
            let mut p = Plan::new("Undo everything?", "Every system change Turbine Hub made goes back to what was there before, newest first.", "Undo all");
            p.danger = true;
            p.items = ledger.entries.iter().rev().map(|e| (change_words(&e.change), String::new())).collect();
            p.steps.push(Step::Host(helper::job("undo-all", None)));
            p.done = "Everything is back the way it was".into();
            return Some(with_helper(ui, p));
        }
        "reapply" => {
            let mut p = Plan::new("Put the settings back?", "A SteamOS update reset some of Turbine Hub's settings. This puts them back from its record.", "Put back");
            p.items = ui.facts.drift.iter().map(|d| (d.clone(), String::new())).collect();
            p.steps.push(Step::Host(helper::job("reapply", None)));
            p.done = "Settings put back".into();
            return Some(with_helper(ui, p));
        }
        "setup-save" => {
            let mut p = Plan::new("Save my setup?", "Writes the look, the system changes and the Store apps into one file. It holds no passwords and nothing personal.", "Save");
            p.items.push((setup_file(ui).to_string_lossy().into_owned(), String::new()));
            p.steps.push(Step::SaveSetup);
            p.done = "Setup saved".into();
            return Some(dialog(ui, p));
        }
        "setup-load" => {
            let text = std::fs::read_to_string(setup_file(ui)).unwrap_or_default();
            return Some(match setup_wants(ui, &text) {
                Err(e) => toast(&e),
                Ok((settings, apps)) if settings.is_empty() && apps.is_empty() => toast("This machine matches the setup file already."),
                Ok((settings, apps)) => {
                    let count = settings.len() + apps.len();
                    let mut p = Plan::new("Apply my setup file?", "Turbine Hub saves the current settings first. Every change can be undone in Review changes.", &format!("Apply {count} changes"));
                    p.items = settings.iter().map(|x| (x.words(), String::new())).chain(apps.iter().map(|a| (a.name.to_string(), "From Flathub".to_string()))).collect();
                    p.steps = apps.iter().map(|a| super::store::install_step(a.id)).collect();
                    p.done = "Setup applied".into();
                    if settings.is_empty() { dialog(ui, p) } else { helper_plan(ui, p, &settings) }
                }
            });
        }
        "remove-app" => {
            let mut p = Plan::new("Remove Turbine Hub completely?", "Every system change is undone, the helper and its files go, then the app itself. Your games, saves and Store apps stay.", "Remove");
            p.danger = true;
            if ui.agent_on == Some(true) {
                p.items.push(("Turn off the background helper".into(), String::new()));
                p.steps.extend(crate::agent::turn_off(&ui.ctx.home));
            }
            if helper::installed(&ui.ctx.root) {
                p.items.push(("Undo every system change and remove the helper".into(), String::new()));
                p.steps.push(Step::Host(helper::job("uninstall", None)));
            }
            p.items.push(("Remove the app and its settings".into(), String::new()));
            p.steps.push(Step::Host(host(&["flatpak", "uninstall", "-y", "--noninteractive", "--delete-data", FLATPAK_ID])));
            p.done = "Turbine Hub is removed".into();
            return Some(dialog(ui, p));
        }
        "update-app" => {
            let v = ui.update.clone()?;
            let mut p = Plan::new(&format!("Update to Turbine Hub {v}?"), "Turbine Hub downloads the new version from its GitHub releases, checks it against the published checksum and installs it. It takes effect the next time Turbine Hub starts.", "Update");
            p.steps.push(Step::SelfUpdate);
            if ui.agent_on == Some(true) {
                // The background helper is the same program: it starts again as the new version.
                p.steps.push(Step::Host(crate::agent::restart()));
            }
            p.done = format!("Turbine Hub {v} installed. It starts next time.");
            return Some(dialog(ui, p));
        }
        "menu" => {
            let rows = [("Where's my...?", "open", "wheres"), ("Bring it over", "open", "bring"), ("Review changes", "open", "changes"), ("Settings", "tab", "settings"), ("Close Turbine Hub", "quit", "")];
            return Some(obj! {"dialog": obj! {"title": s("Menu"), "text": s(""), "items": arr([]), "confirm": s(""), "cancel": s("Close"), "ask": s(""), "danger": b(false), "rows": arr(rows.iter().map(|(t, a, g)| obj! {"title": s(*t), "act": s(*a), "arg": s(*g)}))}});
        }
        _ => return None,
    };
    ui.save(&st);
    Some(out)
}

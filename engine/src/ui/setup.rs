//! First run: the mode picker, Guided's five steps and Veteran's one-screen quick setup.
use super::home::step_title;
use super::plan::{Ask, Plan, Step, dialog, helper_plan, toast};
use super::store::{self, HEROIC, LUDUSAVI};
use super::{Ui, arr, b, hint, kind_word, n, obj, row, s, section, with};
use crate::helper::{self, Setting};
use crate::json::{self, Json};
use crate::modules;
use crate::settings::{Mode, SETUP_STEPS};
use crate::tuneup::{self, State};
use std::collections::BTreeSet;

/// The first screen: two cards.
pub(crate) fn pick() -> Json {
    let card = |id: &str, kicker: &str, title: &str, line: &str, chips: [&str; 3]| obj! {"id": s(id), "kicker": s(kicker), "title": s(title), "line": s(line), "chips": arr(chips.iter().map(|c| s(*c)))};
    obj! {
        "title": s("Welcome to Turbine Hub"),
        "line": s("How do you want to start? You can switch any time in Settings."),
        "cards": arr([
            card("guided", "Guided mode", "New to SteamOS", "Coming from Windows? I'll walk you through it.", ["Step-by-step setup", "Will my games run?", "Windows terms"]),
            card("veteran", "Veteran mode", "I know more than you.", "Skip the tour. Every control, one screen.", ["One-screen setup", "Every control", "Save my setup"]),
        ]),
    }
}

/// The quick panel isn't installed and installs only through Decky Loader's own steps.
const PANEL_STEPS: &str = "Steps in Decky";

/// One thing the quick setup can do.
struct Pick {
    id: String,
    group: &'static str,
    title: String,
    note: String,
    /// Ticked on the first visit.
    default: bool,
    /// Why it can't be ticked ("On already"), when it can't.
    locked: Option<&'static str>,
    settings: Vec<Setting>,
    app: Option<&'static str>,
}

fn picks_for(ui: &Ui, guided: bool) -> Vec<Pick> {
    let m = ui.machine();
    let st = ui.settings();
    let mut out = Vec::new();
    let mut add = |id: &str, group: &'static str, title: &str, note: &str, default: bool, locked: Option<&'static str>, settings: Vec<Setting>, app: Option<&'static str>| out.push(Pick { id: id.into(), group, title: title.into(), note: note.into(), default: default && locked.is_none(), locked, settings, app });
    let has_decky = ui.ctx.home.join("homebrew/services/PluginLoader").exists();
    add("library", "Essentials", super::selftile::TITLE, "Opens from Game Mode like a game, with its own art", true, super::selftile::pick_lock(ui), Vec::new(), None);
    add("decky", "Essentials", "Decky Loader", if guided { "Add-ons for the menu Steam's button with three dots opens while you play" } else { "Plugins for Steam's quick menu" }, ui.decky_tag.is_some(), if has_decky { Some("Installed") } else if ui.decky_tag.is_none() { Some("Needs the internet once") } else { None }, Vec::new(), None);
    // The quick panel installs through Decky Loader itself, so Guided only shows the steps (its How button).
    if guided && has_decky {
        add("panel", "Essentials", store::PANEL_TITLE, "Turbine Hub in Steam's quick menu while you play. Not installed yet: a few steps in Decky Loader's settings.", false, Some(if store::panel_installed(ui) { "Installed" } else { PANEL_STEPS }), Vec::new(), None);
    }
    let apps = if guided {
        [(LUDUSAVI, "Save backups (Ludusavi)", "Backs up your game saves, so you can go back to an earlier one", true), ("com.vysp3r.ProtonPlus", "More Proton versions (ProtonPlus)", "Proton is what runs Windows games here. Other versions fix games that won't start", true)]
    } else {
        [(LUDUSAVI, "Ludusavi save backups", "Back up and restore saves", true), ("com.vysp3r.ProtonPlus", "ProtonPlus", "Extra Proton versions", true)]
    };
    for (id, title, note, default) in apps {
        add(id, "Essentials", title, note, default, store::is_installed(ui, id).then_some("Installed"), Vec::new(), Some(id));
    }
    add("boost", "Performance", "Game Boost while playing", "Full processor speed in games", true, st.game_boost.then_some("On already"), Vec::new(), None);
    for i in tuneup::plan(&ui.ctx.root, &ui.ledger()) {
        let locked = match i.state {
            State::On => Some("On already"),
            State::SteamOsDoes => Some("SteamOS does it"),
            State::Off => None,
        };
        let (title, why) = if guided { (i.plain_title, i.plain_why) } else { (i.title, i.why) };
        add(&format!("tune:{}", i.id), "Performance", title, why, true, locked, i.settings.clone(), None);
    }
    for (id, title, note, default) in [(HEROIC, "Heroic Games Launcher", "Epic, GOG and Amazon games", true), ("com.discordapp.Discord", "Discord", "Voice chat and friends", false), ("com.moonlight_stream.Moonlight", "Moonlight", "Stream games from your PC", false), ("com.obsproject.Studio", "OBS Studio", "Record and stream", false), ("io.github.radiolamp.mangojuice", "MangoJuice", "Shows frame rate and temperatures over a game", false)] {
        add(id, "Apps", title, note, default, store::is_installed(ui, id).then_some("Installed"), Vec::new(), Some(id));
    }
    let group = match m.kind {
        crate::hardware::Kind::Handheld => "This handheld",
        crate::hardware::Kind::Laptop => "This laptop",
        crate::hardware::Kind::Desktop => "This PC",
    };
    if let Some(limit) = m.battery().and_then(|x| x.charge_limit) {
        add("charge80", group, "Charge limit 80%", "Keeps the battery healthy", true, (limit == 80).then_some("On already"), modules::charge_limit(&m, 80), None);
    }
    if !guided {
        let ledger = ui.ledger();
        for d in m.wake_devices.iter().filter(|d| d.enabled) {
            let blocked = ledger.settings().iter().any(|x| matches!(x, Setting::Wake { vendor, product, wake: false } if *vendor == d.vendor && *product == d.product));
            add(&format!("wake:{}:{}", d.vendor, d.product), group, &format!("No wake-ups from {}", if d.name.is_empty() { "a USB device" } else { &d.name }), "Stops it waking the machine", false, blocked.then_some("On already"), vec![modules::usb_wake(&d.vendor, &d.product, false)], None);
        }
        add("ssh", group, "SSH", "Sign in from another computer", false, None, vec![Setting::Service { unit: "sshd.service".into(), enabled: true }], None);
    }
    out
}

pub(crate) fn all_ids(ui: &Ui) -> BTreeSet<String> {
    picks_for(ui, !ui.veteran()).into_iter().filter(|p| p.locked.is_none()).map(|p| p.id).collect()
}

fn chosen(ui: &mut Ui, guided: bool) -> (Vec<Pick>, BTreeSet<String>) {
    let all = picks_for(ui, guided);
    let defaults: BTreeSet<String> = all.iter().filter(|p| p.default).map(|p| p.id.clone()).collect();
    let picked = ui.picked.entry("quick".into()).or_insert(defaults).clone();
    (all, picked)
}

fn pick_sections(all: &[Pick], picked: &BTreeSet<String>) -> (Vec<Json>, usize) {
    let mut groups: Vec<&'static str> = Vec::new();
    for p in all {
        if !groups.contains(&p.group) {
            groups.push(p.group);
        }
    }
    let count = all.iter().filter(|p| p.locked.is_none() && picked.contains(&p.id)).count();
    let sections = groups
        .iter()
        .map(|g| {
            section(
                g,
                all.iter()
                    .filter(|p| p.group == *g)
                    .map(|p| match p.locked {
                        // Not done and not something a tick can do: the steps, never a badge that looks done.
                        Some(PANEL_STEPS) => with(with(row("panel-guide", "", "button", &p.title, &p.note), "button", s("How")), "icon", s("help")),
                        Some(why) => with(with(row("none", "", "info", &p.title, &p.note), "badge", s(why)), "tone", s("ok")),
                        None => with(row("tick", &format!("quick|{}", p.id), "check", &p.title, &p.note), "on", b(picked.contains(&p.id))),
                    })
                    .collect(),
            )
        })
        .collect();
    (sections, count)
}

fn changes_words(count: usize) -> String {
    if count == 1 { "1 change".into() } else { format!("{count} changes") }
}

/// Veteran's quick setup: everything on one screen.
pub(crate) fn quick(ui: &mut Ui) -> Json {
    let (all, picked) = chosen(ui, false);
    let (sections, count) = pick_sections(&all, &picked);
    let has_file = super::settings::setup_file(ui).is_file();
    let mut hints = vec![hint("x", "Tick all")];
    if has_file {
        hints.push(hint("y", "Load setup"));
    }
    obj! {
        "crumb": s("Veteran mode"),
        "title": s("Quick setup"),
        "intro": s("Tick what you want. One password, one go. Everything can be undone from Settings."),
        "flow": s("columns"),
        "sections": arr(sections),
        "note": s(format!("Turbine Hub backs up every setting first. {}.", changes_words(count))),
        "primary": obj! {"label": s(if count == 0 { "Finish".to_string() } else { format!("Apply {}", changes_words(count)) }), "k": s("start"), "act": s("quick-apply"), "arg": s("")},
        "hints": arr(hints),
        "x": obj! {"act": s("tick-all"), "arg": s("quick")},
        "y": if has_file { obj! {"act": s("setup-load"), "arg": s("")} } else { Json::Null },
    }
}

fn button(label: &str, k: &str, act: &str, arg: &str, primary: bool) -> Json {
    obj! {"label": s(label), "k": s(k), "act": s(act), "arg": s(arg), "primary": b(primary)}
}

/// Guided setup. `arg` names the step to show; empty shows the first one not done yet.
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    let st = ui.settings();
    let m = ui.machine();
    let step = SETUP_STEPS.iter().copied().find(|x| *x == arg).or_else(|| SETUP_STEPS.iter().copied().find(|x| !st.setup_done.iter().any(|d| d == x))).unwrap_or("picks");
    let index = SETUP_STEPS.iter().position(|x| *x == step).unwrap_or(0);
    let rail = arr(SETUP_STEPS.iter().enumerate().map(|(i, x)| obj! {"id": s(*x), "title": s(step_title(x)), "done": b(st.setup_done.iter().any(|d| d == x)), "current": b(i == index)}));
    let found = obj! {"title": s(format!("Found: {}", m.name())), "line": s(format!("{} \u{b7} {}", match super::kind_key(&m) { "tv" => "TV PC", "desktop" => "Desktop PC", "laptop" => "Laptop", _ => "Handheld" }, super::machine::machine_line(&m))), "chip": s("Detected")};
    let next = button("Continue", "a", "setup-next", step, true);
    let (title, text, embed, sections, buttons, note): (&str, String, &str, Vec<Json>, Vec<Json>, String) = match step {
        "machine" => ("This is your machine", format!("Turbine Hub shows only what this {} has. Everything it changes is shown first, saved first and can be undone.", kind_word(&m)), "", Vec::new(), vec![next], String::new()),
        "password" if ui.facts.has_password == Some(true) => ("This machine has a password", "SteamOS asks for a password before changing system settings, like the admin password on Windows. Turbine Hub asks for it once, uses it only for changes you approve, and never stores it.".into(), "", Vec::new(), vec![next], String::new()),
        "password" => (
            "One password for system changes",
            "SteamOS asks for a password before changing system settings, like the admin password on Windows. Turbine Hub asks for it once, uses it only for changes you approve, and never stores it.".into(),
            "",
            Vec::new(),
            vec![button("Set password", "a", "password", "", true), button("I already have one", "x", "setup-next", step, false), button("Skip for now", "y", "setup-next", step, false)],
            "Write it down somewhere safe. SteamOS can't recover it.".into(),
        ),
        "games" => ("", String::new(), "library", Vec::new(), vec![button("Next", "start", "setup-next", step, true)], String::new()),
        "bring" => ("", String::new(), "bring", Vec::new(), vec![button("Next", "start", "setup-next", step, true)], String::new()),
        _ => {
            let (all, picked) = chosen(ui, true);
            let (sections, count) = pick_sections(&all, &picked);
            ("Picks", "A good start for most people. Untick what you don't want.".into(), "", sections, vec![button(&if count == 0 { "Finish".to_string() } else { format!("Apply {}", changes_words(count)) }, "start", "quick-apply", "", true)], "Turbine Hub backs up every setting first. Everything can be undone from Settings.".into())
        }
    };
    obj! {
        "rail": rail,
        "step": s(step),
        "count": s(format!("Step {} of {}", index + 1, SETUP_STEPS.len())),
        "found": if step == "machine" || step == "password" { found } else { Json::Null },
        "title": s(title),
        "text": s(text),
        "note": s(note),
        "embed": s(embed),
        "flow": s("columns"),
        "sections": arr(sections),
        "buttons": arr(buttons),
        "hints": arr([]),
        "index": n(index as f64),
    }
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        "pick" => {
            let mut st = ui.settings();
            let veteran = arg == "veteran";
            st.mode = Some(if veteran { Mode::Veteran } else { Mode::Guided });
            let finished = st.setup_finished();
            ui.save(&st);
            Some(obj! {"state": b(true), "root": s(if finished { "home" } else if veteran { "quick" } else { "setup" })})
        }
        "setup-next" => {
            let mut st = ui.settings();
            if SETUP_STEPS.contains(&arg) && !st.setup_done.iter().any(|d| d == arg) {
                st.setup_done.push(arg.to_string());
            }
            let finished = st.setup_finished() || arg == "picks";
            if arg == "picks" {
                st.setup_done = SETUP_STEPS.iter().map(|x| x.to_string()).collect();
            }
            ui.save(&st);
            let next = SETUP_STEPS.iter().position(|x| *x == arg).and_then(|i| SETUP_STEPS.get(i + 1)).copied().unwrap_or("");
            Some(if finished { obj! {"state": b(true), "root": s("home"), "toast": s("Setup done")} } else { obj! {"state": b(true), "setupStep": s(next), "refresh": b(true)} })
        }
        "password" => {
            if ui.facts.has_password == Some(true) {
                return Some(toast("This machine has a password already."));
            }
            let mut p = Plan::new("Set a password", "The SteamOS password for system changes. Write it down somewhere safe: SteamOS can't recover it.", "Set password");
            p.ask = Ask::New;
            p.steps.push(Step::SetPassword);
            p.steps.push(Step::Done { setup: Some("password"), brought: None });
            p.done = "Password set".into();
            Some(dialog(ui, p))
        }
        "quick-apply" => {
            let guided = !ui.veteran();
            let (all, picked) = chosen(ui, guided);
            let go: Vec<&Pick> = all.iter().filter(|p| p.locked.is_none() && picked.contains(&p.id)).collect();
            let boost = go.iter().any(|p| p.id == "boost");
            if go.is_empty() {
                let mut st = ui.settings();
                st.setup_done = SETUP_STEPS.iter().map(|x| x.to_string()).collect();
                ui.save(&st);
                return Some(obj! {"state": b(true), "root": s("home"), "toast": s("Setup done")});
            }
            let mut p = Plan::new(&format!("Apply {}?", changes_words(go.len())), "Turbine Hub saves the current settings first. Every change can be undone in Settings, Review changes.", "Apply");
            p.items = go.iter().map(|x| (x.title.clone(), x.note.clone())).collect();
            let settings: Vec<Setting> = go.iter().flat_map(|x| x.settings.clone()).collect();
            let mut steps: Vec<Step> = Vec::new();
            if go.iter().any(|x| x.id == "decky")
                && let Some(tag) = &ui.decky_tag
            {
                steps.push(Step::Host(helper::job("decky-install", Some(&format!("{{\"tag\":{}}}", json::quote(tag))))));
            }
            steps.extend(go.iter().filter_map(|x| x.app).map(store::install_step));
            if go.iter().any(|x| x.id == "library") {
                steps.push(Step::SelfTile { add: true });
            }
            steps.push(Step::SetupFinished { boost });
            p.steps = steps;
            p.done = "Setup done".into();
            p.then_open = "home".into();
            let needs_helper = !settings.is_empty() || p.steps.iter().any(|x| matches!(x, Step::Host(c) if c.iter().any(|a| a == helper::BIN)));
            Some(if needs_helper {
                // The helper's settings go first, so a wrong password stops everything before any app installs.
                let rest = std::mem::take(&mut p.steps);
                let j = helper_plan(ui, p, &settings);
                if let Some(plan) = ui.pending.as_mut() {
                    plan.steps.extend(rest);
                }
                j
            } else {
                dialog(ui, p)
            })
        }
        _ => None,
    }
}

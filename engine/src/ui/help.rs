//! Help: each page's own short help (what the page is, what its buttons do), "Where's my...?" (Windows things and
//! where they are on SteamOS, and the glossary), and where each checkup button leads.
use super::plan::{guide, open};
use super::{Ui, arr, b, obj, row, s, section, with};
use crate::glossary;
use crate::json::Json;

/// A Windows thing and where it is here.
pub const WHERE: &[(&str, &str)] = &[
    ("Task Manager", "To close a stuck game, hold the Steam button and B. In Desktop Mode, open System Monitor."),
    ("The C: drive", "Your files live in your home folder. Games live in Steam's library, under Storage in Steam's settings."),
    ("Installing programs", "Use the Store tab here, or Discover in Desktop Mode. There are no .exe installers."),
    ("File Explorer", "Dolphin, in Desktop Mode. Steam button, Power, Switch to Desktop."),
    ("Windows Update", "Steam's Settings, System, Updates. Drivers come with SteamOS updates."),
    ("Screenshots", "Steam button and R1. They're in Steam's Media page."),
    ("Device Manager and drivers", "Built in. If something isn't detected, Turbine Hub's checkup on Home says so."),
    ("Admin password", "The SteamOS password. Turbine Hub's first-run steps set it, and Desktop Mode's System Settings, Users, changes it."),
    ("Control Panel", "Steam's Settings in Game Mode, System Settings in Desktop Mode. My machine here has the gaming ones."),
    ("Alt+Tab", "The Steam button shows what's running. In Desktop Mode, Alt+Tab works as on Windows."),
    ("Recording clips", "Steam's Settings, Game Recording. Steam button and R1 saves a clip while it records in the background."),
    ("Graphics driver settings", "Per game, in Games here: the switches for FSR 4 and frame generation. Steam's quick menu has the frame limit."),
    ("Antivirus", "Not needed. SteamOS keeps its system read only, and apps from the Store run fenced off from it."),
    ("Bluetooth and Wi-Fi", "Steam's Settings, Bluetooth and Internet."),
    ("The on-screen keyboard", "Steam button and X."),
    ("Game saves (Documents, My Games)", "Most games keep saves in Steam Cloud, and they come back by themselves. The others: Games, the game, Saves and snapshots. Saves on your Windows drive come over with Bring it over, Your Windows drive."),
];

/// What each page is, in a sentence or two: (page, words). A page not listed here falls back to its kind.
const ABOUT: &[(&str, &str)] = &[
    ("home", "What could use a look on this machine, each with its fix, and how setup is going."),
    ("games", "Every installed game. Pick one to play it and see whether it runs here, with its switches, its Proton and its saves. Games that won't run here are at the end, with the other ways to play them."),
    ("library", "Will my games run? Each game checked against Valve's ratings, the anti-cheat lists and Turbine Hub's own fixes."),
    ("store", "Apps, Decky plugins, launchers and TV tiles. Pick one for its own page with Install, Open, Update and Remove. Apps come from Flathub and keep themselves current."),
    ("store-item", "One thing from the Store: what it is, where it comes from, and Open, Install, Update or Remove."),
    ("tool", "Turbine Hub's launch tool in Steam. Picked for a game in Steam, it carries that game's switches and Proton pick into the game."),
    ("proton", "Which Proton this game starts with. Proton is what runs Windows games here, and another version often fixes a game that won't start. Undo puts the last one back."),
    ("saves", "This game's save snapshots. Back up now takes one, Restore puts one back after keeping the saves as they are now, and Undo goes back to those."),
    ("machine", "What this machine has, one card per part. Each card's page shows what it is set to and what can change."),
    ("module:tuneup", "Memory and kernel settings that make games smoother, from other gaming systems. Each one can be turned off again."),
    ("module:windows", "Starts Windows once, at the next start only. After that, SteamOS starts again as usual."),
    ("module:monitors", "Your screens: refresh rate, variable refresh (VRR) and HDR. These are for Desktop Mode; in Game Mode, Steam's Settings, Display decides."),
    ("module:power", "How much power the processor may use, and Game Boost. Lower is quieter and the battery lasts longer."),
    ("settings", "How Turbine Hub looks and works, updates, your setup file, help and reports."),
    ("changes", "Everything Turbine Hub changed on this machine, with Undo, or a button to the page where it is undone."),
    ("bring", "What to bring over from Windows: other stores, your Windows drive, your Windows apps, TV and streaming."),
    ("windrive", "Games and saves on your Windows drive, copied to this machine. The drive is only read, never changed."),
    ("winapps", "The Windows apps you use and their SteamOS versions, installed in one go."),
    ("setup", "The first steps, one at a time. The list on the left shows where you are."),
    ("quick", "Quick setup: tick what you want, then apply it all in one go."),
    ("pick", "How Turbine Hub talks to you: Guided explains things, Veteran shows every control. Settings switches any time."),
];

/// A button's name on a controller, or its key at a desk.
fn key_name(k: &str, keyboard: bool) -> &'static str {
    match (k, keyboard) {
        ("a", false) => "A",
        ("a", true) => "Enter",
        ("b", false) => "B",
        ("b", true) => "Esc",
        ("x", _) => "X",
        ("y", _) => "Y",
        ("lt", false) => "LT",
        ("lt", true) => "Tab",
        ("rt", false) => "RT",
        ("rt", true) => "/",
        ("start", false) => "MENU",
        ("start", true) => "F10",
        ("view", false) => "VIEW",
        ("view", true) => "F1",
        (_, false) => "LB and RB",
        (_, true) => "Q and E",
    }
}

/// One page's help. `arg` is the page and its own argument, as "page@argument".
pub(crate) fn page_help(ui: &mut Ui, arg: &str) -> Json {
    let (name, page_arg) = arg.split_once('@').unwrap_or((arg, ""));
    let name = if name.is_empty() || name == "help" || name == "wheres" { "home" } else { name };
    let about = ABOUT.iter().find(|(p, _)| *p == name).map(|(_, w)| *w).unwrap_or(if name.starts_with("module:") { "One part of this machine: what it is set to now and what can change." } else { "One of Turbine Hub's pages." });
    // The page's own buttons, as the page itself lists them.
    let shown = crate::json::parse(&ui.page(name, page_arg)).unwrap_or(Json::Null);
    let keyboard = ui.window;
    let mut own: Vec<(String, String)> = Vec::new();
    let mut add = |k: &str, label: &str| {
        if !label.is_empty() && !own.iter().any(|(x, _)| x == k) {
            own.push((k.to_string(), label.to_string()));
        }
    };
    for list in ["buttons", "hints"] {
        for h in shown.get(list).map(Json::as_array).unwrap_or(&[]) {
            add(h.str_at("k").unwrap_or(""), h.str_at("label").unwrap_or(""));
        }
    }
    if let Some(p) = shown.get("primary") {
        add(p.str_at("k").unwrap_or(""), p.str_at("label").unwrap_or(""));
    }
    let two_sides = ["games", "store", "settings"].contains(&name);
    let first_run = ["setup", "quick", "pick"].contains(&name);
    let mut keys = vec![(key_name("a", keyboard).to_string(), "Picks what the highlight is on: opens it, turns it on or off, or presses its button.".to_string())];
    keys.push((key_name("b", keyboard).to_string(), "Back to where you were.".into()));
    if two_sides {
        keys.push((key_name("lt", keyboard).to_string(), "Jumps between the list and what the picked entry holds.".into()));
    }
    for (k, label) in &own {
        if ["a", "b"].contains(&k.as_str()) {
            continue;
        }
        keys.push((key_name(k, keyboard).to_string(), format!("{label}.")));
    }
    if !first_run {
        keys.push((key_name("lb", keyboard).to_string(), "The tabs: Home, Games, Store, My machine, Settings.".into()));
    }
    keys.push((key_name("view", keyboard).to_string(), "This help. Press it again for Where's my...?".into()));
    if !own.iter().any(|(k, _)| k == "start") && !first_run {
        keys.push((key_name("start", keyboard).to_string(), "The menu: Where's my...?, Bring it over, Review changes, Settings, Close.".into()));
    }
    let tab = match name {
        "home" => Some("Home"),
        "games" => Some("Games"),
        "store" => Some("Store"),
        "machine" => Some("My machine"),
        "settings" => Some("Settings"),
        "setup" => Some("First steps"),
        "pick" => Some("Pick a mode"),
        _ => None,
    };
    let page_title = tab.map(String::from).or_else(|| shown.str_at("title").filter(|t| !t.is_empty()).map(String::from)).unwrap_or_else(|| "This page".into());
    let wheres = with(with(row("open", "wheres", "button", "Where's my...?", "Windows things and where they are on SteamOS, and the words Turbine Hub uses"), "icon", s("help")), "button", s("Open"));
    obj! {
        "crumb": s("Help"),
        "title": s(page_title),
        "intro": s(about),
        "sections": arr([
            section("", vec![wheres]),
            with(section("Buttons", keys.iter().map(|(k, w)| row("none", "", "info", k, w)).collect()), "columns", super::n(2.0)),
        ]),
        "hints": arr([]),
    }
}

/// The help page. `arg` is what the player typed into the search box.
pub(crate) fn page(_ui: &mut Ui, arg: &str) -> Json {
    let q = arg.trim().to_lowercase();
    let hit = |a: &str, c: &str| q.is_empty() || a.to_lowercase().contains(&q) || c.to_lowercase().contains(&q);
    let search = with(with(row("search", "", "input", "Search", "Like \"control panel\""), "value", s(arg)), "icon", s("search"));
    let places: Vec<Json> = WHERE.iter().filter(|(t, l)| hit(t, l)).map(|(t, l)| row("none", "", "info", t, l)).collect();
    let words: Vec<Json> = glossary::TERMS.iter().filter(|t| hit(t.term, t.means) || hit(t.windows, "")).map(|t| row("none", "", "info", t.term, &if t.windows.is_empty() { t.means.to_string() } else { format!("{} On Windows: {}.", t.means, t.windows) })).collect();
    let none = places.is_empty() && words.is_empty();
    let mut sections = vec![section("", vec![search])];
    if !places.is_empty() {
        sections.push(with(section("On Windows, on SteamOS", places), "columns", super::n(2.0)));
    }
    if !words.is_empty() {
        sections.push(with(section("Words Turbine Hub uses", words), "columns", super::n(2.0)));
    }
    if none {
        sections.push(with(section("", vec![]), "empty", s("Nothing matches. Try another word.")));
    }
    obj! {
        "crumb": s("Help"),
        "title": s("Where's my...?"),
        "intro": s("Windows things and where they are on SteamOS, and the words Turbine Hub uses. Search finds both."),
        "sections": arr(sections),
        "hints": arr([]),
        "search": b(true),
    }
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    if action != "checkup" {
        return None;
    }
    Some(match arg {
        "password" => return super::setup::act(ui, "password", "", value),
        "reapply" => return super::settings::act(ui, "reapply", "", value),
        "update-all" => return super::store::act(ui, "update-all", "", value),
        "update-app" => return super::settings::act(ui, "update-app", "", value),
        "help:plugins" => guide("Updating Decky plugins", "Plugins update from Decky's own store, in Steam's quick menu.", &[("1", "Open Steam's quick menu and pick the plug icon."), ("2", "Open the store icon at the top, then Updates."), ("3", "Update each plugin, or all of them.")]),
        "bring" => open("bring"),
        "light-test" => return super::lighting::act(ui, "light-test", "", value),
        "self-tile" => return super::selftile::act(ui, "self-tile", "add", value),
        "help:panel" => return super::store::act(ui, "panel-guide", "", value),
        "help:nvidia" => guide("Nvidia graphics on SteamOS", "Valve and Nvidia are still working on it, so some things are rough.", &[("Works", "Most games in Desktop Mode and in Game Mode."), ("Rough", "HDR, variable refresh and sleep can misbehave in Game Mode."), ("If a game stutters", "Try it in Desktop Mode, and keep SteamOS up to date: fixes arrive with its updates.")]),
        "help:hdmi" => guide("4K at 120 Hz over HDMI", "Linux graphics drivers aren't allowed to use HDMI 2.1's fastest mode, so HDMI stops at 4K 60 Hz with full color.", &[("The way around it", "A DisplayPort to HDMI 2.1 adapter between the PC and the TV carries 4K at 120 Hz."), ("Or", "Run 1440p at 120 Hz over HDMI. On a TV from the couch it looks close.")]),
        // Turbine Hub's monitor controls set Desktop Mode's screens. In Game Mode, Steam sets the screen.
        "refresh-rate" if ui.window => open("module:monitors"),
        "refresh-rate" => guide(
            "Set the refresh rate in Steam",
            "In Game Mode, Steam decides how the screen runs. Turbine Hub's Monitors page sets Desktop Mode only.",
            &[("1", "Press the Steam button and open Settings, then Display."), ("2", "Under the external display's settings, pick the highest refresh rate it offers. Some screens show it only with Advanced settings on."), ("Desktop Mode", "There, My machine, Monitors sets it with one press.")],
        ),
        "help:screens" => guide("Which screen Game Mode uses", "Game Mode shows one screen at a time. Steam picks it; Desktop Mode uses all of them.", &[("1", "Press the Steam button and open Settings, then Display."), ("2", "Pick the screen there when it offers a choice. With only one screen plugged in, Game Mode always uses that one.")]),
        "sleep" | "monitors" | "drives" | "battery" => open(&format!("module:{arg}")),
        _ => return None,
    })
}

//! Help: each page's own short help (what the page is, what its buttons do), "Where's my...?" (Windows things and
//! where they are on SteamOS, and the glossary), and where each checkup button leads.
use super::plan::{guide, open};
use super::{Ui, arr, b, obj, row, s, section, with};
use crate::glossary;
use crate::json::Json;

/// A Windows thing and where it is here.
pub const WHERE: &[(&str, &str)] = &[
    ("Task Manager", "To close a stuck game, hold the Steam button and B. In Desktop Mode, open System Monitor."),
    ("The C: drive", "Files, This PC, SteamOS (C:). Your files live in your home folder (C:\\Users there) and games in Steam's library (C:\\Games there)."),
    ("Installing programs", "Use the Store tab here, or Discover in Desktop Mode. There are no .exe installers."),
    ("File Explorer", "The Files tab here, laid out like the one on Windows, in Game Mode too. In Desktop Mode there is also Dolphin."),
    ("Recycle Bin", "Files, Recycle Bin. SteamOS calls it the Trash; it is the same bin Desktop Mode's file manager uses."),
    ("AppData and Program Files", "Files shows both: AppData under your user folder, Program Files for installed apps, each with its real folder."),
    ("Windows Update", "Steam's Settings, System, Updates. Drivers come with SteamOS updates."),
    ("Screenshots", "Steam button and R1. They're in Steam's Media page."),
    ("Device Manager and drivers", "Built in. If something isn't detected, Turbine Hub's checkup on Home says so."),
    ("Admin password", "The SteamOS password. Turbine Hub asks for it only when you first change a system setting, and helps you make one then if there is none. Desktop Mode's System Settings, Users, changes it."),
    ("Control Panel", "Steam's Settings in Game Mode, System Settings in Desktop Mode. My machine here has the gaming ones."),
    ("Alt+Tab", "The Steam button shows what's running. In Desktop Mode, Alt+Tab works as on Windows."),
    ("Recording clips", "Steam's Settings, Game Recording. Steam button and R1 saves a clip while it records in the background."),
    ("Graphics driver settings", "Per game, in Games here: the switches for FSR 4 and frame generation. Steam's quick menu has the frame limit."),
    ("Antivirus", "Not needed. SteamOS keeps its system read only, and apps from the Store run fenced off from it."),
    ("Bluetooth and Wi-Fi", "Steam's Settings, Bluetooth and Internet."),
    ("The on-screen keyboard", "Steam button and X."),
    ("Disk Cleanup and Storage Sense", "My machine, Storage. It shows what is using each drive and the leftovers that can go: the safe ones ticked, the rest only after you tick them."),
    ("Game saves (Documents, My Games)", "Most games keep saves in Steam Cloud, and they come back by themselves. The others: Games, the game, Saves and snapshots. Saves on your Windows drive come over with Bring it over, Your Windows drive."),
];

/// What each page is, in a sentence or two: (page, words). A page not listed here falls back to its kind.
const ABOUT: &[(&str, &str)] = &[
    ("home", "What could use a look on this machine, each with its fix, the readings, and the quick switches: the power mode, Game Boost, the lights and the charge limit, one press each. Start fixes what the checkup can put right by itself, and Show all checks lists what it looked at and found in order."),
    ("games", "Every installed game. Pick one to play it and see whether it runs here, with its switches, its Proton and its saves. Games that won't run here are at the end, with the other ways to play them."),
    ("library", "Will my games run? Each game checked against Valve's ratings, the anti-cheat lists and Turbine Hub's own fixes."),
    ("files", "A file explorer laid out like the one on Windows. Windows-like names the places as Windows does (This PC, C:, Users, Games, Program Files, Recycle Bin) and True Linux shows every real name; the switch is on the command bar and the real path is always in the status bar. A opens, B goes back a folder, Start ticks a line, and Options (X or a right-click) has Cut, Copy, Rename, Delete and the rest. At a keyboard a letter jumps to the name that starts with it, so Options is Shift+F10 and Find is Ctrl+F here; F2, Del, Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+Z, Ctrl+A, Ctrl+T, Ctrl+W, Ctrl+L and Alt+Up work as on Windows. Delete goes to the Recycle Bin and can be undone. SteamOS's own files are shown locked. No password is ever asked."),
    ("store", "Apps, Decky plugins, launchers and TV tiles. Every row says how it stands (Not installed, Installed, Update ready, Working) and has its button: A installs, opens or updates it, after showing what will happen. Options (X) has the rest: Remove, Details, where it comes from. LT and RT change the section. Installs and updates run while you keep browsing; the top bar shows what is being worked on and how many wait, and Options (X) on any page can stop the one being worked on."),
    ("store-item", "One thing from the Store: what it is, where it comes from, and Open, Install, Update or Remove."),
    ("tool", "Turbine Hub's launch tool in Steam. Picked for a game in Steam, it carries that game's switches and Proton pick into the game."),
    ("proton", "Which Proton this game starts with. Proton is what runs Windows games here, and another version often fixes a game that won't start. Undo puts the last one back."),
    ("saves", "This game's save snapshots. Back up now takes one, Restore puts one back after keeping the saves as they are now, and Undo goes back to those."),
    ("machine", "What this machine has, a section each. The chips across the top pick one (LT and RT, or [ and ] on a keyboard, go to the one before and the next, round and round; the highlight can walk up into them too). Overview shows every part with its main switch on its card; every other section has its controls right on the page. B on a section goes back to Overview."),
    ("settings", "How Turbine Hub looks and works, updates, your setup file, help and reports. Look holds the theme, the colors for your eyes (Color vision) and how much moves (Animations)."),
    ("changes", "Everything Turbine Hub changed on this machine, with Undo, or a button to the page where it is undone."),
    ("bring", "What to bring over from Windows: other stores, your Windows drive, your Windows apps, TV and streaming."),
    ("windrive", "Games and saves on your Windows drive, copied to this machine. The drive is only read, never changed."),
    ("winapps", "The Windows apps you use and their SteamOS versions, installed in one go."),
    ("setup", "The first steps, one at a time. The list on the left shows where you are. No step asks for a password: Picks marks the ones that need it, and only applying those asks."),
    ("quick", "Quick setup: tick what you want, then apply it all in one go. Only picks marked \"Needs your password\" ask for it, when you apply them."),
    ("pick", "How Turbine Hub talks to you: Guided explains things, Veteran shows every control. Settings switches any time."),
];

/// One page's help. `arg` is the page and its own argument, as "page@argument".
pub(crate) fn page_help(ui: &mut Ui, arg: &str) -> Json {
    let (name, page_arg) = arg.split_once('@').unwrap_or((arg, ""));
    let name = if name.is_empty() || name == "help" || name == "wheres" { "home" } else { name };
    // A section of My machine has its own words (the same its "About" shows).
    let part = name.strip_prefix("module:").or(Some(page_arg).filter(|a| name == "machine" && !a.is_empty() && *a != "overview"));
    let about = match part {
        Some(id) => super::machine::about(id).0,
        None => ABOUT.iter().find(|(p, _)| *p == name).map(|(_, w)| *w).unwrap_or("One of Turbine Hub's pages."),
    };
    // What the page itself offers, as its Options menu lists it.
    let shown = crate::json::parse(&ui.page(name, page_arg)).unwrap_or(Json::Null);
    let own: Vec<(String, String)> = shown.get("options").map(Json::as_array).unwrap_or(&[]).iter().filter(|o| o.str_at("key") != Some("f1")).map(|o| (o.str_at("title").unwrap_or("").to_string(), o.str_at("key").unwrap_or("").to_string())).filter(|(t, _)| !t.is_empty()).collect();
    let has = |key: &str| own.iter().any(|(_, k)| k == key);
    let first_run = super::controls::first_run(name);
    let tabs = !first_run && ui.settings().mode.is_some();
    let main = own.iter().find(|(_, k)| k == "start").map(|(t, _)| t.clone());
    let mut here: Vec<Json> = Vec::new();
    if !own.is_empty() {
        let names: Vec<&str> = own.iter().map(|(t, _)| t.as_str()).collect();
        here.push(row("none", "", "info", "Options on this page", &format!("{}. X, R or a right-click lists them, after what the highlighted thing itself can do.", names.join(", "))));
    }
    // The scheme, the same on every page; a line the page has no use for is left out.
    let keys: Vec<Json> = super::controls::SCHEME
        .iter()
        .filter(|c| match c.name {
            "Find" => has("y"),
            "Play" => has("play"),
            "Tabs" | "Menu" => tabs,
            "Sections" => shown.get("chips").is_some(),
            _ => true,
        })
        .map(|c| {
            let ways: Vec<&str> = [c.pad, c.keys, c.mouse].into_iter().filter(|w| !w.is_empty()).collect();
            let does = match (c.name, &main) {
                ("Main button", Some(label)) => format!("Here: {label}."),
                ("Main button", None) if first_run => "Presses the page's main button, when it has one.".to_string(),
                ("Main button", None) => "This page has no main button, so it opens Settings.".to_string(),
                _ => c.does.to_string(),
            };
            with(row("none", "", "info", c.name, &does), "value", s(ways.join(" \u{b7} ")))
        })
        .collect();
    let tab = match name {
        "home" => Some("Home"),
        "games" => Some("Games"),
        "store" => Some("Store"),
        "files" => Some("Files"),
        "machine" => Some("My machine"),
        "settings" => Some("Settings"),
        "setup" => Some("First steps"),
        "pick" => Some("Pick a mode"),
        _ => None,
    };
    // (A section of My machine goes by its own name.)
    let tab = match part {
        Some(id) => Some(super::machine::section_title(id, &ui.machine())),
        None => tab,
    };
    let page_title = tab.map(String::from).or_else(|| shown.str_at("title").filter(|t| !t.is_empty()).map(String::from)).unwrap_or_else(|| "This page".into());
    let wheres = with(with(row("open", "wheres", "button", "Where's my...?", "Windows things and where they are on SteamOS, and the words Turbine Hub uses"), "icon", s("help")), "button", s("Open"));
    here.insert(0, wheres);
    obj! {
        "crumb": s("Help"),
        "title": s(page_title),
        "intro": s(about),
        "sections": arr([
            section("", here),
            section("Controls: controller \u{b7} keyboard \u{b7} mouse", keys),
        ]),
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
        "search": b(true),
    }
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    if action != "checkup" {
        return None;
    }
    Some(match arg {
        "reapply" => return super::settings::act(ui, "reapply", "", value),
        "update-all" => return super::store::act(ui, "update-all", "", value),
        "update-app" => return super::settings::act(ui, "update-app", "", value),
        "allow-library" => return super::games::act(ui, "allow-library", "", value),
        "restart-app" => return super::settings::act(ui, "restart-app", "", value),
        "help:plugins" => guide("Updating Decky plugins", "Plugins update from Decky's own store, in Steam's quick menu.", &[("1", "Open Steam's quick menu and pick the plug icon."), ("2", "Open the store icon at the top, then Updates."), ("3", "Update each plugin, or all of them.")]),
        "bring" => open("bring"),
        "light-test" | "light-report" => return super::lighting::act(ui, arg, "", value),
        // Decky Loader's own Repair: its newest release again, over what is there.
        "decky-repair" => return super::store::act(ui, "decky", "", value),
        "lighting" => open("module:lighting"),
        "self-tile" => return super::selftile::act(ui, "self-tile", "add", value),
        "tile-art" => return super::onepress::act(ui, "tile-art", "fix", value),
        "tile-art-undo" => return super::onepress::act(ui, "tile-art", "undo", value),
        "storage" => open("module:storage"),
        "help:panel" => return super::store::act(ui, "panel-guide", "", value),
        "help:controller" => guide(
            "Using a controller in Desktop Mode",
            "No controller was found. Steam passes yours on to the things it starts, so start Turbine Hub from Steam.",
            &[("1", "Add Turbine Hub to your Steam library: Settings, My setup."), ("2", "Close Turbine Hub and start it from its entry in Steam."), ("Meanwhile", "Keyboard and mouse do everything here.")],
        ),
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
        "sleep" | "monitors" | "drives" | "battery" | "gpu" => open(&format!("module:{arg}")),
        _ => return None,
    })
}

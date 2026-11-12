//! "Where's my...?": Windows things and where they are on SteamOS, the glossary, and where each checkup button leads.
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
];

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
        "intro": s("Windows things and where they are on SteamOS."),
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
        "help:nvidia" => guide("Nvidia graphics on SteamOS", "Valve and Nvidia are still working on it, so some things are rough.", &[("Works", "Most games in Desktop Mode and in Game Mode."), ("Rough", "HDR, variable refresh and sleep can misbehave in Game Mode."), ("If a game stutters", "Try it in Desktop Mode, and keep SteamOS up to date: fixes arrive with its updates.")]),
        "help:hdmi" => guide("4K at 120 Hz over HDMI", "Linux graphics drivers aren't allowed to use HDMI 2.1's fastest mode, so HDMI stops at 4K 60 Hz with full color.", &[("The way around it", "A DisplayPort to HDMI 2.1 adapter between the PC and the TV carries 4K at 120 Hz."), ("Or", "Run 1440p at 120 Hz over HDMI. On a TV from the couch it looks close.")]),
        "sleep" | "monitors" | "drives" | "battery" => open(&format!("module:{arg}")),
        _ => return None,
    })
}

//! Turbine Hub in the player's Game Mode library: its own entry among Steam's non-Steam games, with its own art, so
//! it opens from Game Mode like a game. Steam reads its list of non-Steam games only when it starts and writes its
//! own copy back when it closes, so the entry is added and removed only while Steam is closed; while Steam runs the
//! player gets the steps instead. The art goes into Steam's grid folder and never over a picture the player picked.
use super::plan::{Plan, Step, dialog, guide, toast};
use super::{Ui, row, s, with};
use crate::json::Json;
use crate::shortcuts::{self, NewShortcut, TileChange};
use crate::steam_art;
use std::path::{Path, PathBuf};

pub const FLATPAK_ID: &str = "io.github.ravenor907.TurbineHub";
/// The words in the entry's launch options that mark it as the one Turbine Hub added itself.
pub const MARKER: &str = "--command=turbinehub";
pub const TITLE: &str = "Turbine Hub in your Game Mode library";

/// Turbine Hub's own entry: the Flatpak, started the way Steam's "Add a Non-Steam Game" would start it.
pub fn tile() -> NewShortcut {
    NewShortcut { name: "Turbine Hub".into(), exe: "\"/usr/bin/flatpak\"".into(), start_dir: "\"/usr/bin/\"".into(), launch_options: format!("run --branch=stable --arch=x86_64 {MARKER} {FLATPAK_ID}"), flatpak_id: FLATPAK_ID.into(), icon: String::new() }
}

/// Where the art's record is kept: which grid files Turbine Hub wrote.
pub fn record(folder: &Path) -> PathBuf {
    folder.join("steam-art.json")
}

/// The art set inside Turbine Hub's shared files.
pub fn art_dir(share: &Path) -> PathBuf {
    share.join("steam-art")
}

/// Where the entry stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Missing,
    /// In the library. `ours`: Turbine Hub added it itself (else the player did, in Steam). `art_missing`: how many
    /// of its pictures aren't in Steam's grid folder.
    Added { ours: bool, art_missing: usize },
}

/// The pictures an entry gets: all five for the one Turbine Hub added (its icon is set in the entry), and all but
/// the icon for one the player added in Steam (the icon stays the player's choice there).
fn parts(ours: bool) -> &'static [&'static str] {
    if ours { steam_art::COOP_PARTS } else { steam_art::TURBINE_PARTS }
}

fn has_marker(home: &Path) -> bool {
    shortcuts::shortcut_files(home).iter().any(|f| std::fs::read(f).ok().and_then(|bytes| shortcuts::parse(&bytes).ok()).is_some_and(|list| list.iter().any(|x| shortcuts::is_self(x) && x.launch_options.contains(MARKER))))
}

pub fn state(home: &Path) -> State {
    let Some((file, appid)) = shortcuts::find_self_file(home) else { return State::Missing };
    let ours = has_marker(home);
    let grid = steam_art::grid_dir(&file);
    let there = |part: &str| steam_art::grid_name(appid, part).is_some_and(|name| ["png", "jpg", "jpeg", "webp"].iter().any(|ext| grid.join(name.replace(".png", &format!(".{ext}"))).exists()));
    State::Added { ours, art_missing: parts(ours).iter().filter(|p| !there(p)).count() }
}

/// Is Turbine Hub missing from the library? (For the checkup's tip and the first-run picks.)
pub fn missing(ui: &Ui) -> bool {
    state(&ui.ctx.home) == State::Missing
}

/// Why the first-run picks can't offer it right now, when they can't.
pub(crate) fn pick_lock(ui: &Ui) -> Option<&'static str> {
    if !missing(ui) {
        Some("In your library")
    } else if ui.steam_running {
        Some("Later, with Steam closed")
    } else if shortcuts::tile_file(&ui.ctx.home).is_none() {
        Some("Sign in to Steam first")
    } else {
        None
    }
}

/// The rows in Settings: where it stands and the one press that fits, and Remove once it is there.
pub(crate) fn rows(ui: &Ui) -> Vec<Json> {
    let button = |arg: &str, line: &str, label: &str| with(with(row("self-tile", arg, "button", TITLE, line), "icon", s("games")), "button", s(label));
    match state(&ui.ctx.home) {
        State::Missing => vec![button("add", "Not there yet. Add it and it opens from Game Mode like a game, with its own art.", "Add")],
        State::Added { art_missing, .. } => {
            let first = match art_missing {
                0 => with(with(with(row("none", "", "info", TITLE, "It opens from Game Mode like a game, under Non-Steam."), "icon", s("games")), "badge", s("In your library")), "tone", s("ok")),
                1 => button("art", "In your library. 1 of its pictures is missing.", "Fix art"),
                c => button("art", &format!("In your library. {c} of its pictures are missing."), "Fix art"),
            };
            vec![first, with(with(row("self-tile", "remove", "button", "Take Turbine Hub out of the library", "Only the library entry and its pictures go. Turbine Hub stays installed."), "icon", s("remove")), "button", s("Remove"))]
        }
    }
}

fn steam_open_guide(adding: bool) -> Json {
    if adding {
        guide(
            "Steam has to be closed for this",
            "Steam reads its list of non-Steam games only when it starts, and writes its own copy back when it closes. So Turbine Hub adds itself while Steam is closed.",
            &[
                ("1", "Switch to Desktop Mode: Steam button, Power, Switch to Desktop."),
                ("2", "Quit Steam there: its icon by the clock, Exit Steam."),
                ("3", "Open Turbine Hub from the app menu, go to Settings and press Add again."),
                ("4", "Start Steam again, or go back to Game Mode. Turbine Hub is in your library under Non-Steam."),
                ("Or", "In Steam on the desktop: Games, Add a Non-Steam Game, tick Turbine Hub. Then press Fix art here to add its pictures."),
            ],
        )
    } else {
        guide("Remove it in Steam", "Steam is running, and it only lets go of a library entry from its own menu.", &[("1", "Find Turbine Hub in your library, under Non-Steam."), ("2", "Open its options, then Manage, then Remove non-Steam game.")])
    }
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    if action != "self-tile" {
        return None;
    }
    let now = state(&ui.ctx.home);
    Some(match (arg, now) {
        ("add", State::Added { .. }) => toast("Turbine Hub is in your library already."),
        ("add", State::Missing) => {
            if ui.steam_running {
                return Some(steam_open_guide(true));
            }
            let Some(file) = shortcuts::tile_file(&ui.ctx.home) else { return Some(toast("Steam hasn't been signed in to on this machine yet. Start Steam once and sign in.")) };
            let mut p = Plan::new("Add Turbine Hub to your library?", "Turbine Hub adds itself to Steam's list of non-Steam games, with its own art, so it opens from Game Mode like a game. A backup of the list is kept first.", "Add");
            p.items.push(("Turbine Hub".into(), "Under Non-Steam in your library, after Steam's next start".into()));
            p.items.push(("Steam's list of non-Steam games".into(), if ui.veteran() { file.to_string_lossy().into_owned() } else { "A backup of it is kept first".into() }));
            p.items.push(("Its pictures".into(), "Cover, banner, background, logo and icon. Pictures you picked yourself are never replaced.".into()));
            p.steps.push(Step::SelfTile { add: true });
            p.done = "Turbine Hub added. It shows after Steam starts again.".into();
            dialog(ui, p)
        }
        ("art", State::Missing) => toast("Turbine Hub isn't in your library yet."),
        ("art", State::Added { art_missing: 0, .. }) => toast("Its pictures are all there."),
        ("art", State::Added { .. }) => {
            let mut p = Plan::new("Put Turbine Hub's pictures back?", "Turbine Hub writes its own art for its library entry into Steam's folder for library pictures. Pictures you picked yourself are never replaced.", "Fix art");
            p.items.push(("Cover, banner, background and logo".into(), "Steam shows them after its next start".into()));
            p.steps.push(Step::SelfArt);
            p.done = "Pictures in place. Steam shows them after its next start.".into();
            dialog(ui, p)
        }
        ("remove", State::Missing) => toast("Turbine Hub isn't in your library."),
        ("remove", State::Added { ours, .. }) => {
            if ui.steam_running || !ours {
                return Some(steam_open_guide(false));
            }
            let mut p = Plan::new("Take Turbine Hub out of your library?", "Only the library entry and the pictures Turbine Hub wrote for it go. Turbine Hub stays installed and opens from Desktop Mode. A backup of the list is kept first.", "Remove");
            p.danger = true;
            p.items.push(("Turbine Hub".into(), "Gone from your library after Steam's next start".into()));
            p.steps.push(Step::SelfTile { add: false });
            p.done = "Turbine Hub is out of the library".into();
            dialog(ui, p)
        }
        _ => return None,
    })
}

fn write_art(home: &Path, folder: &Path, share: &Path) -> Result<(), String> {
    let (file, appid) = shortcuts::find_self_file(home).ok_or("Turbine Hub isn't in your library yet.")?;
    let done = steam_art::add_art_opts(&art_dir(share), &steam_art::grid_dir(&file), appid, parts(has_marker(home)), &record(folder), true).map_err(|e| format!("Couldn't write the pictures: {e}"))?;
    if steam_art::nothing_shown(&done) { Err(format!("No picture could be placed: {}.", steam_art::summary(&done))) } else { Ok(()) }
}

/// The library entry's own steps of a confirmed dialog. None for any other step. `share`: Turbine Hub's shared
/// files (the art set is in them).
pub fn run_step(step: &Step, home: &Path, folder: &Path, share: &Path, steam_running: bool) -> Option<Result<(), String>> {
    Some(match step {
        Step::SelfTile { add: true } => {
            // An entry the player added in Steam already: only its pictures are missing.
            if shortcuts::find_self_file(home).is_some() {
                return Some(write_art(home, folder, share));
            }
            let Some(file) = shortcuts::tile_file(home) else { return Some(Err("Steam's list of non-Steam games wasn't found. Start Steam once and sign in.".into())) };
            shortcuts::add_tile_with_art(&file, &tile(), MARKER, &art_dir(share), &record(folder), steam_running).map(|_| ())
        }
        Step::SelfTile { add: false } => {
            let Some(file) = shortcuts::tile_file(home) else { return Some(Err("Steam's list of non-Steam games wasn't found.".into())) };
            match shortcuts::remove_tile_with_art(&file, MARKER, &record(folder), steam_running) {
                Ok(TileChange::NotThere) => Err("That entry wasn't added by Turbine Hub. Remove it in Steam: its options, Manage, Remove non-Steam game.".into()),
                r => r.map(|_| ()),
            }
        }
        Step::SelfArt => write_art(home, folder, share),
        _ => return None,
    })
}

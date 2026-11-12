//! Two things done in one press, each shown first as the one dialog: the newest GE-Proton fetched into Steam's
//! folder for tools (`proton_get`), and pictures drawn for library entries that have none (`tile_art`). No page of
//! their own: the first is rows on a game's Proton page, the second is a card in the checkup.
use super::plan::{Plan, Step, dialog, guide, toast};
use super::{Ui, games, row, s, with};
use crate::games::Store;
use crate::json::Json;
use crate::net::Client;
use crate::shortcuts::Steam;
use crate::{proton_get, tile_art};
use std::path::Path;

fn steam_root(ui: &Ui) -> Option<std::path::PathBuf> {
    crate::steam::steam_roots(&ui.ctx.home).into_iter().next()
}

/// The rows under a game's list of Protons: the newest GE-Proton (one press to get it, or "Up to date" when it is
/// there), and Remove for each build Turbine Hub fetched itself.
pub(crate) fn proton_rows(ui: &Ui) -> Vec<Json> {
    let Some(root) = steam_root(ui) else { return Vec::new() };
    let about = "A community build of Proton with extra fixes for some games.";
    let mut rows = Vec::new();
    // While one is being fetched its row says so and has Stop; one that waits for its turn says that.
    let known = proton_get::known(&ui.cache);
    let key = format!("proton:{}", known.as_ref().map(|r| r.tag.as_str()).unwrap_or("*"));
    let fetching = ui.queue.place(&key).filter(|_| known.as_ref().is_none_or(|r| !proton_get::installed(&root, &r.tag)));
    let name = known.as_ref().map(|r| r.tag.clone()).unwrap_or_else(|| "The newest GE-Proton".into());
    rows.push(match known {
        _ if fetching == Some(super::queue::Place::Working) && ui.queue.stopping() => with(with(with(row("none", "", "info", &name, "Stopping. What was downloaded and unpacked so far is taken away."), "icon", s("download")), "badge", s("Stopping")), "tone", s("muted")),
        _ if fetching == Some(super::queue::Place::Working) => with(with(with(row("work-stop", "", "button", &name, "Being downloaded, checked and unpacked. Keep going: the top bar shows how it is doing. Stop takes away what was fetched so far."), "icon", s("download")), "button", s("Stop")), "badge", s("Working")),
        _ if fetching == Some(super::queue::Place::Waiting) => with(with(with(row("none", "", "info", &name, "Waiting for its turn. It is fetched once the work before it has finished."), "icon", s("download")), "badge", s("Waiting")), "tone", s("muted")),
        Some(r) if proton_get::installed(&root, &r.tag) => with(with(with(row("none", "", "info", &proton_get::installed_as(&root, &r.tag).unwrap_or(r.tag), "The newest GE-Proton is installed. Pick it in the list above to use it for this game."), "icon", s("download")), "badge", s("Up to date")), "tone", s("ok")),
        Some(r) => with(with(row("proton-get", &r.tag, "button", "Get the newest GE-Proton", &format!("{}: {about} Downloaded, checked and added to the list above.", r.tag)), "icon", s("download")), "button", s("Get")),
        None => with(with(row("proton-get", "", "button", "Get the newest GE-Proton", &format!("{about} Downloaded, checked and added to the list above.")), "icon", s("download")), "button", s("Get")),
    });
    for name in proton_get::fetched(&root, &ui.folder()) {
        rows.push(with(with(row("proton-remove", &name, "button", &format!("Remove {name}"), "Turbine Hub fetched it. It goes from Steam's folder for tools."), "icon", s("remove")), "button", s("Remove")));
    }
    rows
}

/// The games (by name) set to use a build, for the dialog that says to switch them first.
fn users(ui: &Ui, ids: &[String]) -> Vec<(String, String)> {
    let all = games::listed(ui);
    ids.iter()
        .map(|id| {
            if id == "0" {
                ("Steam's choice for all games".to_string(), "In Steam: Settings, Compatibility, pick another default.".to_string())
            } else {
                (all.iter().find(|l| l.game.store == Store::Steam && l.game.store_id == *id).map(|l| l.game.name.clone()).unwrap_or_else(|| format!("Steam app {id}")), "Pick another Proton for it: here under Try another Proton, or in Steam under Properties, Compatibility.".to_string())
            }
        })
        .collect()
}

fn entries_words(n: usize) -> String {
    if n == 1 { "1 library entry".to_string() } else { format!("{n} library entries") }
}

/// The checkup's words for entries without pictures: "3 library entries have no pictures".
pub fn checkup_title(n: usize) -> String {
    format!("{} {} no pictures", entries_words(n), if n == 1 { "has" } else { "have" })
}

fn steam_closed_guide(unsure: bool, what: &str) -> Json {
    guide(
        "Steam has to be closed for this",
        if unsure { "Turbine Hub couldn't check whether Steam is running just now, so it leaves Steam's library alone. Steam reads its list of non-Steam games only when it starts, and writes its own copy back when it closes." } else { "Steam reads its list of non-Steam games and their pictures when it starts, and writes its own copy of the list back when it closes. So Turbine Hub changes them while Steam is closed." },
        &[("1", "Switch to Desktop Mode: Steam button, Power, Switch to Desktop."), ("2", "Quit Steam there: its icon by the clock, Exit Steam."), ("3", &format!("Open Turbine Hub from the app menu and press {what} again.")), ("4", "Start Steam again, or go back to Game Mode.")],
    )
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        "proton-get" => {
            let Some(root) = steam_root(ui) else { return Some(toast("Steam wasn't found on this machine.")) };
            let known = proton_get::known(&ui.cache);
            // The release the row named, else the newest one known, else whichever is newest when it runs.
            let release = proton_get::release_for_tag(arg).map(|r| known.clone().filter(|k| k.tag == r.tag).unwrap_or(r)).or(known);
            let tag = release.as_ref().map(|r| r.tag.clone()).unwrap_or_default();
            if !tag.is_empty() && proton_get::installed(&root, &tag) {
                return Some(with(toast(&format!("{tag} is installed already.")), "refresh", super::b(true)));
            }
            let title = if tag.is_empty() { "Get the newest GE-Proton?".to_string() } else { format!("Get {tag}?") };
            let mut p = Plan::new(&title, "GE-Proton is a community build of Proton with extra fixes for some games. Turbine Hub downloads it from its GitHub page, checks it against the checksum published there, and unpacks it into Steam's folder for tools. None of Steam's own files change.", "Get");
            let size = release.as_ref().and_then(|r| r.archive_size).map(crate::net::human_size).unwrap_or_else(|| "about half a gigabyte".into());
            p.items.push((if tag.is_empty() { "The newest release".to_string() } else { tag.clone() }, format!("From {}, {size} to download", proton_get::WHERE)));
            p.items.push(("Checked before it is unpacked".into(), if ui.veteran() { "Its SHA-512 has to match the .sha512sum published with it".into() } else { "It has to match the checksum its makers published, or nothing is installed".into() }));
            p.items.push(("Where it goes".into(), if ui.veteran() { crate::compat_tool::tools_dir(&root).to_string_lossy().into_owned() } else { "Steam's folder for tools, in a folder of its own".into() }));
            p.items.push(("Afterwards".into(), "It shows in this list at once. Steam lists it after its next start.".into()));
            p.steps.push(Step::ProtonGet { tag: tag.clone() });
            // It runs in the line of work while the player browses: the top bar names it, and the same build is
            // never fetched twice at once.
            p.what = if tag.is_empty() { "GE-Proton".to_string() } else { tag.clone() };
            if let Some(why) = ui.work_refusal(&p) {
                return Some(toast(&why));
            }
            p.done = format!("{} is installed. Steam lists it after its next start.", if tag.is_empty() { "The newest GE-Proton" } else { tag.as_str() });
            Some(dialog(ui, p))
        }
        "proton-remove" => {
            let Some(root) = steam_root(ui) else { return Some(toast("Steam wasn't found on this machine.")) };
            if !proton_get::fetched(&root, &ui.folder()).iter().any(|t| t == arg) {
                return Some(toast("Turbine Hub only removes the Proton builds it fetched itself."));
            }
            let using = proton_get::used_by(&root, &ui.folder(), arg);
            if !using.is_empty() {
                let names = users(ui, &using);
                let items: Vec<(&str, &str)> = names.iter().map(|(a, t)| (a.as_str(), t.as_str())).collect();
                return Some(guide("Switch these to another Proton first", &format!("They are set to use {arg}. Without it they couldn't start, so it stays until they use another one."), &items));
            }
            let mut p = Plan::new(&format!("Remove {arg}?"), "Turbine Hub takes this Proton build out of Steam's folder for tools. No game is set to use it. It can be fetched again any time.", "Remove");
            p.danger = true;
            p.items.push(("Removed".into(), if ui.veteran() { proton_get::build_dir(&root, arg).to_string_lossy().into_owned() } else { "Its folder in Steam's folder for tools".into() }));
            p.steps.push(Step::ProtonRemove { name: arg.to_string() });
            p.done = format!("{arg} removed");
            Some(dialog(ui, p))
        }
        "tile-art" => Some(match arg {
            "undo" => {
                let n = tile_art::undoable(&ui.folder());
                if n == 0 {
                    return Some(toast("There are no pictures of Turbine Hub's to take back."));
                }
                if ui.steam_running {
                    return Some(steam_closed_guide(ui.steam_unsure, "Undo"));
                }
                let mut p = Plan::new("Take the pictures back?", "Turbine Hub removes the pictures it drew for your library entries, and the icons it set. A picture you replaced since stays.", "Undo");
                p.danger = true;
                p.items.push((if n == 1 { "1 picture".to_string() } else { format!("{n} pictures") }, "Exactly the files Turbine Hub wrote, nothing else".into()));
                p.steps.push(Step::TileArtUndo);
                p.done = "Turbine Hub's pictures are out of the library".into();
                dialog(ui, p)
            }
            _ => {
                let todo = tile_art::to_fix(&ui.ctx.home);
                if todo.is_empty() {
                    return Some(toast("Every library entry has its pictures."));
                }
                if ui.steam_running {
                    return Some(steam_closed_guide(ui.steam_unsure, "Fix"));
                }
                let mut p = Plan::new(&format!("Draw pictures for {}?", entries_words(todo.len())), "These non-Steam games show as blank tiles. Turbine Hub draws a cover, a banner and a background for each, on this machine, from its name and the color of its icon. A picture you set yourself is never replaced, and Undo removes exactly what was drawn.", "Fix");
                for e in todo.iter().take(12) {
                    let n = e.missing.len() + usize::from(e.icon.trim().is_empty());
                    p.items.push((e.name.clone(), if n == 1 { "1 picture".to_string() } else { format!("{n} pictures") }));
                }
                if todo.len() > 12 {
                    p.items.push((format!("And {} more", todo.len() - 12), String::new()));
                }
                p.items.push(("Where they go".into(), if ui.veteran() { tile_art::list_file(&ui.ctx.home).map(|f| crate::steam_art::grid_dir(&f).to_string_lossy().into_owned()).unwrap_or_default() } else { "Steam's folder for library pictures. Steam shows them after its next start.".into() }));
                p.steps.push(Step::TileArt);
                p.done = format!("Pictures drawn for {}. Steam shows them after its next start.", entries_words(todo.len()));
                dialog(ui, p)
            }
        }),
        _ => None,
    }
}

/// These steps of a confirmed dialog. None for any other step. `steam`: asked right before (only the pictures need
/// the answer). `tick` gets a line for the player at each stage of a download and how far it is, and stops the
/// download by answering false (nothing half fetched stays).
pub fn run_step(step: &Step, home: &Path, folder: &Path, cache: &Path, client: &Client, steam: Steam, tick: crate::Tick) -> Option<Result<(), String>> {
    let root = || crate::steam::steam_roots(home).into_iter().next().ok_or_else(|| "Steam wasn't found on this machine.".to_string());
    Some(match step {
        Step::ProtonGet { tag } => root().and_then(|r| proton_get::install(client, cache, &r, folder, tag, crate::now(), tick).map(|_| ())),
        Step::ProtonRemove { name } => root().and_then(|r| proton_get::remove(&r, folder, name)),
        Step::TileArt => tile_art::fix(home, folder, steam).and_then(|done| if done.files == 0 { Err("Every library entry has its pictures already.".into()) } else { Ok(()) }),
        Step::TileArtUndo => tile_art::undo(home, folder, steam).map(|_| ()),
        _ => return None,
    })
}

//! Saves: one game's snapshots with Back up now, Restore and Undo, and the list of games to pick from. The
//! snapshots are `crate::snapshots` (the background helper takes one at each game start); Ludusavi does the copying
//! and `crate::saves` builds its commands.
use super::games::{self, Listed};
use super::plan::{Plan, Step, dialog, toast};
use super::{Ui, arr, obj, row, s, section, size_words, with};
use crate::json::Json;
use crate::saves::{self, GameRef, Job};
use crate::snapshots::{self, Snapshot};

/// When something happened, in words: "5 min ago", "Yesterday", and the date once it is over a month back.
pub fn when_words(now: u64, at: u64) -> String {
    let d = now.saturating_sub(at);
    match d {
        0..60 => "Just now".into(),
        60..3600 => format!("{} min ago", d / 60),
        3600..86_400 => format!("{} h ago", d / 3600),
        86_400..172_800 => "Yesterday".into(),
        172_800..2_678_400 => format!("{} days ago", d / 86_400),
        _ => date_words(at),
    }
}

/// The day a moment falls on, as "3 October 2026" (by the clock in Greenwich: a snapshot's age matters, not its
/// hour).
fn date_words(at: u64) -> String {
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    // Days since 1970 to a calendar day (the usual era arithmetic, 400-year cycles from 1 March 0000).
    let z = (at / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day} {} {year}", MONTHS[(month - 1) as usize])
}

pub(crate) fn game_ref(l: &Listed) -> GameRef {
    GameRef { id: games::game_id(&l.game), name: l.game.name.clone(), steam_id: (l.game.store == crate::Store::Steam).then(|| l.game.store_id.clone()) }
}

fn has_ludusavi(ui: &Ui) -> bool {
    super::store::is_installed(ui, saves::LUDUSAVI)
}

fn ludusavi_row() -> Json {
    with(with(row("open", &format!("store-item|{}", saves::LUDUSAVI), "button", "Ludusavi does the copying", "It knows where each game keeps its saves. Install it once, from the Store."), "icon", s("download")), "button", s("Open"))
}

fn snapshot_row(now: u64, id: &str, snap: &Snapshot) -> Json {
    let line = format!("{} \u{b7} {}", snap.kind.words(), size_words(snap.bytes));
    with(row("saves", &format!("restore|{id}|{}", snap.at), "button", &when_words(now, snap.at), &line), "button", s("Restore"))
}

/// The games to pick from, each with how many snapshots it has.
fn chooser(ui: &Ui) -> Json {
    let rows: Vec<Json> = games::listed(ui)
        .iter()
        .map(|l| {
            let id = games::game_id(&l.game);
            let line = match snapshots::list(&ui.data, &id).len() {
                0 => "No snapshots yet".to_string(),
                1 => "1 snapshot".into(),
                c => format!("{c} snapshots"),
            };
            row("open", &format!("saves|{id}"), "button", &l.game.name, &line)
        })
        .collect();
    let empty = if rows.is_empty() { "No games found. Install one in Steam or Heroic and it shows up here." } else { "" };
    obj! {"crumb": s("Games"), "title": s("Saves"), "intro": s("Pick a game to see its snapshots."), "sections": arr([with(with(section("", rows), "columns", super::n(2.0)), "empty", s(empty))]), "hints": arr([])}
}

/// One game's saves. `arg` is the game; without one, the list of games.
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    let Some(l) = games::find(ui, arg) else { return chooser(ui) };
    let id = games::game_id(&l.game);
    let now = crate::now();
    let ready = has_ludusavi(ui);
    let list = snapshots::list(&ui.data, &id);
    let mut first = Vec::new();
    if !ready {
        first.push(ludusavi_row());
    }
    first.push(with(with(row("saves", &format!("backup|{id}"), "button", "Back up now", "A snapshot of the saves as they are right now"), "icon", s("save")), "button", s("Back up")));
    if let Some(kept) = saves::undo_snapshot(&ui.data, &id) {
        first.push(with(with(row("saves", &format!("undo|{id}"), "button", "Undo the last restore", &format!("Puts back the saves from before it, kept {}", when_words(now, kept.at).to_lowercase())), "icon", s("restore")), "button", s("Undo")));
    }
    let rows: Vec<Json> = list.iter().map(|snap| snapshot_row(now, &id, snap)).collect();
    let auto = ui.settings().save_snapshots;
    let empty = match (rows.is_empty(), auto) {
        (false, _) => "",
        (true, true) => "No snapshots yet. One is taken each time the game starts, and Back up now takes one right away.",
        (true, false) => "No snapshots yet. Back up now takes one. Settings has the save time machine, which takes one each time a game starts.",
    };
    let title = match list.len() {
        0 => "Snapshots".to_string(),
        1 => "Snapshots \u{b7} 1".into(),
        c => format!("Snapshots \u{b7} {c}"),
    };
    obj! {
        "crumb": s("Games"),
        "title": s(format!("Saves of {}", l.game.name)),
        "intro": s(if auto { "A snapshot is taken each time the game starts. Restoring one keeps the current saves first, so Undo always leads back." } else { "Restoring a snapshot keeps the current saves first, so Undo always leads back." }),
        "sections": arr([section("", first), with(section(&title, rows), "empty", s(empty))]),
        "note": s(format!("The newest {} of each kind are kept. The game's saves change only when you restore one.", snapshots::KEEP)),
        "hints": arr([]),
    }
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    if action != "saves" {
        return None;
    }
    let mut parts = arg.splitn(3, '|');
    let (what, id, snapshot) = (parts.next()?, parts.next()?, parts.next().unwrap_or(""));
    let l = games::find(ui, id)?;
    if !has_ludusavi(ui) {
        return Some(with(toast("Saves need Ludusavi. Install it first."), "open", s(format!("store-item|{}", saves::LUDUSAVI))));
    }
    let game = game_ref(&l);
    let name = l.game.name.clone();
    let now = crate::now();
    Some(match what {
        "backup" => {
            let mut p = Plan::new("Back up saves now?", &format!("Ludusavi copies the saves of {name} into a new snapshot in Turbine Hub's folder. The game's own saves stay as they are."), "Back up");
            p.steps.push(Step::Saves(Job::Backup { game }));
            p.done = "Saves backed up".into();
            dialog(ui, p)
        }
        "restore" => {
            let Some(snap) = snapshots::list(&ui.data, id).into_iter().find(|x| x.at.to_string() == snapshot) else { return Some(with(toast("That snapshot isn't there any more."), "refresh", super::b(true))) };
            let mut p = Plan::new("Restore this snapshot?", &format!("The saves of {name} go back to how they were {}. The saves as they are now are kept first, so Undo brings them back.", when_words(now, snap.at).to_lowercase()), "Restore");
            p.danger = true;
            p.items.push(("Close the game first".into(), "It can't be running while its saves change.".into()));
            p.items.push((format!("Snapshot: {}", when_words(now, snap.at)), format!("{} \u{b7} {}", snap.kind.words(), size_words(snap.bytes))));
            p.items.push(("The current saves are kept first".into(), "As a snapshot of their own. Undo the last restore puts them back.".into()));
            p.items.push(("Files the game added since stay".into(), "Ludusavi puts back what the snapshot holds and leaves other save files in place.".into()));
            p.steps.push(Step::Saves(Job::Restore { game, at: snap.at }));
            p.done = "Save restored. Undo is on this page.".into();
            dialog(ui, p)
        }
        "undo" => {
            let Some(kept) = saves::undo_snapshot(&ui.data, id) else { return Some(with(toast("There is nothing to undo any more."), "refresh", super::b(true))) };
            let mut p = Plan::new("Undo the last restore?", &format!("The saves of {name} from before the restore come back. The saves as they are now are kept in turn."), "Undo");
            p.items.push(("Close the game first".into(), "It can't be running while its saves change.".into()));
            p.items.push((format!("Kept {}", when_words(now, kept.at).to_lowercase()), size_words(kept.bytes)));
            p.steps.push(Step::Saves(Job::Undo { game }));
            p.done = "The saves from before the restore are back".into();
            dialog(ui, p)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_read_plainly_and_old_ones_get_their_date() {
        let now = 1_794_455_000;
        assert_eq!(when_words(now, now - 5), "Just now");
        assert_eq!(when_words(now, now - 600), "10 min ago");
        assert_eq!(when_words(now, now - 7200), "2 h ago");
        assert_eq!(when_words(now, now - 90_000), "Yesterday");
        assert_eq!(when_words(now, now - 5 * 86_400), "5 days ago");
        assert_eq!(date_words(0), "1 January 1970");
        assert_eq!(date_words(951_782_400), "29 February 2000");
        assert_eq!(date_words(1_794_455_000), "12 November 2026");
        assert_eq!(when_words(now, now - 40 * 86_400), "3 October 2026");
    }
}

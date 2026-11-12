//! Freeing what was ticked: the plan the player sees, the step that does it, and its Undo.
//!
//! Nothing is taken on trust from the look: right before each thing goes, Steam's lists are read again and the
//! thing has to be exactly what it was offered as (still a leftover, still in its own folder and not behind a
//! link, Steam closed for anything inside a library). What fails a check is left alone and named with the reason.
//!
//! One thing inside a library may go while Steam runs (in Game Mode it always runs): what a game that is no longer
//! installed left behind (its shader cache, its Windows files, its unfinished download). Steam has no use for
//! those, and right before each one goes Steam's lists are read once more: still no manifest in any library, not
//! named in the library list, not a non-Steam game, and no game with that id running.

use super::{Finding, Group, How, Host, Kind, Report, World, downloads_dir, drives, old_steam_files, plain_folder, reset_leftover, unused_step};
use crate::compat_tool;
use crate::files::fs::{Fs, NodeKind, clean as clean_path, remove_tree};
use crate::files::places::Places;
use crate::files::{Trashed, When, size_words, trash};
use crate::json::{self, Json};
use crate::shortcuts::Steam;
use crate::ui::{Plan, Step, arr, n, obj, s};
use std::path::{Path, PathBuf};

/// Where a look or a cleaning works: the disk, the machine's places, Turbine Hub's own two folders (as the app
/// finds them) and the time on the player's clock.
pub struct Site<'a> {
    pub fs: &'a dyn Fs,
    pub places: &'a Places,
    /// Turbine Hub's own folder: the launch tool's picks, and the records of the last cleaning.
    pub folder: &'a Path,
    /// Turbine Hub's data folder: play sessions and save snapshots.
    pub data: &'a Path,
    pub when: When,
}

/// One ticked finding, as the step carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub how: How,
    pub title: String,
    pub paths: Vec<PathBuf>,
    pub inside: PathBuf,
    pub app_id: String,
    /// Inside a Steam library's steamapps.
    pub steam: bool,
    /// Offered because its game isn't installed: left alone when the game is back.
    pub orphan: bool,
}

/// The storage saver's work in a confirmed dialog.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// Frees these.
    Clean(Vec<Item>),
    /// Puts back what the last cleaning moved to the Recycle Bin.
    Undo,
}

/// Asks the host one question (a command line): its exit code and what it printed, or None when it couldn't run.
pub type Ask<'a> = &'a mut dyn FnMut(&[String]) -> Option<(i32, String)>;

const STEAM_OPEN: &str = "Close Steam first: Steam uses its library folders while it runs";
const STEAM_UNSURE: &str = "Turbine Hub couldn't check whether Steam is running, so it left Steam's folders alone. Close Steam, then try again";
const GAME_OPEN: &str = "The game is running. Close it first";
const GAME_UNSURE: &str = "Turbine Hub couldn't check whether the game is running, so it left this alone";
const LINKED: &str = "It is a link to another place, or sits behind one, so Turbine Hub leaves it alone";
const CHANGED: &str = "It changed since the look, so it was left alone. Look again";
const BACK: &str = "The game is in Steam again, so its files stay";

/// Why this thing can't go right now, from what the host says. Three answers about Steam, as everywhere: a
/// question that couldn't be asked is not a "no". `orphan`: it is what a game that is no longer installed left
/// behind, which may go while Steam runs once it is known that no game with its id is running.
pub(crate) fn refusal(steam_item: bool, orphan: bool, app_id: &str, steam: Steam, running: Option<&[String]>) -> Option<&'static str> {
    if !app_id.is_empty() && running.is_some_and(|r| r.iter().any(|x| x == app_id)) {
        return Some(GAME_OPEN);
    }
    if steam_item {
        // With Steam closed none of its games runs.
        return match steam {
            Steam::Closed => None,
            Steam::Running if orphan && !app_id.is_empty() => running.is_none().then_some(GAME_UNSURE),
            Steam::Running => Some(STEAM_OPEN),
            Steam::Unknown => Some(STEAM_UNSURE),
        };
    }
    (!app_id.is_empty() && running.is_none() && steam != Steam::Closed).then_some(GAME_UNSURE)
}

fn record_file(folder: &Path) -> PathBuf {
    folder.join("storage-last.json")
}

fn undo_file(folder: &Path) -> PathBuf {
    folder.join("storage-undo.json")
}

fn item_of(f: &Finding) -> Item {
    Item { id: f.id.clone(), kind: f.kind, how: f.how, title: f.title.clone(), paths: f.paths.clone(), inside: f.inside.clone(), app_id: f.app_id.clone(), steam: f.steam, orphan: f.orphan() }
}

/// The dialog for the ticked findings: every thing by name with its size and what happens to it (erased for good,
/// or to the Recycle Bin), what is left alone and why, and the room before and after. Refused with a plain reason
/// when nothing ticked can go. What is in use is never in a plan, ticked or not.
pub fn plan(report: &Report, ticked: &[String], host: &Host) -> Result<Plan, String> {
    let chosen: Vec<&Finding> = report.chosen(ticked).collect();
    if chosen.is_empty() {
        return Err("Nothing is ticked. Tick what should go first.".into());
    }
    let (mut go, mut left): (Vec<&Finding>, Vec<(&Finding, &str)>) = (Vec::new(), Vec::new());
    for f in chosen {
        match refusal(f.steam, f.orphan(), &f.app_id, host.steam, host.running.as_deref()) {
            Some(why) => left.push((f, why)),
            None => go.push(f),
        }
    }
    let Some(_) = go.first() else {
        return Err(format!("{}.", left[0].1));
    };
    let ids: Vec<String> = go.iter().map(|f| f.id.clone()).collect();
    let mut plan = Plan::new("Free up space", "", &report.free_words(&ids));
    // Something that may hold saves goes for good: Cancel is the highlighted button.
    plan.danger = go.iter().any(|f| f.group == Group::Check && f.how != How::Bin);
    let sum = |how: &[How]| go.iter().filter(|f| how.contains(&f.how)).fold(super::Size::default(), |t, f| t.plus(f.size));
    let mut text: Vec<String> = Vec::new();
    let (gone, binned) = (sum(&[How::Erase, How::EmptyBin]), sum(&[How::Bin]));
    if go.iter().any(|f| matches!(f.how, How::Erase | How::EmptyBin)) {
        text.push(format!("{} is erased for good.", cap(&gone.words())));
    }
    if go.iter().any(|f| f.how == How::Bin) {
        text.push(format!("{} goes to the Recycle Bin, where Undo can put it back. Its room comes back when the bin is emptied.", cap(&binned.words())));
    }
    if go.iter().any(|f| f.how == How::Flatpak) {
        text.push("Flatpak removes the app parts nothing uses.".into());
    }
    for (i, d) in report.drives.iter().enumerate() {
        let now = go.iter().filter(|f| f.drive == i && f.how != How::Bin).fold(super::Size::default(), |t, f| t.plus(f.size));
        if now.bytes > 0 && d.total > 0 {
            text.push(format!("{} has {} free now, and about {} after.", d.label, size_words(d.free), size_words(d.free + now.bytes)));
        }
    }
    if !left.is_empty() {
        text.push(format!("{} left alone. {}.", if left.len() == 1 { "1 thing is".to_string() } else { format!("{} things are", left.len()) }, left[0].1));
    }
    plan.text = text.join(" ");
    for f in &go {
        plan.items.push((format!("{} ({})", f.title, f.size.words()), f.how.words().to_string()));
    }
    for (f, why) in &left {
        plan.items.push((format!("Left alone: {}", f.title), format!("{why}.")));
    }
    plan.steps.push(Step::Storage(Job::Clean(go.iter().filter(|f| f.how != How::Flatpak).map(|f| item_of(f)).collect())));
    if go.iter().any(|f| f.how == How::Flatpak) {
        plan.steps.push(Step::Host(unused_step()));
    }
    plan.done = "Space freed.".into();
    Ok(plan)
}

fn cap(words: &str) -> String {
    let mut c = words.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Is this thing, right now, exactly what it was offered as?
fn still_right(item: &Item, world: &World, site: &Site) -> Result<(), &'static str> {
    let (fs, places) = (site.fs, site.places);
    let Some(first) = item.paths.first() else { return Err(CHANGED) };
    let name = first.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    let in_library = |sub: &[&str]| world.libs.iter().any(|l| sub.iter().any(|x| l.join("steamapps").join(x) == item.inside));
    if item.kind == Kind::Snapshots {
        // Turbine Hub's own folder: only a snapshot of a game that isn't that game's newest one.
        let games = crate::snapshots::games(site.data);
        for p in &item.paths {
            let game = games.iter().map(|g| &g.0).find(|g| Some(crate::snapshots::game_dir(site.data, g).as_path()) == p.parent()).ok_or(CHANGED)?;
            let list = crate::snapshots::list(site.data, game);
            if !list.iter().skip(1).any(|x| x.dir == *p) || std::fs::symlink_metadata(p).is_ok_and(|m| !m.is_dir()) {
                return Err(CHANGED);
            }
        }
        return Ok(());
    }
    if !plain_folder(fs, &item.inside) {
        return Err(LINKED);
    }
    let mut there = 0;
    for p in &item.paths {
        let own_proton = item.kind == Kind::Proton && item.steam;
        let parent_ok = p.parent() == Some(item.inside.as_path()) || (own_proton && p.parent() == Some(item.inside.join("common").as_path()) && plain_folder(fs, &item.inside.join("common")));
        if clean_path(p) != *p || !parent_ok {
            return Err(LINKED);
        }
        match fs.meta(p) {
            Ok(m) if m.kind == NodeKind::Link || m.kind == NodeKind::Other => return Err(LINKED),
            Ok(_) => there += 1,
            Err(_) => {}
        }
    }
    if there == 0 {
        return Err("It isn't there any more");
    }
    if item.orphan && (world.installed(&item.app_id) || world.shortcuts.contains_key(&item.app_id)) {
        return Err(BACK);
    }
    let ok = match item.kind {
        Kind::ShaderCache => in_library(&["shadercache"]) && name == item.app_id && super::all_digits(&name),
        Kind::Compatdata => in_library(&["compatdata"]) && name == item.app_id && super::all_digits(&name),
        Kind::ResetBackup => reset_leftover(&name).is_some() && world.libs.iter().any(|l| l.join("steamapps/compatdata").join(&item.app_id) == item.inside) && super::all_digits(&item.app_id),
        Kind::Download => in_library(&["downloading", "temp"]) && super::all_digits(&name),
        Kind::Proton => {
            let found = world.protons.iter().find(|(_, p)| fs.real(&p.dir).as_ref() == Some(first));
            let manifests = item.paths.iter().skip(1).all(|p| item.steam && world.manifests.iter().any(|m| m.file == *p));
            let place = if item.steam { world.libs.iter().any(|l| l.join("steamapps") == item.inside) } else { world.roots.iter().any(|(r, _)| compat_tool::tools_dir(r) == item.inside) };
            match found {
                Some((root, p)) if world.proton_user(*root, p).is_some() => return Err("Something uses it now, so it stays"),
                Some(_) => manifests && place && item.paths.len() <= 2,
                None => false,
            }
        }
        Kind::SteamLogs => world.roots.iter().any(|(r, _)| old_steam_files(fs, r, site.when.secs).iter().any(|(dir, files)| *dir == item.inside && item.paths.iter().all(|p| fs.meta(p).is_err() || files.contains(p)))),
        Kind::FlatpakData => item.inside == places.who.home.join(".var/app") && !places.apps.iter().any(|a| a.id == name) && !name.eq_ignore_ascii_case(super::OWN_APP) && places.locked(first).is_none(),
        Kind::RecycleBin => item.paths.len() == 1 && trash::bins(fs, places).contains(first),
        Kind::DownloadsFile => downloads_dir(places).as_deref() == Some(item.inside.as_path()) && fs.meta(first).is_ok_and(|m| m.kind == NodeKind::File),
        Kind::FlatpakUnused | Kind::Snapshots => false,
    };
    if ok { Ok(()) } else { Err(CHANGED) }
}

/// One thing in the Recycle Bin, as the Undo record keeps it.
#[derive(Debug, Clone, PartialEq)]
struct Binned {
    title: String,
    trash: PathBuf,
    name: String,
    original: PathBuf,
    /// The Steam app whose leftover it is ("" for anything else): it may go back while Steam runs, as it went.
    orphan: String,
}

impl Binned {
    fn trashed(&self) -> Trashed {
        Trashed { trash: self.trash.clone(), name: self.name.clone(), at: self.trash.join("files").join(&self.name), original: self.original.clone(), deleted: String::new() }
    }
    fn to_json(&self) -> Json {
        obj! {"title": s(&self.title), "trash": s(self.trash.to_string_lossy()), "name": s(&self.name), "original": s(self.original.to_string_lossy()), "orphan": s(&self.orphan)}
    }
}

fn read_undo(folder: &Path) -> Vec<Binned> {
    let Some(j) = std::fs::read_to_string(undo_file(folder)).ok().and_then(|t| json::parse(&t).ok()) else { return Vec::new() };
    j.get("items").map(Json::as_array).unwrap_or(&[]).iter().filter_map(|x| Some(Binned { title: x.str_at("title")?.into(), trash: x.str_at("trash")?.into(), name: x.str_at("name")?.into(), original: x.str_at("original")?.into(), orphan: x.str_at("orphan").unwrap_or("").into() })).collect()
}

fn write_undo(folder: &Path, at: u64, items: &[Binned]) {
    if items.is_empty() {
        let _ = std::fs::remove_file(undo_file(folder));
    } else {
        let _ = crate::write_atomic(&undo_file(folder), (obj! {"at": n(at as f64), "items": arr(items.iter().map(Binned::to_json))}.to_text() + "\n").as_bytes());
    }
}

fn free_one(item: &Item, site: &Site, binned: &mut Vec<Binned>) -> Result<(), String> {
    let (fs, places) = (site.fs, site.places);
    match (item.kind, item.how) {
        (Kind::Snapshots, How::Erase) => {
            for game in crate::snapshots::games(site.data) {
                for snap in crate::snapshots::list(site.data, &game.0).iter().skip(1).filter(|x| item.paths.contains(&x.dir)) {
                    crate::snapshots::remove(snap);
                }
            }
            Ok(())
        }
        (_, How::Erase) => {
            for p in &item.paths {
                // Each one looked at again: what is gone is fine, a link is never followed and never offered.
                match fs.meta(p) {
                    Ok(m) if m.kind == NodeKind::File || m.kind == NodeKind::Folder => remove_tree(fs, p).map_err(|e| format!("{} couldn't be erased ({})", item.title, crate::files::guard::io_words(&e)))?,
                    _ => {}
                }
            }
            Ok(())
        }
        (_, How::Bin) => {
            for p in item.paths.iter().filter(|p| fs.meta(p).is_ok()) {
                let t = trash::put(fs, places, p, site.when).map_err(|e| e.trim_end_matches('.').to_string())?;
                binned.push(Binned { title: item.title.clone(), trash: t.trash, name: t.name, original: t.original, orphan: if item.orphan { item.app_id.clone() } else { String::new() } });
            }
            Ok(())
        }
        (Kind::RecycleBin, How::EmptyBin) => match trash::empty_bin(fs, places, &item.paths[0]).1.first() {
            Some(problem) => Err(problem.trim_end_matches('.').to_string()),
            None => Ok(()),
        },
        _ => Err(CHANGED.into()),
    }
}

/// Is Steam running, asked on the host now.
fn steam_now(ask: Ask) -> Steam {
    Steam::from_answer(ask(&Steam::question()).as_ref().map(|(code, out)| (*code, out.as_str())))
}

/// Runs the storage saver's step of a confirmed dialog. `ask` puts a question to the host (is Steam running, which
/// games run): both are asked right before anything is touched, never taken from the look. Answers what was done
/// in plain words; an error when nothing could be done.
pub fn run(job: &Job, site: &Site, say: &dyn Fn(String), ask: Ask) -> Result<String, String> {
    let (fs, places) = (site.fs, site.places);
    let world = World::read(fs, places, site.folder, site.data);
    match job {
        Job::Clean(items) => {
            let before: Vec<(String, PathBuf, u64)> = drives(fs, places).into_iter().filter(|d| d.total > 0).map(|d| (d.label, d.mount, d.free)).collect();
            let steam = if items.iter().any(|i| i.steam || !i.app_id.is_empty()) { steam_now(&mut *ask) } else { Steam::Closed };
            let running: Option<Vec<String>> = match steam {
                Steam::Closed => Some(Vec::new()),
                Steam::Unknown => None,
                // pgrep ends with 1 when it found nothing.
                Steam::Running => ask(&crate::agent::games_question()).filter(|(code, _)| matches!(code, 0 | 1)).map(|(_, out)| crate::agent::running_games(&out)),
            };
            let (mut binned, mut skipped, mut done): (Vec<Binned>, Vec<(String, String)>, usize) = (Vec::new(), Vec::new(), 0);
            for item in items {
                say(format!("Freeing: {}", item.title));
                // While Steam runs its lists can change under the cleaning: a leftover is looked up in them again
                // right before it goes, not in what was read when the cleaning began.
                let fresh = (item.orphan && steam != Steam::Closed).then(|| World::read(fs, places, site.folder, site.data));
                let result = match refusal(item.steam, item.orphan, &item.app_id, steam, running.as_deref()) {
                    Some(why) => Err(why.to_string()),
                    None => still_right(item, fresh.as_ref().unwrap_or(&world), site).map_err(String::from).and_then(|_| free_one(item, site, &mut binned)),
                };
                match result {
                    Ok(()) => done += 1,
                    Err(why) => skipped.push((item.title.clone(), why)),
                }
            }
            if !binned.is_empty() {
                write_undo(site.folder, site.when.secs, &binned);
            }
            let record = obj! {
                "at": n(crate::now() as f64),
                "before": arr(before.iter().map(|(label, mount, free)| obj! {"label": s(label), "mount": s(mount.to_string_lossy()), "free": n(*free as f64)})),
                "done": n(done as f64),
                "binned": n(binned.len() as f64),
                "skipped": arr(skipped.iter().map(|(title, why)| obj! {"title": s(title), "why": s(why)})),
            };
            let _ = crate::write_atomic(&record_file(site.folder), (record.to_text() + "\n").as_bytes());
            match skipped.first() {
                Some((title, why)) if done == 0 => Err(format!("Nothing was freed. {title}: {why}.")),
                Some(_) => Ok(format!("{done} freed, {} left alone.", skipped.len())),
                None => Ok(format!("{done} freed.")),
            }
        }
        Job::Undo => {
            let items = read_undo(site.folder);
            if items.is_empty() {
                return Err("There is nothing to put back.".into());
            }
            // Putting something back into a Steam library needs Steam closed just as taking it out did; what a
            // removed game left behind goes back while Steam runs, as long as the game is still gone.
            let in_steam = |b: &Binned| world.libs.iter().any(|l| b.original.starts_with(l.join("steamapps")));
            let still_gone = |b: &Binned| !b.orphan.is_empty() && !world.installed(&b.orphan) && !world.shortcuts.contains_key(&b.orphan);
            let steam = if items.iter().any(in_steam) { steam_now(&mut *ask) } else { Steam::Closed };
            let running: Option<Vec<String>> = if steam == Steam::Running && items.iter().any(|b| in_steam(b) && still_gone(b)) { ask(&crate::agent::games_question()).filter(|(code, _)| matches!(code, 0 | 1)).map(|(_, out)| crate::agent::running_games(&out)) } else { None };
            let (mut kept, mut problems, mut back) = (Vec::new(), Vec::new(), 0);
            for b in items {
                say(format!("Putting back: {}", b.title));
                let gone = still_gone(&b);
                let result = match refusal(in_steam(&b), gone, if gone { &b.orphan } else { "" }, steam, running.as_deref()) {
                    Some(why) => Err(format!("{why}.")),
                    None => trash::restore(fs, places, &b.trashed(), false).map(|_| ()),
                };
                match result {
                    Ok(()) => back += 1,
                    Err(why) => {
                        problems.push(why);
                        kept.push(b);
                    }
                }
            }
            write_undo(site.folder, site.when.secs, &kept);
            match problems.first() {
                Some(why) if back == 0 => Err(why.clone()),
                Some(why) => Ok(format!("{back} put back. {} still in the Recycle Bin: {why}", kept.len())),
                None => Ok(format!("{back} put back.")),
            }
        }
    }
}

/// `run` on the machine the app runs on, for the app's step runner: reads the places, asks the clock's zone and
/// does the job.
pub fn run_here(job: &Job, home: &Path, folder: &Path, data: &Path, say: &dyn Fn(String), ask: Ask) -> Result<String, String> {
    let here = super::Here::read(Path::new("/"), home, folder, data);
    let zone = ask(&["date".to_string(), "+%z".to_string()]).map(|(_, out)| out).unwrap_or_default();
    run(job, &here.site(When::from_date_z(crate::now(), &zone)), say, ask)
}

/// The dialog that puts back what the last cleaning moved to the Recycle Bin. None when there is nothing of it
/// left there to put back.
pub fn undo_plan(fs: &dyn Fs, folder: &Path) -> Option<Plan> {
    let items: Vec<Binned> = read_undo(folder).into_iter().filter(|b| fs.meta(&b.trashed().at).is_ok()).collect();
    if items.is_empty() {
        return None;
    }
    let mut plan = Plan::new("Put back what was freed", "Everything the last cleaning moved to the Recycle Bin goes back to where it was. What it erased for good can't come back, and Steam builds that again by itself.", "Put back");
    for b in &items {
        if !plan.items.iter().any(|(t, _)| *t == b.title) {
            plan.items.push((b.title.clone(), format!("Back to {}", b.original.display())));
        }
    }
    plan.steps.push(Step::Storage(Job::Undo));
    plan.done = "Put back.".into();
    Some(plan)
}

/// The note for a cleaning that just ended, with the room each drive really had before and has now. None for
/// any other plan (its own `done` is the note then).
pub fn done_words(steps: &[Step], folder: &Path, fs: &dyn Fs) -> Option<String> {
    if !steps.iter().any(|x| matches!(x, Step::Storage(Job::Clean(_)))) {
        return None;
    }
    let j = json::parse(&std::fs::read_to_string(record_file(folder)).ok()?).ok()?;
    let num = |j: &Json, k: &str| j.get(k).and_then(Json::as_f64).unwrap_or(0.0) as u64;
    let mut freed = 0u64;
    let mut rooms: Vec<String> = Vec::new();
    for d in j.get("before").map(Json::as_array).unwrap_or(&[]) {
        let (Some(label), Some(mount)) = (d.str_at("label"), d.str_at("mount")) else { continue };
        let before = num(d, "free");
        let Some(now) = fs.space(Path::new(mount)).map(|x| x.free) else { continue };
        // Less than a megabyte either way is other programs writing, not this cleaning.
        if now > before + (1 << 20) {
            freed += now - before;
            rooms.push(format!("{label} had {} free and has {} now.", size_words(before), size_words(now)));
        }
    }
    let binned = num(&j, "binned");
    let mut out = vec![if freed > 0 { format!("Freed {}.", size_words(freed)) } else if binned > 0 { "Moved to the Recycle Bin.".to_string() } else { "Done.".to_string() }];
    out.extend(rooms);
    if binned > 0 {
        out.push(format!("{} in the Recycle Bin: Undo puts {} back, and emptying the bin frees the room.", if binned == 1 { "1 thing waits".to_string() } else { format!("{binned} things wait") }, if binned == 1 { "it" } else { "them" }));
    }
    let skipped = j.get("skipped").map(Json::as_array).unwrap_or(&[]);
    if let Some(first) = skipped.first() {
        let more = if skipped.len() > 1 { format!(" (and {} more)", skipped.len() - 1) } else { String::new() };
        out.push(format!("Left alone: {}{more}. {}.", first.str_at("title").unwrap_or(""), first.str_at("why").unwrap_or("")));
    }
    Some(out.join(" "))
}

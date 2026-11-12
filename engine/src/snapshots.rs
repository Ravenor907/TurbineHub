//! The save time machine's snapshots: one folder per snapshot, each a complete Ludusavi backup of one game's saves,
//! so any of them can be put back on its own.
//!
//! Layout, under Turbine Hub's data folder:
//!
//! ```text
//! snapshots/<game>/<time>/               one snapshot (time: seconds since 1970)
//! snapshots/<game>/<time>/snapshot.json  what it is: game id, name, time, kind, size, file count
//! snapshots/<game>/<time>/...            Ludusavi's backup (mapping.yaml and the save files)
//! snapshots/<game>/<time>.part/          a snapshot still being written; never listed
//! ```
//!
//! `<game>` is the game's id ("steam:1245620") with everything but letters, digits, dots and dashes turned into
//! underscores. The background helper takes a snapshot at each game start (`Kind::Launch`); the app may take one by
//! hand (`Kind::Manual`) and one right before putting a snapshot back (`Kind::BeforeRestore`), which is what Undo
//! restores.
//!
//! Taking one: `due`, then `begin` for the folder, run `find_args` (Steam games) and `backup_args` on the host, then
//! `finish` with Ludusavi's answer. Putting one back: run `restore_args`. Nothing here runs a command.

use crate::json::{self, Json};
use crate::ui::{n, obj, s};
use std::fs;
use std::path::{Path, PathBuf};

pub const LUDUSAVI: &str = "com.github.mtkennerly.ludusavi";
/// How many snapshots of each kind a game keeps.
pub const KEEP: usize = 5;
/// At most one launch snapshot per game in this many seconds.
pub const EVERY_SECS: u64 = 600;
/// Snapshots bigger than this aren't compared file by file with the one before.
const COMPARE_UP_TO: u64 = 256 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Taken by the background helper when the game started.
    Launch,
    /// Taken by the player.
    Manual,
    /// Taken right before a snapshot was put back, so that can be undone.
    BeforeRestore,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Launch => "launch",
            Kind::Manual => "manual",
            Kind::BeforeRestore => "before-restore",
        }
    }
    fn from_key(k: &str) -> Kind {
        match k {
            "manual" => Kind::Manual,
            "before-restore" => Kind::BeforeRestore,
            _ => Kind::Launch,
        }
    }
    /// A few words for a list.
    pub fn words(self) -> &'static str {
        match self {
            Kind::Launch => "When the game started",
            Kind::Manual => "Taken by hand",
            Kind::BeforeRestore => "Before a restore",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The game's id, as everywhere in Turbine Hub ("steam:1245620").
    pub game: String,
    /// The game's name as Ludusavi knows it: the name `restore_args` passes.
    pub name: String,
    /// When it was taken, seconds since 1970.
    pub at: u64,
    pub kind: Kind,
    /// Size of the saves in it.
    pub bytes: u64,
    pub files: u32,
    /// The snapshot's folder.
    pub dir: PathBuf,
}

/// Where every snapshot lives, under Turbine Hub's data folder.
pub fn root(data: &Path) -> PathBuf {
    data.join("snapshots")
}

fn safe(game: &str) -> String {
    game.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect()
}

/// One game's folder of snapshots.
pub fn game_dir(data: &Path, game: &str) -> PathBuf {
    root(data).join(safe(game))
}

fn read_one(dir: &Path) -> Option<Snapshot> {
    let j = json::parse(&fs::read_to_string(dir.join("snapshot.json")).ok()?).ok()?;
    let num = |k: &str| j.get(k).and_then(Json::as_f64).filter(|v| *v >= 0.0);
    Some(Snapshot { game: j.str_at("game")?.to_string(), name: j.str_at("name")?.to_string(), at: num("at")? as u64, kind: Kind::from_key(j.str_at("kind").unwrap_or("")), bytes: num("bytes").unwrap_or(0.0) as u64, files: num("files").unwrap_or(0.0) as u32, dir: dir.to_path_buf() })
}

fn in_folder(folder: &Path) -> Vec<Snapshot> {
    let mut v: Vec<Snapshot> = fs::read_dir(folder).map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())).filter_map(|e| read_one(&e.path())).collect()).unwrap_or_default();
    v.sort_by_key(|x| std::cmp::Reverse(x.at));
    v
}

/// One game's snapshots, newest first. Half-written ones and folders without their `snapshot.json` are left out.
pub fn list(data: &Path, game: &str) -> Vec<Snapshot> {
    in_folder(&game_dir(data, game)).into_iter().filter(|x| x.game == game).collect()
}

/// Every game that has snapshots: (game id, name, how many, when the newest was taken), newest first.
pub fn games(data: &Path) -> Vec<(String, String, usize, u64)> {
    let mut out: Vec<(String, String, usize, u64)> = fs::read_dir(root(data))
        .map(|d| {
            d.flatten()
                .filter_map(|e| {
                    let all = in_folder(&e.path());
                    let first = all.first()?;
                    Some((first.game.clone(), first.name.clone(), all.len(), first.at))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));
    out
}

/// May the background helper take a launch snapshot of this game now? Not within `EVERY_SECS` of its last one.
pub fn due(data: &Path, game: &str, now: u64) -> bool {
    list(data, game).iter().filter(|x| x.kind == Kind::Launch).all(|x| now < x.at || now - x.at >= EVERY_SECS)
}

/// The folder a new snapshot is written into (not made yet). `finish` gives it its final name.
pub fn begin(data: &Path, game: &str, now: u64) -> PathBuf {
    game_dir(data, game).join(format!("{now}.part"))
}

fn ludusavi(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host", "flatpak", "run", LUDUSAVI].iter().chain(args).map(|x| x.to_string()).collect()
}

/// Asks Ludusavi what it calls a Steam game. Its answer goes to `title_from_find`.
pub fn find_args(steam_id: &str) -> Option<Vec<String>> {
    (!steam_id.is_empty() && steam_id.bytes().all(|b| b.is_ascii_digit())).then(|| ludusavi(&["find", "--api", "--steam-id", steam_id]))
}

/// The game's name in Ludusavi's answer to `find_args`.
pub fn title_from_find(stdout: &str) -> Option<String> {
    json::parse(stdout).ok()?.get("games")?.entries().first().map(|(name, _)| name.clone())
}

/// Backs one game's saves up into `dir` (from `begin`). `title` is Ludusavi's name for the game. None for a name
/// Ludusavi could mistake for an option.
pub fn backup_args(dir: &Path, title: &str) -> Option<Vec<String>> {
    (!title.is_empty() && !title.starts_with('-')).then(|| ludusavi(&["backup", "--api", "--force", "--path", &dir.to_string_lossy(), "--", title]))
}

/// Puts one snapshot's saves back where the game keeps them. Saves made since are replaced, so take a
/// `Kind::BeforeRestore` snapshot first.
pub fn restore_args(snap: &Snapshot) -> Option<Vec<String>> {
    (!snap.name.is_empty() && !snap.name.starts_with('-')).then(|| ludusavi(&["restore", "--api", "--force", "--path", &snap.dir.to_string_lossy(), "--", &snap.name]))
}

/// What Ludusavi's `backup --api` answer says was saved: (name, bytes, files). None when it found no saves.
pub fn parse_backup(stdout: &str) -> Option<(String, u64, u32)> {
    let start = stdout.find('{')?;
    let j = json::parse(&stdout[start..]).ok()?;
    let (name, game) = j.get("games")?.entries().first()?;
    let files = game.get("files").map(Json::entries).unwrap_or(&[]);
    let kept: Vec<&Json> = files.iter().map(|(_, f)| f).filter(|f| f.get("ignored").and_then(Json::as_bool) != Some(true) && f.get("failed").and_then(Json::as_bool) != Some(true)).collect();
    let bytes: f64 = kept.iter().filter_map(|f| f.get("bytes").and_then(Json::as_f64)).sum();
    let registry = game.get("registry").map(Json::entries).unwrap_or(&[]).len();
    (!kept.is_empty() || registry > 0).then(|| (name.clone(), bytes as u64, (kept.len() + registry) as u32))
}

/// Every file under `dir` except Turbine Hub's own note and Ludusavi's index (it holds the time): path and size.
fn tree(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            let Ok(meta) = fs::symlink_metadata(&p) else { continue };
            if meta.is_dir() {
                todo.push(p);
            } else if meta.is_file() && !["snapshot.json", "mapping.yaml"].contains(&e.file_name().to_string_lossy().as_ref()) {
                out.push((p.strip_prefix(dir).unwrap_or(&p).to_path_buf(), meta.len()));
            }
        }
    }
    out.sort();
    out
}

/// Do two snapshot folders hold the same saves?
fn same_saves(a: &Path, b: &Path) -> bool {
    let (ta, tb) = (tree(a), tree(b));
    if ta != tb || ta.iter().map(|(_, len)| len).sum::<u64>() > COMPARE_UP_TO {
        return false;
    }
    ta.iter().all(|(rel, _)| matches!((fs::read(a.join(rel)), fs::read(b.join(rel))), (Ok(x), Ok(y)) if x == y))
}

/// Removes all but the newest `keep` of each kind.
fn prune(data: &Path, game: &str, keep: usize) {
    for kind in [Kind::Launch, Kind::Manual, Kind::BeforeRestore] {
        for old in list(data, game).iter().filter(|x| x.kind == kind).skip(keep) {
            remove(old);
        }
    }
}

/// Completes a snapshot after Ludusavi ran: `dir` is the folder from `begin`, `stdout` Ludusavi's answer. Answers
/// the snapshot, or None when there was nothing to keep: Ludusavi found no saves, or (launch snapshots only) the
/// saves are the same as in the newest snapshot. Older snapshots beyond `keep` of the same kind are removed.
pub fn finish(data: &Path, game: &str, kind: Kind, at: u64, dir: &Path, stdout: &str, keep: usize) -> Result<Option<Snapshot>, String> {
    let drop = |dir: &Path| {
        let _ = fs::remove_dir_all(dir);
    };
    let Some((name, bytes, files)) = parse_backup(stdout) else {
        drop(dir);
        return Ok(None);
    };
    if !dir.is_dir() {
        return Err("Ludusavi didn't write the snapshot.".into());
    }
    let before = list(data, game);
    if kind == Kind::Launch && before.first().is_some_and(|last| same_saves(&last.dir, dir)) {
        drop(dir);
        return Ok(None);
    }
    let text = obj! {"game": s(game), "name": s(&name), "at": n(at as f64), "kind": s(kind.key()), "bytes": n(bytes as f64), "files": n(files as f64)}.to_text() + "\n";
    let done = game_dir(data, game).join(at.to_string());
    let moved = crate::write_atomic(&dir.join("snapshot.json"), text.as_bytes()).and_then(|_| {
        // A second snapshot in the same second replaces the first.
        let _ = fs::remove_dir_all(&done);
        fs::rename(dir, &done)
    });
    if let Err(e) = moved {
        drop(dir);
        return Err(format!("Couldn't keep the snapshot: {e}"));
    }
    prune(data, game, keep.max(1));
    Ok(Some(Snapshot { game: game.into(), name, at, kind, bytes, files, dir: done }))
}

/// Removes one snapshot. Only ever a folder directly inside a game's snapshot folder.
pub fn remove(snap: &Snapshot) {
    if snap.dir.join("snapshot.json").is_file() {
        let _ = fs::remove_dir_all(&snap.dir);
    }
}

/// Half-written snapshots left behind by a run that was cut short.
pub fn clear_unfinished(data: &Path) {
    for game in fs::read_dir(root(data)).into_iter().flatten().flatten() {
        for e in fs::read_dir(game.path()).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().ends_with(".part") {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-snapshots-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    /// What Ludusavi does: writes the saves under the folder and prints its answer.
    fn ludusavi_ran(dir: &Path, save: &str) -> String {
        fs::create_dir_all(dir.join("Hades II/drive-0/saves")).unwrap();
        fs::write(dir.join("Hades II/mapping.yaml"), format!("when: {:?}", std::time::Instant::now())).unwrap();
        fs::write(dir.join("Hades II/drive-0/saves/profile1.sav"), save).unwrap();
        format!(r#"{{"overall":{{"totalGames":1,"totalBytes":{0},"processedGames":1,"processedBytes":{0}}},"games":{{"Hades II":{{"decision":"Processed","change":"New","files":{{"/home/deck/saves/profile1.sav":{{"change":"New","bytes":{0}}},"/home/deck/saves/skip.tmp":{{"ignored":true,"bytes":9}}}},"registry":{{}}}}}}}}"#, save.len())
    }

    fn take(data: &Path, kind: Kind, at: u64, save: &str) -> Option<Snapshot> {
        let dir = begin(data, "steam:1145360", at);
        let out = ludusavi_ran(&dir, save);
        finish(data, "steam:1145360", kind, at, &dir, &out, KEEP).unwrap()
    }

    #[test]
    fn command_lines_are_fixed_and_refuse_odd_names() {
        let dir = Path::new("/data/snapshots/steam_1/100.part");
        assert_eq!(backup_args(dir, "Hades II").unwrap().join(" "), "flatpak-spawn --host flatpak run com.github.mtkennerly.ludusavi backup --api --force --path /data/snapshots/steam_1/100.part -- Hades II");
        assert!(backup_args(dir, "--config").is_none() && backup_args(dir, "").is_none());
        assert_eq!(find_args("1145360").unwrap()[5..], ["find", "--api", "--steam-id", "1145360"]);
        assert!(find_args("1; reboot").is_none() && find_args("").is_none());
        assert_eq!(title_from_find(r#"{"games":{"Hades II":{"score":1.0}}}"#).as_deref(), Some("Hades II"));
        assert_eq!(title_from_find(r#"{"games":{}}"#), None);
        assert_eq!(title_from_find("No matching games"), None);
        let snap = Snapshot { game: "steam:1".into(), name: "Hades II".into(), at: 100, kind: Kind::Launch, bytes: 1, files: 1, dir: "/data/snapshots/steam_1/100".into() };
        assert_eq!(restore_args(&snap).unwrap()[5..], ["restore", "--api", "--force", "--path", "/data/snapshots/steam_1/100", "--", "Hades II"]);
        assert_eq!(game_dir(Path::new("/d"), "epic:../../x"), Path::new("/d/snapshots/epic_.._.._x"));
        assert_eq!(game_dir(Path::new("/d"), "steam:1145360"), Path::new("/d/snapshots/steam_1145360"));
    }

    #[test]
    fn reads_what_ludusavi_saved() {
        assert_eq!(parse_backup(r#"{"games":{"A":{"files":{"/x":{"bytes":10},"/y":{"bytes":5,"failed":true}},"registry":{"HKEY_CURRENT_USER/Software/A":{}}}}}"#), Some(("A".into(), 10, 2)));
        assert_eq!(parse_backup(r#"{"overall":{"totalGames":0},"games":{}}"#), None);
        assert_eq!(parse_backup(r#"{"games":{"A":{"files":{},"registry":{}}}}"#), None);
        assert_eq!(parse_backup("Error: unknown game"), None);
        // Lines a wrapper printed before the answer don't hide it.
        assert!(parse_backup("warning: something\n{\"games\":{\"A\":{\"files\":{\"/x\":{\"bytes\":1}}}}}").is_some());
    }

    #[test]
    fn keeps_the_newest_few_and_skips_what_did_not_change() {
        let d = data("keep");
        assert!(due(&d, "steam:1145360", 1000));
        let first = take(&d, Kind::Launch, 1000, "save one").unwrap();
        assert_eq!((first.name.as_str(), first.bytes, first.files, first.kind), ("Hades II", 8, 1, Kind::Launch));
        assert!(first.dir.ends_with("snapshots/steam_1145360/1000") && first.dir.join("Hades II/drive-0/saves/profile1.sav").is_file());
        assert!(!due(&d, "steam:1145360", 1000 + EVERY_SECS - 1));
        assert!(due(&d, "steam:1145360", 1000 + EVERY_SECS));
        // The same saves again: nothing new is kept.
        assert_eq!(take(&d, Kind::Launch, 2000, "save one"), None);
        assert_eq!(list(&d, "steam:1145360").len(), 1);
        // A manual one is kept even when nothing changed, and doesn't count against the launch ones.
        assert!(take(&d, Kind::Manual, 2500, "save one").is_some());
        for i in 0..7u64 {
            assert!(take(&d, Kind::Launch, 3000 + i * 1000, &format!("save {i}")).is_some());
        }
        let all = list(&d, "steam:1145360");
        let launch: Vec<u64> = all.iter().filter(|x| x.kind == Kind::Launch).map(|x| x.at).collect();
        assert_eq!(launch, vec![9000, 8000, 7000, 6000, 5000]);
        assert_eq!(all.iter().filter(|x| x.kind == Kind::Manual).count(), 1);
        assert!(!game_dir(&d, "steam:1145360").join("1000").exists());
        assert_eq!(games(&d), vec![("steam:1145360".to_string(), "Hades II".to_string(), 6, 9000)]);
        assert!(list(&d, "steam:1").is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn nothing_saved_leaves_nothing_behind() {
        let d = data("empty");
        let dir = begin(&d, "steam:7", 50);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(finish(&d, "steam:7", Kind::Launch, 50, &dir, r#"{"games":{}}"#, KEEP), Ok(None));
        assert!(!dir.exists() && list(&d, "steam:7").is_empty());
        // Ludusavi said it saved something but the folder isn't there.
        let dir = begin(&d, "steam:7", 60);
        assert!(finish(&d, "steam:7", Kind::Launch, 60, &dir, r#"{"games":{"A":{"files":{"/x":{"bytes":1}}}}}"#, KEEP).is_err());
        // A run cut short leaves a .part folder: never listed, and cleared at the next start.
        let dir = begin(&d, "steam:7", 70);
        ludusavi_ran(&dir, "x");
        assert!(list(&d, "steam:7").is_empty() && games(&d).is_empty());
        clear_unfinished(&d);
        assert!(!dir.exists());
        // A folder someone else put there isn't a snapshot, and remove leaves it alone.
        let stray = game_dir(&d, "steam:7").join("123");
        fs::create_dir_all(&stray).unwrap();
        remove(&Snapshot { game: "steam:7".into(), name: "A".into(), at: 123, kind: Kind::Launch, bytes: 0, files: 0, dir: stray.clone() });
        assert!(stray.is_dir() && list(&d, "steam:7").is_empty());
        let _ = fs::remove_dir_all(&d);
    }
}

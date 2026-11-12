//! Saves, on top of the Ludusavi command line: what the app does with a game's snapshots (Back up now, restore one
//! with the current saves kept first, Undo), and the saves on a Windows drive (found with the drive as an "other
//! Windows" root, then restored into each game's SteamOS folders). Everything here only builds command lines and
//! reads their answers; the app runs them one at a time through `run`.
//!
//! The snapshots themselves are `crate::snapshots`: the same folders the background helper writes at each game
//! start. A restore first takes a `Kind::BeforeRestore` snapshot and notes its time in the game's snapshot folder
//! (a file called `undo`); Undo restores that one.
//!
//! The Windows drive's saves use a folder of their own in Turbine Hub's folder, `saves/windows`: Ludusavi's settings
//! for the run (the drive as a root, and where each file goes), the backup read from the drive, the saves kept from
//! before, and a note of what the last run did. The Windows drive is only read: every file is sent to a folder on
//! this machine, and anything that has no place here stays in the backup.

use crate::json::{self, Json, quote};
use crate::snapshots::{self, Kind, Snapshot};
use std::fs;
use std::path::{Path, PathBuf};

pub const LUDUSAVI: &str = snapshots::LUDUSAVI;

fn undo_file(data: &Path, game_id: &str) -> PathBuf {
    snapshots::game_dir(data, game_id).join("undo")
}

/// The snapshot Undo would put back: the saves kept by the last restore, while that snapshot is still there.
pub fn undo_snapshot(data: &Path, game_id: &str) -> Option<Snapshot> {
    let at: u64 = fs::read_to_string(undo_file(data, game_id)).ok()?.trim().parse().ok()?;
    snapshots::list(data, game_id).into_iter().find(|s| s.at == at && s.kind == Kind::BeforeRestore)
}

/// A game as Ludusavi is asked about it.
#[derive(Debug, Clone, PartialEq)]
pub struct GameRef {
    /// Turbine Hub's id ("steam:1086940").
    pub id: String,
    pub name: String,
    /// The Steam app id, for Steam games: Ludusavi finds its own title for the game by it.
    pub steam_id: Option<String>,
}

/// What one command gave back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ran {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Runs one command line to its end. The app's runs it on the host; the tests' answers from a script.
pub type Runner<'a> = &'a mut dyn FnMut(&[String]) -> Result<Ran, String>;

fn ludusavi(config: Option<&Path>, args: &[&str]) -> Vec<String> {
    let mut line: Vec<String> = ["flatpak-spawn", "--host", "flatpak", "run", LUDUSAVI].iter().map(|x| x.to_string()).collect();
    if let Some(c) = config {
        line.extend(["--config".to_string(), c.to_string_lossy().into_owned()]);
    }
    // A manifest that can't be refreshed (no internet) is no reason to stop: the last one is used.
    line.push("--try-manifest-update".into());
    line.extend(args.iter().map(|x| x.to_string()));
    line
}

/// The command that asks Ludusavi for its own title of a game.
pub fn find_args(game: &GameRef) -> Vec<String> {
    match game.steam_id.as_deref().and_then(snapshots::find_args) {
        Some(line) => line,
        None => ludusavi(None, &["find", "--api", "--normalized", "--", &game.name]),
    }
}

fn backup_args(config: Option<&Path>, dir: &Path, titles: &[String]) -> Vec<String> {
    let dir = dir.to_string_lossy();
    let mut line = ludusavi(config, &["backup", "--force", "--api", "--path", &dir, "--"]);
    line.extend(titles.iter().cloned());
    line
}

fn restore_args(config: Option<&Path>, dir: &Path, titles: &[String]) -> Vec<String> {
    let dir = dir.to_string_lossy();
    let mut line = ludusavi(config, &["restore", "--force", "--api", "--path", &dir, "--"]);
    line.extend(titles.iter().cloned());
    line
}

/// What a backup or restore with `--api` said.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    /// The titles that had files, each with how many.
    pub games: Vec<(String, usize)>,
    pub bytes: u64,
    /// Some file couldn't be read or written.
    pub failed: bool,
    /// Titles Ludusavi doesn't know.
    pub unknown: Vec<String>,
}

/// Reads a backup's or restore's JSON answer. Err when there is none (Ludusavi stopped before it got that far): the
/// words are what it printed last.
pub fn outcome(ran: &Ran) -> Result<Outcome, String> {
    let Ok(j) = json::parse(&ran.stdout) else {
        let last = |t: &str| t.lines().map(str::trim).rfind(|l| !l.is_empty()).map(String::from);
        return Err(last(&ran.stderr).or_else(|| last(&ran.stdout)).unwrap_or_else(|| "Ludusavi gave no answer.".into()));
    };
    let errors = j.get("errors");
    let games = j.get("games").map(Json::entries).unwrap_or(&[]).iter().filter(|(_, g)| g.str_at("decision") != Some("Ignored")).map(|(t, g)| (t.clone(), g.get("files").map(|f| f.entries().len()).unwrap_or(0))).filter(|(_, n)| *n > 0).collect();
    Ok(Outcome {
        games,
        bytes: j.get("overall").and_then(|o| o.get("processedBytes")).and_then(Json::as_f64).unwrap_or(0.0) as u64,
        failed: errors.and_then(|e| e.get("someGamesFailed")).and_then(Json::as_bool).unwrap_or(false),
        unknown: errors.and_then(|e| e.get("unknownGames")).map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect(),
    })
}

/// Ludusavi's title for a game: asked by Steam app id, else the game's own name.
fn title_of(game: &GameRef, run: Runner) -> Result<String, String> {
    let ran = run(&find_args(game))?;
    Ok(snapshots::title_from_find(&ran.stdout).unwrap_or_else(|| game.name.clone()))
}

/// Takes one snapshot of a game's saves as they are now. None when Ludusavi found no saves. A "before a restore"
/// snapshot lets go of no older one yet: the one being restored may be among them (`restore` tidies afterwards).
fn take(data: &Path, game_id: &str, title: &str, kind: Kind, now: u64, run: Runner) -> Result<Option<Snapshot>, String> {
    let dir = snapshots::begin(data, game_id, now);
    let _ = fs::remove_dir_all(&dir);
    let args = snapshots::backup_args(&dir, title).ok_or("The game's name can't be given to Ludusavi.")?;
    let ran = run(&args)?;
    snapshots::finish(data, game_id, kind, now, &dir, &ran.stdout, if kind == Kind::BeforeRestore { usize::MAX } else { snapshots::KEEP })
}

/// One thing to do with a game's saves.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// A snapshot of the saves as they are now.
    Backup { game: GameRef },
    /// Puts the snapshot taken at `at` back. The saves as they are now are kept as a snapshot first, and Undo
    /// leads back to them.
    Restore { game: GameRef, at: u64 },
    /// Puts back the saves the last restore kept.
    Undo { game: GameRef },
    /// Brings the saves on the Windows drive into the games installed here.
    Windows { mount: PathBuf, targets: Vec<Target> },
    /// Puts back the saves the last run of `Windows` kept.
    WindowsUndo,
}

/// Where a job reads and writes: Turbine Hub's own folder, its data folder (the snapshots) and the home folder.
pub struct Places<'a> {
    pub folder: &'a Path,
    pub data: &'a Path,
    pub home: &'a Path,
}

/// Runs a job's commands in order and answers what happened in a few words. `say` gets what is going on now.
pub fn run(job: &Job, at: &Places, now: u64, say: &dyn Fn(String), run: Runner) -> Result<String, String> {
    match job {
        Job::Backup { game } => {
            say(format!("Backing up the saves of {}", game.name));
            let title = title_of(game, run)?;
            match take(at.data, &game.id, &title, Kind::Manual, now, run)? {
                Some(_) => Ok("Saves backed up".into()),
                None => Err(format!("No saves of {} were found on this machine yet.", game.name)),
            }
        }
        Job::Restore { game, at: taken } => restore(game, *taken, at.data, now, say, run),
        Job::Undo { game } => {
            let Some(kept) = undo_snapshot(at.data, &game.id) else { return Err("There is nothing to undo any more.".into()) };
            restore(game, kept.at, at.data, now, say, run).map(|_| "The saves from before the restore are back".into())
        }
        Job::Windows { mount, targets } => windows(mount, targets, at.folder, at.home, now, say, run),
        Job::WindowsUndo => {
            let Some(last) = WindowsRun::load(at.folder) else { return Err("There is nothing to undo any more.".into()) };
            let Some(kept) = last.kept.as_ref().filter(|d| d.is_dir()) else { return Err("There is nothing to undo: these games had no saves here before.".into()) };
            say("Putting the saves from before back".into());
            let out = outcome(&run(&restore_args(None, kept, &last.titles))?)?;
            if out.failed || out.games.is_empty() {
                return Err("The saves from before couldn't all be put back. They are still in Turbine Hub's folder.".into());
            }
            WindowsRun { kept: None, undone: true, ..last }.save(at.folder);
            Ok("The saves from before are back".into())
        }
    }
}

fn restore(game: &GameRef, taken: u64, data: &Path, now: u64, say: &dyn Fn(String), run: Runner) -> Result<String, String> {
    let Some(snap) = snapshots::list(data, &game.id).into_iter().find(|s| s.at == taken) else { return Err("That snapshot isn't there any more.".into()) };
    // Never the second the snapshot itself carries: a snapshot of the same second would take its place.
    let now = if now == snap.at { now + 1 } else { now };
    say(format!("Keeping the current saves of {}", game.name));
    let kept = take(data, &game.id, &snap.name, Kind::BeforeRestore, now, run).map_err(|e| format!("The current saves couldn't be kept, so nothing was restored. {e}"))?;
    say(format!("Restoring the saves of {}", game.name));
    let args = snapshots::restore_args(&snap).ok_or("The game's name can't be given to Ludusavi.")?;
    let out = outcome(&run(&args)?)?;
    if out.games.is_empty() || out.failed {
        return Err("The snapshot couldn't be restored. The saves are as they were.".into());
    }
    match kept {
        Some(k) => {
            let _ = crate::write_atomic(&undo_file(data, &game.id), k.at.to_string().as_bytes());
        }
        // Nothing was there to keep: no saves on this machine before, so nothing to undo to.
        None => {
            let _ = fs::remove_file(undo_file(data, &game.id));
        }
    }
    // Now the older "before a restore" snapshots can go, down to the usual few.
    for old in snapshots::list(data, &game.id).iter().filter(|s| s.kind == Kind::BeforeRestore).skip(snapshots::KEEP) {
        snapshots::remove(old);
    }
    Ok("Save restored".into())
}

// ---- The Windows drive's saves ----

/// A game installed here that saves from the Windows drive can go into.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub name: String,
    /// The game's Windows files here (its Proton prefix: the folder that holds drive_c). None until the game has
    /// been started once.
    pub prefix: Option<PathBuf>,
    /// The game's folder here.
    pub install_dir: PathBuf,
    /// The same game's folder on the Windows drive, when it is installed there.
    pub windows_dir: Option<PathBuf>,
}

pub fn windows_dir(folder: &Path) -> PathBuf {
    folder.join("saves").join("windows")
}

fn yaml(p: &Path) -> String {
    format!("\"{}\"", p.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\""))
}

/// Ludusavi's roots for reading the Windows drive: the drive itself as an "other Windows" root (its user folders),
/// and each Steam folder on it (saves kept inside game folders).
fn roots_yaml(mount: &Path) -> String {
    let mut out = format!("roots:\n  - path: {}\n    store: otherWindows\n", yaml(mount));
    for lib in crate::bring::steam_libraries(mount) {
        if let Some(steam) = lib.parent() {
            out.push_str(&format!("  - path: {}\n    store: steam\n", yaml(steam)));
        }
    }
    out
}

/// The settings for reading the drive.
pub fn windows_backup_config(mount: &Path) -> String {
    roots_yaml(mount)
}

/// The settings for restoring one game: where each place on the Windows drive is on this machine. The game's folder
/// first, then each Windows user's folder into the game's Windows files, then everything else on the drive into a
/// folder of Turbine Hub's, so nothing is ever written to the drive itself.
pub fn windows_restore_config(mount: &Path, target: &Target, unplaced: &Path) -> String {
    let mut pairs: Vec<(PathBuf, PathBuf)> = Vec::new();
    if let Some(w) = &target.windows_dir {
        pairs.push((w.clone(), target.install_dir.clone()));
    }
    if let Some(pfx) = &target.prefix {
        let c = pfx.join("drive_c");
        for user in crate::bring::windows_users(mount) {
            pairs.push((user, c.join("users/steamuser")));
        }
        pairs.push((mount.join("Users/Public"), c.join("users/Public")));
        pairs.push((mount.join("ProgramData"), c.join("ProgramData")));
    }
    pairs.push((mount.to_path_buf(), unplaced.to_path_buf()));
    let mut out = roots_yaml(mount);
    out.push_str("redirects:\n");
    for (from, to) in pairs {
        out.push_str(&format!("  - kind: restore\n    source: {}\n    target: {}\n", yaml(&from), yaml(&to)));
    }
    out
}

/// Names compared without case, spaces and marks: "Baldur's Gate 3" is "baldursgate3".
pub fn plain_name(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// What the last run on the Windows drive did, for the page and for Undo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindowsRun {
    pub at: u64,
    /// The games whose saves were brought over.
    pub brought: Vec<String>,
    /// Games with saves on the drive that have no place here yet (not installed, or never started).
    pub waiting: Vec<String>,
    /// Ludusavi's titles of the brought games, for Undo.
    pub titles: Vec<String>,
    /// Where the saves from before are kept.
    pub kept: Option<PathBuf>,
    pub undone: bool,
}

impl WindowsRun {
    fn file(folder: &Path) -> PathBuf {
        windows_dir(folder).join("last.json")
    }
    pub fn load(folder: &Path) -> Option<WindowsRun> {
        let j = json::parse(&fs::read_to_string(Self::file(folder)).ok()?).ok()?;
        let list = |k: &str| j.get(k).map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect();
        Some(WindowsRun { at: j.get("at").and_then(Json::as_f64)? as u64, brought: list("brought"), waiting: list("waiting"), titles: list("titles"), kept: j.str_at("kept").filter(|k| !k.is_empty()).map(PathBuf::from), undone: j.get("undone").and_then(Json::as_bool).unwrap_or(false) })
    }
    pub fn save(&self, folder: &Path) {
        let list = |v: &[String]| format!("[{}]", v.iter().map(|x| quote(x)).collect::<Vec<_>>().join(","));
        let text = format!("{{\"at\":{},\"brought\":{},\"waiting\":{},\"titles\":{},\"kept\":{},\"undone\":{}}}\n", self.at, list(&self.brought), list(&self.waiting), list(&self.titles), quote(&self.kept.as_ref().map(|k| k.to_string_lossy().into_owned()).unwrap_or_default()), self.undone);
        let _ = crate::write_atomic(&Self::file(folder), text.as_bytes());
    }
}

fn windows(mount: &Path, targets: &[Target], folder: &Path, home: &Path, now: u64, say: &dyn Fn(String), run: Runner) -> Result<String, String> {
    let base = windows_dir(folder);
    let config = base.join("config");
    let backup = base.join("backup");
    let write = |text: String| crate::write_atomic(&config.join("config.yaml"), text.as_bytes()).map_err(|e| format!("Couldn't write Ludusavi's settings for this: {e}"));
    write(windows_backup_config(mount))?;
    // Ludusavi's own list of where games keep saves, when it has one already: no need to fetch it again.
    let own = home.join(".var/app").join(LUDUSAVI).join("config/ludusavi/manifest.yaml");
    if own.is_file() && !config.join("manifest.yaml").exists() {
        let _ = fs::copy(&own, config.join("manifest.yaml"));
    }
    say("Looking for saves on the Windows drive".into());
    let _ = fs::remove_dir_all(&backup);
    let found = outcome(&run(&backup_args(Some(&config), &backup, &[]))?)?;
    if found.games.is_empty() {
        return Err("No saves were found on the Windows drive.".into());
    }
    // Each title with saves, and the game here it belongs to (one that has been started once).
    let mut place: Vec<(String, &Target)> = Vec::new();
    let mut waiting = Vec::new();
    for (title, _) in &found.games {
        match targets.iter().find(|t| plain_name(&t.name) == plain_name(title)) {
            Some(t) if t.prefix.is_some() => place.push((title.clone(), t)),
            _ => waiting.push(title.clone()),
        }
    }
    let mut last = WindowsRun { at: now, waiting, ..WindowsRun::default() };
    if place.is_empty() {
        last.save(folder);
        return Err("Saves were found, but none for a game that is installed and has been started here. They are kept in Turbine Hub's folder: start those games once, then bring the saves over again.".into());
    }
    let titles: Vec<String> = place.iter().map(|(t, _)| t.clone()).collect();
    say("Keeping the saves these games have here now".into());
    let kept = base.join(format!("kept-{now}"));
    let before = outcome(&run(&backup_args(None, &kept, &titles))?)?;
    if before.failed {
        let _ = fs::remove_dir_all(&kept);
        return Err("The saves on this machine couldn't be kept first, so nothing was brought over.".into());
    }
    if before.games.is_empty() {
        let _ = fs::remove_dir_all(&kept);
    } else {
        last.kept = Some(kept);
    }
    let mut problems = Vec::new();
    for (title, target) in &place {
        say(format!("Bringing over the saves of {}", target.name));
        write(windows_restore_config(mount, target, &base.join("unplaced").join(plain_name(title))))?;
        match outcome(&run(&restore_args(Some(&config), &backup, std::slice::from_ref(title)))?) {
            Ok(o) if !o.failed && !o.games.is_empty() => {
                last.brought.push(target.name.clone());
                last.titles.push(title.clone());
            }
            _ => problems.push(target.name.clone()),
        }
    }
    // The settings go back to the plain ones, with no places to send files to.
    let _ = write(windows_backup_config(mount));
    last.save(folder);
    // Older kept saves go once a newer run has its own.
    if let Ok(rd) = fs::read_dir(&base) {
        let mut old: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("kept-")) && Some(p) != last.kept.as_ref()).collect();
        old.sort();
        old.reverse();
        for p in old.into_iter().skip(1) {
            let _ = fs::remove_dir_all(p);
        }
    }
    match (last.brought.len(), problems.is_empty()) {
        (0, _) => Err("The saves couldn't be put into the games here. Nothing changed.".into()),
        (_, false) => Err(format!("Saves brought over for {}. These didn't work: {}.", last.brought.join(", "), problems.join(", "))),
        (1, true) => Ok(format!("Saves of {} brought over", last.brought[0])),
        (c, true) => Ok(format!("Saves of {c} games brought over")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-saves-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn game() -> GameRef {
        GameRef { id: "steam:1086940".into(), name: "Baldur's Gate 3".into(), steam_id: Some("1086940".into()) }
    }

    /// A stand-in for Ludusavi: answers `find` with a title, writes a file for every backup, and says each named
    /// game was processed. Every command line is kept.
    struct Fake {
        lines: Vec<Vec<String>>,
        /// Titles with saves "on the machine" (a backup of anything else finds nothing).
        have: Vec<String>,
        fail_restore: bool,
    }

    impl Fake {
        fn new(have: &[&str]) -> Fake {
            Fake { lines: Vec::new(), have: have.iter().map(|x| x.to_string()).collect(), fail_restore: false }
        }
        fn run(&mut self, line: &[String]) -> Result<Ran, String> {
            self.lines.push(line.to_vec());
            let after = |flag: &str| line.iter().position(|a| a == flag).and_then(|i| line.get(i + 1)).cloned();
            let verb = line.iter().find(|a| ["find", "backup", "restore"].contains(&a.as_str())).cloned().unwrap_or_default();
            let path_at = line.iter().position(|a| a == "--path");
            let named: Vec<String> = path_at.map(|i| line[i + 2..].iter().filter(|a| *a != "--").cloned().collect()).unwrap_or_default();
            let games = |titles: &[String]| titles.iter().map(|t| format!("{}:{{\"decision\":\"Processed\",\"files\":{{\"/a/save.dat\":{{\"bytes\":5}}}}}}", quote(t))).collect::<Vec<_>>().join(",");
            let stdout = match verb.as_str() {
                "find" => "{\"games\":{\"Baldur's Gate 3\":{}}}".to_string(),
                "backup" => {
                    let titles: Vec<String> = if named.is_empty() { self.have.clone() } else { named.into_iter().filter(|t| self.have.contains(t)).collect() };
                    if !titles.is_empty() {
                        let d = PathBuf::from(after("--path").unwrap());
                        fs::create_dir_all(&d).unwrap();
                        fs::write(d.join("mapping.yaml"), "saves").unwrap();
                    }
                    format!("{{\"overall\":{{\"processedBytes\":5}},\"games\":{{{}}}}}", games(&titles))
                }
                _ if self.fail_restore => "{\"errors\":{\"someGamesFailed\":true},\"games\":{}}".to_string(),
                _ => format!("{{\"overall\":{{\"processedBytes\":5}},\"games\":{{{}}}}}", games(&named)),
            };
            Ok(Ran { code: 0, stdout, stderr: String::new() })
        }
    }

    fn quiet(_: String) {}

    #[test]
    fn the_answers_of_ludusavi_are_read() {
        let ran = |out: &str, err: &str| Ran { code: 1, stdout: out.into(), stderr: err.into() };
        let o = outcome(&ran("{\"errors\":{\"someGamesFailed\":true,\"unknownGames\":[\"Nope\"]},\"overall\":{\"processedBytes\":12},\"games\":{\"A\":{\"decision\":\"Processed\",\"files\":{\"/x\":{\"bytes\":12}}},\"B\":{\"decision\":\"Ignored\",\"files\":{\"/y\":{}}},\"C\":{\"decision\":\"Processed\",\"files\":{}}}}", "")).unwrap();
        assert_eq!(o, Outcome { games: vec![("A".into(), 1)], bytes: 12, failed: true, unknown: vec!["Nope".into()] });
        assert_eq!(outcome(&ran("", "warning\nError: no manifest\n")), Err("Error: no manifest".into()));
        assert_eq!(outcome(&ran("", "")), Err("Ludusavi gave no answer.".into()));
        let g = GameRef { id: "gog:1".into(), name: "Some Game".into(), steam_id: None };
        assert!(find_args(&g).ends_with(&["find".to_string(), "--api".into(), "--normalized".into(), "--".into(), "Some Game".into()]));
        assert!(find_args(&game()).ends_with(&["--steam-id".to_string(), "1086940".into()]));
    }

    fn places<'a>(f: &'a Path) -> Places<'a> {
        Places { folder: f, data: f, home: f }
    }

    fn ids(data: &Path) -> Vec<(u64, Kind)> {
        snapshots::list(data, "steam:1086940").iter().map(|s| (s.at, s.kind)).collect()
    }

    #[test]
    fn a_restore_keeps_the_current_saves_first_and_undo_brings_them_back() {
        let f = dir("restore");
        let mut fake = Fake::new(&["Baldur's Gate 3"]);
        assert_eq!(run(&Job::Backup { game: game() }, &places(&f), 1000, &quiet, &mut |l| fake.run(l)), Ok("Saves backed up".into()));
        assert_eq!(ids(&f), vec![(1000, Kind::Manual)]);
        fake.lines.clear();
        let job = Job::Restore { game: game(), at: 1000 };
        assert_eq!(run(&job, &places(&f), 2000, &quiet, &mut |l| fake.run(l)), Ok("Save restored".into()));
        // The current saves go into a "before a restore" snapshot first, under the name the snapshot carries, then
        // the restore runs from the snapshot's own folder.
        let verbs: Vec<&str> = fake.lines.iter().map(|l| l.iter().find(|a| ["find", "backup", "restore"].contains(&a.as_str())).unwrap().as_str()).collect();
        assert_eq!(verbs, vec!["backup", "restore"]);
        assert!(fake.lines[0].contains(&snapshots::begin(&f, "steam:1086940", 2000).to_string_lossy().into_owned()));
        assert!(fake.lines[1].contains(&snapshots::game_dir(&f, "steam:1086940").join("1000").to_string_lossy().into_owned()));
        assert!(fake.lines.iter().all(|l| l[..5] == ["flatpak-spawn", "--host", "flatpak", "run", LUDUSAVI] && l.last().unwrap() == "Baldur's Gate 3"));
        assert_eq!(ids(&f), vec![(2000, Kind::BeforeRestore), (1000, Kind::Manual)]);
        assert_eq!(undo_snapshot(&f, "steam:1086940").map(|s| s.at), Some(2000));
        // Undo restores the kept one, and keeps what it replaces in turn.
        fake.lines.clear();
        assert!(run(&Job::Undo { game: game() }, &places(&f), 3000, &quiet, &mut |l| fake.run(l)).is_ok());
        assert!(fake.lines[1].contains(&snapshots::game_dir(&f, "steam:1086940").join("2000").to_string_lossy().into_owned()));
        assert_eq!(undo_snapshot(&f, "steam:1086940").map(|s| s.at), Some(3000));
        // A snapshot that is gone, and a restore that fails, say so.
        assert!(run(&Job::Restore { game: game(), at: 1 }, &places(&f), 4000, &quiet, &mut |l| fake.run(l)).unwrap_err().contains("isn't there"));
        fake.fail_restore = true;
        assert!(run(&job, &places(&f), 5000, &quiet, &mut |l| fake.run(l)).unwrap_err().contains("couldn't be restored"));
        assert_eq!(undo_snapshot(&f, "steam:1086940").map(|s| s.at), Some(3000));
        // Restoring the oldest kept one many times over never lets go of the one being restored.
        fake.fail_restore = false;
        for i in 0..8 {
            let oldest = ids(&f).into_iter().rfind(|(_, k)| *k == Kind::BeforeRestore).unwrap().0;
            assert!(run(&Job::Restore { game: game(), at: oldest }, &places(&f), 6000 + i, &quiet, &mut |l| fake.run(l)).is_ok(), "{i}");
        }
        assert_eq!(ids(&f).iter().filter(|(_, k)| *k == Kind::BeforeRestore).count(), snapshots::KEEP);
        let _ = fs::remove_dir_all(&f);
    }

    #[test]
    fn with_no_saves_here_there_is_nothing_to_keep_or_back_up() {
        let f = dir("none");
        let mut have = Fake::new(&["Baldur's Gate 3"]);
        run(&Job::Backup { game: game() }, &places(&f), 500, &quiet, &mut |l| have.run(l)).unwrap();
        let mut fake = Fake::new(&[]);
        assert!(run(&Job::Backup { game: game() }, &places(&f), 1000, &quiet, &mut |l| fake.run(l)).unwrap_err().starts_with("No saves of Baldur's Gate 3"));
        assert_eq!(ids(&f), vec![(500, Kind::Manual)]);
        // Nothing here to keep: the restore still runs, and there is nothing to undo to.
        assert!(run(&Job::Restore { game: game(), at: 500 }, &places(&f), 2000, &quiet, &mut |l| fake.run(l)).is_ok());
        assert_eq!(undo_snapshot(&f, "steam:1086940"), None);
        assert!(run(&Job::Undo { game: game() }, &places(&f), 3000, &quiet, &mut |l| fake.run(l)).is_err());
        assert_eq!(ids(&f), vec![(500, Kind::Manual)]);
        let _ = fs::remove_dir_all(&f);
    }

    fn drive(d: &Path) -> PathBuf {
        let mount = d.join("win \"C\"");
        for p in ["Users/Sam/Documents", "Users/Public", "Users/Default", "Program Files (x86)/Steam/steamapps/common/Hades II"] {
            fs::create_dir_all(mount.join(p)).unwrap();
        }
        mount
    }

    #[test]
    fn the_windows_drive_is_a_root_and_every_file_gets_a_place_off_it() {
        let d = dir("yaml");
        let mount = drive(&d);
        let m = mount.to_string_lossy().replace('"', "\\\"");
        assert_eq!(windows_backup_config(&mount), format!("roots:\n  - path: \"{m}\"\n    store: otherWindows\n  - path: \"{m}/Program Files (x86)/Steam\"\n    store: steam\n"));
        let t = Target { name: "Hades II".into(), prefix: Some(PathBuf::from("/steam/compatdata/1145360/pfx")), install_dir: PathBuf::from("/steam/common/Hades II"), windows_dir: Some(mount.join("Program Files (x86)/Steam/steamapps/common/Hades II")) };
        let text = windows_restore_config(&mount, &t, Path::new("/hub/unplaced/hadesii"));
        let pairs: Vec<(&str, &str)> = text.split("  - kind: restore\n").skip(1).map(|b| { let mut l = b.lines(); (l.next().unwrap().trim().trim_start_matches("source: "), l.next().unwrap().trim().trim_start_matches("target: ")) }).collect();
        assert_eq!(pairs.len(), 5);
        assert_eq!(pairs[0], (format!("\"{m}/Program Files (x86)/Steam/steamapps/common/Hades II\"").as_str(), "\"/steam/common/Hades II\""));
        assert_eq!(pairs[1], (format!("\"{m}/Users/Sam\"").as_str(), "\"/steam/compatdata/1145360/pfx/drive_c/users/steamuser\""));
        assert_eq!(pairs[2].1, "\"/steam/compatdata/1145360/pfx/drive_c/users/Public\"");
        // The last one catches whatever is left, so nothing is ever sent to the drive itself.
        assert_eq!(pairs[4], (format!("\"{m}\"").as_str(), "\"/hub/unplaced/hadesii\""));
        assert!(pairs.iter().all(|(_, to)| !to.contains("win ")));
        assert_eq!(plain_name("Baldur's Gate 3"), "baldursgate3");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn windows_saves_go_into_started_games_and_the_rest_wait() {
        let d = dir("win");
        let mount = drive(&d);
        let (f, home) = (d.join("hub"), d.join("home"));
        let own = home.join(".var/app").join(LUDUSAVI).join("config/ludusavi/manifest.yaml");
        fs::create_dir_all(own.parent().unwrap()).unwrap();
        fs::write(&own, "manifest").unwrap();
        let target = |name: &str, started: bool| Target { name: name.into(), prefix: started.then(|| d.join("pfx").join(plain_name(name))), install_dir: d.join("common").join(name), windows_dir: None };
        let targets = vec![target("Hades II", true), target("Disco Elysium", false), target("Balatro", true)];
        // On the drive: Hades II, Disco Elysium and a game that isn't installed here. Here: Hades II has saves.
        let mut fake = Fake::new(&["Hades II", "Disco Elysium", "Celeste"]);
        let job = Job::Windows { mount: mount.clone(), targets };
        let at = Places { folder: &f, data: &f, home: &home };
        let done = run(&job, &at, 1000, &quiet, &mut |l| fake.run(l));
        assert_eq!(done, Ok("Saves of Hades II brought over".into()));
        let config = windows_dir(&f).join("config");
        assert_eq!(fs::read_to_string(config.join("manifest.yaml")).unwrap(), "manifest");
        // The drive is read with Turbine Hub's own settings; the saves here are kept with Ludusavi's own; the
        // restore uses Turbine Hub's again, for the one game that has a place.
        let cfg = config.to_string_lossy().into_owned();
        assert_eq!(fake.lines.len(), 3);
        assert!(fake.lines[0].contains(&"--config".to_string()) && fake.lines[0].contains(&cfg) && fake.lines[0].contains(&"backup".to_string()));
        assert!(!fake.lines[1].contains(&"--config".to_string()) && fake.lines[1].ends_with(&["Hades II".to_string()]));
        assert!(fake.lines[2].contains(&cfg) && fake.lines[2].contains(&"restore".to_string()) && fake.lines[2].ends_with(&["Hades II".to_string()]));
        assert!(fake.lines.iter().all(|l| l.contains(&"--try-manifest-update".to_string())));
        // Afterwards the settings hold no places to send files to.
        assert!(!fs::read_to_string(config.join("config.yaml")).unwrap().contains("redirects"));
        let last = WindowsRun::load(&f).unwrap();
        assert_eq!((last.at, last.brought.clone(), last.waiting.clone(), last.titles.clone()), (1000, vec!["Hades II".to_string()], vec!["Disco Elysium".to_string(), "Celeste".to_string()], vec!["Hades II".to_string()]));
        assert_eq!(last.kept, Some(windows_dir(&f).join("kept-1000")));
        // Undo puts the kept saves back with Ludusavi's own settings, once.
        fake.lines.clear();
        assert!(run(&Job::WindowsUndo, &at, 2000, &quiet, &mut |l| fake.run(l)).is_ok());
        assert!(fake.lines[0].contains(&"restore".to_string()) && fake.lines[0].contains(&windows_dir(&f).join("kept-1000").to_string_lossy().into_owned()) && !fake.lines[0].contains(&"--config".to_string()));
        assert!(WindowsRun::load(&f).unwrap().undone);
        assert!(run(&Job::WindowsUndo, &at, 3000, &quiet, &mut |l| fake.run(l)).is_err());
        // Nothing on the drive, or nothing with a place here, changes nothing and says so.
        let mut none = Fake::new(&[]);
        assert_eq!(run(&job, &at, 4000, &quiet, &mut |l| none.run(l)), Err("No saves were found on the Windows drive.".into()));
        let mut other = Fake::new(&["Celeste"]);
        assert!(run(&job, &at, 5000, &quiet, &mut |l| other.run(l)).unwrap_err().starts_with("Saves were found, but none for a game"));
        assert_eq!(other.lines.len(), 1);
        let _ = fs::remove_dir_all(&d);
    }
}

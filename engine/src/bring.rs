//! Bring it over from Windows: what's on the player's Windows drive (Steam games to copy instead of downloading again,
//! Windows apps they use) and what each Windows app becomes on SteamOS. The Windows drive is only ever read: it is
//! mounted read only by the app, and nothing here writes to it.

use crate::vdf;
use std::fs;
use std::path::{Path, PathBuf};

/// A Steam game installed on the Windows drive.
#[derive(Debug, Clone, PartialEq)]
pub struct WinGame {
    pub app_id: String,
    pub name: String,
    /// The game's folder (steamapps/common/<installdir>).
    pub dir: PathBuf,
    pub manifest: PathBuf,
    pub bytes: u64,
}

/// Steam libraries on a Windows partition: the default one under Program Files and any "SteamLibrary" folder at the
/// top of the drive (the name Steam suggests). Each is a steamapps folder.
pub fn steam_libraries(mount: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for c in ["Program Files (x86)/Steam/steamapps", "Program Files/Steam/steamapps", "SteamLibrary/steamapps", "Steam/steamapps", "Games/SteamLibrary/steamapps"] {
        let p = mount.join(c);
        if p.is_dir() && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// The installed games in those libraries, sorted by name. Only fully installed ones (StateFlags 4).
pub fn steam_games(mount: &Path) -> Vec<WinGame> {
    let mut games = Vec::new();
    for lib in steam_libraries(mount) {
        let Ok(dir) = fs::read_dir(&lib) else { continue };
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(e.path()) else { continue };
            let Ok(doc) = vdf::parse(&text) else { continue };
            let s = |k: &str| doc.str_at(&["AppState", k]).unwrap_or("").to_string();
            let (app_id, install_dir) = (s("appid"), s("installdir"));
            let installed = s("StateFlags").parse::<u32>().map(|f| f & 4 != 0).unwrap_or(false);
            if app_id.is_empty() || !app_id.bytes().all(|b| b.is_ascii_digit()) || install_dir.is_empty() || install_dir.contains(['/', '\\']) || install_dir == ".." || !installed {
                continue;
            }
            let dir = lib.join("common").join(&install_dir);
            if dir.is_dir() {
                games.push(WinGame { name: s("name"), bytes: s("SizeOnDisk").parse().unwrap_or(0), app_id, dir, manifest: e.path() });
            }
        }
    }
    games.sort_by_key(|g| g.name.to_lowercase());
    games.dedup_by(|a, b| a.app_id == b.app_id);
    games
}

/// Where a game is put in a SteamOS library: its folder under steamapps/common and its manifest next to it. Steam
/// sees the game at its next start and checks the files instead of downloading them.
pub fn copy_plan(g: &WinGame, steamapps: &Path) -> Vec<(PathBuf, PathBuf)> {
    let name = g.dir.file_name().map(PathBuf::from).unwrap_or_default();
    vec![(g.dir.clone(), steamapps.join("common").join(name)), (g.manifest.clone(), steamapps.join(format!("appmanifest_{}.acf", g.app_id)))]
}

/// What marks a game's folder as still being copied: "<its folder>.turbinehub-part" beside where it goes.
pub const PART: &str = ".turbinehub-part";
/// The note inside such a folder that says it is Turbine Hub's, and the folder in it the game's files go into.
const MARK: &str = "turbinehub-copy.txt";
const FILES: &str = "files";

/// How far a copy is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Copied {
    pub files: u64,
    pub of_files: u64,
    pub bytes: u64,
    pub of_bytes: u64,
}

impl Copied {
    /// "Copying Hades II: 120 of 3400 files, 2.1 GB of 48 GB".
    pub fn words(&self, name: &str) -> String {
        format!("Copying {name}: {} of {} files, {} of {}", self.files, self.of_files, crate::net::human_size(self.bytes), crate::net::human_size(self.of_bytes))
    }
    /// From 0 to 1, by bytes (by files for a game of empty files).
    pub fn part(&self) -> f64 {
        let (at, of) = if self.of_bytes > 0 { (self.bytes, self.of_bytes) } else { (self.files, self.of_files.max(1)) };
        (at as f64 / of as f64).min(1.0)
    }
}

/// Where a game's folder is copied to before it gets its name.
pub fn part_of(to: &Path) -> Option<PathBuf> {
    let name = to.file_name()?.to_string_lossy().into_owned();
    Some(to.with_file_name(format!("{name}{PART}")))
}

/// Is this a half copied game of Turbine Hub's: a real folder (no link) with that ending and Turbine Hub's note
/// in it? Only such a folder is ever removed.
fn ours(part: &Path) -> bool {
    part.file_name().is_some_and(|n| n.to_string_lossy().ends_with(PART)) && fs::symlink_metadata(part).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) && fs::symlink_metadata(part.join(MARK)).is_ok_and(|m| m.is_file())
}

/// Takes away the half copied games an earlier run left in a library's folder of games when it was cut short.
/// Answers how many went.
pub fn clean_leftovers(common: &Path) -> usize {
    let Ok(rd) = fs::read_dir(common) else { return 0 };
    rd.flatten().map(|e| e.path()).filter(|p| ours(p) && fs::remove_dir_all(p).is_ok()).count()
}

/// The room left on the disk a folder is on (the nearest folder above it that is there), from `df`: std has no
/// call for it. None when it couldn't be asked.
pub fn free_at(dir: &Path) -> Option<u64> {
    let there = dir.ancestors().find(|a| a.is_dir())?;
    let out = std::process::Command::new("df").args(["-B1", "--output=avail,size"]).arg(there).output().ok()?;
    crate::files::fs::space_from_df(&String::from_utf8_lossy(&out.stdout)).map(|s| s.free)
}

/// Counts what a folder holds, the way `copy_dir` copies it (links are passed over).
fn measure(dir: &Path, all: &mut Copied, go: &dyn Fn() -> bool) -> std::io::Result<bool> {
    for e in fs::read_dir(dir)? {
        let e = e?;
        let m = fs::symlink_metadata(e.path())?;
        if m.is_dir() {
            if !go() || !measure(&e.path(), all, go)? {
                return Ok(false);
            }
        } else if m.is_file() {
            all.of_files += 1;
            all.of_bytes += m.len();
        }
    }
    Ok(true)
}

/// One file, a piece at a time, never over a file that is there. Ok(false): stopped between two pieces.
fn copy_file(from: &Path, to: &Path, now: &mut Copied, tick: &mut dyn FnMut(&Copied) -> bool) -> std::io::Result<bool> {
    use std::io::{Read, Write};
    let mut src = fs::File::open(from)?;
    let mut dst = fs::OpenOptions::new().write(true).create_new(true).open(to)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if !tick(now) {
            return Ok(false);
        }
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        dst.write_all(&buf[..n])?;
        now.bytes += n as u64;
    }
    if let Ok(m) = src.metadata() {
        let _ = dst.set_permissions(m.permissions());
    }
    now.files += 1;
    Ok(true)
}

/// A folder and everything in it. Nothing at the source is changed; links are passed over. Ok(false): stopped.
fn copy_dir(from: &Path, to: &Path, now: &mut Copied, tick: &mut dyn FnMut(&Copied) -> bool) -> std::io::Result<bool> {
    fs::create_dir(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let m = fs::symlink_metadata(e.path())?;
        let went = if m.is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()), now, tick)?
        } else if m.is_file() {
            copy_file(&e.path(), &to.join(e.file_name()), now, tick)?
        } else {
            true
        };
        if !went {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Copies one game from the Windows drive into a SteamOS library: `pairs` as `copy_plan` gives them (the game's
/// folder, then its manifest). The source is only read.
///
/// Steam never sees a game whose files are incomplete: the folder is copied to "<its name>.turbinehub-part" beside
/// its place and gets its name in one step once every file is there, and the manifest, which is what makes Steam
/// list the game, is written last, after that. A folder or a manifest that is there already is never written over
/// or added to: the copy is refused before anything is touched. So is a copy the disk has no room for (`free`
/// says how much there is; `free_at` asks the system).
///
/// `tick` hears a line and how far it is, and stops the copy by answering false: between the pieces of a file,
/// between files, and right before the folder gets its name. A stop or a failure removes the half copied folder
/// (only the one this run made, which carries Turbine Hub's note) and answers `STOPPED` or what went wrong. What
/// a run that was cut short left in that library goes at the start of the next.
pub fn copy_game(name: &str, pairs: &[(PathBuf, PathBuf)], free: &dyn Fn(&Path) -> Option<u64>, tick: crate::Tick) -> Result<Copied, String> {
    let stopped = || crate::STOPPED.to_string();
    let failed = |e: std::io::Error| format!("Couldn't copy {name}: {e}. Nothing was left behind.");
    let go = || tick("", None);
    let (mut folders, mut files) = (Vec::new(), Vec::new());
    for (from, to) in pairs {
        match fs::metadata(from) {
            Ok(m) if m.is_dir() => folders.push((from, to, part_of(to).ok_or_else(|| format!("Couldn't copy {name}: its folder has no name."))?)),
            Ok(_) => files.push((from, to)),
            Err(_) => return Err(format!("Couldn't copy {name}: its files aren't on the Windows drive any more. Nothing was copied.")),
        }
    }
    // Never over or into what is there: not the game's folder, not its manifest.
    for (_, to) in pairs {
        if fs::symlink_metadata(to).is_ok() {
            return Err(format!("{name} is in Steam's library here already ({}). Turbine Hub never writes over a game, so nothing was copied.", to.file_name().unwrap_or_default().to_string_lossy()));
        }
    }
    for (_, to, part) in &folders {
        if let Some(common) = to.parent() {
            clean_leftovers(common);
        }
        if fs::symlink_metadata(part).is_ok() {
            return Err(format!("Something called {} is in Steam's library, and it isn't Turbine Hub's. Turbine Hub left it alone, and nothing was copied.", part.file_name().unwrap_or_default().to_string_lossy()));
        }
    }
    let mut all = Copied::default();
    for (from, _, _) in &folders {
        if !measure(from, &mut all, &go).map_err(failed)? {
            return Err(stopped());
        }
    }
    for (from, _) in &files {
        all.of_files += 1;
        all.of_bytes += fs::metadata(from).map(|m| m.len()).unwrap_or(0);
    }
    if let Some((_, to)) = pairs.first()
        && let Some(room) = free(to.parent().unwrap_or(to))
        && room < all.of_bytes
    {
        return Err(format!("Not enough room for {name}: it needs {}, and {} is free. Nothing was copied.", crate::net::human_size(all.of_bytes), crate::net::human_size(room)));
    }
    if !tick(&all.words(name), Some(0.0)) {
        return Err(stopped());
    }
    // What this run made, so exactly that goes again when it doesn't get through.
    let mut made: Vec<PathBuf> = Vec::new();
    let mut placed: Vec<&PathBuf> = Vec::new();
    let mut now = all;
    let result = (|| {
        // A line a few times a second, not at every piece.
        let mut said = std::time::Instant::now();
        let mut each = |c: &Copied| {
            if said.elapsed() >= std::time::Duration::from_millis(250) {
                said = std::time::Instant::now();
                tick(&c.words(name), Some(c.part()))
            } else {
                tick("", None)
            }
        };
        for (from, to, part) in &folders {
            if let Some(common) = to.parent() {
                fs::create_dir_all(common).map_err(failed)?;
            }
            fs::create_dir(part).map_err(failed)?;
            made.push(part.clone());
            fs::write(part.join(MARK), format!("Turbine Hub is copying {name} into this folder. It gets its name when every file is here. If no copy is running, this folder is a leftover and can be deleted.\n")).map_err(failed)?;
            if !copy_dir(from, &part.join(FILES), &mut now, &mut each).map_err(failed)? {
                return Err(stopped());
            }
        }
        // Every file is there. The last moment it can be stopped: from here each thing is one step.
        if !tick(&now.words(name), Some(now.part())) {
            return Err(stopped());
        }
        for (_, to, part) in &folders {
            fs::rename(part.join(FILES), to).map_err(failed)?;
            placed.push(*to);
            let _ = fs::remove_dir_all(part);
        }
        // The manifest last: only now does Steam have a game to list, and its files are whole.
        for (from, to) in &files {
            let bytes = fs::read(from).map_err(failed)?;
            placed.push(*to);
            crate::write_atomic(to, &bytes).map_err(failed)?;
            now.files += 1;
            now.bytes += bytes.len() as u64;
        }
        Ok(())
    })();
    if let Err(why) = result {
        for part in made.iter().filter(|p| ours(p)) {
            let _ = fs::remove_dir_all(part);
        }
        // A step after the folder got its name failed (the manifest couldn't be written): the folder this run
        // put there goes again, so no game stays that Steam doesn't know.
        for p in placed {
            let _ = if fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()) { fs::remove_dir_all(p) } else { fs::remove_file(p) };
        }
        return Err(why);
    }
    Ok(now)
}

/// What a Windows app becomes on SteamOS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Becomes {
    /// The same or an equivalent app from Flathub.
    Flatpak(&'static str),
    /// A library tile added to Steam (a cloud service).
    Tile(&'static str),
    /// A Windows launcher NonSteamLaunchers installs (its id in `crate::nsl::LAUNCHERS`).
    Nsl(&'static str),
    /// Nothing to install: SteamOS or Steam already does it, or a Turbine Hub switch does.
    BuiltIn,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppMap {
    pub id: &'static str,
    /// Its name on Windows.
    pub windows: &'static str,
    /// What it becomes, in words.
    pub steamos: &'static str,
    pub note: &'static str,
    pub becomes: Becomes,
    /// Where it lives on a Windows drive (relative to the drive, or to a user's folder when it starts with "~/").
    pub found_at: &'static [&'static str],
}

/// The Windows apps Turbine Hub knows, in the order shown.
pub const APPS: &[AppMap] = &[
    AppMap { id: "discord", windows: "Discord", steamos: "Discord", note: "Same app, same account", becomes: Becomes::Flatpak("com.discordapp.Discord"), found_at: &["~/AppData/Local/Discord"] },
    AppMap { id: "afterburner", windows: "MSI Afterburner", steamos: "MangoHud and MangoJuice", note: "FPS and temps on screen", becomes: Becomes::Flatpak("io.github.radiolamp.mangojuice"), found_at: &["Program Files (x86)/MSI Afterburner"] },
    AppMap { id: "ds4windows", windows: "DS4Windows", steamos: "Steam Input", note: "Already part of Steam", becomes: Becomes::BuiltIn, found_at: &["~/AppData/Roaming/DS4Windows", "Program Files/DS4Windows"] },
    AppMap { id: "xbox", windows: "Xbox app", steamos: "Xbox Cloud", note: "Game Pass in your library", becomes: Becomes::Tile("xcloud"), found_at: &["XboxGames"] },
    AppMap { id: "gamebar", windows: "Xbox Game Bar", steamos: "Steam recording", note: "Record from Steam's overlay", becomes: Becomes::BuiltIn, found_at: &[] },
    AppMap { id: "adrenalin", windows: "AMD Software: Adrenalin", steamos: "Game switches", note: "FSR 4 and frame generation per game", becomes: Becomes::BuiltIn, found_at: &["Program Files/AMD/CNext"] },
    AppMap { id: "nvidiaapp", windows: "NVIDIA app", steamos: "Game switches", note: "Upscaling and frame generation per game", becomes: Becomes::BuiltIn, found_at: &["Program Files/NVIDIA Corporation/NVIDIA app"] },
    AppMap { id: "obs", windows: "OBS Studio", steamos: "OBS Studio", note: "Same app", becomes: Becomes::Flatpak("com.obsproject.Studio"), found_at: &["Program Files/obs-studio"] },
    AppMap { id: "spotify", windows: "Spotify", steamos: "Spotify", note: "Same app", becomes: Becomes::Flatpak("com.spotify.Client"), found_at: &["~/AppData/Roaming/Spotify"] },
    AppMap { id: "chrome", windows: "Chrome", steamos: "Chrome", note: "Same browser, sign in to sync", becomes: Becomes::Flatpak("com.google.Chrome"), found_at: &["Program Files/Google/Chrome", "Program Files (x86)/Google/Chrome"] },
    AppMap { id: "firefox", windows: "Firefox", steamos: "Firefox", note: "Same browser, sign in to sync", becomes: Becomes::Flatpak("org.mozilla.firefox"), found_at: &["Program Files/Mozilla Firefox"] },
    AppMap { id: "edge", windows: "Microsoft Edge", steamos: "Microsoft Edge", note: "Same browser, sign in to sync", becomes: Becomes::Flatpak("com.microsoft.Edge"), found_at: &[] },
    AppMap { id: "epic", windows: "Epic Games Launcher", steamos: "Heroic Games Launcher", note: "Your Epic games in your library", becomes: Becomes::Flatpak("com.heroicgameslauncher.hgl"), found_at: &["Program Files (x86)/Epic Games", "Program Files/Epic Games"] },
    AppMap { id: "gog", windows: "GOG Galaxy", steamos: "Heroic Games Launcher", note: "Your GOG games in your library", becomes: Becomes::Flatpak("com.heroicgameslauncher.hgl"), found_at: &["Program Files (x86)/GOG Galaxy"] },
    AppMap { id: "battlenet", windows: "Battle.net", steamos: "Battle.net", note: "Through NonSteamLaunchers, in your library", becomes: Becomes::Nsl("battlenet"), found_at: &["Program Files (x86)/Battle.net"] },
    AppMap { id: "ea", windows: "EA app", steamos: "EA app", note: "Through NonSteamLaunchers, in your library", becomes: Becomes::Nsl("ea"), found_at: &["Program Files/Electronic Arts/EA Desktop"] },
    AppMap { id: "ubisoft", windows: "Ubisoft Connect", steamos: "Ubisoft Connect", note: "Through NonSteamLaunchers, in your library", becomes: Becomes::Nsl("ubisoft"), found_at: &["Program Files (x86)/Ubisoft/Ubisoft Game Launcher"] },
    AppMap { id: "rockstar", windows: "Rockstar Games Launcher", steamos: "Rockstar Games Launcher", note: "Through NonSteamLaunchers, in your library", becomes: Becomes::Nsl("rockstar"), found_at: &["Program Files/Rockstar Games/Launcher"] },
    AppMap { id: "moonlight", windows: "Moonlight", steamos: "Moonlight", note: "Same app", becomes: Becomes::Flatpak("com.moonlight_stream.Moonlight"), found_at: &["Program Files/Moonlight Game Streaming"] },
    AppMap { id: "playnite", windows: "Playnite", steamos: "Steam library", note: "Every store's games in one library", becomes: Becomes::BuiltIn, found_at: &["~/AppData/Local/Playnite"] },
];

/// The user folders on a Windows drive (Users/<name>, without the built-in ones).
pub fn windows_users(mount: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(mount.join("Users"))
        .map(|d| d.flatten().filter(|e| e.path().is_dir()).filter(|e| !["Public", "Default", "Default User", "All Users"].contains(&e.file_name().to_string_lossy().as_ref())).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// The apps from `APPS` found on this Windows drive.
pub fn apps_found(mount: &Path) -> Vec<&'static str> {
    let users = windows_users(mount);
    APPS.iter()
        .filter(|a| {
            a.found_at.iter().any(|p| match p.strip_prefix("~/") {
                Some(rest) => users.iter().any(|u| u.join(rest).exists()),
                None => mount.join(p).exists(),
            })
        })
        .map(|a| a.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_has_words_and_a_valid_target() {
        let mut ids = std::collections::HashSet::new();
        for a in APPS {
            assert!(ids.insert(a.id), "{} twice", a.id);
            assert!(!a.windows.is_empty() && !a.steamos.is_empty() && !a.note.is_empty());
            match a.becomes {
                Becomes::Flatpak(id) => assert!(id.split('.').count() >= 3, "{id}"),
                Becomes::Nsl(id) => assert!(crate::nsl::launcher(id).is_some(), "{id}"),
                _ => {}
            }
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-bring-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Everything under a folder, as paths from it, sorted.
    fn tree(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                out.push(e.path().strip_prefix(dir).unwrap().to_string_lossy().into_owned());
                if e.path().is_dir() {
                    stack.push(e.path());
                }
            }
        }
        out.sort();
        out
    }

    /// A game on a made-up Windows drive (files of several pieces among them) and a library to copy it into.
    fn game(d: &Path) -> (WinGame, PathBuf) {
        let lib = d.join("win/steamapps");
        let dir = lib.join("common/Hades II");
        fs::create_dir_all(dir.join("Content/Maps")).unwrap();
        fs::write(dir.join("Hades2.exe"), vec![7u8; (2 << 20) + 5]).unwrap();
        fs::write(dir.join("Content/a.pak"), vec![1u8; (1 << 20) + 1]).unwrap();
        fs::write(dir.join("Content/Maps/b.map"), b"map").unwrap();
        fs::write(dir.join("Content/empty"), b"").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.join("link")).unwrap();
        let manifest = lib.join("appmanifest_1145360.acf");
        fs::write(&manifest, "\"AppState\"\n{\n\"appid\" \"1145360\"\n}\n").unwrap();
        let here = d.join("home/steamapps");
        fs::create_dir_all(here.join("common")).unwrap();
        (WinGame { app_id: "1145360".into(), name: "Hades II".into(), dir, manifest, bytes: 0 }, here)
    }

    #[test]
    fn a_game_is_copied_whole_with_its_manifest_last() {
        let d = scratch("whole");
        let (g, here) = game(&d);
        let pairs = copy_plan(&g, &here);
        let source = tree(&d.join("win"));
        // At every look while it copies, Steam has nothing to find: no folder of that name, no manifest.
        let seen = std::cell::RefCell::new(Vec::new());
        let done = copy_game("Hades II", &pairs, &|_| None, &|words, part| {
            assert!(!pairs[0].1.exists() && !pairs[1].1.exists(), "something was in place while it still copied");
            if !words.is_empty() {
                seen.borrow_mut().push((words.to_string(), part));
            }
            true
        })
        .unwrap();
        assert_eq!((done.files, done.of_files, done.bytes, done.of_bytes), (5, 5, done.of_bytes, (3 << 20) + 6 + 3 + fs::metadata(&g.manifest).unwrap().len()));
        assert_eq!(seen.borrow().first().map(|(w, p)| (w.as_str(), *p)), Some(("Copying Hades II: 0 of 5 files, 0 bytes of 3.0 MB", Some(0.0))));
        // The files are there (the link was passed over), the manifest too, and nothing else.
        assert_eq!(tree(&here), ["appmanifest_1145360.acf", "common", "common/Hades II", "common/Hades II/Content", "common/Hades II/Content/Maps", "common/Hades II/Content/Maps/b.map", "common/Hades II/Content/a.pak", "common/Hades II/Content/empty", "common/Hades II/Hades2.exe"]);
        assert_eq!(fs::read(here.join("common/Hades II/Hades2.exe")).unwrap(), vec![7u8; (2 << 20) + 5]);
        assert_eq!(fs::read(here.join("appmanifest_1145360.acf")).unwrap(), fs::read(&g.manifest).unwrap());
        assert_eq!(tree(&d.join("win")), source);
        // A second go is refused in plain words and changes nothing: a game that is there is never written over.
        fs::write(here.join("common/Hades II/save.sav"), b"mine").unwrap();
        let before = tree(&here);
        let why = copy_game("Hades II", &pairs, &|_| None, &|_, _| true).unwrap_err();
        assert!(why.starts_with("Hades II is in Steam's library here already") && why.ends_with("nothing was copied."), "{why}");
        // The folder alone (no manifest), and the manifest alone: refused just the same.
        fs::remove_file(here.join("appmanifest_1145360.acf")).unwrap();
        assert!(copy_game("Hades II", &pairs, &|_| None, &|_, _| true).is_err());
        assert_eq!(fs::read(here.join("common/Hades II/save.sav")).unwrap(), b"mine");
        assert!(!here.join("appmanifest_1145360.acf").exists());
        fs::remove_dir_all(here.join("common/Hades II")).unwrap();
        fs::write(here.join("appmanifest_1145360.acf"), "kept").unwrap();
        assert!(copy_game("Hades II", &pairs, &|_| None, &|_, _| true).is_err());
        assert_eq!((fs::read_to_string(here.join("appmanifest_1145360.acf")).unwrap().as_str(), before.len(), here.join("common/Hades II").exists()), ("kept", before.len(), false));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_copy_stopped_anywhere_leaves_nothing_behind() {
        let d = scratch("stop");
        let (g, here) = game(&d);
        let pairs = copy_plan(&g, &here);
        // How often it asks whether to go on, from the first look to right before the folder gets its name.
        let asked = std::cell::Cell::new(0usize);
        copy_game("Hades II", &pairs, &|_| None, &|_, _| {
            asked.set(asked.get() + 1);
            true
        })
        .unwrap();
        let looks = asked.get();
        assert!(looks > 8, "{looks}");
        fs::remove_dir_all(here.join("common/Hades II")).unwrap();
        fs::remove_file(here.join("appmanifest_1145360.acf")).unwrap();
        // Stopped at each of them in turn (while it counts, in the middle of a file, between files, at the very
        // last look): no folder, no half folder, no manifest.
        for at in 0..looks {
            let n = std::cell::Cell::new(0usize);
            let result = copy_game("Hades II", &pairs, &|_| None, &|_, _| {
                n.set(n.get() + 1);
                n.get() <= at
            });
            assert_eq!(result.unwrap_err(), crate::STOPPED, "stopped at look {at}");
            assert_eq!(tree(&here), ["common"], "stopped at look {at}");
        }
        // A file that can't be read fails the copy and leaves as little.
        let (g2, _) = (g.clone(), ());
        fs::set_permissions(g2.dir.join("Content/a.pak"), std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();
        if fs::read(g2.dir.join("Content/a.pak")).is_err() {
            let why = copy_game("Hades II", &pairs, &|_| None, &|_, _| true).unwrap_err();
            assert!(why.starts_with("Couldn't copy Hades II:"), "{why}");
            assert_eq!(tree(&here), ["common"]);
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_copy_needs_room_and_clears_what_a_cut_short_run_left() {
        let d = scratch("room");
        let (g, here) = game(&d);
        let pairs = copy_plan(&g, &here);
        // Not enough room: said before anything is made.
        let why = copy_game("Hades II", &pairs, &|_| Some(1 << 20), &|_, _| true).unwrap_err();
        assert!(why.starts_with("Not enough room for Hades II: it needs 3.0 MB, and 1.0 MB is free."), "{why}");
        assert_eq!(tree(&here), ["common"]);
        // The system's own answer is a number for a folder that is there, and for one that isn't yet.
        assert!(free_at(&here.join("common/not/there/yet")).is_some_and(|f| f > 0));
        // What a run that was killed left: its half folder with Turbine Hub's note goes at the next start, the
        // same name without the note is somebody else's and stops the copy, untouched.
        let part = part_of(&pairs[0].1).unwrap();
        assert_eq!(part, here.join("common/Hades II.turbinehub-part"));
        fs::create_dir_all(part.join(FILES).join("Content")).unwrap();
        fs::write(part.join(FILES).join("Content/a.pak"), b"half").unwrap();
        fs::write(part.join(MARK), "").unwrap();
        let other = here.join("common/Other Game.turbinehub-part");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("mine.txt"), "x").unwrap();
        copy_game("Hades II", &pairs, &|_| None, &|_, _| true).unwrap();
        assert!(!part.exists() && other.join("mine.txt").exists() && here.join("appmanifest_1145360.acf").exists());
        assert_eq!(fs::read(here.join("common/Hades II/Content/a.pak")).unwrap().len(), (1 << 20) + 1);
        fs::remove_dir_all(here.join("common/Hades II")).unwrap();
        fs::remove_file(here.join("appmanifest_1145360.acf")).unwrap();
        fs::create_dir_all(&part).unwrap();
        fs::write(part.join("mine.txt"), "x").unwrap();
        let why = copy_game("Hades II", &pairs, &|_| None, &|_, _| true).unwrap_err();
        assert!(why.contains("isn't Turbine Hub's"), "{why}");
        assert!(part.join("mine.txt").exists() && !here.join("common/Hades II").exists() && !here.join("appmanifest_1145360.acf").exists());
        let _ = fs::remove_dir_all(&d);
    }
}

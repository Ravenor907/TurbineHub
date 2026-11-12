//! The places of the Windows view and the real folders behind them, and the two-way mapping between a Windows-style
//! path (C:\Users\deck\Downloads) and the real one (/home/deck/Downloads).

use super::fs::{Fs, Meta, NodeKind, clean};
use super::quick::Quick;
use super::{Place, View};
use crate::games::{Game, Runtime, Scan, Store};
use crate::hardware::Drive;
use std::path::{Component, Path, PathBuf};

pub const ROAMING: &str = "Roaming";
pub const LOCAL: &str = "Local";
pub const STORE_APPS: &str = "Apps from the Store";
pub const GAME_FILES: &str = "Game files";
pub const GAME_WINDOWS: &str = "Its Windows C: drive";
pub const GAME_SAVES: &str = "Saves";
pub const GAME_SNAPSHOTS: &str = "Save snapshots";
pub const BIN: &str = "Recycle Bin";

/// Who is looking: the explorer does everything as this user and never as anyone else.
#[derive(Debug, Clone, Default)]
pub struct Who {
    /// The home folder's real path.
    pub home: PathBuf,
    pub user: String,
    pub uid: u32,
    pub gids: Vec<u32>,
    /// Turbine Hub's own folders (real paths): shown, never changed from here.
    pub own: Vec<PathBuf>,
    /// Turbine Hub's data folder, for the save snapshots of each game.
    pub data: Option<PathBuf>,
}

impl Who {
    /// The user this program runs as, with `home` as the home folder.
    pub fn read(fs: &dyn Fs, home: &Path) -> Who {
        use std::os::unix::fs::MetadataExt;
        let home = clean(home);
        let at_home = fs.follow(&home).ok();
        let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).ok().or(at_home.map(|m| m.uid)).unwrap_or(1000);
        let mut gids: Vec<u32> = std::fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("Gid:") || l.starts_with("Groups:"))
            .flat_map(|l| l.split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect::<Vec<u32>>())
            .collect();
        if gids.is_empty() {
            gids.extend(at_home.map(|m| m.gid));
        }
        gids.sort_unstable();
        gids.dedup();
        let user = home.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "deck".into());
        Who { home, user, uid, gids, own: Vec::new(), data: None }
    }

    /// May this user write what the permission bits describe? (For a folder: add and remove names in it.)
    pub fn may_write(&self, m: &Meta) -> bool {
        if self.uid == 0 {
            true
        } else if m.uid == self.uid {
            m.mode & 0o200 != 0
        } else if self.gids.contains(&m.gid) {
            m.mode & 0o020 != 0
        } else {
            m.mode & 0o002 != 0
        }
    }
}

/// One of the player's folders with the name Windows gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct UserDir {
    /// Desktop, Documents, Downloads, Music, Pictures or Videos.
    pub name: &'static str,
    pub path: PathBuf,
    /// Shown under its Windows name inside the home folder (not when another folder there has that very name).
    pub mapped: bool,
}

/// A drive other than the one SteamOS is on.
#[derive(Debug, Clone, PartialEq)]
pub struct DriveAt {
    pub letter: char,
    /// "SD card", the drive's own label or its folder's name.
    pub label: String,
    pub mount: PathBuf,
    /// The file system, when known ("ext4", "ntfs", ...).
    pub fs: String,
    pub removable: bool,
    /// The partition's name ("mmcblk0p1"), when known.
    pub device: String,
    /// A Windows drive: Turbine Hub only ever reads those.
    pub read_only: bool,
}

impl DriveAt {
    /// "SD card (D:)".
    pub fn title(&self) -> String {
        format!("{} ({}:)", self.label, self.letter)
    }
}

/// An installed game and its folders.
#[derive(Debug, Clone, PartialEq)]
pub struct GameAt {
    /// "steam:1086940".
    pub id: String,
    /// The name shown under C:\Games (the game's own name, made unique and free of slashes).
    pub name: String,
    pub files: PathBuf,
    /// The C: drive of the pretend Windows it runs in, when there is one.
    pub windows: Option<PathBuf>,
    /// Where a game that runs without Windows keeps its saves, when one of the usual places is there.
    pub saves: Option<PathBuf>,
    /// Turbine Hub's snapshots of its saves, when it has any.
    pub snapshots: Option<PathBuf>,
}

/// An installed Flatpak app.
#[derive(Debug, Clone, PartialEq)]
pub struct AppAt {
    /// "com.discordapp.Discord".
    pub id: String,
    /// The app's own name when its files give one, else the id.
    pub name: String,
    /// The app's folder: <install>/app/<id>.
    pub dir: PathBuf,
    /// The files it runs from: <dir>/current/active/files.
    pub files: PathBuf,
    /// Installed for everyone (/var/lib/flatpak) and not only this user.
    pub system: bool,
}

/// One line of the navigation pane.
#[derive(Debug, Clone, PartialEq)]
pub struct NavItem {
    pub label: String,
    pub place: Place,
    /// home, folder, pc, drive, game, app or bin.
    pub icon: &'static str,
    /// A pinned Quick access folder.
    pub pinned: bool,
    /// 1 for a drive under This PC.
    pub depth: u8,
    /// A line is drawn above it.
    pub starts_group: bool,
}

/// Everything the explorer knows about where things are on this machine. Read once, and again after a drive comes or
/// goes or a game or an app is installed.
#[derive(Debug, Clone, Default)]
pub struct Places {
    pub who: Who,
    /// The system's name for the C: drive ("SteamOS").
    pub os: String,
    pub dirs: Vec<UserDir>,
    pub drives: Vec<DriveAt>,
    pub games: Vec<GameAt>,
    pub apps: Vec<AppAt>,
    /// Every Steam library's steamapps/common folder, Steam's own first.
    pub libraries: Vec<PathBuf>,
    /// Steam's own folders, which are never renamed, moved or deleted from here.
    pub steam: Vec<PathBuf>,
    /// The folders Flatpak apps are installed in (<install>/app), the system's first.
    pub app_roots: Vec<PathBuf>,
    /// The pinned folders and the files opened lately (the interface loads and saves it).
    pub quick: Quick,
    /// (real folder, how it reads in the Windows view), the made-up names only.
    rules: Vec<(PathBuf, String)>,
}

const XDG: &[(&str, &str)] = &[("DESKTOP", "Desktop"), ("DOWNLOAD", "Downloads"), ("DOCUMENTS", "Documents"), ("PICTURES", "Pictures"), ("MUSIC", "Music"), ("VIDEOS", "Videos")];

/// A name that can stand between two backslashes.
fn plain_name(name: &str) -> String {
    let n: String = name.chars().map(|c| if c == '/' || c == '\\' || c.is_control() { ' ' } else { c }).collect();
    n.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn exists(fs: &dyn Fs, p: &Path) -> bool {
    fs.meta(p).is_ok()
}

fn is_dir(fs: &dyn Fs, p: &Path) -> bool {
    fs.follow(p).is_ok_and(|m| m.is_dir())
}

fn text(fs: &dyn Fs, p: &Path) -> String {
    fs.head(p, 256 * 1024).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// The folders ~/.config/user-dirs.dirs names (the standard names where it says nothing), those that are there.
pub fn user_dirs(fs: &dyn Fs, home: &Path) -> Vec<UserDir> {
    let file = text(fs, &home.join(".config/user-dirs.dirs"));
    let mut out: Vec<UserDir> = Vec::new();
    for (key, name) in XDG {
        let named = file.lines().map(str::trim).find_map(|l| l.strip_prefix(&format!("XDG_{key}_DIR="))).map(|v| {
            let v = v.trim().trim_matches('"').replace("\\\"", "\"").replace("\\\\", "\\");
            match v.strip_prefix("$HOME") {
                Some(rest) => home.join(rest.trim_start_matches('/')),
                None => PathBuf::from(v),
            }
        });
        let path = clean(&named.filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(name)));
        // "$HOME/" switches a folder off; a folder that isn't there is no tile.
        if path == home || !is_dir(fs, &path) || out.iter().any(|d| d.path == path) {
            continue;
        }
        let own_name = path.file_name().is_some_and(|n| n.to_string_lossy() == *name);
        let mapped = path.parent() == Some(home) && (own_name || !exists(fs, &home.join(name)));
        out.push(UserDir { name, path, mapped });
    }
    out
}

/// The other drives: what is mounted under /run/media/<user> and every other real partition that isn't part of the
/// system, with letters from D: on.
pub fn drives(fs: &dyn Fs, who: &Who, hardware: &[Drive]) -> Vec<DriveAt> {
    let media = Path::new("/run/media").join(&who.user);
    let mut found: Vec<PathBuf> = Vec::new();
    let mut names = fs.names(&media).unwrap_or_default();
    names.sort();
    for n in names {
        let p = media.join(n);
        // A link here is a second name for a drive (SteamOS keeps /run/media/mmcblk0p1 that way).
        if fs.meta(&p).is_ok_and(|m| m.kind == NodeKind::Folder) {
            found.push(p);
        }
    }
    const SYSTEM: &[&str] = &["/var", "/boot", "/efi", "/esp", "/etc", "/usr", "/opt", "/root", "/srv", "/nix", "/tmp", "/proc", "/sys", "/dev"];
    let mut others: Vec<PathBuf> = hardware
        .iter()
        .flat_map(|d| d.partitions.iter())
        .filter_map(|p| p.mount.as_deref())
        .map(|m| clean(Path::new(m)))
        .filter(|m| m != Path::new("/") && m != Path::new("/home") && *m != who.home && !SYSTEM.iter().any(|s| m.starts_with(s)) && m.parent() != Some(Path::new("/run/media")))
        .filter(|m| !found.contains(m) && is_dir(fs, m))
        .collect();
    others.sort();
    others.dedup();
    found.extend(others);
    let mut out = Vec::new();
    for (mount, letter) in found.into_iter().zip('D'..='Z') {
        let part = hardware.iter().flat_map(|d| d.partitions.iter().map(move |p| (d, p))).find(|(_, p)| p.mount.as_deref().map(|m| clean(Path::new(m))).as_deref() == Some(mount.as_path()));
        let folder = mount.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (fs_type, removable, device, own_label) = match part {
            Some((d, p)) => (p.fs.clone(), d.removable, p.name.clone(), p.label.clone()),
            None => (String::new(), true, String::new(), String::new()),
        };
        let windows = fs_type.starts_with("ntfs") || fs_type == "BitLocker";
        let label = if removable && device.starts_with("mmcblk") {
            "SD card".to_string()
        } else if !own_label.is_empty() {
            plain_name(&own_label)
        } else if windows {
            "Windows drive".to_string()
        } else {
            plain_name(&folder)
        };
        out.push(DriveAt { letter, label, mount, fs: fs_type, removable, device, read_only: windows });
    }
    out
}

/// The name in a .desktop file's [Desktop Entry].
fn desktop_name(text: &str) -> Option<String> {
    let mut inside = false;
    for l in text.lines().map(str::trim) {
        if l.starts_with('[') {
            inside = l == "[Desktop Entry]";
        } else if inside && let Some(v) = l.strip_prefix("Name=") {
            let v = plain_name(v);
            return (!v.is_empty()).then_some(v);
        }
    }
    None
}

/// The installed Flatpak apps, for everyone and for this user, by name.
pub fn apps(fs: &dyn Fs, home: &Path) -> (Vec<AppAt>, Vec<PathBuf>) {
    let mut out: Vec<AppAt> = Vec::new();
    let mut roots = Vec::new();
    for (root, system) in [(PathBuf::from("/var/lib/flatpak/app"), true), (home.join(".local/share/flatpak/app"), false)] {
        let Ok(mut ids) = fs.names(&root) else { continue };
        roots.push(root.clone());
        ids.sort();
        for id in ids {
            let id = id.to_string_lossy().into_owned();
            let dir = root.join(&id);
            let active = dir.join("current/active");
            if !is_dir(fs, &dir) {
                continue;
            }
            let files = if is_dir(fs, &active.join("files")) { active.join("files") } else { dir.clone() };
            let name = [active.join(format!("export/share/applications/{id}.desktop")), active.join(format!("files/share/applications/{id}.desktop"))]
                .iter()
                .find_map(|f| desktop_name(&text(fs, f)))
                .unwrap_or_else(|| id.clone());
            // The same app installed twice keeps both entries apart.
            let name = if out.iter().any(|a| same(&a.name, &name)) { format!("{name} ({id})") } else { name };
            out.push(AppAt { id, name, dir, files, system });
        }
    }
    (out, roots)
}

/// Where a game that runs without Windows usually keeps its saves, when that folder is there.
fn native_saves(fs: &dyn Fs, home: &Path, names: &[String]) -> Option<PathBuf> {
    let unity = home.join(".config/unity3d");
    for company in fs.names(&unity).unwrap_or_default() {
        for n in names {
            let p = unity.join(&company).join(n);
            if is_dir(fs, &p) {
                return Some(p);
            }
        }
    }
    for base in [".local/share", ".config"] {
        for n in names {
            for n in [n.clone(), n.to_lowercase()] {
                let p = home.join(base).join(n);
                if is_dir(fs, &p) {
                    return Some(p);
                }
            }
        }
    }
    None
}

impl Places {
    /// Reads the places of this machine. `hardware` is `Machine::drives`; `games` what `scan_games` found (with the
    /// paths the app sees them at).
    pub fn read(fs: &dyn Fs, who: Who, hardware: &[Drive], games: &[Game]) -> Places {
        let home = who.home.clone();
        let os = text(fs, Path::new("/etc/os-release")).lines().find_map(|l| l.strip_prefix("NAME=")).map(|v| plain_name(v.trim_matches('"'))).filter(|v| !v.is_empty()).unwrap_or_else(|| "SteamOS".into());
        let dirs = user_dirs(fs, &home);
        let drives = drives(fs, &who, hardware);
        let (apps, app_roots) = apps(fs, &home);

        // Steam's own folders and every library.
        let mut libraries = Vec::new();
        let mut steam = vec![home.join(".steam")];
        if let Some(disk_home) = fs.disk(&home) {
            let mut scan = Scan::default();
            for root in crate::steam::steam_roots(&disk_home) {
                for (i, lib) in crate::steam::libraries(&root, &mut scan).into_iter().enumerate() {
                    let Some(lib) = fs.real(&lib) else { continue };
                    let apps = lib.join("steamapps");
                    for p in [lib.clone(), apps.clone(), apps.join("common"), apps.join("compatdata")] {
                        if !steam.contains(&p) {
                            steam.push(p);
                        }
                    }
                    if i == 0 {
                        steam.extend([lib.join("userdata"), lib.join("config")]);
                    }
                    if !libraries.contains(&apps.join("common")) {
                        libraries.push(apps.join("common"));
                    }
                }
            }
        }

        let mut list: Vec<GameAt> = Vec::new();
        for g in games {
            let Some(files) = fs.real(&g.install_dir) else { continue };
            let folder = files.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut name = plain_name(&g.name);
            if name.is_empty() {
                name = plain_name(&folder);
            }
            if list.iter().any(|x| same(&x.name, &name)) {
                name = format!("{name} ({})", g.store.label());
            }
            if list.iter().any(|x| same(&x.name, &name)) {
                name = format!("{name} {}", plain_name(&g.store_id));
            }
            let windows = g.prefix.as_ref().and_then(|p| fs.real(&p.join("drive_c"))).filter(|p| is_dir(fs, p));
            let saves = (g.runtime == Runtime::Native || (g.store == Store::Steam && windows.is_none())).then(|| native_saves(fs, &home, &[g.name.clone(), folder.clone()])).flatten();
            let snapshots = who.data.as_ref().map(|d| crate::snapshots::game_dir(d, &g.id())).filter(|p| fs.names(p).is_ok_and(|n| !n.is_empty()));
            list.push(GameAt { id: g.id(), name, files, windows, saves, snapshots });
        }

        let mut places = Places { who, os, dirs, drives, games: list, apps, libraries, steam, app_roots, quick: Quick::default(), rules: Vec::new() };
        places.quick = Quick::standard(&places);
        places.rules = places.make_rules();
        places
    }

    /// Reads the places with the games found under the home folder.
    pub fn scan(fs: &dyn Fs, who: Who, hardware: &[Drive]) -> Places {
        let games = fs.disk(&who.home).map(|h| crate::scan_games(&h).games).unwrap_or_default();
        Places::read(fs, who, hardware, &games)
    }

    /// C:\Users: the folder the home folder is in.
    pub fn users(&self) -> PathBuf {
        match self.who.home.parent() {
            Some(p) if p != Path::new("/") => p.to_path_buf(),
            _ => self.who.home.clone(),
        }
    }

    /// The Recycle Bin of the home folder's drive.
    pub fn home_trash(&self) -> PathBuf {
        // Never $XDG_DATA_HOME: inside the Flatpak that is the app's own folder, not the one every file manager uses.
        self.who.home.join(".local/share/Trash")
    }

    fn appdata(&self) -> [(&'static str, PathBuf); 3] {
        let h = &self.who.home;
        [(ROAMING, h.join(".config")), (LOCAL, h.join(".local/share")), (STORE_APPS, h.join(".var/app"))]
    }

    fn make_rules(&self) -> Vec<(PathBuf, String)> {
        let home = &self.who.home;
        let user = format!("C:\\Users\\{}", plain_name(&home.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        let mut r: Vec<(PathBuf, String)> = vec![("/".into(), "C:".into()), ("/usr".into(), "C:\\Windows".into()), ("/etc".into(), "C:\\Windows\\etc".into())];
        let user = if self.users() == *home { "C:\\Users".to_string() } else { user };
        r.push((self.users(), "C:\\Users".into()));
        r.push((home.clone(), user.clone()));
        for d in self.dirs.iter().filter(|d| d.mapped) {
            r.push((d.path.clone(), format!("{user}\\{}", d.name)));
        }
        for (name, path) in self.appdata() {
            r.push((path, format!("{user}\\AppData\\{name}")));
        }
        for a in &self.apps {
            r.push((home.join(".var/app").join(&a.id), format!("{user}\\AppData\\{STORE_APPS}\\{}", a.name)));
            r.push((a.files.clone(), format!("C:\\Program Files\\{}", a.name)));
        }
        for root in &self.app_roots {
            r.push((root.clone(), "C:\\Program Files".into()));
        }
        for lib in &self.libraries {
            r.push((lib.clone(), "C:\\Games".into()));
        }
        for g in &self.games {
            let base = format!("C:\\Games\\{}", g.name);
            r.push((g.files.clone(), format!("{base}\\{GAME_FILES}")));
            for (path, name) in [(&g.windows, GAME_WINDOWS), (&g.saves, GAME_SAVES), (&g.snapshots, GAME_SNAPSHOTS)] {
                if let Some(p) = path {
                    r.push((p.clone(), format!("{base}\\{name}")));
                }
            }
        }
        for d in &self.drives {
            r.push((d.mount.clone(), format!("{}:", d.letter)));
        }
        for t in self.trash_folders() {
            r.push((t.join("files"), BIN.into()));
        }
        r
    }

    /// Every folder a Recycle Bin can be in: the home folder's and one per other drive (both forms the Trash
    /// specification allows). They need not exist.
    pub fn trash_folders(&self) -> Vec<PathBuf> {
        let mut out = vec![self.home_trash()];
        for d in &self.drives {
            out.push(d.mount.join(".Trash").join(self.who.uid.to_string()));
            out.push(d.mount.join(format!(".Trash-{}", self.who.uid)));
        }
        out
    }

    fn rule_for(&self, real: &Path) -> (&Path, &str) {
        let first = |base: &Path| real.strip_prefix(base).ok().and_then(|r| r.components().next()).map(|c| c.as_os_str().to_string_lossy().into_owned());
        self.rules
            .iter()
            .filter(|(base, shown)| {
                if !real.starts_with(base) {
                    return false;
                }
                // A real name that reads like a made-up one keeps its plain path, so every path has one meaning.
                match (shown.as_str(), first(base)) {
                    ("C:\\Windows", Some(n)) => !same(&n, "etc"),
                    ("C:\\Games", Some(n)) => !self.games.iter().any(|g| same(&g.name, &n)),
                    ("C:\\Program Files", Some(n)) => !self.apps.iter().any(|a| same(&a.name, &n)),
                    _ => true,
                }
            })
            .max_by_key(|(base, _)| base.components().count())
            .map(|(b, s)| (b.as_path(), s.as_str()))
            .unwrap_or((Path::new("/"), "C:"))
    }

    /// The Windows-style path to show for a real path: C:\Users\deck\Downloads for /home/deck/Downloads. Worked out
    /// from the text of the path alone (a link keeps the name it was reached by).
    pub fn windows_path(&self, real: &Path) -> String {
        let real = clean(real);
        let (base, shown) = self.rule_for(&real);
        let mut out = shown.to_string();
        for c in real.strip_prefix(base).unwrap_or(&real).components() {
            if let Component::Normal(n) = c {
                out.push('\\');
                out.push_str(&n.to_string_lossy());
            }
        }
        // A drive by itself reads "C:\".
        if out.len() == 2 && out.ends_with(':') {
            out.push('\\');
        }
        out
    }

    /// Both forms of "Copy as path": the Windows-style one in quotes, as Windows hands it out, and the real one.
    pub fn copy_as_path(&self, real: &Path) -> (String, String) {
        (format!("\"{}\"", self.windows_path(real)), clean(real).to_string_lossy().into_owned())
    }

    pub fn game(&self, id: &str) -> Option<&GameAt> {
        self.games.iter().find(|g| g.id == id)
    }

    /// The other drive this path is on, if any.
    pub fn drive_of(&self, real: &Path) -> Option<&DriveAt> {
        self.drives.iter().filter(|d| real.starts_with(&d.mount)).max_by_key(|d| d.mount.components().count())
    }

    /// The drive a path is on, by its title in this view.
    pub fn drive_title(&self, real: &Path, view: View) -> String {
        match (self.drive_of(real), view) {
            (Some(d), View::Windows) => d.title(),
            (Some(d), View::Linux) => d.mount.to_string_lossy().into_owned(),
            (None, View::Windows) => format!("{} (C:)", self.os),
            (None, View::Linux) => "/".into(),
        }
    }

    /// The real folders behind a place (none for the two start pages).
    pub fn real_paths(&self, place: &Place) -> Vec<PathBuf> {
        match place {
            Place::Home | Place::ThisPc => Vec::new(),
            Place::Folder(p) => vec![p.clone()],
            Place::AppData => self.appdata().into_iter().map(|(_, p)| p).collect(),
            Place::Games => self.libraries.clone(),
            Place::Game(id) => self.game(id).map(|g| std::iter::once(g.files.clone()).chain(g.windows.clone()).chain(g.saves.clone()).collect()).unwrap_or_default(),
            Place::Programs => self.app_roots.clone(),
            Place::Windows => vec!["/usr".into(), "/etc".into()],
            Place::Trash => vec![self.home_trash().join("files")],
        }
    }

    /// The same place the way this view thinks of it: in the Windows view the real folder behind a made-up place is
    /// that place, and the Linux view has no Home page of its own.
    pub fn canon(&self, place: Place, view: View) -> Place {
        match (view, place) {
            (View::Linux, Place::Home) => Place::Folder(self.who.home.clone()),
            (View::Windows, Place::Folder(p)) => {
                if p == Path::new("/usr") {
                    Place::Windows
                } else if self.libraries.contains(&p) {
                    Place::Games
                } else if self.app_roots.contains(&p) {
                    Place::Programs
                } else if self.trash_folders().iter().any(|t| t.join("files") == p) {
                    Place::Trash
                } else {
                    Place::Folder(p)
                }
            }
            (_, place) => place,
        }
    }

    /// The place's name in this view.
    pub fn title(&self, place: &Place, view: View) -> String {
        let win = view == View::Windows;
        match place {
            Place::Home => "Home".into(),
            Place::ThisPc => if win { "This PC" } else { "Computer" }.into(),
            Place::AppData => if win { "AppData" } else { ".config, .local/share, .var/app" }.into(),
            Place::Games => if win { "Games" } else { "Steam games" }.into(),
            Place::Game(id) => self.game(id).map(|g| if win { g.name.clone() } else { name_of(&g.files) }).unwrap_or_else(|| "Game".into()),
            Place::Programs => if win { "Program Files" } else { "Flatpak apps" }.into(),
            Place::Windows => if win { "Windows" } else { "usr, etc" }.into(),
            Place::Trash => if win { BIN } else { "Trash" }.into(),
            Place::Folder(p) if win => {
                let (base, shown) = self.rule_for(p);
                if p != base {
                    name_of(p)
                } else if shown == "C:" {
                    format!("{} (C:)", self.os)
                } else if let Some(d) = self.drives.iter().find(|d| d.mount == *p) {
                    d.title()
                } else {
                    shown.rsplit('\\').next().unwrap_or(shown).to_string()
                }
            }
            Place::Folder(p) => if p == Path::new("/") { "/".into() } else { name_of(p) },
        }
    }

    /// What the address bar holds when it is typed into: the Windows-style path or the real one.
    pub fn shown_path(&self, place: &Place, view: View) -> String {
        let user = || self.windows_path(&self.who.home);
        match (view, place) {
            (View::Windows, Place::Folder(p)) => self.windows_path(p),
            (View::Windows, Place::AppData) => format!("{}\\AppData", user()),
            (View::Windows, Place::Games) => "C:\\Games".into(),
            (View::Windows, Place::Game(_)) => format!("C:\\Games\\{}", self.title(place, view)),
            (View::Windows, Place::Programs) => "C:\\Program Files".into(),
            (View::Windows, Place::Windows) => "C:\\Windows".into(),
            (View::Linux, p) if !matches!(p, Place::Home | Place::ThisPc) => self.real_paths(p).first().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| self.title(p, view)),
            (_, p) => self.title(p, view),
        }
    }

    /// The real folder names of `dir` that are there, as one real name: exactly `name`, or the only one that differs
    /// from it by upper and lower case alone (a path typed the Windows way).
    fn real_child(&self, fs: &dyn Fs, dir: &Path, name: &str) -> PathBuf {
        let exact = dir.join(name);
        if exists(fs, &exact) {
            return exact;
        }
        let mut close = fs.names(dir).unwrap_or_default().into_iter().filter(|n| same(&n.to_string_lossy(), name));
        match (close.next(), close.next()) {
            (Some(one), None) => dir.join(one),
            _ => exact,
        }
    }

    /// The place one step into `at` by a name of the Windows view (or a real name).
    pub fn child(&self, fs: &dyn Fs, at: &Place, name: &str) -> Place {
        let home = &self.who.home;
        match at {
            Place::Home | Place::ThisPc => {
                if same(name, BIN) {
                    Place::Trash
                } else if same(name, &format!("{} (C:)", self.os)) || same(name, "C:") {
                    Place::Folder("/".into())
                } else if let Some(d) = self.drives.iter().find(|d| same(&d.title(), name) || same(&format!("{}:", d.letter), name)) {
                    Place::Folder(d.mount.clone())
                } else if let Some(d) = self.dirs.iter().find(|d| same(d.name, name)) {
                    Place::Folder(d.path.clone())
                } else {
                    Place::Folder(self.real_child(fs, home, name))
                }
            }
            Place::Trash => {
                let items = super::trash::items(fs, self);
                let hit = items.iter().find(|t| t.name == name).or_else(|| items.iter().find(|t| name_of(&t.original) == name));
                Place::Folder(hit.map(|t| t.at.clone()).unwrap_or_else(|| self.home_trash().join("files").join(name)))
            }
            Place::AppData => match self.appdata().into_iter().find(|(n, _)| same(n, name)) {
                Some((_, p)) => Place::Folder(p),
                None => Place::Folder(self.real_child(fs, home, name)),
            },
            Place::Games => {
                if let Some(g) = self.games.iter().find(|g| same(&g.name, name)) {
                    return Place::Game(g.id.clone());
                }
                let found = self.libraries.iter().map(|l| self.real_child(fs, l, name)).find(|p| exists(fs, p));
                Place::Folder(found.unwrap_or_else(|| self.libraries.first().cloned().unwrap_or_else(|| home.join(".local/share/Steam/steamapps/common")).join(name)))
            }
            Place::Game(id) => {
                let Some(g) = self.game(id) else { return Place::Games };
                let named = [(Some(&g.files), GAME_FILES), (g.windows.as_ref(), GAME_WINDOWS), (g.saves.as_ref(), GAME_SAVES), (g.snapshots.as_ref(), GAME_SNAPSHOTS)];
                match named.iter().find(|(p, n)| p.is_some() && same(n, name)) {
                    Some((Some(p), _)) => Place::Folder((*p).clone()),
                    _ => Place::Folder(self.real_child(fs, &g.files, name)),
                }
            }
            Place::Programs => {
                if let Some(a) = self.apps.iter().find(|a| same(&a.name, name)) {
                    Place::Folder(a.files.clone())
                } else if let Some(a) = self.apps.iter().find(|a| a.id == name) {
                    Place::Folder(a.dir.clone())
                } else {
                    Place::Folder(self.app_roots.first().cloned().unwrap_or_else(|| "/var/lib/flatpak/app".into()).join(name))
                }
            }
            Place::Windows => Place::Folder(if same(name, "etc") { "/etc".into() } else { self.real_child(fs, Path::new("/usr"), name) }),
            Place::Folder(p) => {
                if p == Path::new("/") {
                    for (n, place) in [("Users", Place::Folder(self.users())), ("Games", Place::Games), ("Program Files", Place::Programs), ("Windows", Place::Windows)] {
                        if same(n, name) {
                            return place;
                        }
                    }
                }
                if p == home && same(name, "AppData") && !exists(fs, &home.join(name)) {
                    return Place::AppData;
                }
                if exists(fs, &p.join(name)) {
                    return Place::Folder(p.join(name));
                }
                if p == home && let Some(d) = self.dirs.iter().find(|d| d.mapped && same(d.name, name)) {
                    return Place::Folder(d.path.clone());
                }
                if *p == home.join(".var/app") && let Some(a) = self.apps.iter().find(|a| same(&a.name, name)) {
                    return Place::Folder(p.join(&a.id));
                }
                Place::Folder(self.real_child(fs, p, name))
            }
        }
    }

    /// One step up from a place, or None at the top.
    pub fn up(&self, fs: &dyn Fs, place: &Place, view: View) -> Option<Place> {
        let root = || Place::Folder("/".into());
        let home = || Place::Folder(self.who.home.clone());
        Some(match (view, place) {
            (_, Place::Home | Place::ThisPc) => return None,
            (_, Place::Trash) => Place::ThisPc,
            (_, Place::Game(_)) => Place::Games,
            (_, Place::AppData) => home(),
            (View::Windows, Place::Games | Place::Programs | Place::Windows) => root(),
            (View::Linux, Place::Windows) => root(),
            (View::Linux, p @ (Place::Games | Place::Programs)) => match self.real_paths(p).first().and_then(|p| p.parent()) {
                Some(up) => Place::Folder(up.to_path_buf()),
                None => root(),
            },
            (View::Linux, Place::Folder(p)) => match p.parent() {
                Some(up) => Place::Folder(up.to_path_buf()),
                None => Place::ThisPc,
            },
            (View::Windows, Place::Folder(p)) => {
                let (base, shown) = self.rule_for(p);
                if p != base {
                    return p.parent().map(|up| self.canon(Place::Folder(up.to_path_buf()), view));
                }
                match shown.rsplit_once('\\') {
                    None => Place::ThisPc,
                    Some((up, _)) => self.resolve(fs, &format!("{up}\\")).unwrap_or(Place::ThisPc),
                }
            }
        })
    }

    /// The address bar's pieces, from the top down to this place, each with where it leads.
    pub fn crumbs(&self, fs: &dyn Fs, place: &Place, view: View) -> Vec<(String, Place)> {
        let place = self.canon(place.clone(), view);
        if view == View::Linux && let Place::Folder(p) = &place {
            let mut at = PathBuf::from("/");
            let mut out = vec![("/".to_string(), Place::Folder(at.clone()))];
            for c in p.components().filter(|c| matches!(c, Component::Normal(_))) {
                at.push(c);
                out.push((c.as_os_str().to_string_lossy().into_owned(), Place::Folder(at.clone())));
            }
            return out;
        }
        let mut out = vec![(self.title(&place, view), place.clone())];
        let mut at = place;
        // A path is never deeper than this; a loop never is one.
        for _ in 0..256 {
            if at == Place::Trash {
                break;
            }
            let Some(up) = self.up(fs, &at, view) else { break };
            out.push((self.title(&up, view), up.clone()));
            at = up;
        }
        out.reverse();
        out
    }

    /// Where a typed path leads. Windows-style (C:\Users\deck\Downloads, upper or lower case, either slash, in
    /// quotes, with "..", %AppData%), a real path (/home/deck/Downloads, ~/Downloads, file://) and the names of
    /// the made-up places all work. The place need not exist.
    pub fn resolve(&self, fs: &dyn Fs, typed: &str) -> Result<Place, String> {
        const HOW: &str = "Type a full path, like C:\\Users or /home.";
        let mut t = typed.trim().trim_matches('"').trim().to_string();
        if t.is_empty() {
            return Err(HOW.into());
        }
        if let Some(rest) = t.strip_prefix("file://") {
            t = String::from_utf8_lossy(&super::trash::unescape(rest)).into_owned();
        }
        let [roaming, local, _] = self.appdata();
        for (var, to) in [("%appdata%", &roaming.1), ("%localappdata%", &local.1), ("%userprofile%", &self.who.home)] {
            if t.len() >= var.len() && t.is_char_boundary(var.len()) && same(&t[..var.len()], var) {
                t = format!("{}{}", to.display(), t[var.len()..].replace('\\', "/"));
            }
        }
        if t == "~" || t.starts_with("~/") {
            t = format!("{}{}", self.who.home.display(), &t[1..]);
        }
        if t.starts_with('/') {
            return Ok(Place::Folder(clean(Path::new(&t))));
        }
        let parts: Vec<&str> = t.split(['\\', '/']).collect();
        let first = parts[0].trim();
        let letter = first.chars().next().filter(|c| c.is_ascii_alphabetic() && first.len() == 2 && first.ends_with(':')).map(|c| c.to_ascii_uppercase());
        let mut at = match letter {
            Some('C') => Place::Folder("/".into()),
            Some(l) => match self.drives.iter().find(|d| d.letter == l) {
                Some(d) => Place::Folder(d.mount.clone()),
                None => return Err(format!("There is no drive {l}: here.")),
            },
            None if same(first, BIN) || same(first, "Trash") => Place::Trash,
            None if same(first, "This PC") || same(first, "Computer") => Place::ThisPc,
            None if same(first, "Home") || same(first, "Quick access") => Place::Home,
            None => return Err(HOW.into()),
        };
        let mut i = 1;
        while i < parts.len() {
            let name = parts[i];
            i += 1;
            if name.is_empty() || name == "." {
                continue;
            }
            if name == ".." {
                // ".." at the top of a drive stays there, as on Windows.
                if let Some(up) = self.up(fs, &at, View::Windows).filter(|p| !matches!(p, Place::ThisPc | Place::Home)) {
                    at = up;
                }
                continue;
            }
            let mut next = self.child(fs, &at, name);
            // A real name may hold a backslash: when the plain piece isn't there, a longer run of pieces may be.
            if matches!(&next, Place::Folder(p) if !exists(fs, p)) {
                for end in i..parts.len() {
                    let joined = parts[i - 1..=end].join("\\");
                    if let Place::Folder(p) = self.child(fs, &at, &joined) && exists(fs, &p) {
                        next = Place::Folder(p);
                        i = end + 1;
                        break;
                    }
                }
            }
            at = next;
        }
        Ok(self.canon(at, View::Windows))
    }

    /// The real path a typed path stands for (the first one, where a place is made of several; None for the start
    /// pages and for text that is no path).
    pub fn real_path(&self, fs: &dyn Fs, typed: &str) -> Option<PathBuf> {
        self.real_paths(&self.resolve(fs, typed).ok()?).into_iter().next()
    }

    /// Where the address bar goes for a typed path: the folder to open and, when the path names a file, that file
    /// to put the highlight on. Says so in plain words when nothing is there.
    pub fn go(&self, fs: &dyn Fs, typed: &str, view: View) -> Result<(Place, Option<PathBuf>), String> {
        let place = self.resolve(fs, typed)?;
        let Place::Folder(p) = &place else { return Ok((self.canon(place, view), None)) };
        match fs.follow(p).or_else(|_| fs.meta(p)) {
            Ok(m) if m.is_dir() => Ok((self.canon(place, view), None)),
            Ok(_) => Ok((self.canon(Place::Folder(p.parent().unwrap_or(Path::new("/")).to_path_buf()), view), Some(p.clone()))),
            Err(_) => Err(format!("Can't find {}. Check the spelling and try again.", typed.trim())),
        }
    }

    /// The navigation pane, top to bottom.
    pub fn nav(&self, view: View) -> Vec<NavItem> {
        let item = |label: String, place: Place, icon: &'static str| NavItem { label, place, icon, pinned: false, depth: 0, starts_group: false };
        let line = |mut i: NavItem| {
            i.starts_group = true;
            i
        };
        let mut out = Vec::new();
        let root = Place::Folder("/".into());
        if view == View::Windows {
            out.push(item("Home".into(), Place::Home, "home"));
            for (n, p) in self.quick.pins.iter().enumerate() {
                let mut i = item(self.title(&Place::Folder(p.clone()), view), self.canon(Place::Folder(p.clone()), view), "folder");
                i.pinned = true;
                i.starts_group = n == 0;
                out.push(i);
            }
            out.push(line(item("This PC".into(), Place::ThisPc, "pc")));
            out.push(NavItem { depth: 1, ..item(self.title(&root, view), root, "drive") });
            for d in &self.drives {
                out.push(NavItem { depth: 1, ..item(d.title(), Place::Folder(d.mount.clone()), "drive") });
            }
            out.push(item("Games".into(), Place::Games, "game"));
            out.push(item("Apps".into(), Place::Programs, "app"));
            out.push(line(item(BIN.into(), Place::Trash, "bin")));
        } else {
            out.push(item(format!("Home  {}", self.who.home.display()), Place::Folder(self.who.home.clone()), "home"));
            out.push(item("Root  /".into(), root, "drive"));
            for d in &self.drives {
                out.push(item(d.mount.to_string_lossy().into_owned(), Place::Folder(d.mount.clone()), "drive"));
            }
            out.push(line(item("Steam games".into(), Place::Games, "game")));
            out.push(item("Flatpak apps".into(), Place::Programs, "app"));
            out.push(item("App data".into(), Place::AppData, "folder"));
            out.push(line(item("Trash".into(), Place::Trash, "bin")));
        }
        out
    }

    /// The line of help above a place's list, for the places that need one.
    pub fn note(&self, place: &Place, view: View) -> Option<String> {
        let win = view == View::Windows;
        let text = match place {
            Place::AppData if win => "App settings and saves. Windows keeps these in AppData; SteamOS keeps them in three hidden folders, shown here together.",
            Place::AppData => "Three hidden folders hold what Windows calls AppData.",
            Place::Games if win => "Every installed game, from all your Steam libraries and Heroic, in one list.",
            Place::Game(id) if win => match self.game(id) {
                Some(g) if g.windows.is_some() => "Looking for saves or config files? They are in this game's own Windows C: drive.",
                Some(g) if g.saves.is_some() => "This game runs without Windows. Its saves are in Saves.",
                _ => return None,
            },
            Place::Programs if win => "Installed apps. On SteamOS each app is a sealed package: open one to see its files, and use the Store tab to remove it.",
            Place::Windows if win => "This is SteamOS itself. It is locked so updates always work and nothing here can break the system. You can look; nothing can be changed.",
            Place::Windows => "Read only. SteamOS keeps /usr locked; Turbine Hub never unlocks it.",
            _ => return None,
        };
        Some(text.into())
    }
}

/// The last name of a path, as text.
pub fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into())
}

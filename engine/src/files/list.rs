//! What a place holds: its entries with everything the list shows, hidden files left out or not, sorted, grouped the
//! way Explorer groups, and searched (in the folder, or below it within limits).

use super::fs::{Fs, Meta, NodeKind, Space};
use super::places::{BIN, GAME_FILES, GAME_SAVES, GAME_SNAPSHOTS, GAME_WINDOWS, Places, name_of};
use super::trash;
use super::{Place, View, When};
use crate::json::quote;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    File,
    /// A link; `Entry::link` says where to.
    Link,
    Drive,
    App,
    Game,
    /// The Recycle Bin, on This PC.
    Bin,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Folder => "folder",
            Kind::File => "file",
            Kind::Link => "link",
            Kind::Drive => "drive",
            Kind::App => "app",
            Kind::Game => "game",
            Kind::Bin => "bin",
        }
    }
}

/// One line of a list.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The name this view shows.
    pub name: String,
    /// The name on the disk.
    pub real_name: String,
    /// The real path.
    pub path: PathBuf,
    /// Where pressing it goes, for everything that opens as a folder.
    pub opens: Option<Place>,
    pub kind: Kind,
    /// It opens as a folder (a folder, a link to one, a drive, a game, an app).
    pub is_dir: bool,
    /// What a link says, as written.
    pub link: Option<String>,
    /// A link that leads nowhere.
    pub broken: bool,
    /// A plain word for what it is: Picture, Text, Zip archive, File folder...
    pub type_word: String,
    /// Bytes, for files.
    pub size: Option<u64>,
    /// Seconds since 1970.
    pub modified: Option<u64>,
    pub hidden: bool,
    /// The player may change it.
    pub writable: bool,
    /// It is in a place that is only looked at (SteamOS itself, a Windows drive, Turbine Hub's own folders).
    pub locked: bool,
    /// "drwxr-xr-x", in the Linux view.
    pub mode: String,
    /// Who owns it, in the Linux view.
    pub owner: String,
    pub group: String,
    /// Free and total room, for a drive.
    pub space: Option<Space>,
    /// A second line: the real name behind a made-up one, where a search hit is, where a deleted file was.
    pub detail: String,
    /// When it was deleted (2026-11-12T09:31:05), in the Recycle Bin.
    pub deleted: Option<String>,
    /// The group it is listed under, when the list is grouped.
    pub group_label: String,
}

impl Entry {
    /// What the icon shows: "locked" for anything in a locked place, else the kind.
    pub fn icon(&self) -> &'static str {
        if self.locked { "locked" } else { self.kind.key() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Name,
    Date,
    Size,
    Type,
}

/// How a list is cut into groups, as Explorer's "Group by" does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Group {
    #[default]
    None,
    /// Today, Yesterday, Earlier this week, Last week, Earlier this month, Last month, Earlier this year, A long
    /// time ago.
    Date,
    Type,
    /// Empty, Tiny, Small, Medium, Large, Huge, Gigantic.
    Size,
    /// 0 to 9, A to H, I to P, Q to Z, Other.
    Name,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Show hidden files. None takes the view's own way: off in the Windows view, on in the Linux view.
    pub hidden: Option<bool>,
    pub sort: Sort,
    pub descending: bool,
    pub folders_first: bool,
    pub group: Group,
    /// Only names that hold this text (upper and lower case alike).
    pub search: String,
    /// Now on the player's clock, for the date groups.
    pub now: When,
}

impl Default for Options {
    fn default() -> Options {
        Options { hidden: None, sort: Sort::Name, descending: false, folders_first: true, group: Group::None, search: String::new(), now: When::utc(crate::now()) }
    }
}

/// A place and what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub place: Place,
    pub title: String,
    /// What the address bar holds when typed into.
    pub shown_path: String,
    /// The real folders behind the place.
    pub real: Vec<PathBuf>,
    pub entries: Vec<Entry>,
    /// The groups in order, each with how many entries it has (empty when the list isn't grouped).
    pub groups: Vec<(String, usize)>,
    /// A line of help for this place.
    pub note: Option<String>,
    /// Why nothing here can be changed, when that is so.
    pub locked: Option<String>,
    /// How many hidden entries were left out.
    pub hidden: usize,
    /// Room on the drive this place is on.
    pub free: Option<Space>,
    /// Why the place couldn't be read, in plain words.
    pub problem: Option<String>,
}

/// The plain word for a file by its ending.
pub fn type_by_name(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()).map(|(_, e)| e)?;
    Some(match ext {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tif" | "tiff" | "avif" | "heic" => "Picture",
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" | "wmv" | "mpg" | "mpeg" => "Video",
        "mp3" | "flac" | "ogg" | "opus" | "wav" | "m4a" | "aac" | "wma" => "Music",
        "txt" | "log" | "md" | "nfo" | "csv" => "Text",
        "cfg" | "conf" | "ini" | "yaml" | "yml" | "toml" | "json" | "xml" | "vdf" | "acf" | "reg" => "Settings file",
        "pdf" => "PDF document",
        "doc" | "docx" | "odt" | "rtf" => "Document",
        "xls" | "xlsx" | "ods" => "Spreadsheet",
        "ppt" | "pptx" | "odp" => "Presentation",
        "zip" => "Zip archive",
        "7z" | "rar" | "tar" | "gz" | "xz" | "zst" | "bz2" | "tgz" => "Archive",
        "flatpak" | "flatpakref" => "App installer",
        "appimage" => "App",
        "exe" | "msi" | "bat" | "cmd" => "Windows program",
        "dll" => "Windows program part",
        "lnk" | "url" => "Windows shortcut",
        "desktop" => "Shortcut",
        "sh" | "py" => "Script",
        "iso" | "img" | "chd" => "Disc image",
        "ttf" | "otf" | "woff" | "woff2" => "Font",
        "html" | "htm" => "Web page",
        "sav" | "save" => "Saved game",
        "so" => "Program part",
        _ => return None,
    })
}

/// The plain word for a file by its first bytes, for names that say nothing.
pub fn type_by_bytes(b: &[u8]) -> &'static str {
    let at = |i: usize, sig: &[u8]| b.len() >= i + sig.len() && &b[i..i + sig.len()] == sig;
    if at(0, b"\x89PNG") || at(0, b"\xff\xd8\xff") || at(0, b"GIF8") || at(0, b"BM") && b.len() > 14 && b[6..10] == [0; 4] || at(0, b"RIFF") && at(8, b"WEBP") {
        "Picture"
    } else if at(0, b"RIFF") && at(8, b"WAVE") || at(0, b"OggS") || at(0, b"fLaC") || at(0, b"ID3") {
        "Music"
    } else if at(0, b"RIFF") && at(8, b"AVI ") || at(0, b"\x1a\x45\xdf\xa3") || at(4, b"ftyp") {
        "Video"
    } else if at(0, b"%PDF") {
        "PDF document"
    } else if at(0, b"PK\x03\x04") {
        "Zip archive"
    } else if at(0, b"7z\xbc\xaf") || at(0, b"\x1f\x8b") || at(0, b"\xfd7zXZ") || at(0, b"\x28\xb5\x2f\xfd") || at(0, b"Rar!") || at(257, b"ustar") {
        "Archive"
    } else if at(0, b"\x7fELF") {
        "Program"
    } else if at(0, b"MZ") {
        "Windows program"
    } else if at(0, b"#!") {
        "Script"
    } else if b.is_empty() {
        "Empty file"
    } else if !b.contains(&0) && (std::str::from_utf8(b).is_ok() || b.iter().filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()).count() * 10 >= b.len() * 9) {
        "Text"
    } else {
        "File"
    }
}

/// "drwxr-xr-x" for a kind and its permission bits.
pub fn mode_text(kind: NodeKind, mode: u32) -> String {
    let mut s = String::with_capacity(10);
    s.push(match kind {
        NodeKind::Folder => 'd',
        NodeKind::Link => 'l',
        NodeKind::File => '-',
        NodeKind::Other => '?',
    });
    for (shift, special, low, up) in [(6, 0o4000, 's', 'S'), (3, 0o2000, 's', 'S'), (0, 0o1000, 't', 'T')] {
        let bits = mode >> shift;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(match (bits & 1 != 0, mode & special != 0) {
            (true, false) => 'x',
            (false, false) => '-',
            (true, true) => low,
            (false, true) => up,
        });
    }
    s
}

/// Is this name hidden the way file managers on Linux hide names?
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('.') || name.ends_with('~')
}

/// Builds entries, remembering the names of users and groups as it goes.
struct Look<'a> {
    fs: &'a dyn Fs,
    places: &'a Places,
    view: View,
    users: Option<HashMap<u32, String>>,
    groups: Option<HashMap<u32, String>>,
}

fn id_names(fs: &dyn Fs, file: &str) -> HashMap<u32, String> {
    let text = fs.head(Path::new(file), 1 << 20).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut f = l.split(':');
            let name = f.next()?;
            Some((f.nth(1)?.parse().ok()?, name.to_string()))
        })
        .collect()
}

impl<'a> Look<'a> {
    fn new(fs: &'a dyn Fs, places: &'a Places, view: View) -> Look<'a> {
        Look { fs, places, view, users: None, groups: None }
    }

    fn folder_word(&self) -> &'static str {
        if self.view == View::Windows { "File folder" } else { "Folder" }
    }

    /// The entry for a real path, listed under `path` but read at `at` (the same unless a link led somewhere the app
    /// finds elsewhere). Something that can't be read still gets its line.
    fn real(&mut self, path: &Path, at: &Path) -> Entry {
        let name = name_of(path);
        let mut e = Entry {
            name: name.clone(),
            real_name: name.clone(),
            path: path.to_path_buf(),
            opens: None,
            kind: Kind::File,
            is_dir: false,
            link: None,
            broken: false,
            type_word: "Can't be read".into(),
            size: None,
            modified: None,
            hidden: is_hidden(&name),
            writable: false,
            locked: self.places.locked(path).is_some(),
            mode: String::new(),
            owner: String::new(),
            group: String::new(),
            space: None,
            detail: String::new(),
            deleted: None,
            group_label: String::new(),
        };
        let Ok(own) = self.fs.meta(at) else { return e };
        let mut meta = own;
        if own.kind == NodeKind::Link {
            e.kind = Kind::Link;
            e.link = self.fs.link_target(at).ok().map(|t| t.to_string_lossy().into_owned());
            match self.fs.follow(at) {
                Ok(m) => meta = m,
                Err(_) => e.broken = true,
            }
        }
        e.modified = Some(meta.modified);
        match (own.kind, meta.kind) {
            (NodeKind::Link, _) if e.broken => e.type_word = "Broken link".into(),
            (k, NodeKind::Folder) => {
                e.is_dir = true;
                e.opens = Some(Place::Folder(path.to_path_buf()));
                e.type_word = if k == NodeKind::Link { "Link to a folder" } else { self.folder_word() }.into();
                if k != NodeKind::Link {
                    e.kind = Kind::Folder;
                }
            }
            (k, NodeKind::File) => {
                e.size = Some(meta.size);
                // A link without an ending of its own is what its target's name says.
                let word = type_by_name(&name).or_else(|| e.link.as_deref().and_then(type_by_name)).unwrap_or("File");
                e.type_word = if k == NodeKind::Link { format!("Link to a {}", word.to_lowercase()) } else { word.to_string() };
            }
            _ => e.type_word = "System file".into(),
        }
        e.writable = !e.locked && self.places.who.may_write(&meta);
        if self.view == View::Linux {
            self.ownership(&mut e, &own);
        }
        e
    }

    fn ownership(&mut self, e: &mut Entry, m: &Meta) {
        let fs = self.fs;
        e.mode = mode_text(m.kind, m.mode);
        e.owner = self.users.get_or_insert_with(|| id_names(fs, "/etc/passwd")).get(&m.uid).cloned().unwrap_or_else(|| m.uid.to_string());
        e.group = self.groups.get_or_insert_with(|| id_names(fs, "/etc/group")).get(&m.gid).cloned().unwrap_or_else(|| m.gid.to_string());
    }

    /// A real folder under a made-up name.
    fn named(&mut self, name: &str, path: &Path, opens: Place, kind: Kind, type_word: &str) -> Entry {
        let mut e = self.real(path, path);
        if self.view == View::Windows {
            e.detail = if e.real_name == name { String::new() } else { path.to_string_lossy().into_owned() };
            e.name = name.to_string();
            e.type_word = type_word.to_string();
            e.hidden = false;
        }
        e.kind = kind;
        e.is_dir = true;
        e.opens = Some(opens);
        e
    }

    /// Every entry of one real folder. Err with plain words when the folder can't be read.
    fn folder(&mut self, dir: &Path) -> Result<Vec<Entry>, String> {
        // Where the folder really is: a link to /usr leads to the machine's /usr, wherever the app finds that.
        let at = self.fs.canonical(dir).unwrap_or_else(|_| dir.to_path_buf());
        let names = self.fs.names(&at).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => "You aren't allowed to look in this folder.".to_string(),
            std::io::ErrorKind::NotFound => "This folder isn't there (any more).".to_string(),
            std::io::ErrorKind::NotADirectory => "This is a file, not a folder.".to_string(),
            _ => format!("This folder can't be read ({e})."),
        })?;
        let hide: Vec<String> = if names.iter().any(|n| n == ".hidden") {
            self.fs.head(&at.join(".hidden"), 64 * 1024).map(|b| String::from_utf8_lossy(&b).lines().map(|l| l.trim().to_string()).collect()).unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut out = Vec::with_capacity(names.len());
        for n in names {
            let mut e = self.real(&dir.join(&n), &at.join(&n));
            e.hidden |= hide.contains(&e.real_name);
            out.push(e);
        }
        Ok(out)
    }
}

fn drive_entry(look: &mut Look, name: String, path: &Path, room_of: &Path) -> Entry {
    let mut e = look.real(path, path);
    e.name = name;
    e.kind = Kind::Drive;
    e.is_dir = true;
    e.opens = Some(Place::Folder(path.to_path_buf()));
    e.type_word = "Drive".into();
    e.hidden = false;
    e.space = look.fs.space(room_of);
    e
}

fn entries_of(look: &mut Look, place: &Place) -> Result<Vec<Entry>, String> {
    let (fs, places, view) = (look.fs, look.places, look.view);
    let win = view == View::Windows;
    let home = &places.who.home;
    Ok(match place {
        Place::Home => {
            let mut out = Vec::new();
            for p in &places.quick.pins {
                let target = Place::Folder(p.clone());
                let mut e = look.named(&places.title(&target, view), p, places.canon(target, view), Kind::Folder, "File folder");
                e.group_label = "Quick access".into();
                e.detail = if win { places.windows_path(p) } else { p.to_string_lossy().into_owned() };
                out.push(e);
            }
            for p in &places.quick.recent {
                let mut e = look.real(p, p);
                e.group_label = "Recent".into();
                e.hidden = false;
                e.detail = p.parent().map(|d| if win { places.windows_path(d) } else { d.to_string_lossy().into_owned() }).unwrap_or_default();
                out.push(e);
            }
            out
        }
        Place::ThisPc => {
            let mut out = Vec::new();
            for d in &places.dirs {
                let mut e = look.named(d.name, &d.path, Place::Folder(d.path.clone()), Kind::Folder, "File folder");
                e.group_label = if win { "Folders" } else { "Home" }.into();
                out.push(e);
            }
            let label = if win { "Devices and drives" } else { "Mounted" };
            // SteamOS keeps the player's files on the home partition: that is the room that matters for C:.
            let mut c = drive_entry(look, places.drive_title(Path::new("/"), view), Path::new("/"), home);
            c.group_label = label.into();
            out.push(c);
            for d in &places.drives {
                let mut e = drive_entry(look, places.drive_title(&d.mount, view), &d.mount, &d.mount);
                e.group_label = label.into();
                e.detail = if win { d.mount.to_string_lossy().into_owned() } else { d.label.clone() };
                out.push(e);
            }
            let bin = places.home_trash().join("files");
            let mut e = look.real(&bin, &bin);
            e.name = if win { BIN } else { "Trash" }.into();
            e.kind = Kind::Bin;
            e.is_dir = true;
            e.hidden = false;
            e.opens = Some(Place::Trash);
            e.size = Some(trash::count(fs, places) as u64);
            e.type_word = "Deleted files".into();
            e.group_label = label.into();
            out.push(e);
            out
        }
        Place::AppData => {
            let folders = [(super::places::ROAMING, ".config", "App settings"), (super::places::LOCAL, ".local/share", "App data"), (super::places::STORE_APPS, ".var/app", "Each Store app's own files")];
            folders
                .iter()
                .filter(|(_, rel, _)| fs.meta(&home.join(rel)).is_ok())
                .map(|(name, rel, word)| {
                    let p = home.join(rel);
                    let mut e = look.named(name, &p, Place::Folder(p.clone()), Kind::Folder, word);
                    if !win {
                        e.name = rel.to_string();
                        e.hidden = false;
                        e.detail = (*name).to_string();
                    }
                    e
                })
                .collect()
        }
        Place::Games => {
            let mut out = Vec::new();
            for g in &places.games {
                let mut e = look.named(&g.name, &g.files, Place::Game(g.id.clone()), Kind::Game, "Game");
                if !win {
                    e.opens = Some(Place::Folder(g.files.clone()));
                    e.detail = g.name.clone();
                }
                out.push(e);
            }
            // What else Steam keeps beside the games (Proton, its runtimes): there, and out of the way.
            for lib in &places.libraries {
                for mut e in look.folder(lib).unwrap_or_default() {
                    if places.games.iter().any(|g| g.files == e.path) {
                        continue;
                    }
                    if win {
                        e.hidden = true;
                        e.type_word = if e.is_dir { "Steam tool" } else { "Steam file" }.into();
                    }
                    out.push(e);
                }
            }
            out
        }
        Place::Game(id) => {
            let Some(g) = places.game(id) else { return Err("This game isn't installed (any more).".into()) };
            let parts = [(Some(&g.files), GAME_FILES, "The game itself"), (g.windows.as_ref(), GAME_WINDOWS, "This game's pretend Windows"), (g.saves.as_ref(), GAME_SAVES, "This game runs without Windows"), (g.snapshots.as_ref(), GAME_SNAPSHOTS, "Kept by Turbine Hub")];
            let mut out = Vec::new();
            for (path, name, word) in parts {
                let Some(path) = path else { continue };
                let mut e = look.named(name, path, Place::Folder(path.clone()), Kind::Folder, word);
                if !win {
                    e.detail = name.to_string();
                }
                e.detail = if win { path.to_string_lossy().into_owned() } else { name.to_string() };
                out.push(e);
            }
            out
        }
        Place::Programs => places
            .apps
            .iter()
            .map(|a| {
                let (path, opens) = if win { (&a.files, &a.files) } else { (&a.dir, &a.dir) };
                let mut e = look.named(&a.name, path, Place::Folder(opens.clone()), Kind::App, if a.system { "App" } else { "App (only for you)" });
                e.real_name = a.id.clone();
                e.detail = if win { a.id.clone() } else { a.name.clone() };
                e
            })
            .collect(),
        Place::Windows => {
            if !win {
                return Ok(["/usr", "/etc"].iter().map(|p| look.real(Path::new(p), Path::new(p))).collect());
            }
            let mut out: Vec<Entry> = look.folder(Path::new("/usr"))?.into_iter().filter(|e| e.real_name != "etc").collect();
            out.push(look.named("etc", Path::new("/etc"), Place::Folder("/etc".into()), Kind::Folder, "Settings"));
            out
        }
        Place::Trash => trash::items(fs, places)
            .into_iter()
            .map(|t| {
                let mut e = look.real(&t.at, &t.at);
                e.name = name_of(&t.original);
                e.hidden = false;
                // What is in the bin opens only after it is put back.
                e.opens = None;
                e.writable = false;
                if e.type_word == "File" {
                    e.type_word = type_by_name(&e.name).unwrap_or("File").into();
                }
                e.detail = t.original.parent().map(|d| if win { places.windows_path(d) } else { d.to_string_lossy().into_owned() }).unwrap_or_default();
                e.deleted = Some(t.deleted);
                e
            })
            .collect(),
        Place::Folder(p) if win && p == Path::new("/") => {
            let mut out = vec![
                look.named("Games", places.libraries.first().map(PathBuf::as_path).unwrap_or(home), Place::Games, Kind::Game, "Your installed games"),
                look.named("Program Files", places.app_roots.first().map(PathBuf::as_path).unwrap_or(Path::new("/var/lib/flatpak/app")), Place::Programs, Kind::App, "Your installed apps"),
                look.named("Users", &places.users(), Place::Folder(places.users()), Kind::Folder, "File folder"),
                look.named("Windows", Path::new("/usr"), Place::Windows, Kind::Folder, "SteamOS system, locked"),
            ];
            // The rest of the top of the disk is the system's business: there for those who ask for hidden files.
            for mut e in look.folder(p)? {
                if ["usr", "etc"].contains(&e.real_name.as_str()) || e.path == places.users() {
                    continue;
                }
                e.hidden = true;
                out.push(e);
            }
            out
        }
        Place::Folder(p) => {
            let mut out = look.folder(p)?;
            if win && p == home {
                for e in &mut out {
                    if let Some(d) = places.dirs.iter().find(|d| d.mapped && d.path == e.path) {
                        e.name = d.name.to_string();
                    }
                }
                if fs.meta(&home.join("AppData")).is_err() {
                    let config = home.join(".config");
                    out.push(look.named("AppData", &config, Place::AppData, Kind::Folder, "App settings and saves"));
                }
            }
            if win && *p == home.join(".var/app") {
                for e in &mut out {
                    if let Some(a) = places.apps.iter().find(|a| a.id == e.real_name) {
                        e.name = a.name.clone();
                        e.detail = a.id.clone();
                    }
                }
            }
            if win {
                for e in &mut out {
                    if let Some(place) = e.opens.take() {
                        e.opens = Some(places.canon(place, view));
                    }
                }
            }
            out
        }
    })
}

/// A key that puts names in the order people expect: upper and lower case alike, and "file2" before "file10".
fn name_key(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut out = String::with_capacity(lower.len() + 8);
    let mut digits = String::new();
    let flush = |digits: &mut String, out: &mut String| {
        if !digits.is_empty() {
            let trimmed = digits.trim_start_matches('0');
            for _ in trimmed.len()..12 {
                out.push('0');
            }
            out.push_str(trimmed);
            digits.clear();
        }
    };
    for c in lower.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            flush(&mut digits, &mut out);
            out.push(c);
        }
    }
    flush(&mut digits, &mut out);
    out
}

/// Which of Explorer's date groups a moment falls in (0 is Today), with the group's name.
pub fn date_group(now: When, secs: u64) -> (i64, &'static str) {
    let today = now.day_of(now.secs);
    let day = now.day_of(secs);
    // Weeks start on Monday (day 0, 1 Jan 1970, was a Thursday).
    let week = today - (today + 3).rem_euclid(7);
    let (y, m, _) = When::date(today);
    let month = When::day_number(y, m, 1);
    let last_month = if m == 1 { When::day_number(y - 1, 12, 1) } else { When::day_number(y, m - 1, 1) };
    if day >= today {
        (0, "Today")
    } else if day == today - 1 {
        (1, "Yesterday")
    } else if day >= week {
        (2, "Earlier this week")
    } else if day >= week - 7 {
        (3, "Last week")
    } else if day >= month {
        (4, "Earlier this month")
    } else if day >= last_month {
        (5, "Last month")
    } else if day >= When::day_number(y, 1, 1) {
        (6, "Earlier this year")
    } else {
        (7, "A long time ago")
    }
}

/// Explorer's size groups.
pub fn size_group(bytes: u64) -> (i64, &'static str) {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    match bytes {
        0 => (1, "Empty (0 KB)"),
        b if b <= 16 * KB => (2, "Tiny (up to 16 KB)"),
        b if b <= MB => (3, "Small (16 KB to 1 MB)"),
        b if b <= 128 * MB => (4, "Medium (1 to 128 MB)"),
        b if b <= GB => (5, "Large (128 MB to 1 GB)"),
        b if b <= 4 * GB => (6, "Huge (1 to 4 GB)"),
        _ => (7, "Gigantic (over 4 GB)"),
    }
}

fn name_group(name: &str) -> (i64, &'static str) {
    match name.chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('0'..='9') => (0, "0 to 9"),
        Some('A'..='H') => (1, "A to H"),
        Some('I'..='P') => (2, "I to P"),
        Some('Q'..='Z') => (3, "Q to Z"),
        _ => (4, "Other"),
    }
}

/// Leaves out hidden entries and those the search doesn't name, then sorts and groups. Answers the groups in order
/// and how many hidden entries were left out.
fn arrange(entries: &mut Vec<Entry>, view: View, opt: &Options) -> (Vec<(String, usize)>, usize) {
    let show_hidden = opt.hidden.unwrap_or(view == View::Linux);
    let before = entries.len();
    if !show_hidden {
        entries.retain(|e| !e.hidden);
    }
    let hidden = before - entries.len();
    let needle = opt.search.trim().to_lowercase();
    if !needle.is_empty() {
        entries.retain(|e| e.name.to_lowercase().contains(&needle) || e.real_name.to_lowercase().contains(&needle));
    }
    let ranks: Vec<i64> = match opt.group {
        Group::None => Vec::new(),
        _ => {
            let mut types: Vec<&str> = entries.iter().map(|e| e.type_word.as_str()).collect();
            types.sort_unstable();
            types.dedup();
            let ranked: Vec<(i64, String)> = entries
                .iter()
                .map(|e| match opt.group {
                    Group::Date => e.modified.map(|m| date_group(opt.now, m)).map(|(r, l)| (r, l.to_string())).unwrap_or((8, "No date".into())),
                    Group::Type => (types.binary_search(&e.type_word.as_str()).unwrap_or(0) as i64, e.type_word.clone()),
                    Group::Size => e.size.filter(|_| e.kind != Kind::Bin).map(size_group).map(|(r, l)| (r, l.to_string())).unwrap_or((0, "Folders".into())),
                    _ => {
                        let (r, l) = name_group(&e.name);
                        (r, l.to_string())
                    }
                })
                .collect();
            let turned = opt.descending && matches!((opt.group, opt.sort), (Group::Type, Sort::Type) | (Group::Size, Sort::Size) | (Group::Name, Sort::Name)) || !opt.descending && opt.group == Group::Date && opt.sort == Sort::Date;
            for (e, (_, label)) in entries.iter_mut().zip(&ranked) {
                e.group_label = label.clone();
            }
            ranked.into_iter().map(|(r, _)| if turned { -r } else { r }).collect()
        }
    };
    // One key per entry, made once: (group, folders first, the sort's own number, the name).
    type Key = (i64, bool, u64, String, String);
    let mut keyed: Vec<(Key, Entry)> = std::mem::take(entries)
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let number = match opt.sort {
                Sort::Date => e.modified.unwrap_or(0),
                Sort::Size => e.size.unwrap_or(0),
                _ => 0,
            };
            let number = if opt.descending { u64::MAX - number } else { number };
            let word = if opt.sort == Sort::Type { e.type_word.to_lowercase() } else { String::new() };
            ((ranks.get(i).copied().unwrap_or(0), opt.folders_first && !e.is_dir, number, word, name_key(&e.name)), e)
        })
        .collect();
    let by_text_down = opt.descending && matches!(opt.sort, Sort::Name | Sort::Type);
    keyed.sort_by(|(a, _), (b, _)| {
        let head = (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2));
        let tail = (&a.3, &a.4).cmp(&(&b.3, &b.4));
        head.then(if by_text_down { tail.reverse() } else { tail })
    });
    *entries = keyed.into_iter().map(|(_, e)| e).collect();
    (groups_of(entries), hidden)
}

fn groups_of(entries: &[Entry]) -> Vec<(String, usize)> {
    let mut groups: Vec<(String, usize)> = Vec::new();
    for e in entries.iter().filter(|e| !e.group_label.is_empty()) {
        match groups.last_mut() {
            Some((label, n)) if *label == e.group_label => *n += 1,
            _ => groups.push((e.group_label.clone(), 1)),
        }
    }
    groups
}

/// How many nameless files a list looks into for their type: enough for what is on screen, never a slow list.
const SNIFF_MAX: usize = 300;

/// What a place holds in this view. Never fails: a folder that can't be read comes back empty with the reason.
pub fn list(fs: &dyn Fs, places: &Places, place: &Place, view: View, opt: &Options) -> Listing {
    let place = places.canon(place.clone(), view);
    let mut look = Look::new(fs, places, view);
    let (mut entries, problem) = match entries_of(&mut look, &place) {
        Ok(e) => (e, None),
        Err(why) => (Vec::new(), Some(why)),
    };
    let fixed = matches!(place, Place::Home | Place::ThisPc);
    let (groups, hidden) = if fixed {
        // The start pages keep their own order and groups; only the search applies.
        let needle = opt.search.trim().to_lowercase();
        entries.retain(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle));
        (groups_of(&entries), 0)
    } else {
        arrange(&mut entries, view, opt)
    };
    // A name without an ending says nothing: the first bytes do. Only for the top of the list.
    for e in entries.iter_mut().filter(|e| e.type_word == "File" && e.kind == Kind::File).take(SNIFF_MAX) {
        if let Ok(bytes) = fs.canonical(&e.path).and_then(|at| fs.head(&at, 512)) {
            e.type_word = type_by_bytes(&bytes).into();
        }
    }
    let real = places.real_paths(&place);
    let locked = match &place {
        Place::Windows => Some(super::guard::SYSTEM_WHY),
        Place::Folder(p) => places.locked(p),
        _ => None,
    };
    let room_of = match &place {
        Place::Folder(p) if p == Path::new("/") => Some(places.who.home.clone()),
        Place::Home | Place::ThisPc | Place::Windows => Some(places.who.home.clone()),
        _ => real.first().cloned(),
    };
    Listing {
        title: places.title(&place, view),
        shown_path: places.shown_path(&place, view),
        note: places.note(&place, view),
        locked: locked.map(String::from),
        free: room_of.and_then(|p| fs.space(&p)),
        real,
        entries,
        groups,
        hidden,
        problem,
        place,
    }
}

/// How far a search below a folder may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Folders below the start (1 is the start folder alone).
    pub depth: usize,
    /// Stop after this many hits.
    pub hits: usize,
    /// Stop after looking at this many names.
    pub looked: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { depth: 6, hits: 200, looked: 50_000 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub entries: Vec<Entry>,
    /// A limit was reached: there may be more.
    pub more: bool,
}

/// Places a search never walks into: they aren't files on a drive.
const NEVER: &[&str] = &["/proc", "/sys", "/dev"];

/// Looks for names that hold `needle` in a place and the folders below it, nearest first. Never follows a link out
/// of the folder, never walks into /proc, /sys or /dev, and stops at the limits.
pub fn search(fs: &dyn Fs, places: &Places, place: &Place, view: View, needle: &str, limits: Limits) -> Found {
    let needle = needle.trim().to_lowercase();
    let mut found = Found { entries: Vec::new(), more: false };
    if needle.is_empty() {
        return found;
    }
    let mut look = Look::new(fs, places, view);
    let mut queue: VecDeque<(PathBuf, usize)> = places.real_paths(&places.canon(place.clone(), view)).into_iter().map(|p| (p, 1)).collect();
    let mut looked = 0;
    while let Some((dir, depth)) = queue.pop_front() {
        if NEVER.iter().any(|n| dir.starts_with(n)) {
            continue;
        }
        let mut names = fs.names(&dir).unwrap_or_default();
        names.sort();
        for name in names {
            looked += 1;
            if looked > limits.looked || found.entries.len() >= limits.hits {
                found.more = true;
                return found;
            }
            let path = dir.join(&name);
            if name.to_string_lossy().to_lowercase().contains(&needle) {
                let mut e = look.real(&path, &path);
                e.detail = if view == View::Windows { places.windows_path(&dir) } else { dir.to_string_lossy().into_owned() };
                found.entries.push(e);
            }
            // Only real folders are walked: a link could lead anywhere.
            if fs.meta(&path).is_ok_and(|m| m.kind == NodeKind::Folder) {
                if depth < limits.depth {
                    queue.push_back((path, depth + 1));
                } else {
                    found.more = true;
                }
            }
        }
    }
    found
}

impl Entry {
    /// One JSON object for the interface.
    pub fn to_json(&self) -> String {
        let num = |n: Option<u64>| n.map(|n| n.to_string()).unwrap_or_else(|| "null".into());
        let text = |s: &Option<String>| s.as_deref().map(quote).unwrap_or_else(|| "null".into());
        format!(
            "{{\"name\":{},\"realName\":{},\"path\":{},\"opens\":{},\"kind\":{},\"icon\":{},\"isDir\":{},\"link\":{},\"broken\":{},\"type\":{},\"size\":{},\"modified\":{},\"hidden\":{},\"writable\":{},\"locked\":{},\"mode\":{},\"owner\":{},\"group\":{},\"free\":{},\"total\":{},\"detail\":{},\"deleted\":{},\"under\":{}}}",
            quote(&self.name),
            quote(&self.real_name),
            quote(&self.path.to_string_lossy()),
            text(&self.opens.as_ref().map(Place::key)),
            quote(self.kind.key()),
            quote(self.icon()),
            self.is_dir,
            text(&self.link),
            self.broken,
            quote(&self.type_word),
            num(self.size),
            num(self.modified),
            self.hidden,
            self.writable,
            self.locked,
            quote(&self.mode),
            quote(&self.owner),
            quote(&self.group),
            num(self.space.map(|s| s.free)),
            num(self.space.map(|s| s.total)),
            quote(&self.detail),
            text(&self.deleted),
            quote(&self.group_label),
        )
    }
}

impl Listing {
    /// One JSON object for the interface.
    pub fn to_json(&self) -> String {
        let text = |s: &Option<String>| s.as_deref().map(quote).unwrap_or_else(|| "null".into());
        let num = |n: Option<u64>| n.map(|n| n.to_string()).unwrap_or_else(|| "null".into());
        format!(
            "{{\"place\":{},\"title\":{},\"shownPath\":{},\"real\":[{}],\"entries\":[{}],\"groups\":[{}],\"note\":{},\"locked\":{},\"hidden\":{},\"free\":{},\"total\":{},\"problem\":{}}}",
            quote(&self.place.key()),
            quote(&self.title),
            quote(&self.shown_path),
            self.real.iter().map(|p| quote(&p.to_string_lossy())).collect::<Vec<_>>().join(","),
            self.entries.iter().map(Entry::to_json).collect::<Vec<_>>().join(","),
            self.groups.iter().map(|(l, n)| format!("{{\"label\":{},\"count\":{n}}}", quote(l))).collect::<Vec<_>>().join(","),
            text(&self.note),
            text(&self.locked),
            self.hidden,
            num(self.free.map(|s| s.free)),
            num(self.free.map(|s| s.total)),
            text(&self.problem),
        )
    }
}

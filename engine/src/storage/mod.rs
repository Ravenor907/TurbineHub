//! The storage saver: one look at what is using space on every drive, and what of it can go.
//!
//! `scan` reads (it never changes anything): each drive and Steam library, each installed game's real size (its
//! folder, its shader cache, its Proton files) and the leftovers. Every leftover is a `Finding` in one of three
//! groups: Safe (Steam builds it again, or it is plainly garbage; ticked), Check first (it may hold saves or
//! settings; never ticked, with the reason in plain words) and Never (in use; shown, never offered).
//! `plan` turns the ticked findings into the one dialog and its steps, and `run` (see `clean`) does them after
//! looking at every single thing again. Sizes are counted with a bound on time and entries, and say "at least"
//! when the count was cut short.

mod clean;
#[cfg(test)]
mod tests;

pub use clean::{Item, Job, Site, done_words, plan, run, run_here, undo_plan};

use crate::compat_tool::{self, Proton, ProtonKind};
use crate::files::fs::{Fs, NodeKind, clean as clean_path};
use crate::files::places::Places;
use crate::files::{Disk, Map, When, Who, size_words, trash};
use crate::games::{Game, Store};
use crate::json::Json;
use crate::shortcuts::Steam;
use crate::ui::{arr, b, n, obj, s};
use crate::vdf;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Turbine Hub's own app id: its folder under ~/.var/app is never a leftover.
const OWN_APP: &str = "io.github.ravenor907.TurbineHub";
/// A drive this full gets a line in the checkup.
pub const NEARLY_FULL_PERCENT: u64 = 90;

/// A size that may have been cut short.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub bytes: u64,
    /// The count stopped early (too many files, or out of time): the real size is this or more.
    pub at_least: bool,
}

impl Size {
    pub fn exact(bytes: u64) -> Size {
        Size { bytes, at_least: false }
    }
    pub fn plus(self, o: Size) -> Size {
        Size { bytes: self.bytes.saturating_add(o.bytes), at_least: self.at_least || o.at_least }
    }
    /// "12 GB", "at least 12 GB", or "size not known" when nothing could be counted.
    pub fn words(&self) -> String {
        match (self.bytes, self.at_least) {
            (0, true) => "size not known".into(),
            (v, true) => format!("at least {}", size_words(v)),
            (v, false) => size_words(v),
        }
    }
    fn json(&self) -> Json {
        obj! {"bytes": n(self.bytes as f64), "atLeast": b(self.at_least), "words": s(self.words())}
    }
}

/// How far a look may go.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    /// Files and folders counted for one thing before its size says "at least".
    pub entries: u64,
    /// Time for all the counting together. What isn't counted by then says "at least".
    pub time: Duration,
    /// Files in Downloads are listed when they are older than this many days,
    pub downloads_days: u64,
    /// and at least this big.
    pub downloads_bytes: u64,
    /// Save snapshots kept of each kind for an installed game.
    pub keep: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { entries: 200_000, time: Duration::from_secs(10), downloads_days: 30, downloads_bytes: 100 << 20, keep: crate::snapshots::KEEP }
    }
}

/// Counts sizes inside the limits.
pub(crate) struct Meter {
    deadline: Instant,
    each: u64,
}

impl Meter {
    pub(crate) fn new(l: &Limits) -> Meter {
        Meter { deadline: Instant::now() + l.time, each: l.entries }
    }
    /// The bytes of a file or of everything in a folder. Links count as nothing and are never followed.
    pub(crate) fn size(&self, fs: &dyn Fs, p: &Path) -> Size {
        let (mut total, mut seen) = (0u64, 0u64);
        let mut stack = vec![p.to_path_buf()];
        while let Some(at) = stack.pop() {
            seen += 1;
            if seen > self.each || (seen % 64 == 1 && Instant::now() >= self.deadline) {
                return Size { bytes: total, at_least: true };
            }
            let Ok(m) = fs.meta(&at) else { continue };
            match m.kind {
                NodeKind::File => total += m.size,
                NodeKind::Folder => stack.extend(fs.names(&at).unwrap_or_default().into_iter().map(|name| at.join(name))),
                _ => {}
            }
        }
        Size::exact(total)
    }
}

/// The three groups every finding is sorted into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// Builds itself again or is plainly garbage. Ticked.
    Safe,
    /// May hold saves or settings. Never ticked, and it says what to check.
    Check,
    /// In use. Shown so the player sees why, never offered.
    Never,
}

impl Group {
    pub fn key(self) -> &'static str {
        match self {
            Group::Safe => "safe",
            Group::Check => "check",
            Group::Never => "never",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Group::Safe => "Safe to clear",
            Group::Check => "Check first",
            Group::Never => "In use",
        }
    }
    pub fn line(self) -> &'static str {
        match self {
            Group::Safe => "Steam builds these again by itself, or they are leftovers nothing needs.",
            Group::Check => "These may hold saves or settings. Nothing here is ticked until you tick it.",
            Group::Never => "Something uses these, so they stay.",
        }
    }
}

/// What a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// steamapps/shadercache/<id> of a game that is no longer installed.
    ShaderCache,
    /// steamapps/compatdata/<id> of a game that is no longer installed.
    Compatdata,
    /// pfx.turbinehub-backup-* or pfx.turbinehub-fresh-* left by "Reset Windows files".
    ResetBackup,
    /// steamapps/downloading/<id> or steamapps/temp/<id>.
    Download,
    /// A Proton no game is set to use (or, in the Never group, one that is in use).
    Proton,
    /// Steam's old client logs, or its crash dumps.
    SteamLogs,
    /// ~/.var/app/<id> of a Flatpak app that is no longer installed.
    FlatpakData,
    /// The runtimes Flatpak itself calls unused.
    FlatpakUnused,
    /// Turbine Hub's own save snapshots beyond the kept count.
    Snapshots,
    /// The Recycle Bin of one drive.
    RecycleBin,
    /// A big, old file in Downloads.
    DownloadsFile,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::ShaderCache => "shader-cache",
            Kind::Compatdata => "windows-files",
            Kind::ResetBackup => "reset-backup",
            Kind::Download => "download",
            Kind::Proton => "proton",
            Kind::SteamLogs => "steam-logs",
            Kind::FlatpakData => "app-data",
            Kind::FlatpakUnused => "flatpak-unused",
            Kind::Snapshots => "snapshots",
            Kind::RecycleBin => "recycle-bin",
            Kind::DownloadsFile => "downloads-file",
        }
    }
}

/// How a finding is freed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// Erased for good: only ever what builds itself again or is plainly garbage, and Turbine Hub's own snapshots.
    Erase,
    /// Moved to the Recycle Bin of its drive: Undo puts it back, and its room comes back when the bin is emptied.
    Bin,
    /// The Recycle Bin itself is emptied: for good.
    EmptyBin,
    /// Flatpak does it: `flatpak uninstall --unused` on the host.
    Flatpak,
}

impl How {
    pub fn key(self) -> &'static str {
        match self {
            How::Erase => "erase",
            How::Bin => "bin",
            How::EmptyBin => "empty-bin",
            How::Flatpak => "flatpak",
        }
    }
    /// What the plan says under each thing.
    pub fn words(self) -> &'static str {
        match self {
            How::Erase => "Erased for good.",
            How::Bin => "Goes to the Recycle Bin: Undo puts it back, and its room comes back when the bin is emptied.",
            How::EmptyBin => "Erased for good: nothing in it can be restored after.",
            How::Flatpak => "Flatpak removes them itself. An app that needs one later downloads it again.",
        }
    }
}

/// One thing that takes space and could go (or, in the Never group, has to stay).
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Stays the same from one look to the next, so ticks hold.
    pub id: String,
    pub kind: Kind,
    pub group: Group,
    pub title: String,
    /// Why it is safe, what to check first, or who uses it. Plain words.
    pub why: String,
    /// What would go. Real paths; Turbine Hub's own snapshots are in its data folder as the app sees it.
    pub paths: Vec<PathBuf>,
    /// The folder the paths have to be in, really and not through a link.
    pub inside: PathBuf,
    pub size: Size,
    /// Which of `Report::drives` it is on.
    pub drive: usize,
    pub how: How,
    /// Ticked without the player doing anything: only ever true in the Safe group.
    pub ticked: bool,
    /// The Steam app it belongs to, when it belongs to one.
    pub app_id: String,
    /// Inside a Steam library's steamapps: only touched while Steam is closed, unless it is an `orphan`.
    pub steam: bool,
}

impl Finding {
    /// Is it what a game that is no longer installed left behind (its shader cache, its Windows files, its
    /// unfinished download)? Steam has no use for those, so they may go while Steam runs; everything else inside
    /// a library waits for Steam to be closed.
    pub fn orphan(&self) -> bool {
        matches!(self.kind, Kind::ShaderCache | Kind::Compatdata) || (self.kind == Kind::Download && self.group == Group::Safe)
    }
}

/// One drive and what is on it.
#[derive(Debug, Clone, PartialEq)]
pub struct DriveUse {
    /// "Internal storage", "SD card" or the drive's own name.
    pub label: String,
    pub mount: PathBuf,
    pub free: u64,
    pub total: u64,
    pub removable: bool,
    /// A Windows drive: shown, never changed.
    pub read_only: bool,
    pub libraries: Vec<LibraryUse>,
}

impl DriveUse {
    /// How full it is, 0 to 100. None when the drive didn't say.
    pub fn percent_full(&self) -> Option<u64> {
        (self.total > 0).then(|| (self.total.saturating_sub(self.free)) * 100 / self.total)
    }
    pub fn nearly_full(&self) -> bool {
        self.percent_full().is_some_and(|p| p >= NEARLY_FULL_PERCENT)
    }
}

/// One Steam library on a drive.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryUse {
    pub path: PathBuf,
    pub games: usize,
    /// Its games with their shader caches and Proton files.
    pub size: Size,
}

/// What one installed game really takes.
#[derive(Debug, Clone, PartialEq)]
pub struct GameSize {
    /// "steam:1086940".
    pub id: String,
    pub name: String,
    pub store: Store,
    /// Which of `Report::drives` the game is on.
    pub drive: usize,
    pub library: PathBuf,
    /// Its own folder.
    pub files: Size,
    /// steamapps/shadercache/<id>.
    pub shaders: Size,
    /// Its Proton files: steamapps/compatdata/<id>, or the Heroic prefix.
    pub proton: Size,
}

impl GameSize {
    pub fn total(&self) -> Size {
        self.files.plus(self.shaders).plus(self.proton)
    }
    pub fn to_json(&self) -> Json {
        obj! {"id": s(&self.id), "name": s(&self.name), "store": s(self.store.label()), "drive": n(self.drive as f64), "library": s(self.library.to_string_lossy()), "files": self.files.json(), "shaders": self.shaders.json(), "proton": self.proton.json(), "total": self.total().json()}
    }
}

/// What only the host can say. Everything left at its default reads as "couldn't be asked", which refuses more,
/// never less.
#[derive(Debug, Clone, PartialEq)]
pub struct Host {
    pub steam: Steam,
    /// The Steam app ids of the games running now (`agent::running_games`). None when it couldn't be asked.
    pub running: Option<Vec<String>>,
    /// What `unused_question` printed, when it ran.
    pub flatpak_unused: Option<String>,
    /// What `sizes_question` printed, when it ran.
    pub flatpak_sizes: Option<String>,
}

impl Default for Host {
    fn default() -> Host {
        Host { steam: Steam::Unknown, running: None, flatpak_unused: None, flatpak_sizes: None }
    }
}

fn host_line(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host"].iter().chain(args).map(|x| x.to_string()).collect()
}

/// Flatpak's own dry run: it lists what it calls unused and asks before it removes anything. Run it with "n" typed
/// into it, so it only ever lists.
pub fn unused_question() -> Vec<String> {
    host_line(&["flatpak", "uninstall", "--unused"])
}

/// Every installed runtime with its size, to put a size on the dry run's list.
pub fn sizes_question() -> Vec<String> {
    host_line(&["flatpak", "list", "--runtime", "--columns=application,branch,size"])
}

/// The plan step that lets Flatpak remove what it calls unused.
pub fn unused_step() -> Vec<String> {
    host_line(&["flatpak", "uninstall", "--unused", "--noninteractive", "-y"])
}

fn looks_like_app_id(w: &str) -> bool {
    w.matches('.').count() >= 2 && w.starts_with(|c: char| c.is_ascii_alphabetic()) && !w.ends_with('.') && w.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// The runtimes in the dry run's answer: (id, branch). Reads both of Flatpak's ways of listing them: a table with
/// the id and the branch in columns, and whole refs ("runtime/org.kde.Platform/x86_64/6.7").
pub fn unused_from_answer(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        for (i, w) in words.iter().enumerate() {
            let parts: Vec<&str> = w.trim_start_matches("runtime/").split('/').collect();
            let found = match parts.as_slice() {
                [id, _arch, branch] if looks_like_app_id(id) => Some((id.to_string(), branch.to_string())),
                [id] if looks_like_app_id(id) => Some((id.to_string(), words.get(i + 1).filter(|b| b.len() <= 40 && !looks_like_app_id(b)).map(|b| b.to_string()).unwrap_or_default())),
                _ => None,
            };
            if let Some(f) = found {
                if !out.contains(&f) {
                    out.push(f);
                }
                break;
            }
        }
    }
    out
}

/// "1.2 GB" as bytes, the way Flatpak writes sizes.
fn bytes_from_words(text: &str) -> Option<u64> {
    let t = text.replace(['\u{a0}', '\u{202f}'], " ").replace(',', ".");
    let mut words = t.split_whitespace();
    let value: f64 = words.next()?.parse().ok()?;
    let unit = match words.next()?.to_ascii_lowercase().as_str() {
        "bytes" | "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    (value >= 0.0).then_some((value * unit) as u64)
}

/// The size of the runtimes Flatpak calls unused, from the list of installed runtimes. "At least" when one of
/// them isn't in the list.
fn unused_size(unused: &[(String, String)], sizes: Option<&str>) -> Size {
    let Some(text) = sizes else { return Size { bytes: 0, at_least: true } };
    let mut total = Size::default();
    for (id, branch) in unused {
        let found = text.lines().find_map(|l| {
            let mut cols = l.split('\t').map(str::trim);
            (cols.next() == Some(id.as_str()) && (branch.is_empty() || cols.next() == Some(branch.as_str()))).then(|| l.rsplit('\t').next().and_then(bytes_from_words)).flatten()
        });
        total = total.plus(found.map(Size::exact).unwrap_or(Size { bytes: 0, at_least: true }));
    }
    total
}

fn text_of(fs: &dyn Fs, p: &Path, max: usize) -> String {
    fs.head(p, max).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

fn all_digits(name: &str) -> bool {
    !name.is_empty() && name.len() <= 20 && name.bytes().all(|b| b.is_ascii_digit())
}

fn is_folder(fs: &dyn Fs, p: &Path) -> bool {
    fs.meta(p).is_ok_and(|m| m.kind == NodeKind::Folder)
}

/// Is this folder really where its path says, with no link on the way? What is in a folder that is a link to
/// another place is never offered and never touched.
pub(crate) fn plain_folder(fs: &dyn Fs, p: &Path) -> bool {
    is_folder(fs, p) && fs.canonical(p).is_ok_and(|real| real == clean_path(p))
}

/// The name of a "Reset Windows files" leftover: the backup itself, or fresh files set aside by its Undo.
pub(crate) fn reset_leftover(name: &str) -> Option<bool> {
    for (prefix, backup) in [("pfx.turbinehub-backup-", true), ("pfx.turbinehub-fresh-", false)] {
        if name.strip_prefix(prefix).is_some_and(all_digits) {
            return Some(backup);
        }
    }
    None
}

/// One appmanifest in a library.
#[derive(Debug, Clone)]
pub(crate) struct Manifest {
    pub id: String,
    pub name: String,
    pub lib: PathBuf,
    pub installdir: String,
    pub size_on_disk: u64,
    pub file: PathBuf,
}

/// What Steam and Turbine Hub's own records say right now. Read for the look, and read again right before anything
/// is freed: what was a leftover a minute ago may be a game again.
pub(crate) struct World {
    /// Steam's own folders: (real path, the path the app finds it at).
    pub roots: Vec<(PathBuf, PathBuf)>,
    /// Every library that is there (real paths; each holds steamapps).
    pub libs: Vec<PathBuf>,
    /// Libraries Steam lists that aren't there now (a card that is out, a folder the app may not read).
    pub missing: Vec<PathBuf>,
    pub manifests: Vec<Manifest>,
    /// App ids Steam's library list names as installed, also for libraries that aren't there now.
    pub listed: BTreeSet<String>,
    /// Non-Steam games: app id and name.
    pub shortcuts: BTreeMap<String, String>,
    /// Names Turbine Hub's own records remember: play sessions and save snapshots.
    pub names: BTreeMap<String, String>,
    /// Steam's compatibility tool for each app ("0" is its choice for all games): (app id, tool name).
    pub mappings: Vec<(String, String)>,
    /// Proton folders named by Turbine Hub's own launch tool (a game's pick, the one for all games).
    pub picks: Vec<PathBuf>,
    /// Heroic's settings, as text: a Proton named in them is in use.
    pub heroic: String,
    /// Every Proton installed (folders as the app finds them), with the Steam root it belongs to.
    pub protons: Vec<(usize, Proton)>,
}

impl World {
    pub(crate) fn read(fs: &dyn Fs, places: &Places, folder: &Path, data: &Path) -> World {
        let mut w = World { roots: Vec::new(), libs: Vec::new(), missing: Vec::new(), manifests: Vec::new(), listed: BTreeSet::new(), shortcuts: BTreeMap::new(), names: BTreeMap::new(), mappings: Vec::new(), picks: Vec::new(), heroic: String::new(), protons: Vec::new() };
        let Some(disk_home) = fs.disk(&places.who.home) else { return w };
        let mut scratch = crate::Scan::default();
        for root in crate::steam::steam_roots(&disk_home) {
            let Some(real_root) = fs.real(&root) else { continue };
            for lib in crate::steam::libraries(&root, &mut scratch).into_iter().filter_map(|l| fs.real(&l)) {
                if !w.libs.contains(&lib) {
                    w.libs.push(lib);
                }
            }
            // The library list also names what is installed in libraries that aren't there right now.
            if let Ok(v) = vdf::parse(&text_of(fs, &real_root.join("steamapps/libraryfolders.vdf"), 4 << 20)) {
                for (_, entry) in v.get("libraryfolders").map(|l| l.entries()).unwrap_or(&[]) {
                    w.listed.extend(entry.get("apps").map(|a| a.entries()).unwrap_or(&[]).iter().map(|(id, _)| id.clone()));
                    if let Some(p) = entry.str_at(&["path"]).or(entry.as_str()).map(PathBuf::from)
                        && !p.join("steamapps").is_dir()
                        && !w.missing.contains(&p)
                    {
                        w.missing.push(p);
                    }
                }
            }
            if let Ok(v) = vdf::parse(&compat_tool::config_text(&root))
                && let Some(map) = v.path(&["InstallConfigStore", "Software", "Valve", "Steam", "CompatToolMapping"])
            {
                w.mappings.extend(map.entries().iter().filter_map(|(id, e)| Some((id.clone(), e.str_at(&["name"]).filter(|x| !x.is_empty())?.to_string()))));
            }
            let i = w.roots.len();
            w.protons.extend(compat_tool::installed_protons(&root).into_iter().map(|p| (i, p)));
            w.roots.push((real_root, root));
        }
        for lib in &w.libs {
            let apps = lib.join("steamapps");
            for name in fs.names(&apps).unwrap_or_default() {
                let name = name.to_string_lossy().into_owned();
                let Some(id) = name.strip_prefix("appmanifest_").and_then(|x| x.strip_suffix(".acf")).filter(|id| all_digits(id)) else { continue };
                let v = vdf::parse(&text_of(fs, &apps.join(&name), 256 * 1024)).ok();
                let at = |key: &str| v.as_ref().and_then(|v| v.str_at(&["AppState", key])).unwrap_or("").to_string();
                w.manifests.push(Manifest { id: id.to_string(), name: at("name"), lib: lib.clone(), installdir: at("installdir"), size_on_disk: at("SizeOnDisk").parse().unwrap_or(0), file: apps.join(&name) });
            }
        }
        for file in crate::shortcuts::shortcut_files(&disk_home) {
            if let Ok(list) = std::fs::read(&file).map_err(|_| ()).and_then(|bytes| crate::shortcuts::parse(&bytes).map_err(|_| ())) {
                w.shortcuts.extend(list.into_iter().map(|x| (x.appid.to_string(), x.name)));
            }
        }
        for (game, name) in crate::history::sessions(data).into_iter().map(|x| (x.game, x.name)).chain(crate::snapshots::games(data).into_iter().map(|x| (x.0, x.1))) {
            if let Some(id) = game.strip_prefix("steam:")
                && !name.is_empty()
            {
                w.names.insert(id.to_string(), name);
            }
        }
        for e in std::fs::read_dir(compat_tool::launch_dir(folder)).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().ends_with(".proton")
                && let Some(line) = std::fs::read_to_string(e.path()).ok().and_then(|t| t.lines().next().map(|l| l.trim().to_string())).filter(|l| !l.is_empty())
            {
                w.picks.push(PathBuf::from(line));
            }
        }
        for root in crate::heroic::heroic_roots(&disk_home) {
            for file in std::iter::once(root.join("config.json")).chain(std::fs::read_dir(root.join("GamesConfig")).into_iter().flatten().flatten().map(|e| e.path())) {
                if w.heroic.len() < (8 << 20)
                    && let Ok(t) = std::fs::read_to_string(&file)
                {
                    w.heroic.push_str(&t);
                    w.heroic.push('\n');
                }
            }
        }
        w
    }

    /// Does Steam still have this app: a manifest in a library, or the library list naming it?
    pub(crate) fn installed(&self, id: &str) -> bool {
        self.listed.contains(id) || self.manifests.iter().any(|m| m.id == id)
    }

    /// The game's name: Steam's manifest, the non-Steam games, then Turbine Hub's own records.
    pub(crate) fn name(&self, id: &str) -> Option<String> {
        self.manifests.iter().find(|m| m.id == id && !m.name.is_empty()).map(|m| m.name.clone()).or_else(|| self.shortcuts.get(id).cloned()).or_else(|| self.names.get(id).cloned()).filter(|x| !x.is_empty())
    }

    /// Who uses this Proton, when anything does. Everything that could name it counts: Steam's choice for a game
    /// or for all games, the launch tool's pick and its automatic choice, and Heroic's settings.
    pub(crate) fn proton_user(&self, root: usize, p: &Proton) -> Option<String> {
        for (app, tool) in &self.mappings {
            if p.matches_steam_name(tool) {
                return Some(if app == "0" { "Steam starts games with it when nothing else is set".into() } else { format!("{} uses it", self.name(app).unwrap_or_else(|| format!("The game with app id {app}"))) });
            }
        }
        if self.picks.contains(&p.dir) {
            return Some("A game is set to start with it in Turbine Hub".into());
        }
        let own: Vec<Proton> = self.protons.iter().filter(|(r, _)| *r == root).map(|(_, x)| x.clone()).collect();
        if let Some((_, disk_root)) = self.roots.get(root)
            && compat_tool::automatic(&own, &compat_tool::config_text(disk_root)).is_some_and(|a| a.dir == p.dir)
        {
            return Some("Turbine Hub's launch tool starts games with it".into());
        }
        let dir = p.dir.to_string_lossy();
        if !self.heroic.is_empty() && (self.heroic.contains(dir.as_ref()) || self.heroic.contains(&format!("\"{}\"", p.name))) {
            return Some("A game in Heroic uses it".into());
        }
        None
    }
}

/// The drives: the one the home folder is on first, then every other one.
fn drives(fs: &dyn Fs, places: &Places) -> Vec<DriveUse> {
    let room = |p: &Path| fs.space(p).map(|x| (x.free, x.total)).unwrap_or((0, 0));
    let (free, total) = room(&places.who.home);
    let mut out = vec![DriveUse { label: "Internal storage".into(), mount: places.who.home.clone(), free, total, removable: false, read_only: false, libraries: Vec::new() }];
    for d in &places.drives {
        let (free, total) = room(&d.mount);
        out.push(DriveUse { label: d.label.clone(), mount: d.mount.clone(), free, total, removable: d.removable, read_only: d.read_only, libraries: Vec::new() });
    }
    out
}

/// Which drive a path is on: the other drive whose top it is under, else the internal one.
fn drive_of(drives: &[DriveUse], p: &Path) -> usize {
    drives.iter().enumerate().skip(1).filter(|(_, d)| p.starts_with(&d.mount)).max_by_key(|(_, d)| d.mount.components().count()).map(|(i, _)| i).unwrap_or(0)
}

/// Everything one look found.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// When the look was taken, seconds since 1970.
    pub at: u64,
    pub drives: Vec<DriveUse>,
    /// Every installed game, biggest first.
    pub games: Vec<GameSize>,
    /// Safe first, then Check first, then what is in use; biggest first in each.
    pub findings: Vec<Finding>,
    /// What couldn't be looked at, in plain sentences.
    pub problems: Vec<String>,
}

struct Scanner<'a> {
    fs: &'a dyn Fs,
    meter: Meter,
    drives: &'a [DriveUse],
    out: Vec<Finding>,
}

impl Scanner<'_> {
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, kind: Kind, group: Group, how: How, title: String, why: impl Into<String>, paths: Vec<PathBuf>, inside: &Path, app_id: &str, steam: bool) {
        let size = paths.iter().fold(Size::default(), |t, p| t.plus(self.meter.size(self.fs, p)));
        self.add_sized(kind, group, how, title, why, paths, inside, app_id, steam, size);
    }
    #[allow(clippy::too_many_arguments)]
    fn add_sized(&mut self, kind: Kind, group: Group, how: How, title: String, why: impl Into<String>, paths: Vec<PathBuf>, inside: &Path, app_id: &str, steam: bool, size: Size) {
        let first = paths.first().cloned().unwrap_or_else(|| inside.to_path_buf());
        let id = format!("{}:{}", kind.key(), first.to_string_lossy());
        let drive = drive_of(self.drives, &first);
        self.out.push(Finding { id, kind, group, title, why: why.into(), paths, inside: inside.to_path_buf(), size, drive, how, ticked: group == Group::Safe, app_id: app_id.into(), steam });
    }
}

/// The places a look and a cleaning work in.
pub(crate) fn sorted_names(fs: &dyn Fs, p: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs.names(p).unwrap_or_default().into_iter().map(|x| x.to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

/// The folder Downloads really is, when the player has one.
pub(crate) fn downloads_dir(places: &Places) -> Option<PathBuf> {
    places.dirs.iter().find(|d| d.name == "Downloads").map(|d| d.path.clone())
}

/// Steam's logs that are old (the copy from the run before, or untouched for two weeks) and its crash dumps older
/// than a day: (the folder, the files).
pub(crate) fn old_steam_files(fs: &dyn Fs, root: &Path, now: u64) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let mut out = Vec::new();
    for (sub, days) in [("logs", 14u64), ("dumps", 1)] {
        let dir = root.join(sub);
        if !plain_folder(fs, &dir) {
            continue;
        }
        let files: Vec<PathBuf> = sorted_names(fs, &dir)
            .into_iter()
            .filter(|name| fs.meta(&dir.join(name)).is_ok_and(|m| m.kind == NodeKind::File && ((sub == "logs" && name.ends_with(".previous.txt")) || m.modified + days * 86_400 < now)))
            .map(|name| dir.join(name))
            .take(2000)
            .collect();
        if !files.is_empty() {
            out.push((dir, files));
        }
    }
    out
}

/// One game's snapshots that are beyond the kept count: for an installed game the ones past `keep` of each kind,
/// for a game that is gone all but the newest. The newest snapshot of a game always stays.
pub(crate) fn extra_snapshots(data: &Path, game: &str, installed: bool, keep: usize) -> Vec<crate::snapshots::Snapshot> {
    let all = crate::snapshots::list(data, game);
    if !installed {
        return all.into_iter().skip(1).collect();
    }
    let mut seen: Vec<(crate::snapshots::Kind, usize)> = Vec::new();
    let mut out = Vec::new();
    for (i, snap) in all.into_iter().enumerate() {
        let count = match seen.iter_mut().find(|(k, _)| *k == snap.kind) {
            Some((_, c)) => {
                *c += 1;
                *c
            }
            None => {
                seen.push((snap.kind, 1));
                1
            }
        };
        if i > 0 && count > keep.max(1) {
            out.push(snap);
        }
    }
    out
}

/// Looks at everything. Reads only.
pub fn scan(site: &Site, host: &Host, limits: &Limits) -> Report {
    let (fs, places) = (site.fs, site.places);
    let now = site.when.secs;
    let world = World::read(fs, places, site.folder, site.data);
    let mut drive_list = drives(fs, places);
    let mut problems: Vec<String> = Vec::new();
    for lib in &world.missing {
        problems.push(format!("The Steam library at {} isn't there right now, so what belongs to its games can't be told apart for sure.", lib.display()));
    }
    let games = fs.disk(&places.who.home).map(|h| crate::scan_games(&h).games).unwrap_or_default();
    let mut sc = Scanner { fs, meter: Meter::new(limits), drives: &drive_list, out: Vec::new() };
    let gone = |id: &str| world.name(id).unwrap_or_else(|| format!("a removed game (app {id})"));
    let away = if world.missing.is_empty() { "" } else { " A Steam library that isn't connected right now may still use it." };

    for lib in &world.libs {
        let apps = lib.join("steamapps");
        if !plain_folder(fs, &apps) {
            problems.push(format!("{} is a link to another place, so Turbine Hub leaves what is in it alone.", apps.display()));
            continue;
        }
        for (sub, kind) in [("shadercache", Kind::ShaderCache), ("compatdata", Kind::Compatdata)] {
            let dir = apps.join(sub);
            if fs.meta(&dir).is_err() {
                continue;
            }
            if !plain_folder(fs, &dir) {
                problems.push(format!("{} is a link to another place, so Turbine Hub leaves what is in it alone.", dir.display()));
                continue;
            }
            for id in sorted_names(fs, &dir).into_iter().filter(|x| all_digits(x) && x != "0") {
                let p = dir.join(&id);
                if !is_folder(fs, &p) {
                    continue;
                }
                let in_use = world.installed(&id) || world.shortcuts.contains_key(&id);
                if kind == Kind::Compatdata {
                    // What "Reset Windows files" set aside, whoever the folder belongs to.
                    for name in sorted_names(fs, &p) {
                        let Some(backup) = reset_leftover(&name) else { continue };
                        if !is_folder(fs, &p.join(&name)) {
                            continue;
                        }
                        let who = world.name(&id).unwrap_or_else(|| format!("app {id}"));
                        let (title, why) = if backup {
                            (format!("Windows files of {who} from before a reset"), "A reset set them aside. They hold the game's saves and settings from before, if it kept them there, and that reset's Undo needs them.")
                        } else {
                            (format!("Windows files of {who} set aside when a reset was undone"), "The game made them between a reset and its Undo. They can hold saves from that time.")
                        };
                        sc.add(Kind::ResetBackup, Group::Check, How::Bin, title, why, vec![p.join(&name)], &p, &id, true);
                    }
                }
                if in_use {
                    continue;
                }
                match kind {
                    Kind::ShaderCache => sc.add(kind, Group::Safe, How::Erase, format!("Shader cache of {}", gone(&id)), "The game isn't installed. Steam builds the cache again if it ever comes back.", vec![p], &dir, &id, true),
                    _ => sc.add(kind, Group::Check, How::Bin, format!("Windows files of {}", gone(&id)), format!("The game isn't installed, but these can hold its saves and settings when it doesn't keep them in Steam Cloud. Keep them if you might install it again.{away}"), vec![p], &dir, &id, true),
                }
            }
        }
        for sub in ["downloading", "temp"] {
            let dir = apps.join(sub);
            if !plain_folder(fs, &dir) {
                continue;
            }
            for id in sorted_names(fs, &dir).into_iter().filter(|x| all_digits(x)) {
                let p = dir.join(&id);
                if !is_folder(fs, &p) {
                    continue;
                }
                if world.installed(&id) {
                    sc.add(Kind::Download, Group::Check, How::Erase, format!("Unfinished update of {}", world.name(&id).unwrap_or_else(|| format!("app {id}"))), "The game is installed and Steam was updating it. Without this Steam downloads the update again from the start.", vec![p], &dir, &id, true);
                } else {
                    sc.add(Kind::Download, Group::Safe, How::Erase, format!("Unfinished download of {}", gone(&id)), "The game isn't installed. Steam downloads it again if you ask for it.", vec![p], &dir, &id, true);
                }
            }
        }
    }

    // Protons: the ones something uses are shown and stay; the rest wait for a look.
    let mut seen: Vec<PathBuf> = Vec::new();
    for (root, p) in &world.protons {
        let Some(dir) = fs.real(&p.dir) else { continue };
        if seen.contains(&dir) || !is_folder(fs, &dir) {
            continue;
        }
        seen.push(dir.clone());
        let own = p.kind != ProtonKind::Other;
        let (inside, mut paths) = if own {
            let Some(apps) = dir.parent().and_then(Path::parent).map(Path::to_path_buf) else { continue };
            (apps, vec![dir.clone()])
        } else {
            (dir.parent().map(Path::to_path_buf).unwrap_or_default(), vec![dir.clone()])
        };
        if !plain_folder(fs, &inside) || (own && !plain_folder(fs, &inside.join("common"))) {
            continue;
        }
        if own && let Some(m) = world.manifests.iter().find(|m| m.lib.join("steamapps") == inside && inside.join("common").join(&m.installdir) == dir) {
            // Without its manifest Steam no longer counts it as installed; with it Steam would download it again.
            paths.push(m.file.clone());
        }
        match world.proton_user(*root, p) {
            Some(who) => sc.add(Kind::Proton, Group::Never, How::Bin, p.name.clone(), format!("{who}."), paths, &inside, "", own),
            None if own => sc.add(Kind::Proton, Group::Check, How::Bin, p.name.clone(), "No game is set to use it. Steam may still pick it for a game by itself, and downloads it again then.", paths, &inside, "", true),
            None => sc.add(Kind::Proton, Group::Check, How::Bin, p.name.clone(), "No game is set to use it. You added it yourself, so it only comes back if you get it again.", paths, &inside, "", false),
        }
    }

    for (root, _) in &world.roots {
        for (dir, files) in old_steam_files(fs, root, now) {
            let logs = dir.ends_with("logs");
            let (title, why) = if logs { ("Steam's old logs", "Notes Steam wrote in earlier runs. Nothing reads them again.") } else { ("Steam's crash dumps", "Written when Steam or a game crashed. Nothing reads them again.") };
            sc.add(Kind::SteamLogs, Group::Safe, How::Erase, title.into(), why, files, &dir, "", false);
        }
    }

    // What apps from the Store left behind in ~/.var/app, told only when the list of installed apps could be read.
    let var_app = places.who.home.join(".var/app");
    if places.app_roots.iter().any(|r| r.starts_with("/var/lib/flatpak")) && plain_folder(fs, &var_app) {
        for id in sorted_names(fs, &var_app) {
            let p = var_app.join(&id);
            if id.eq_ignore_ascii_case(OWN_APP) || !looks_like_app_id(&id) || places.apps.iter().any(|a| a.id == id) || places.who.own.iter().any(|o| o.starts_with(&p) || p.starts_with(o)) || !is_folder(fs, &p) {
                continue;
            }
            sc.add(Kind::FlatpakData, Group::Check, How::Bin, format!("Files left by {id}"), "The app isn't installed any more. This is what it kept: its settings, and for a game or a launcher maybe saves or whole games.", vec![p], &var_app, "", false);
        }
    }
    if let Some(answer) = &host.flatpak_unused {
        let unused = unused_from_answer(answer);
        if !unused.is_empty() {
            let title = if unused.len() == 1 { "1 app part nothing uses".to_string() } else { format!("{} app parts nothing uses", unused.len()) };
            let size = unused_size(&unused, host.flatpak_sizes.as_deref());
            sc.add_sized(Kind::FlatpakUnused, Group::Safe, How::Flatpak, title, format!("Flatpak keeps shared parts (runtimes) for apps from the Store, and says no installed app needs these: {}.", unused.iter().map(|(id, br)| if br.is_empty() { id.clone() } else { format!("{id} {br}") }).collect::<Vec<_>>().join(", ")), Vec::new(), Path::new("/var/lib/flatpak"), "", false, size);
        }
    }

    // Turbine Hub's own save snapshots beyond the kept count.
    let snap_root = crate::snapshots::root(site.data);
    for (game, name, _, _) in crate::snapshots::games(site.data) {
        let installed = games.iter().any(|g| g.id() == game);
        let extra = extra_snapshots(site.data, &game, installed, limits.keep);
        if extra.is_empty() {
            continue;
        }
        let size = Size::exact(extra.iter().map(|x| x.bytes).sum());
        let count = if extra.len() == 1 { "1 older save snapshot".to_string() } else { format!("{} older save snapshots", extra.len()) };
        let why = if installed { "Older copies of this game's saves, beyond the ones Turbine Hub keeps. The newest ones stay." } else { "The game isn't installed any more. These are older copies of its saves; the newest one stays, so the saves can still come back." };
        let app = game.strip_prefix("steam:").unwrap_or("");
        sc.add_sized(Kind::Snapshots, Group::Check, How::Erase, format!("{count} of {name}"), why, extra.into_iter().map(|x| x.dir).collect(), &snap_root, app, false, size);
    }

    for bin in trash::bins(fs, places) {
        let size = sc.meter.size(fs, &bin.join("files"));
        if size.bytes == 0 && !size.at_least {
            continue;
        }
        let d = drive_of(&drive_list, &bin);
        let title = if d == 0 { "Recycle Bin".to_string() } else { format!("Recycle Bin on {}", drive_list[d].label) };
        let inside = bin.parent().map(Path::to_path_buf).unwrap_or_default();
        sc.add_sized(Kind::RecycleBin, Group::Check, How::EmptyBin, title, "What you deleted in Desktop Mode or in Turbine Hub's Files. Emptying is for good: look inside first.", vec![bin], &inside, "", false, size);
    }

    if let Some(dir) = downloads_dir(places).filter(|d| plain_folder(fs, d)) {
        for name in sorted_names(fs, &dir) {
            let p = dir.join(&name);
            let Ok(m) = fs.meta(&p) else { continue };
            if m.kind != NodeKind::File || m.size < limits.downloads_bytes || m.modified + limits.downloads_days * 86_400 > now {
                continue;
            }
            let age = (now - m.modified) / 86_400;
            sc.add_sized(Kind::DownloadsFile, Group::Check, How::Bin, format!("{name} in Downloads"), format!("A big file you downloaded {age} days ago. Only you know whether you still need it."), vec![p], &dir, "", false, Size::exact(m.size));
        }
    }

    // Then the games: findings were counted first, so a slow drive cuts the games' sizes short, not theirs.
    let apps_dirs: Vec<PathBuf> = world.libs.iter().map(|l| l.join("steamapps")).collect();
    let mut sizes: Vec<GameSize> = games.iter().map(|g| size_game(fs, &sc.meter, &drive_list, &apps_dirs, &world.manifests, g)).collect();
    sizes.sort_by(|a, b| b.total().bytes.cmp(&a.total().bytes).then_with(|| a.name.cmp(&b.name)));
    let mut findings = sc.out;
    findings.sort_by(|a, b| a.group.cmp(&b.group).then(b.size.bytes.cmp(&a.size.bytes)).then_with(|| a.id.cmp(&b.id)));
    for lib in &world.libs {
        let of: Vec<&GameSize> = sizes.iter().filter(|g| g.library == *lib).collect();
        let size = of.iter().fold(Size::default(), |t, g| t.plus(g.total()));
        let d = drive_of(&drive_list, lib);
        drive_list[d].libraries.push(LibraryUse { path: lib.clone(), games: of.len(), size });
    }
    Report { at: now, drives: drive_list, games: sizes, findings, problems }
}

fn size_game(fs: &dyn Fs, meter: &Meter, drives: &[DriveUse], apps_dirs: &[PathBuf], manifests: &[Manifest], g: &Game) -> GameSize {
    let real = |p: &Path| fs.real(p).unwrap_or_else(|| p.to_path_buf());
    let dir = real(&g.install_dir);
    let mut files = meter.size(fs, &dir);
    // Cut short: Steam's own figure for the game is the better floor.
    if files.at_least && g.store == Store::Steam && let Some(m) = manifests.iter().find(|m| m.id == g.store_id) {
        files.bytes = files.bytes.max(m.size_on_disk);
    }
    let (mut shaders, mut proton) = (Size::default(), Size::default());
    if g.store == Store::Steam {
        for apps in apps_dirs {
            for (sub, total) in [("shadercache", &mut shaders), ("compatdata", &mut proton)] {
                let p = apps.join(sub).join(&g.store_id);
                if is_folder(fs, &p) {
                    *total = total.plus(meter.size(fs, &p));
                }
            }
        }
    } else if let Some(p) = &g.prefix {
        proton = meter.size(fs, &real(p));
    }
    let library = real(&g.library);
    GameSize { id: g.id(), name: g.name.clone(), store: g.store, drive: drive_of(drives, &dir), library, files, shaders, proton }
}

/// What one game really takes, without looking at anything else: for the Games tab. Counts the game's folder,
/// its shader cache and its Proton files in the library it is installed in.
pub fn game_size(fs: &dyn Fs, places: &Places, game: &Game, limits: &Limits) -> GameSize {
    let apps: Vec<PathBuf> = game.install_dir.parent().and_then(Path::parent).and_then(|p| fs.real(p)).into_iter().collect();
    let manifest: Vec<Manifest> = apps
        .first()
        .filter(|_| game.store == Store::Steam)
        .map(|a| {
            let file = a.join(format!("appmanifest_{}.acf", game.store_id));
            let size = vdf::parse(&text_of(fs, &file, 256 * 1024)).ok().and_then(|v| v.str_at(&["AppState", "SizeOnDisk"]).and_then(|x| x.parse().ok())).unwrap_or(0);
            Manifest { id: game.store_id.clone(), name: game.name.clone(), lib: PathBuf::new(), installdir: String::new(), size_on_disk: size, file }
        })
        .into_iter()
        .collect();
    size_game(fs, &Meter::new(limits), &drives(fs, places), &apps, &manifest, game)
}

impl Report {
    /// The ids ticked before the player touches anything: the Safe group, and nothing else.
    pub fn default_ticks(&self) -> Vec<String> {
        self.findings.iter().filter(|f| f.ticked && f.group == Group::Safe).map(|f| f.id.clone()).collect()
    }

    /// One group's findings.
    pub fn group(&self, group: Group) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(move |f| f.group == group)
    }

    /// How much one group holds.
    pub fn total(&self, group: Group) -> Size {
        self.group(group).fold(Size::default(), |t, f| t.plus(f.size))
    }

    /// The ticked findings that can be freed (what is in use never counts, ticked or not).
    pub fn chosen<'a>(&'a self, ticked: &'a [String]) -> impl Iterator<Item = &'a Finding> {
        self.findings.iter().filter(move |f| f.group != Group::Never && ticked.contains(&f.id))
    }

    /// How much the ticked set holds.
    pub fn ticked_total(&self, ticked: &[String]) -> Size {
        self.chosen(ticked).fold(Size::default(), |t, f| t.plus(f.size))
    }

    /// The part of the ticked set that only moves to the Recycle Bin: its room comes back when the bin is emptied.
    pub fn ticked_to_bin(&self, ticked: &[String]) -> Size {
        self.chosen(ticked).filter(|f| f.how == How::Bin).fold(Size::default(), |t, f| t.plus(f.size))
    }

    /// The button's words for the ticked set: "Free 12 GB", "Free at least 12 GB".
    pub fn free_words(&self, ticked: &[String]) -> String {
        if self.chosen(ticked).next().is_none() {
            return "Nothing ticked".into();
        }
        match self.ticked_total(ticked) {
            Size { bytes: 0, at_least: true } => "Free space".into(),
            size => format!("Free {}", size.words()),
        }
    }

    /// One game's sizes, by Turbine Hub's id for it ("steam:1086940").
    pub fn game(&self, id: &str) -> Option<&GameSize> {
        self.games.iter().find(|g| g.id == id)
    }

    /// What could be freed on one drive: (the Safe group, the Check first group).
    pub fn can_free(&self, drive: usize) -> (Size, Size) {
        let of = |group: Group| self.group(group).filter(|f| f.drive == drive).fold(Size::default(), |t, f| t.plus(f.size));
        (of(Group::Safe), of(Group::Check))
    }

    /// The checkup's line when a drive is nearly full (the fullest one, the internal one first when two are as
    /// full): the title and what can be done.
    pub fn checkup(&self) -> Option<(String, String)> {
        let (i, d) = self.drives.iter().enumerate().rev().filter(|(_, d)| d.nearly_full() && !d.read_only).max_by_key(|(_, d)| d.percent_full())?;
        let title = match d.label.as_str() {
            "Internal storage" => "Your internal storage is nearly full".to_string(),
            "SD card" => "Your SD card is nearly full".to_string(),
            other => format!("{other} is nearly full"),
        };
        let (safe, check) = self.can_free(i);
        let round = |v: u64| size_words(v);
        let line = if safe.bytes > 0 {
            format!("{} can be freed.", round(safe.bytes))
        } else if check.bytes > 0 {
            format!("Up to {} could be freed, after a look at what it is.", round(check.bytes))
        } else {
            format!("Only {} left. See what is using the room.", round(d.free))
        };
        Some((title, line))
    }

    /// Everything a page needs, with the ticks as they are now (`default_ticks` at first).
    pub fn to_json(&self, ticked: &[String]) -> Json {
        let finding = |f: &Finding| {
            obj! {
                "id": s(&f.id),
                "kind": s(f.kind.key()),
                "group": s(f.group.key()),
                "title": s(&f.title),
                "why": s(&f.why),
                "size": f.size.json(),
                "drive": n(f.drive as f64),
                "driveLabel": s(self.drives.get(f.drive).map(|d| d.label.as_str()).unwrap_or("")),
                "how": s(f.how.key()),
                "howWords": s(f.how.words()),
                "ticked": b(f.group != Group::Never && ticked.contains(&f.id)),
                "offered": b(f.group != Group::Never),
                "needsSteamClosed": b(f.steam && !f.orphan()),
                "paths": arr(f.paths.iter().map(|p| s(p.to_string_lossy()))),
            }
        };
        let drive = |(i, d): (usize, &DriveUse)| {
            let (safe, check) = self.can_free(i);
            let games = self.games.iter().filter(|g| g.drive == i).fold(Size::default(), |t, g| t.plus(g.total()));
            obj! {
                "label": s(&d.label),
                "mount": s(d.mount.to_string_lossy()),
                "free": n(d.free as f64),
                "total": n(d.total as f64),
                "freeWords": s(size_words(d.free)),
                "totalWords": s(size_words(d.total)),
                "percentFull": d.percent_full().map(|p| n(p as f64)).unwrap_or(Json::Null),
                "nearlyFull": b(d.nearly_full()),
                "removable": b(d.removable),
                "readOnly": b(d.read_only),
                "games": games.json(),
                "canFreeSafe": safe.json(),
                "canFreeCheck": check.json(),
                "libraries": arr(d.libraries.iter().map(|l| obj! {"path": s(l.path.to_string_lossy()), "games": n(l.games as f64), "size": l.size.json()})),
            }
        };
        let group = |g: Group| obj! {"key": s(g.key()), "title": s(g.title()), "line": s(g.line()), "total": self.total(g).json(), "items": arr(self.group(g).map(finding))};
        let checkup = match self.checkup() {
            Some((title, line)) => obj! {"title": s(title), "line": s(line)},
            None => Json::Null,
        };
        obj! {
            "at": n(self.at as f64),
            "drives": arr(self.drives.iter().enumerate().map(drive)),
            "games": arr(self.games.iter().map(GameSize::to_json)),
            "groups": arr([Group::Safe, Group::Check, Group::Never].into_iter().map(group)),
            "ticked": self.ticked_total(ticked).json(),
            "toBin": self.ticked_to_bin(ticked).json(),
            "free": s(self.free_words(ticked)),
            "checkup": checkup,
            "problems": arr(self.problems.iter().map(s)),
        }
    }
}

/// A machine read for the storage saver: its disk, its places and Turbine Hub's own two folders. `root` is "/" on a
/// real machine and the made-up machine's folder otherwise.
pub struct Here {
    pub disk: Disk,
    pub places: Places,
    /// Turbine Hub's own folder (its records), as the app finds it.
    pub folder: PathBuf,
    /// Turbine Hub's data folder (the save snapshots), as the app finds it.
    pub data: PathBuf,
}

impl Here {
    pub fn read(root: &Path, home: &Path, folder: &Path, data: &Path) -> Here {
        let (disk, real_home) = if root == Path::new("/") {
            (Disk::here(), home.to_path_buf())
        } else {
            match Map::under(root).real(home) {
                Some(real) => (Disk::under(root), real),
                // A made-up home that isn't inside the made-up machine's folder.
                None => (Disk { map: Map::under(root).with(Path::new("/home/deck"), home), room: None }, PathBuf::from("/home/deck")),
            }
        };
        let mut who = Who::read(&disk, &real_home);
        // Turbine Hub's own folders are never a leftover and never changed through the Recycle Bin.
        who.own = [real_home.join(".var/app").join(OWN_APP)].into_iter().chain([folder, data].into_iter().filter_map(|p| disk.real(p))).collect();
        who.data = Some(data.to_path_buf());
        let hardware = crate::hardware::Machine::read(root, "", &crate::hardware::device_of(home));
        let places = Places::scan(&disk, who, &hardware.drives);
        Here { disk, places, folder: folder.to_path_buf(), data: data.to_path_buf() }
    }

    pub fn site(&self, when: When) -> Site<'_> {
        Site { fs: &self.disk, places: &self.places, folder: &self.folder, data: &self.data, when }
    }
}

/// A question put to the host: the command line and what is typed into it (None: nothing). Its exit code and what
/// it printed, or None when it couldn't run.
pub type AskTyped<'a> = &'a mut dyn FnMut(&[String], Option<&str>) -> Option<(i32, String)>;

/// What the host says for a look: whether Steam runs, which games run, and what Flatpak calls unused (its own
/// list, with "n" typed into it so it only ever lists).
pub fn ask_host(ask: AskTyped) -> Host {
    let steam = Steam::from_answer(ask(&Steam::question(), None).as_ref().map(|(code, out)| (*code, out.as_str())));
    let running = match steam {
        Steam::Closed => Some(Vec::new()),
        Steam::Unknown => None,
        // pgrep ends with 1 when it found nothing.
        Steam::Running => ask(&crate::agent::games_question(), None).filter(|(code, _)| matches!(code, 0 | 1)).map(|(_, out)| crate::agent::running_games(&out)),
    };
    // Flatpak asks once for each place apps are installed in (the system, the user): every question gets a no.
    let flatpak_unused = ask(&unused_question(), Some("n\nn\nn\nn\n")).map(|(_, out)| out);
    let flatpak_sizes = flatpak_unused.as_ref().and_then(|_| ask(&sizes_question(), None)).filter(|(code, _)| *code == 0).map(|(_, out)| out);
    Host { steam, running, flatpak_unused, flatpak_sizes }
}

/// One whole look on the machine the app runs on, for the app's worker thread: the places are read, the host is
/// asked, then everything is counted. It reads only, and can take seconds on a slow card.
pub fn look_here(home: &Path, folder: &Path, data: &Path, ask: AskTyped) -> (Report, Host) {
    let here = Here::read(Path::new("/"), home, folder, data);
    let zone = ask(&["date".to_string(), "+%z".to_string()], None).map(|(_, out)| out).unwrap_or_default();
    let host = ask_host(ask);
    (scan(&here.site(When::from_date_z(crate::now(), &zone)), &host, &Limits::default()), host)
}

/// The checkup's line from the drives alone (how full each is, nothing counted), for the moment before any look
/// was taken: quick enough for every start. None when no drive is nearly full.
pub fn nearly_full(fs: &dyn Fs, places: &Places) -> Option<(String, String)> {
    Report { at: 0, drives: drives(fs, places), games: Vec::new(), findings: Vec::new(), problems: Vec::new() }.checkup()
}

/// `nearly_full` on the machine the app runs on.
pub fn nearly_full_here(home: &Path, folder: &Path, data: &Path) -> Option<(String, String)> {
    let here = Here::read(Path::new("/"), home, folder, data);
    nearly_full(&here.disk, &here.places)
}

/// What one game really takes on the machine the app runs on, by Turbine Hub's id for it, for the app's worker
/// thread. None when the game isn't installed.
pub fn game_size_here(home: &Path, folder: &Path, data: &Path, id: &str) -> Option<GameSize> {
    let game = crate::scan_games(home).games.into_iter().find(|g| g.id() == id)?;
    let here = Here::read(Path::new("/"), home, folder, data);
    Some(game_size(&here.disk, &here.places, &game, &Limits::default()))
}

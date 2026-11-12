//! Files: the explorer's page and every press on it. `crate::files` decides what is where and what may be done;
//! this file keeps what the window is looking at (the tabs with their way back and forward, the view, the sorting,
//! what was cut or copied, the last things done) and turns it into the one page the window draws.
//!
//! The page is laid out the way Windows Explorer is: tabs, back, forward, up, the address, the search box, the
//! command bar, the navigation pane, the list, the details pane and the status bar. Every operation is the
//! engine's plan first. A name that is taken asks (skip, replace, keep both); a long copy runs on its own thread
//! and reports to the status bar, so the rest of the app stays usable; the last thing done can be undone. No file
//! action ever asks for a password.
use super::controls::{local, option, page_button};
use super::plan::{open, refresh, toast};
use super::{Ui, arr, b, n, obj, s, with};
use crate::files::{self as fx, Answers, Choice, Disk, Entry, Found, Fs, Group, History, Kind, Limits, Map, Op, OpKind, Place, Places, Preview, Progress, Quick, Sort, Space, View, When, Who};
use crate::json::Json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

/// How the list is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    #[default]
    Details,
    Icons,
    List,
}

impl Layout {
    fn key(self) -> &'static str {
        match self {
            Layout::Details => "details",
            Layout::Icons => "icons",
            Layout::List => "list",
        }
    }
    fn from_key(k: &str) -> Layout {
        match k {
            "icons" => Layout::Icons,
            "list" => Layout::List,
            _ => Layout::Details,
        }
    }
}

/// A search through the folders below the one on screen, on its own thread: what it found so far, and whether it
/// is through.
struct Deep {
    needle: String,
    shared: Arc<Mutex<(Found, bool)>>,
    stop: Arc<AtomicBool>,
    /// How much of it the window has been told about.
    told: (usize, bool),
}

/// One tab: where it is, the way back and forward, and what is typed into its search box.
struct Tab {
    place: Place,
    back: Vec<Place>,
    fwd: Vec<Place>,
    search: String,
    deep: Option<Deep>,
    /// The real path of the line the highlight goes to after the next look (the folder just left, something new).
    focus: Option<PathBuf>,
    /// A text or a picture shown inside the app.
    viewing: Option<PathBuf>,
}

impl Tab {
    fn at(place: Place) -> Tab {
        Tab { place, back: Vec::new(), fwd: Vec::new(), search: String::new(), deep: None, focus: None, viewing: None }
    }
    fn leave(&mut self) {
        self.search.clear();
        if let Some(d) = self.deep.take() {
            d.stop.store(true, Ordering::SeqCst);
        }
        self.viewing = None;
    }
    fn go(&mut self, place: Place) {
        if place == self.place {
            self.viewing = None;
            return;
        }
        self.leave();
        let from = std::mem::replace(&mut self.place, place);
        self.back.push(from);
        self.back.drain(..self.back.len().saturating_sub(64));
        self.fwd.clear();
        self.focus = None;
    }
}

/// A question about names that are taken, waiting for its answers.
struct Asking {
    plan: fx::Plan,
    answers: Answers,
    at: usize,
    all: bool,
}

/// A copy, a move or an emptying that runs on its own thread.
struct Job {
    verb: &'static str,
    progress: Arc<Mutex<Progress>>,
    stop: Arc<AtomicBool>,
    rx: Receiver<Result<fx::Done, String>>,
    told: (u64, u64),
}

/// Everything the explorer remembers between presses.
pub struct Explorer {
    tabs: Vec<Tab>,
    at: usize,
    /// The player's pick; None follows the mode (Guided: Windows-like, Veteran: True Linux).
    view: Option<View>,
    layout: Layout,
    pane: bool,
    /// The player's pick; None is the folder's own way (Downloads by date, everything else by name).
    sort: Option<(Sort, bool)>,
    group: Option<Group>,
    hidden: Option<bool>,
    /// What was cut or copied, and which of the two.
    clip: Vec<PathBuf>,
    cut: bool,
    history: History,
    places: Option<(Places, u64)>,
    asking: Option<Asking>,
    job: Option<Job>,
    /// The page argument that was last taken in (a link into Files, a picture's setup): taken in once.
    applied: String,
    zone: Option<i32>,
    /// A picture of a copy under way (made-up machines only).
    demo: bool,
    /// Counts every change of place, so the window knows a new list from the same list read again.
    seq: u64,
    /// Every copy runs on its own thread, however small (the tests of that path).
    pub(crate) always_background: bool,
}

impl Default for Explorer {
    fn default() -> Explorer {
        Explorer { tabs: Vec::new(), at: 0, view: None, layout: Layout::Details, pane: false, sort: None, group: None, hidden: None, clip: Vec::new(), cut: false, history: History::default(), places: None, asking: None, job: None, applied: String::new(), zone: None, demo: false, seq: 0, always_background: false }
    }
}

/// Room on the made-up machine's drive.
const SAMPLE_ROOM: Space = Space { free: 229_780_000_000, total: 512_000_000_000 };
/// Turbine Hub's own folder as the made-up machine has it.
const SAMPLE_OWN: &str = "/home/deck/.var/app/io.github.ravenor907.TurbineHub";
/// How long the places are kept before the machine is looked at again (a drive plugged in, a game installed).
const PLACES_FOR: u64 = 20;
/// A copy or move smaller than this is done at once; a bigger one runs on its own thread.
const QUICK_BYTES: u64 = 24 * 1024 * 1024;
const QUICK_FILES: u64 = 300;
/// How many names a folder's properties count through.
const COUNT_MAX: usize = 20_000;
/// How much of a text the details pane shows.
const PEEK_CHARS: usize = 1800;
/// The most tabs open at once.
const TABS_MAX: usize = 12;

/// The disk as this run sees it: the real one, or the made-up machine's folders.
pub(crate) fn disk(ui: &Ui) -> Disk {
    if ui.sample {
        let map = Map::under(&ui.ctx.root).with(&Path::new(SAMPLE_OWN).join("config/turbinehub"), &ui.folder());
        Disk { map, room: Some(SAMPLE_ROOM) }
    } else {
        Disk::here()
    }
}

/// Now on the player's clock. The time zone is asked once (`date +%z`); the made-up machine lives on UTC.
fn when(ui: &mut Ui) -> When {
    let now = crate::now();
    if ui.sample {
        return When::utc(now);
    }
    let zone = *ui.files.zone.get_or_insert_with(|| {
        let out = std::process::Command::new("date").arg("+%z").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        When::from_date_z(0, &out).offset
    });
    When { secs: now, offset: zone }
}

fn who(ui: &Ui, fs: &dyn Fs) -> Who {
    if ui.sample {
        let mut w = Who::read(fs, Path::new(crate::fake::FILES_HOME));
        w.own = vec![PathBuf::from(SAMPLE_OWN)];
        w.data = Some(Path::new(SAMPLE_OWN).join("config/turbinehub/data"));
        return w;
    }
    let mut w = Who::read(fs, &ui.ctx.home);
    let mut own = vec![ui.ctx.home.join(".var/app").join(super::selftile::FLATPAK_ID)];
    for p in [ui.folder(), ui.data.clone(), ui.cache.clone()] {
        if !own.iter().any(|o| p.starts_with(o)) {
            own.push(p);
        }
    }
    w.own = own;
    w.data = Some(ui.data.clone());
    w
}

/// The places of this machine, read again when they are older than a moment or were asked for anew.
fn places(ui: &mut Ui) -> Places {
    let now = crate::now();
    if let Some((p, at)) = &ui.files.places
        && now.saturating_sub(*at) < PLACES_FOR
    {
        return p.clone();
    }
    let d = disk(ui);
    let mut p = Places::scan(&d, who(ui, &d), &ui.machine().drives);
    p.quick = Quick::load(&ui.folder(), &p);
    ui.files.places = Some((p.clone(), now));
    p
}

fn forget_places(ui: &mut Ui) {
    ui.files.places = None;
}

fn view(ui: &Ui) -> View {
    ui.files.view.unwrap_or(if ui.veteran() { View::Linux } else { View::Windows })
}

/// Where a view starts: Home in the Windows view, the home folder in the Linux view.
fn start(places: &Places, v: View) -> Place {
    places.canon(Place::Home, v)
}

fn tab(ui: &mut Ui) -> &mut Tab {
    if ui.files.tabs.is_empty() {
        let p = places(ui);
        let at = start(&p, view(ui));
        ui.files.tabs.push(Tab::at(at));
        ui.files.at = 0;
    }
    let i = ui.files.at.min(ui.files.tabs.len() - 1);
    ui.files.at = i;
    &mut ui.files.tabs[i]
}

fn go(ui: &mut Ui, place: Place) {
    let p = places(ui);
    let place = p.canon(place, view(ui));
    tab(ui).go(place);
    ui.files.seq += 1;
}

/// The real folder a place is, when it is one (new things and pasted things go there).
fn folder_of(place: &Place) -> Option<PathBuf> {
    match place {
        Place::Folder(p) => Some(p.clone()),
        _ => None,
    }
}

fn is_downloads(places: &Places, place: &Place) -> bool {
    matches!(place, Place::Folder(p) if places.dirs.iter().any(|d| d.name == "Downloads" && d.path == *p))
}

fn list_options(ui: &mut Ui, places: &Places, place: &Place, v: View, search: &str) -> fx::Options {
    // Downloads comes newest first under its dates, as on Windows, until the player picks another way.
    let auto = v == View::Windows && is_downloads(places, place);
    let (sort, descending) = ui.files.sort.unwrap_or(if auto { (Sort::Date, true) } else { (Sort::Name, false) });
    let group = ui.files.group.unwrap_or(if auto && ui.files.sort.is_none() { Group::Date } else { Group::None });
    fx::Options { hidden: ui.files.hidden, sort, descending, folders_first: true, group, search: search.to_string(), now: when(ui) }
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// A moment the way the list shows it: "Today 09:31", "Yesterday 21:40", "Sun 9 Nov", "28 Oct", "14 Oct 2025".
pub fn date_words(now: When, secs: u64) -> String {
    let (day, today) = (now.day_of(secs), now.day_of(now.secs));
    let local = secs as i64 + now.offset as i64;
    let clock = format!("{:02}:{:02}", local.rem_euclid(86_400) / 3600, local.rem_euclid(3600) / 60);
    let (y, m, d) = When::date(day);
    let month = MONTHS[(m as usize).clamp(1, 12) - 1];
    if day == today {
        format!("Today {clock}")
    } else if day == today - 1 {
        format!("Yesterday {clock}")
    } else if day < today && today - day < 7 {
        format!("{} {d} {month}", DAYS[(day + 4).rem_euclid(7) as usize])
    } else if y == When::date(today).0 {
        format!("{d} {month}")
    } else {
        format!("{d} {month} {y}")
    }
}

/// When the Recycle Bin wrote a deletion (2026-11-12T09:31:05, on the player's clock), as seconds since 1970.
fn stamp_secs(now: When, stamp: &str) -> Option<u64> {
    let num = |r: std::ops::Range<usize>| stamp.get(r).and_then(|t| t.parse::<i64>().ok());
    let day = When::day_number(num(0..4)?, num(5..7)? as u32, num(8..10)? as u32);
    let secs = day * 86_400 + num(11..13)? * 3600 + num(14..16)? * 60 + num(17..19).unwrap_or(0) - now.offset as i64;
    u64::try_from(secs).ok()
}

fn count_words(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// The icon a line gets: its kind, or for a file what it holds.
fn icon_of(e: &Entry) -> &'static str {
    if e.locked {
        return "lock";
    }
    match e.kind {
        Kind::Drive => "drive",
        Kind::Game => "games",
        Kind::App => "apps",
        Kind::Bin => "bin",
        Kind::Folder => "folder",
        Kind::Link if e.is_dir => "folder",
        Kind::Link | Kind::File => match e.type_word.as_str() {
            "Picture" => "picture",
            "Video" => "video",
            "Music" => "music",
            "Zip archive" | "Archive" | "App installer" | "Disc image" => "zip",
            "Text" | "Settings file" | "Script" | "Document" | "Web page" | "PDF document" => "textfile",
            "Shortcut" | "Windows shortcut" | "Link" => "shortcut",
            "App" | "Windows program" => "apps",
            _ => "file",
        },
    }
}

/// Which entries of the item menu a line gets, a letter each (`item_options` holds them).
fn codes(e: &Entry, place: &Place, v: View, places: &Places, window: bool) -> String {
    let mut c = String::new();
    if *place == Place::Trash {
        return "si".into();
    }
    let made_up = e.name != e.real_name || matches!(e.kind, Kind::Drive | Kind::Game | Kind::App | Kind::Bin) || matches!(place, Place::Home | Place::ThisPc | Place::Games | Place::Game(_) | Place::Programs | Place::AppData);
    if e.kind == Kind::Bin {
        return "i".into();
    }
    if !made_up && !e.locked {
        c.push_str("xcrd");
    } else if e.kind != Kind::Drive {
        c.push('c');
    }
    if e.is_dir {
        c.push('t');
    } else if window {
        c.push('w');
    }
    if v == View::Windows {
        c.push('a');
    }
    c.push('p');
    if e.is_dir && e.kind != Kind::Drive {
        c.push(if places.quick.is_pinned(&e.path) { 'u' } else { 'q' });
    }
    c.push('i');
    c
}

/// The item menu, as Windows 11 lays it out: the row of icons first (Cut, Copy, Rename, Delete), then the rest.
/// `sel`: the press is for everything that is ticked. `strip`: it is one of the icons.
fn item_options(ui: &Ui) -> Json {
    let one = |id: &str, title: &str, act: &str, value: &str, icon: &str, key: &str, sel: bool, strip: bool, loc: &str| (id.to_string(), obj! {"title": s(title), "act": s(act), "value": s(value), "icon": s(icon), "key": s(key), "sel": b(sel), "strip": b(strip), "local": s(loc)});
    let bin = if view(ui) == View::Windows { "Recycle Bin" } else { "Trash" };
    Json::Obj(vec![
        one("x", "Cut", "files-cut", "", "cut", "ctrl+x", true, true, ""),
        one("c", "Copy", "files-copy", "", "copy", "ctrl+c", true, true, ""),
        one("r", "Rename", "", "", "rename", "f2", false, true, "rename"),
        one("d", "Delete", "files-delete", "", "remove", "del", true, true, ""),
        one("w", "Open with", "files-open-with", "", "external", "", false, false, ""),
        one("t", "Open in new tab", "files-go", "newtab", "", "", false, false, ""),
        one("a", "Copy as path", "files-copy-path", "windows", "", "ctrl+shift+c", false, false, ""),
        one("p", "Copy real path", "files-copy-path", "real", "", "", false, false, ""),
        one("q", "Pin to Quick access", "files-pin", "", "", "", false, false, ""),
        one("u", "Unpin from Quick access", "files-unpin", "", "", "", false, false, ""),
        one("U", "Move up in Quick access", "files-pin-move", "up", "", "", false, false, ""),
        one("D", "Move down in Quick access", "files-pin-move", "down", "", "", false, false, ""),
        one("s", &format!("Restore from the {bin}"), "files-restore", "", "restore", "", true, false, ""),
        one("i", "Properties", "files-props", "", "", "alt+enter", false, false, ""),
    ])
}

/// What every line of one list is made out with.
struct Shown<'a> {
    place: &'a Place,
    v: View,
    places: &'a Places,
    now: When,
    ui: &'a Ui,
    /// The lines are what a search below the folder found.
    deep: bool,
}

fn row_of(e: &Entry, shown: &Shown) -> Json {
    let Shown { place, v, places, now, ui, deep } = *shown;
    let in_bin = *place == Place::Trash;
    let date = match (&e.deleted, e.modified) {
        (Some(stamp), _) => stamp_secs(now, stamp).map(|t| date_words(now, t)).unwrap_or_default(),
        (None, Some(t)) => date_words(now, t),
        _ => String::new(),
    };
    let size = match (e.kind, e.size, e.space) {
        (Kind::Drive, _, Some(sp)) => format!("{} free", fx::size_words(sp.free)),
        (Kind::Bin, Some(c), _) => count_words(c as usize, "item", "items"),
        (_, Some(bytes), _) if !e.is_dir => fx::size_words(bytes),
        _ => String::new(),
    };
    // The third column: the plain type; where a line was found (a search below) or deleted from (the bin).
    let kind_words = if in_bin || deep { e.detail.clone() } else { e.type_word.clone() };
    let cut = ui.files.cut && ui.files.clip.contains(&e.path);
    let rights = if e.mode.is_empty() { String::new() } else { format!("{}  {}", e.mode, if e.owner == e.group || e.group.is_empty() { e.owner.clone() } else { format!("{}:{}", e.owner, e.group) }) };
    obj! {
        "fid": s(format!("row:{}", e.path.display())),
        "name": s(&e.name),
        "real": s(&e.real_name),
        "path": s(e.path.to_string_lossy()),
        "opens": s(e.opens.as_ref().map(Place::key).unwrap_or_default()),
        "icon": s(icon_of(e)),
        "dir": b(e.is_dir),
        "date": s(date),
        "type": s(kind_words),
        "size": s(size),
        "detail": s(&e.detail),
        "rights": s(rights),
        "link": s(e.link.clone().unwrap_or_default()),
        "locked": b(e.locked),
        "dim": b(e.hidden || cut),
        "o": s(codes(e, place, v, places, ui.window)),
        "used": match e.space { Some(sp) if sp.total > 0 => n(((sp.total - sp.free.min(sp.total)) as f64 / sp.total as f64 * 100.0).round()), _ => Json::Null },
        "room": s(e.space.map(|sp| format!("{} free of {}", fx::size_words(sp.free), fx::size_words(sp.total))).unwrap_or_default()),
    }
}

/// The lines of a list with their group headings in between (a heading is a line of its own: `group`).
fn rows_of(entries: &[Entry], groups: &[(String, usize)], shown: &Shown) -> Vec<Json> {
    let mut out = Vec::with_capacity(entries.len() + groups.len());
    let mut last: Option<&str> = None;
    for e in entries {
        if !groups.is_empty() && last != Some(e.group_label.as_str()) {
            let count = groups.iter().find(|(l, _)| *l == e.group_label).map(|(_, c)| *c).unwrap_or(0);
            out.push(obj! {"group": s(&e.group_label), "count": n(count as f64)});
            last = Some(e.group_label.as_str());
        }
        out.push(row_of(e, shown));
    }
    out
}

fn sort_key(sort: Sort) -> &'static str {
    match sort {
        Sort::Name => "name",
        Sort::Date => "date",
        Sort::Type => "type",
        Sort::Size => "size",
    }
}

fn group_key(g: Group) -> &'static str {
    match g {
        Group::None => "none",
        Group::Date => "date",
        Group::Type => "type",
        Group::Size => "size",
        Group::Name => "name",
    }
}

/// A place's real folders as the status bar shows them. The made-up machine's own folder reads as on a real one.
fn real_words(real: &[PathBuf]) -> String {
    real.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("  \u{b7}  ")
}

/// What a copy under way says in the status bar.
fn job_json(verb: &str, p: &Progress) -> Json {
    let part = if p.bytes > 0 { p.bytes_done as f64 / p.bytes as f64 } else if p.files > 0 { p.files_done as f64 / p.files as f64 } else { 0.0 };
    let words = if p.files > 1 { format!("{verb} {} of {}: {}", (p.files_done + 1).min(p.files), p.files, p.name) } else if p.name.is_empty() { format!("{verb}...") } else { format!("{verb} {}", p.name) };
    let detail = if p.bytes > 0 { format!("{} of {}", fx::size_words(p.bytes_done), fx::size_words(p.bytes)) } else { String::new() };
    obj! {"words": s(words), "part": n((part.clamp(0.0, 1.0) * 1000.0).round() / 1000.0), "detail": s(detail)}
}

/// Takes in a page argument once: "at=<place>;view=linux;layout=icons;pane=1;hidden=1;sel=<name>;tab=<place>;
/// search=<words>;deep=1;sort=<how>;group=<how>". A link into Files and the pictures of the build use it.
fn apply(ui: &mut Ui, arg: &str) {
    if arg.is_empty() || ui.files.applied == arg {
        return;
    }
    ui.files.applied = arg.to_string();
    for part in arg.split(';') {
        let (k, val) = part.split_once('=').unwrap_or((part, ""));
        match k {
            "view" => ui.files.view = Some(View::from_key(val)),
            "layout" => ui.files.layout = Layout::from_key(val),
            "pane" => ui.files.pane = val != "0",
            "hidden" => ui.files.hidden = Some(val != "0"),
            "sort" => set_sort(ui, val),
            "group" => set_group(ui, val),
            "at" | "tab" => {
                let Some(place) = Place::from_key(val) else { continue };
                if k == "tab" && !ui.files.tabs.is_empty() && ui.files.tabs.len() < TABS_MAX {
                    let p = places(ui);
                    let place = p.canon(place, view(ui));
                    ui.files.tabs.push(Tab::at(place));
                    ui.files.at = ui.files.tabs.len() - 1;
                    ui.files.seq += 1;
                } else {
                    go(ui, place);
                }
            }
            "sel" => {
                let here = folder_of(&tab(ui).place);
                tab(ui).focus = here.map(|d| d.join(val));
            }
            "search" => tab(ui).search = val.chars().take(60).collect(),
            "deep" => search_below(ui),
            _ => {}
        }
    }
}

fn set_sort(ui: &mut Ui, how: &str) {
    let sort = match how {
        "date" => Sort::Date,
        "type" => Sort::Type,
        "size" => Sort::Size,
        _ => Sort::Name,
    };
    // The same column again turns the order round; dates and sizes start with the newest and the biggest.
    let first_desc = matches!(sort, Sort::Date | Sort::Size);
    ui.files.sort = Some(match ui.files.sort {
        Some((was, desc)) if was == sort => (sort, !desc),
        _ => (sort, first_desc),
    });
    if ui.files.group.is_none() {
        ui.files.group = Some(Group::None);
    }
}

fn set_group(ui: &mut Ui, how: &str) {
    ui.files.group = Some(match how {
        "date" => Group::Date,
        "type" => Group::Type,
        "size" => Group::Size,
        "name" => Group::Name,
        _ => Group::None,
    });
}

/// The page.
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    apply(ui, arg);
    let d = disk(ui);
    let places = places(ui);
    let v = view(ui);
    let win = v == View::Windows;
    let now = when(ui);
    // A view that was switched, or a drive that went away, may have left a tab somewhere this view doesn't have.
    let place = {
        let t = tab(ui);
        t.place = places.canon(t.place.clone(), v);
        t.place.clone()
    };
    let (search, focus, viewing) = {
        let t = tab(ui);
        (t.search.clone(), t.focus.take(), t.viewing.clone())
    };
    let opt = list_options(ui, &places, &place, v, &search);
    let listing = fx::list(&d, &places, &place, v, &opt);
    let tiles = matches!(listing.place, Place::Home | Place::ThisPc);
    let in_bin = listing.place == Place::Trash;

    // A search below the folder replaces the list with what it found.
    let deep = tab(ui).deep.as_ref().map(|dp| {
        let g = dp.shared.lock().map(|g| (g.0.clone(), g.1)).unwrap_or((Found { entries: Vec::new(), more: false }, true));
        (dp.needle.clone(), g.0, g.1)
    });
    let shown_as = |deep: bool| Shown { place: &listing.place, v, places: &places, now, ui: &*ui, deep };
    let rows = match &deep {
        Some((_, found, _)) => rows_of(&found.entries, &[], &shown_as(true)),
        None => rows_of(&listing.entries, &listing.groups, &shown_as(false)),
    };
    let shown = deep.as_ref().map(|(_, f, _)| f.entries.len()).unwrap_or(listing.entries.len());

    // The two start pages are tiles under headings.
    let mut sections: Vec<Json> = Vec::new();
    if tiles {
        let mut labels: Vec<&str> = Vec::new();
        for e in &listing.entries {
            if !labels.contains(&e.group_label.as_str()) {
                labels.push(&e.group_label);
            }
        }
        for label in labels {
            let of: Vec<Json> = listing.entries.iter().filter(|e| e.group_label == label).map(|e| row_of(e, &Shown { place: &listing.place, v, places: &places, now, ui: &*ui, deep: false })).collect();
            sections.push(obj! {"title": s(label), "tiles": arr(of)});
        }
    }

    let nav: Vec<Json> = places
        .nav(v)
        .into_iter()
        .map(|i| {
            let path = folder_of(&i.place).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            // A pinned folder can be opened in a tab, unpinned and moved; every other line only opened.
            let pins = places.quick.pins.len();
            let at = places.quick.pins.iter().position(|p| Some(p) == folder_of(&i.place).as_ref());
            let mut o = String::from("t");
            if i.pinned {
                o.push('u');
                if at.is_some_and(|a| a > 0) {
                    o.push('U');
                }
                if at.is_some_and(|a| a + 1 < pins) {
                    o.push('D');
                }
            }
            if !path.is_empty() {
                o.push_str("pi");
            }
            obj! {"label": s(&i.label), "key": s(i.place.key()), "icon": s(i.icon), "pinned": b(i.pinned), "depth": n(i.depth as f64), "line": b(i.starts_group), "on": b(i.place == listing.place), "path": s(path), "fid": s(format!("nav:{}", i.place.key())), "o": s(o)}
        })
        .collect();

    let mut way = places.crumbs(&d, &listing.place, v);
    // True Linux starts every way at "/": the made-up page above it isn't part of a real path.
    if !win && way.len() > 1 && way[0].1 == Place::ThisPc {
        way.remove(0);
    }
    let crumbs: Vec<Json> = way.into_iter().map(|(label, p)| obj! {"label": s(label), "key": s(p.key())}).collect();
    let tabs: Vec<Json> = ui
        .files
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let icon = match &t.place {
                Place::Home => "home",
                Place::ThisPc => "pc",
                Place::Trash => "bin",
                Place::Games | Place::Game(_) => "games",
                Place::Programs => "apps",
                Place::Windows => "lock",
                _ => "folder",
            };
            obj! {"title": s(places.title(&t.place, v)), "icon": s(icon), "on": b(i == ui.files.at)}
        })
        .collect();

    let here = folder_of(&listing.place);
    let can_add = here.as_ref().is_some_and(|p| places.may_add_to(&d, p).is_ok());
    let hidden_now = ui.files.hidden.unwrap_or(!win);
    let bin = if win { "Recycle Bin" } else { "Trash" };

    // What the page itself offers (the Options menu lists these after the highlighted thing's own).
    let mut options: Vec<Json> = Vec::new();
    if ui.files.job.is_some() || ui.files.demo {
        options.push(option("Stop the copy", "files-job-stop", ""));
    }
    if in_bin {
        options.push(page_button("Restore all", "files-restore-all", ""));
        options.push(page_button(&format!("Empty the {bin}"), "files-empty", ""));
    }
    if can_add {
        options.push(option("New", "files-menu", "new"));
        if !ui.files.clip.is_empty() {
            options.push(with(option(&format!("Paste {}", count_words(ui.files.clip.len(), "item", "items")), "files-paste", ""), "key", s("ctrl+v")));
        }
    }
    if let Some(u) = ui.files.history.newest() {
        options.push(with(option(&u.words(), "files-undo", ""), "key", s("ctrl+z")));
    }
    if !tiles {
        options.push(with(local("Select all", "all", ""), "key", s("ctrl+a")));
    }
    options.push(option("Sort and view", "files-menu", "view"));
    options.push(with(local("Type a path", "path", ""), "key", s("ctrl+l")));
    options.push(with(option("New tab", "files-tab", "new"), "key", s("ctrl+t")));
    options.push(with(option("Refresh", "files-refresh", ""), "key", s("f5")));

    let title = listing.title.clone();
    let status_count = match (&deep, listing.hidden) {
        (Some((_, f, done)), _) => format!("{} found{}", f.entries.len(), if !*done { ", still looking" } else if f.more { ", there may be more" } else { "" }),
        (None, 0) => count_words(shown, "item", "items"),
        (None, h) => format!("{}, {h} hidden", count_words(shown, "item", "items")),
    };
    let job = if ui.files.demo {
        job_json("Copying", &Progress { files_done: 2, files: 12, bytes_done: 78_000_000, bytes: 184_000_000, name: "trailer-capture.mp4".into() })
    } else {
        match &ui.files.job {
            Some(j) => job_json(j.verb, &j.progress.lock().map(|p| p.clone()).unwrap_or_default()),
            None => Json::Null,
        }
    };
    let viewer = match viewing {
        Some(path) => viewer_json(&d, &path),
        None => Json::Null,
    };
    let (sort_now, desc_now, group_now) = (opt.sort, opt.descending, opt.group);
    let third = if in_bin {
        "Original location"
    } else if deep.is_some() {
        "Folder"
    } else {
        "Type"
    };
    let empty = if listing.problem.is_some() {
        String::new()
    } else if deep.as_ref().is_some_and(|(_, f, done)| f.entries.is_empty() && *done) {
        format!("Nothing below {title} has \"{}\" in its name.", deep.as_ref().map(|x| x.0.as_str()).unwrap_or(""))
    } else if shown == 0 && !search.is_empty() {
        format!("Nothing here has \"{search}\" in its name.")
    } else if shown == 0 && in_bin {
        format!("The {bin} is empty.")
    } else if shown == 0 && listing.place == Place::Home {
        "Pin a folder to keep it here.".into()
    } else if shown == 0 {
        "This folder is empty.".into()
    } else {
        String::new()
    };
    obj! {
        "title": s(&title),
        "files": b(true),
        "seq": n(ui.files.seq as f64),
        "view": s(v.key()),
        "layout": s(ui.files.layout.key()),
        "pane": b(ui.files.pane),
        "tabs": arr(tabs),
        "canBack": b(!tab(ui).back.is_empty()),
        "canFwd": b(!tab(ui).fwd.is_empty()),
        "canUp": b(places.up(&d, &listing.place, v).is_some()),
        "canAdd": b(can_add),
        "crumbs": arr(crumbs),
        "typed": s(&listing.shown_path),
        "place": s(listing.place.key()),
        "here": s(here.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()),
        "kind": s(if tiles { "tiles" } else if in_bin { "bin" } else { "list" }),
        "nav": arr(nav),
        "note": s(listing.note.clone().unwrap_or_default()),
        "locked": s(listing.locked.clone().unwrap_or_default()),
        "problem": s(listing.problem.clone().unwrap_or_default()),
        "empty": s(empty),
        "columns": obj! {"name": s("Name"), "date": s(if in_bin { "Date deleted" } else { "Date modified" }), "type": s(third), "size": s("Size"), "rights": s("Permissions")},
        "sort": s(sort_key(sort_now)),
        "desc": b(desc_now),
        "group": s(group_key(group_now)),
        "hidden": b(hidden_now),
        "rows": arr(rows),
        "sections": arr(sections),
        "focus": s(focus.map(|p| format!("row:{}", p.display())).unwrap_or_default()),
        "status": obj! {
            "count": s(status_count),
            "real": s(if win { real_words(&listing.real) } else { String::new() }),
            "words": s(if win { "" } else { "Showing real names and paths" }),
            "locked": s(if listing.locked.is_some() { "Locked: look, don't change" } else { "" }),
            "room": s(listing.free.map(|sp| format!("{} free", fx::size_words(sp.free))).unwrap_or_default()),
        },
        "search": obj! {"value": s(&search), "arg": s("files"), "hint": s(format!("Search {title}"))},
        "deep": match &deep {
            Some((needle, f, done)) => obj! {"needle": s(needle), "running": b(!*done), "more": b(f.more), "count": n(f.entries.len() as f64)},
            None => Json::Null,
        },
        "canDeep": b(!search.is_empty() && deep.is_none() && !tiles && !in_bin),
        "clip": obj! {"count": n(ui.files.clip.len() as f64), "cut": b(ui.files.cut)},
        "job": job,
        "undo": s(ui.files.history.newest().map(|u| u.words()).unwrap_or_default()),
        "viewer": viewer,
        "itemOptions": item_options(ui),
        "options": arr(options),
        "primary": obj! {"label": s("Select"), "act": s("files-tick"), "arg": s("")},
    }
}

fn viewer_json(d: &Disk, path: &Path) -> Json {
    let name = fx::places::name_of(path);
    match fx::preview(d, path) {
        Preview::Text { text, encoding, cut } => obj! {"name": s(name), "kind": s("text"), "text": s(text), "load": s(""), "note": s(if cut { format!("The first part of the file ({encoding}). It opens whole with its own app in Desktop Mode.") } else { format!("Text ({encoding})") })},
        Preview::Picture { load } => obj! {"name": s(name), "kind": s("picture"), "text": s(""), "load": s(load.to_string_lossy()), "note": s("Picture")},
        Preview::Outside { why } => obj! {"name": s(name), "kind": s("none"), "text": s(""), "load": s(""), "note": s(why)},
    }
}

/// The paths a press is for: a list written as JSON in `value`, or the one path in `arg`.
fn paths_of(arg: &str, value: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = crate::json::parse(value).ok().map(|j| j.as_array().iter().filter_map(Json::as_str).filter(|p| p.starts_with('/')).map(PathBuf::from).collect()).unwrap_or_default();
    if out.is_empty() && arg.starts_with('/') {
        out.push(PathBuf::from(arg));
    }
    out
}

fn dialog_json(title: &str, text: &str, items: Vec<(String, String)>, confirm: &str, cancel: &str, danger: bool) -> Json {
    obj! {"title": s(title), "text": s(text), "items": arr(items.into_iter().map(|(t, l)| obj! {"title": s(t), "line": s(l)})), "confirm": s(confirm), "cancel": s(cancel), "ask": s(""), "danger": b(danger)}
}

/// A menu inside the dialog: rows to choose from.
fn menu(title: &str, rows: Vec<Json>) -> Json {
    obj! {"dialog": with(with(dialog_json(title, "", Vec::new(), "", "Close", false), "rows", arr(rows)), "choose", b(true))}
}

fn size_and_date(d: &Disk, p: &Path, now: When) -> String {
    match d.meta(p) {
        Ok(m) if m.is_dir() => format!("Folder, {}", date_words(now, m.modified)),
        Ok(m) => format!("{}, {}", fx::size_words(m.size), date_words(now, m.modified)),
        Err(_) => String::new(),
    }
}

/// Where a copy or move puts things.
fn target_of(op: &Op) -> Option<&Path> {
    match op {
        Op::Copy { into, .. } | Op::Move { into, .. } => Some(into),
        _ => None,
    }
}

/// The question for one name that is taken.
fn conflict_dialog(ui: &mut Ui) -> Json {
    let d = disk(ui);
    let now = when(ui);
    let Some(a) = &ui.files.asking else { return obj! {} };
    let Some(item) = a.plan.conflicts.get(a.at) else { return obj! {} };
    let name = fx::places::name_of(item);
    let into = target_of(&a.plan.op).map(Path::to_path_buf).unwrap_or_default();
    let left = a.plan.conflicts.len() - a.at - 1;
    let moving = matches!(a.plan.op, Op::Move { .. });
    let items = vec![("The one already there".to_string(), size_and_date(&d, &into.join(&name), now)), (format!("The one you are {}", if moving { "moving" } else { "copying" }), size_and_date(&d, item, now))];
    let mut rows = vec![
        obj! {"title": s("Skip this one"), "act": s("files-answer"), "arg": s("skip")},
        obj! {"title": s("Replace (the one there goes to the Recycle Bin)"), "act": s("files-answer"), "arg": s("replace")},
        obj! {"title": s("Keep both (the new one gets a number)"), "act": s("files-answer"), "arg": s("both")},
    ];
    if left > 0 {
        rows.push(obj! {"title": s(format!("Do this for the other {left} too: {}", if a.all { "yes" } else { "no" })), "act": s("files-answer"), "arg": s("all")});
    }
    let text = format!("{} already has something named {name}.", fx::places::name_of(&into));
    obj! {"dialog": with(with(with(dialog_json(&format!("{name} is already there"), &text, items, "", "Cancel", false), "rows", arr(rows)), "choose", b(true)), "props", b(true))}
}

fn note_of(done: &fx::Done) -> String {
    match done.skipped.len() {
        0 => done.words.clone(),
        1 => format!("{} Left out: {}", done.words, done.skipped[0].1),
        c => format!("{} {c} were left out: {}", done.words, done.skipped[0].1),
    }
}

fn finished(ui: &mut Ui, result: Result<fx::Done, String>) -> String {
    forget_places(ui);
    match result {
        Ok(done) => {
            ui.files.history.remember(&done);
            if let Some(p) = done.made.first() {
                tab(ui).focus = Some(p.clone());
            }
            note_of(&done)
        }
        Err(why) => why,
    }
}

/// Runs a plan whose questions are answered: at once when it is quick, else on its own thread with its progress
/// in the status bar.
fn run_plan(ui: &mut Ui, plan: fx::Plan, answers: Answers) -> Json {
    let d = disk(ui);
    let p = places(ui);
    let now = when(ui);
    let kind = plan.op.kind();
    let slow = match kind {
        OpKind::Copy => true,
        OpKind::Move => !plan.same_drive,
        OpKind::Empty => true,
        _ => false,
    };
    // A made-up machine does everything at once: its files are tiny and its pictures can't wait.
    let quick = ui.sample || (plan.bytes <= QUICK_BYTES && plan.files <= QUICK_FILES);
    if !slow || (quick && !ui.files.always_background) {
        // A cut that was pasted is done with.
        let was_move = kind == OpKind::Move;
        let result = fx::run(&d, &p, &plan, &answers, now, &mut |_| true);
        if was_move && result.is_ok() && ui.files.cut {
            ui.files.clip.clear();
            ui.files.cut = false;
        }
        let words = finished(ui, result);
        return obj! {"refresh": b(true), "toast": s(words)};
    }
    let progress = Arc::new(Mutex::new(Progress { files: plan.files, bytes: plan.bytes, ..Progress::default() }));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    let (pr, st) = (progress.clone(), stop.clone());
    let made = std::thread::Builder::new().name("turbinehub-files".into()).spawn(move || {
        let result = fx::run(&d, &p, &plan, &answers, now, &mut |now_at| {
            if let Ok(mut g) = pr.lock() {
                *g = now_at.clone();
            }
            !st.load(Ordering::SeqCst)
        });
        let _ = tx.send(result);
    });
    if made.is_err() {
        return toast("That couldn't be started just now. Try again.");
    }
    if kind == OpKind::Move && ui.files.cut {
        ui.files.clip.clear();
        ui.files.cut = false;
    }
    let verb = match kind {
        OpKind::Move => "Moving",
        OpKind::Empty => "Emptying",
        _ => "Copying",
    };
    ui.files.job = Some(Job { verb, progress, stop, rx, told: (u64::MAX, 0) });
    obj! {"refresh": b(true), "toast": s(format!("{verb} in the background. The status bar shows how far it is."))}
}

/// One thing to do: looked at first (the engine's plan), asked about where a name is taken or something is erased
/// for good, then done. What may not be done is said in the engine's own plain words.
fn start_op(ui: &mut Ui, op: Op) -> Json {
    if ui.files.job.is_some() {
        return toast("Something is still being copied. Wait for it, or stop it from the status bar.");
    }
    let d = disk(ui);
    let p = places(ui);
    let plan = match fx::plan(&d, &p, op) {
        Ok(plan) => plan,
        Err(why) => return toast(&why),
    };
    if plan.dangerous {
        let text = plan.lines.join(" ");
        let title = plan.title.clone();
        ui.files.asking = Some(Asking { plan, answers: Answers::default(), at: 0, all: false });
        return obj! {"dialog": with(dialog_json(&title, &text, Vec::new(), "Empty", "Cancel", true), "then", obj! {"act": s("files-run"), "arg": s("")})};
    }
    if !plan.conflicts.is_empty() {
        ui.files.asking = Some(Asking { plan, answers: Answers::default(), at: 0, all: false });
        return conflict_dialog(ui);
    }
    run_plan(ui, plan, Answers::default())
}

fn answer(ui: &mut Ui, how: &str) -> Json {
    let Some(mut a) = ui.files.asking.take() else { return toast("That question is no longer open.") };
    let choice = match how {
        "all" => {
            a.all = !a.all;
            ui.files.asking = Some(a);
            return conflict_dialog(ui);
        }
        "replace" => Choice::Replace,
        "both" => Choice::KeepBoth,
        _ => Choice::Skip,
    };
    if a.all {
        a.answers.all = choice;
        a.at = a.plan.conflicts.len();
    } else if let Some(item) = a.plan.conflicts.get(a.at).cloned() {
        a.answers.each.push((item, choice));
        a.at += 1;
    }
    if a.at < a.plan.conflicts.len() {
        ui.files.asking = Some(a);
        return conflict_dialog(ui);
    }
    run_plan(ui, a.plan, a.answers)
}

/// Opens a line: a folder is gone into; a text or a picture shows inside the app; anything else opens with its own
/// app in Desktop Mode, and in Game Mode the page says so with the real path to copy.
fn open_item(ui: &mut Ui, path: &Path, opens: &str) -> Json {
    if let Some(place) = Place::from_key(opens).filter(|_| !opens.is_empty()) {
        go(ui, place);
        return refresh();
    }
    if tab(ui).place == Place::Trash {
        return toast(fx::guard::BIN_WHY);
    }
    let d = disk(ui);
    let name = fx::places::name_of(path);
    // A link to a folder that the list couldn't follow, or a folder typed by hand.
    if d.follow(path).is_ok_and(|m| m.is_dir()) {
        go(ui, Place::Folder(path.to_path_buf()));
        return refresh();
    }
    let mut p = places(ui);
    p.quick.opened(path);
    let _ = p.quick.save(&ui.folder());
    forget_places(ui);
    match fx::preview(&d, path) {
        Preview::Text { .. } | Preview::Picture { .. } => {
            tab(ui).viewing = Some(path.to_path_buf());
            refresh()
        }
        Preview::Outside { why } if !ui.window => {
            let text = format!("{why} In Game Mode Turbine Hub can't start another app's window. Switch to Desktop Mode (Steam button, Power, Switch to Desktop) and open it from Files there.");
            obj! {"dialog": with(dialog_json(&format!("{name} opens in Desktop Mode"), &text, vec![("Its real path".into(), path.to_string_lossy().into_owned())], "Copy real path", "Close", false), "then", obj! {"act": s("files-copy-path"), "arg": s(path.to_string_lossy()), "value": s("real")})}
        }
        Preview::Outside { .. } => {
            let line = fx::open_line(path, fx::fs::in_sandbox());
            obj! {"toast": s(format!("Opening {name} with its own app.")), "run": arr(line.into_iter().map(s))}
        }
    }
}

/// What the details pane shows for one line.
fn peek(ui: &mut Ui, path: &Path) -> Json {
    let d = disk(ui);
    let p = places(ui);
    let v = view(ui);
    let now = when(ui);
    let Ok(props) = fx::properties(&d, &p, path, v, 2000) else { return obj! {"peek": Json::Null} };
    let mut lines: Vec<(String, String)> = vec![("Type".into(), props.type_word.clone())];
    if props.is_dir {
        lines.push(("Holds".into(), format!("{}{}, {}", if props.counted_all { "" } else { "more than " }, count_words(props.files as usize, "file", "files"), count_words(props.folders as usize, "folder", "folders"))));
    } else {
        lines.push(("Size".into(), fx::size_words(props.size)));
    }
    lines.push(("Date modified".into(), date_words(now, props.modified)));
    if v == View::Windows {
        lines.push(("Where it is".into(), props.windows_path.clone()));
    }
    lines.push(("Real path".into(), props.real_path.to_string_lossy().into_owned()));
    if let Some(why) = &props.locked {
        lines.push(("Locked".into(), why.clone()));
    }
    let (kind, text, load) = if props.is_dir {
        ("none", String::new(), String::new())
    } else {
        match fx::preview(&d, path) {
            Preview::Text { text, .. } => ("text", text.chars().take(PEEK_CHARS).collect(), String::new()),
            Preview::Picture { load } => ("picture", String::new(), load.to_string_lossy().into_owned()),
            Preview::Outside { .. } => ("none", String::new(), String::new()),
        }
    };
    obj! {"peek": obj! {"name": s(&props.name), "path": s(path.to_string_lossy()), "kind": s(kind), "text": s(text), "load": s(load), "dir": b(props.is_dir), "lines": arr(lines.into_iter().map(|(t, l)| obj! {"title": s(t), "line": s(l)}))}}
}

fn properties(ui: &mut Ui, path: &Path) -> Json {
    let d = disk(ui);
    let p = places(ui);
    let v = view(ui);
    let now = when(ui);
    let props = match fx::properties(&d, &p, path, v, COUNT_MAX) {
        Ok(x) => x,
        Err(why) => return toast(&why),
    };
    let mut items: Vec<(String, String)> = vec![("Type".into(), props.type_word.clone())];
    if v == View::Windows {
        items.push(("Where it is".into(), props.windows_path.clone()));
    }
    items.push(("Real path".into(), props.real_path.to_string_lossy().into_owned()));
    if let Some(to) = &props.link {
        items.push(("Leads to".into(), to.clone()));
    }
    items.push(("Size".into(), format!("{} ({} bytes)", fx::size_words(props.size), props.size)));
    if props.is_dir {
        items.push(("Holds".into(), format!("{}{}, {}", if props.counted_all { "" } else { "More than " }, count_words(props.files as usize, "file", "files"), count_words(props.folders as usize, "folder", "folders"))));
    }
    if let Some(c) = props.created {
        items.push(("Created".into(), date_words(now, c)));
    }
    items.push(("Date modified".into(), date_words(now, props.modified)));
    items.push(("Drive".into(), match props.space {
        Some(sp) => format!("{}, {} free of {}", props.drive, fx::size_words(sp.free), fx::size_words(sp.total)),
        None => props.drive.clone(),
    }));
    match &props.locked {
        Some(why) => items.push(("Locked".into(), why.clone())),
        None if props.read_only => items.push(("Read only".into(), fx::guard::NOT_ALLOWED.into())),
        None => {}
    }
    if v == View::Linux || ui.veteran() {
        items.push(("Permissions".into(), format!("{}, owner {}, group {}", props.mode, props.uid, props.gid)));
    }
    obj! {"dialog": with(dialog_json(&props.name, "", items, "", "Close", false), "props", b(true))}
}

/// Starts looking through the folders below the one on screen for the words in the search box.
fn search_below(ui: &mut Ui) {
    let needle = tab(ui).search.trim().to_string();
    if needle.is_empty() {
        return;
    }
    let d = disk(ui);
    let p = places(ui);
    let v = view(ui);
    let place = tab(ui).place.clone();
    let shared = Arc::new(Mutex::new((Found { entries: Vec::new(), more: false }, false)));
    let stop = Arc::new(AtomicBool::new(false));
    if ui.sample {
        let found = fx::search(&d, &p, &place, v, &needle, Limits::default());
        if let Ok(mut g) = shared.lock() {
            *g = (found, true);
        }
    } else {
        let (sh, st, nd) = (shared.clone(), stop.clone(), needle.clone());
        let _ = std::thread::Builder::new().name("turbinehub-search".into()).spawn(move || {
            let found = fx::search_with(&d, &p, &place, v, &nd, Limits::default(), &mut |so_far| {
                if let Ok(mut g) = sh.lock() {
                    g.0 = so_far.clone();
                }
                !st.load(Ordering::SeqCst)
            });
            if let Ok(mut g) = sh.lock() {
                *g = (found, true);
            }
        });
    }
    let t = tab(ui);
    if let Some(old) = t.deep.replace(Deep { needle, shared, stop, told: (0, false) }) {
        old.stop.store(true, Ordering::SeqCst);
    }
}

/// What changed in the background since the last look: a copy's progress or its end, a search finding more. The
/// window asks many times a second and hears something only when there is news.
pub fn tick(ui: &mut Ui) -> Option<String> {
    if let Some(j) = ui.files.job.as_mut() {
        if let Ok(result) = j.rx.try_recv() {
            ui.files.job = None;
            let words = finished(ui, result);
            return Some(obj! {"done": b(true), "toast": s(words)}.to_text());
        }
        let p = j.progress.lock().map(|p| p.clone()).unwrap_or_default();
        if j.told != (p.files_done, p.bytes_done >> 20) {
            j.told = (p.files_done, p.bytes_done >> 20);
            return Some(obj! {"job": job_json(j.verb, &p)}.to_text());
        }
    }
    for t in ui.files.tabs.iter_mut() {
        if let Some(dp) = t.deep.as_mut() {
            let now = dp.shared.lock().map(|g| (g.0.entries.len(), g.1)).unwrap_or((0, true));
            if now != dp.told {
                dp.told = now;
                return Some(obj! {"search": b(true)}.to_text());
            }
        }
    }
    None
}

fn copy_answer(text: &str) -> Json {
    obj! {"copy": s(text), "toast": s(format!("Copied: {text}"))}
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    if action == "search" {
        if arg != "files" {
            return None;
        }
        let t = tab(ui);
        t.search = value.chars().filter(|c| !c.is_control()).take(60).collect();
        if let Some(d) = t.deep.take() {
            d.stop.store(true, Ordering::SeqCst);
        }
        return Some(refresh());
    }
    if !action.starts_with("files-") {
        return None;
    }
    let d = disk(ui);
    Some(match &action[6..] {
        // A link into Files from another page: there, and the Files tab.
        "show" => {
            if let Some(place) = Place::from_key(arg) {
                go(ui, place);
            }
            open("files")
        }
        "go" => {
            let Some(place) = Place::from_key(arg) else { return Some(toast("That place isn't there.")) };
            if value == "newtab" {
                if ui.files.tabs.len() >= TABS_MAX {
                    return Some(toast("That is as many tabs as fit. Close one first."));
                }
                let p = places(ui);
                let place = p.canon(place, view(ui));
                tab(ui);
                ui.files.tabs.push(Tab::at(place));
                ui.files.at = ui.files.tabs.len() - 1;
                ui.files.seq += 1;
            } else {
                go(ui, place);
            }
            refresh()
        }
        "open" => return Some(open_item(ui, Path::new(arg), value)),
        "close-view" => {
            tab(ui).viewing = None;
            refresh()
        }
        "back" | "fwd" => {
            let t = tab(ui);
            let (from, to) = if action.ends_with("back") { (&mut t.back, &mut t.fwd) } else { (&mut t.fwd, &mut t.back) };
            let Some(place) = from.pop() else { return Some(obj! {}) };
            let left = std::mem::replace(&mut t.place, place);
            to.push(left.clone());
            t.leave();
            t.focus = folder_of(&left);
            ui.files.seq += 1;
            refresh()
        }
        "up" => {
            let p = places(ui);
            let v = view(ui);
            let here = tab(ui).place.clone();
            let Some(up) = p.up(&d, &here, v) else { return Some(toast("This is the top.")) };
            go(ui, up);
            // The folder just left keeps the highlight, as on Windows.
            tab(ui).focus = p.real_paths(&here).into_iter().next();
            refresh()
        }
        "refresh" => {
            forget_places(ui);
            refresh()
        }
        "path" => {
            let p = places(ui);
            match p.go(&d, value, view(ui)) {
                Ok((place, file)) => {
                    go(ui, place);
                    tab(ui).focus = file;
                    refresh()
                }
                Err(why) => toast(&why),
            }
        }
        "deep" => {
            search_below(ui);
            refresh()
        }
        "deep-stop" => {
            if let Some(dp) = tab(ui).deep.as_ref() {
                dp.stop.store(true, Ordering::SeqCst);
            }
            refresh()
        }
        "sort" => {
            set_sort(ui, arg);
            refresh()
        }
        "group" => {
            set_group(ui, arg);
            refresh()
        }
        "hidden" => {
            let now = ui.files.hidden.unwrap_or(view(ui) == View::Linux);
            ui.files.hidden = Some(!now);
            refresh()
        }
        "layout" => {
            ui.files.layout = Layout::from_key(arg);
            refresh()
        }
        "pane" => {
            ui.files.pane = !ui.files.pane;
            refresh()
        }
        "view" => {
            let v = View::from_key(arg);
            if v != view(ui) {
                ui.files.view = Some(v);
                ui.files.seq += 1;
            }
            refresh()
        }
        "menu" => match arg {
            "new" => menu("New", vec![option("Folder", "files-new", "folder"), option("Text file", "files-new", "text"), option("Shortcut", "files-new", "shortcut")]),
            _ => {
                let v = view(ui);
                let mark = |on: bool, words: &str| if on { format!("{words} (now)") } else { words.to_string() };
                let p = places(ui);
                let place = tab(ui).place.clone();
                let search = tab(ui).search.clone();
                let o = list_options(ui, &p, &place, v, &search);
                let mut rows = Vec::new();
                for (k, words, sort) in [("name", "Sort by name", Sort::Name), ("date", "Sort by date", Sort::Date), ("type", "Sort by type", Sort::Type), ("size", "Sort by size", Sort::Size)] {
                    rows.push(option(&mark(o.sort == sort, words), "files-sort", k));
                }
                for (k, words, g) in [("none", "No groups", Group::None), ("date", "Group by date", Group::Date), ("type", "Group by type", Group::Type), ("size", "Group by size", Group::Size)] {
                    rows.push(option(&mark(o.group == g, words), "files-group", k));
                }
                for (k, words, l) in [("details", "Details", Layout::Details), ("icons", "Large icons", Layout::Icons), ("list", "List", Layout::List)] {
                    rows.push(option(&mark(ui.files.layout == l, words), "files-layout", k));
                }
                let hidden = ui.files.hidden.unwrap_or(v == View::Linux);
                rows.push(option(if hidden { "Hidden files: hide" } else { "Hidden files: show" }, "files-hidden", ""));
                rows.push(option(if ui.files.pane { "Details pane: hide" } else { "Details pane: show" }, "files-pane", ""));
                rows.push(option(if v == View::Windows { "Switch to True Linux" } else { "Switch to Windows-like" }, "files-view", if v == View::Windows { "linux" } else { "windows" }));
                menu("Sort and view", rows)
            }
        },
        "new" => {
            let Some(at) = folder_of(&tab(ui).place) else { return Some(toast("New things go into a folder. Open one first.")) };
            if arg == "shortcut" && value.is_empty() {
                let text = "Type where it leads: a folder, a file or a web address. Windows-style and real paths both work.";
                return Some(obj! {"dialog": with(with(dialog_json("New shortcut", text, Vec::new(), "Make", "Cancel", false), "input", obj! {"label": s("Where it leads"), "value": s(""), "hint": s("C:\\Games or https://...")}), "then", obj! {"act": s("files-new"), "arg": s("shortcut")})});
            }
            let free = |name: &str| fx::places::name_of(&fx::guard::free_name(&d, &at, std::ffi::OsStr::new(name)));
            let op = match arg {
                "folder" => Op::NewFolder { at: at.clone(), name: if value.is_empty() { free("New folder") } else { value.to_string() } },
                "text" => Op::NewText { at: at.clone(), name: if value.is_empty() { free("New text file.txt") } else { value.to_string() } },
                _ => {
                    let target = value.trim();
                    let last = target.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").trim_matches('"');
                    let name = if last.is_empty() || last.ends_with(':') { "Shortcut".to_string() } else { format!("Shortcut to {last}") };
                    Op::NewShortcut { at: at.clone(), name: free(&format!("{name}.desktop")).trim_end_matches(".desktop").to_string(), target: target.to_string() }
                }
            };
            let renames = arg != "shortcut" && value.is_empty();
            let answer = start_op(ui, op);
            // A new folder or text file is named right away, as on Windows.
            match tab(ui).focus.clone() {
                Some(made) if renames => with(answer, "rename", s(made.to_string_lossy())),
                _ => answer,
            }
        }
        // Rename asked for by a press that isn't the page's own (a script, a picture): the page starts the typing.
        "rename-start" => obj! {"rename": s(arg)},
        "rename" => {
            let name = value.trim();
            if arg.is_empty() || name.is_empty() || name == fx::places::name_of(Path::new(arg)) {
                return Some(refresh());
            }
            start_op(ui, Op::Rename { path: PathBuf::from(arg), name: name.to_string() })
        }
        "cut" | "copy" => {
            let items = paths_of(arg, value);
            if items.is_empty() {
                return Some(toast("Pick something first."));
            }
            let cut = action.ends_with("cut");
            let words = format!("{} {}. Paste puts {} where you are.", if cut { "Cut" } else { "Copied" }, count_words(items.len(), "item", "items"), if items.len() == 1 { "it" } else { "them" });
            ui.files.clip = items;
            ui.files.cut = cut;
            obj! {"refresh": b(true), "toast": s(words)}
        }
        "paste" => {
            let into = if arg.starts_with('/') { Some(PathBuf::from(arg)) } else { folder_of(&tab(ui).place) };
            let Some(into) = into else { return Some(toast("Things are pasted into a folder. Open one first.")) };
            if ui.files.clip.is_empty() {
                return Some(toast("Nothing was cut or copied yet."));
            }
            let items = ui.files.clip.clone();
            start_op(ui, if ui.files.cut { Op::Move { items, into } } else { Op::Copy { items, into } })
        }
        // Dropped with the mouse: moved, or copied with Ctrl held (`arg` is "move|<folder>" or "copy|<folder>").
        "drop" => {
            let (how, into) = arg.split_once('|').unwrap_or(("move", arg));
            let items = paths_of("", value);
            if items.is_empty() || !into.starts_with('/') {
                return Some(obj! {});
            }
            let into = PathBuf::from(into);
            // Dropped where it already is: nothing to do.
            if how != "copy" && items.iter().all(|i| i.parent() == Some(into.as_path())) {
                return Some(obj! {});
            }
            start_op(ui, if how == "copy" { Op::Copy { items, into } } else { Op::Move { items, into } })
        }
        "delete" => {
            let items = paths_of(arg, value);
            if items.is_empty() {
                return Some(toast("Pick something first."));
            }
            start_op(ui, Op::Delete { items })
        }
        "restore" => {
            let items = paths_of(arg, value);
            if items.is_empty() {
                return Some(toast("Pick something first."));
            }
            start_op(ui, Op::Restore { items })
        }
        "restore-all" => {
            let p = places(ui);
            let items: Vec<PathBuf> = fx::trash::items(&d, &p).into_iter().map(|t| t.at).collect();
            if items.is_empty() {
                return Some(toast("There is nothing to restore."));
            }
            start_op(ui, Op::Restore { items })
        }
        "empty" => start_op(ui, Op::EmptyBin),
        "run" => {
            let Some(a) = ui.files.asking.take() else { return Some(toast("That question is no longer open.")) };
            run_plan(ui, a.plan, a.answers)
        }
        "answer" => return Some(answer(ui, arg)),
        "undo" => {
            let p = places(ui);
            let now = when(ui);
            let Some(u) = ui.files.history.take_newest() else { return Some(toast("There is nothing to undo.")) };
            let result = fx::undo(&d, &p, &u, now);
            forget_places(ui);
            match result {
                Ok(done) => {
                    if let Some(made) = done.made.first() {
                        tab(ui).focus = Some(made.clone());
                    }
                    obj! {"refresh": b(true), "toast": s(note_of(&done))}
                }
                Err(why) => obj! {"refresh": b(true), "toast": s(why)},
            }
        }
        "job-stop" => {
            ui.files.demo = false;
            if let Some(j) = &ui.files.job {
                j.stop.store(true, Ordering::SeqCst);
                toast("Stopping. What was half copied is removed.")
            } else {
                refresh()
            }
        }
        "copy-path" => {
            let p = places(ui);
            let (windows, real) = p.copy_as_path(Path::new(arg));
            copy_answer(if value == "windows" { &windows } else { &real })
        }
        "pin" | "unpin" | "pin-move" => {
            let mut p = places(ui);
            let folder = Path::new(arg);
            let changed = match &action[6..] {
                "pin" => p.quick.pin(folder),
                "unpin" => p.quick.unpin(folder),
                _ => match p.quick.pins.iter().position(|x| x == folder) {
                    Some(i) if value == "up" && i > 0 => p.quick.move_pin(i, i - 1),
                    Some(i) if value == "down" => p.quick.move_pin(i, i + 1),
                    _ => false,
                },
            };
            if !changed {
                return Some(toast(if action.ends_with("-pin") { "That folder is pinned already." } else { "Nothing changed." }));
            }
            if let Err(e) = p.quick.save(&ui.folder()) {
                return Some(toast(&format!("The pins couldn't be saved ({}).", fx::guard::io_words(&e))));
            }
            forget_places(ui);
            let name = fx::places::name_of(folder);
            obj! {"refresh": b(true), "toast": s(match &action[6..] { "pin" => format!("{name} pinned to Quick access."), "unpin" => format!("{name} unpinned."), _ => format!("{name} moved.") })}
        }
        "props" => return Some(properties(ui, Path::new(arg))),
        "peek" => return Some(peek(ui, Path::new(arg))),
        "open-with" => {
            if !ui.window {
                return Some(toast("Open with needs Desktop Mode: there the system shows its list of apps."));
            }
            match d.disk(Path::new(arg)) {
                Some(at) => obj! {"openWith": s(at.to_string_lossy()), "toast": s("Pick an app in the window that opens.")},
                None => toast("Turbine Hub can't reach this file."),
            }
        }
        "tab" => {
            tab(ui);
            let count = ui.files.tabs.len();
            let (what, which) = arg.split_once(':').unwrap_or((arg, ""));
            let which = which.parse::<usize>().unwrap_or(ui.files.at).min(count - 1);
            match what {
                "new" => {
                    if count >= TABS_MAX {
                        return Some(toast("That is as many tabs as fit. Close one first."));
                    }
                    let p = places(ui);
                    let at = start(&p, view(ui));
                    ui.files.tabs.push(Tab::at(at));
                    ui.files.at = count;
                }
                "close" => {
                    if count == 1 {
                        return Some(toast("The last tab stays open."));
                    }
                    let mut gone = ui.files.tabs.remove(which);
                    gone.leave();
                    if ui.files.at > which || ui.files.at >= ui.files.tabs.len() {
                        ui.files.at = ui.files.at.saturating_sub(1);
                    }
                }
                "go" => ui.files.at = which,
                "next" => ui.files.at = (ui.files.at + 1) % count,
                "prev" => ui.files.at = (ui.files.at + count - 1) % count,
                _ => {}
            }
            ui.files.seq += 1;
            refresh()
        }
        // Ticking is the window's own (it knows what is ticked); the press comes here only from a script.
        "tick" => obj! {"tick": b(true)},
        // A picture of a copy under way, on a made-up machine.
        "demo" if ui.sample => {
            ui.files.demo = true;
            refresh()
        }
        _ => return None,
    })
}

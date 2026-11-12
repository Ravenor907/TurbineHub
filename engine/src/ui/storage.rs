//! The storage saver's screen: the Storage section of My machine. Every drive with a meter of its room and what
//! could be freed on it, then the leftovers exactly as the engine sorts them (`crate::storage`): Safe to clear
//! (ticked), Check first (not ticked, each with its reason and what happens to it), and nothing of what is in
//! use. Start opens the engine's plan for the ticked set as the one dialog. The biggest games by their real size
//! are a short list that opens the game.
//!
//! A look can take seconds, so the page never takes one itself: it asks for one (`wanted`), the app runs it on a
//! worker thread and hands the report back (`storage_found`), and the page says "Looking" meanwhile. The last
//! report is kept with its time and shown until the player asks to look again or a cleaning ends. The checkup's
//! line about a nearly full drive comes from that report, or, before any look, from how full the drives are.
//! On the made-up machine the look is taken at once: its few files are counted in no time.
use super::plan::{Plan, dialog, refresh, toast};
use super::{Ui, b, n, obj, row, s, section, with};
use crate::files::fs::Fs;
use crate::files::{Disk, Space, When, size_words};
use crate::json::Json;
use crate::shortcuts::Steam;
use crate::storage::{self, Group, Here, Host, How, Job, Limits, Report, Size};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How many games the short list shows.
const BIGGEST: usize = 5;

/// What the screen keeps between presses.
#[derive(Default)]
pub struct Look {
    /// The last look, and when it was taken on this machine's clock.
    report: Option<Report>,
    looked: u64,
    /// What the host said at that look (which games ran, what Flatpak calls unused).
    host: Host,
    /// The ticked findings, by id.
    ticks: Vec<String>,
    /// A look is asked for and not started yet (the app starts it), or running.
    wanted: bool,
    looking: bool,
    /// Before any look: the checkup's line from how full the drives are.
    quick: Option<(String, String)>,
    /// What each game really takes, for the Games tab: asked for one game at a time. None: couldn't be counted.
    sizes: BTreeMap<String, Option<Size3>>,
    size_wanted: Option<String>,
    /// The made-up machine's drives: (where the drive is, its room). Everything else has `SAMPLE_ROOM`.
    rooms: Vec<(PathBuf, Space)>,
}

/// One game's sizes: its own files, its shader cache, its Proton files.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size3 {
    pub files: Size,
    pub shaders: Size,
    pub proton: Size,
}

impl Size3 {
    fn of(g: &storage::GameSize) -> Size3 {
        Size3 { files: g.files, shaders: g.shaders, proton: g.proton }
    }
    fn total(&self) -> Size {
        self.files.plus(self.shaders).plus(self.proton)
    }
}

/// Room on a made-up drive that has none of its own.
const SAMPLE_ROOM: Space = Space { free: 229_780_000_000, total: 512_000_000_000 };

/// The made-up machine's disk, with a room for each drive.
struct Rooms<'a> {
    disk: &'a Disk,
    rooms: &'a [(PathBuf, Space)],
}

impl Fs for Rooms<'_> {
    fn disk(&self, real: &Path) -> Option<PathBuf> {
        self.disk.disk(real)
    }
    fn real(&self, disk: &Path) -> Option<PathBuf> {
        self.disk.real(disk)
    }
    fn space(&self, real: &Path) -> Option<Space> {
        Some(self.rooms.iter().filter(|(mount, _)| real.starts_with(mount)).max_by_key(|(mount, _)| mount.components().count()).map(|(_, room)| *room).unwrap_or(SAMPLE_ROOM))
    }
}

/// What the host would say on the made-up machine: Steam as the run was set up, no game running, nothing from
/// Flatpak.
fn sample_host(ui: &Ui) -> Host {
    let steam = steam_now(ui);
    Host { steam, running: (steam != Steam::Unknown).then(Vec::new), ..Host::default() }
}

/// Whether Steam runs, as the app last heard from the host.
fn steam_now(ui: &Ui) -> Steam {
    if ui.steam_unsure {
        Steam::Unknown
    } else if ui.steam_running {
        Steam::Running
    } else {
        Steam::Closed
    }
}

/// The made-up machine, read for the storage saver.
fn sample_here(ui: &Ui) -> Here {
    Here::read(&ui.ctx.root, &ui.ctx.home, &ui.folder(), &ui.data)
}

/// The moment the made-up machine's leftovers are dated from, so its pictures and tests don't change by the day.
fn sample_when() -> When {
    When::utc(crate::fake::STORAGE_NOW)
}

/// A look taken right now on the made-up machine.
fn sample_look(ui: &mut Ui) {
    let here = sample_here(ui);
    let host = sample_host(ui);
    let rooms = std::mem::take(&mut ui.storage.rooms);
    let fs = Rooms { disk: &here.disk, rooms: &rooms };
    let site = storage::Site { fs: &fs, places: &here.places, folder: &here.folder, data: &here.data, when: sample_when() };
    let report = storage::scan(&site, &host, &Limits::default());
    ui.storage.rooms = rooms;
    ui.storage_found(report, host);
}

/// The disk the records of the last cleaning are checked against.
pub(crate) fn disk(ui: &Ui) -> Disk {
    if ui.sample { Disk::under(&ui.ctx.root) } else { Disk::here() }
}

impl Ui {
    /// Should a look start now? True once for each look that was asked for; the app starts it on a worker thread
    /// and the page says "Looking" until `storage_found`.
    pub fn storage_take_wanted(&mut self) -> bool {
        if self.sample || !self.storage.wanted || self.storage.looking {
            return false;
        }
        self.storage.wanted = false;
        self.storage.looking = true;
        true
    }

    /// A look ended: its report is the one shown from now on. Ticks the player set on things that are still there
    /// stay as they were; everything new starts as the engine sorts it (only the safe group ticked).
    pub fn storage_found(&mut self, report: Report, host: Host) {
        // A cleaning ended while this look ran: what it found is from before, so it is dropped and the look that
        // was asked for since takes its place.
        if self.storage.wanted && self.storage.looking && self.storage.report.is_none() {
            self.storage.looking = false;
            return;
        }
        let before: Vec<String> = self.storage.report.as_ref().map(|r| r.findings.iter().map(|f| f.id.clone()).collect()).unwrap_or_default();
        let was = std::mem::take(&mut self.storage.ticks);
        self.storage.ticks = report.findings.iter().filter(|f| f.group != Group::Never).filter(|f| if before.contains(&f.id) { was.contains(&f.id) } else { f.ticked && f.group == Group::Safe }).map(|f| f.id.clone()).collect();
        for g in &report.games {
            self.storage.sizes.insert(g.id.clone(), Some(Size3::of(g)));
        }
        self.storage.looked = crate::now();
        self.storage.report = Some(report);
        self.storage.host = host;
        self.storage.looking = false;
    }

    /// The last look as it is kept, for the tests.
    #[cfg(test)]
    pub(crate) fn storage_last(&self) -> Option<(Report, Host)> {
        self.storage.report.clone().map(|r| (r, self.storage.host.clone()))
    }

    /// How full the drives are, looked at when the app starts (nothing is counted for it). A nearly full drive
    /// is worth a look of its own, so the checkup can say how much can be freed.
    pub fn storage_quick(&mut self, line: Option<(String, String)>) {
        if line.is_some() && self.storage.report.is_none() && !self.storage.looking {
            self.storage.wanted = true;
        }
        self.storage.quick = line;
    }

    /// The checkup's line about a nearly full drive: from the last look when there is one (never from a new
    /// one), else from how full the drives are.
    pub(crate) fn storage_checkup(&self) -> Option<(String, String)> {
        match &self.storage.report {
            Some(r) => r.checkup(),
            None => self.storage.quick.clone(),
        }
    }

    /// The game whose size the Games tab is waiting for, once: the app counts it on a worker thread and hands
    /// it back with `storage_game_size`.
    pub fn storage_take_size_wanted(&mut self) -> Option<String> {
        if self.sample { None } else { self.storage.size_wanted.take() }
    }

    /// One game's sizes arrived (None: it couldn't be counted, and isn't asked for again in this run).
    pub fn storage_game_size(&mut self, id: &str, size: Option<storage::GameSize>) {
        self.storage.sizes.insert(id.to_string(), size.as_ref().map(Size3::of));
    }

    /// What a game really takes, when it is known. When it isn't, it is asked for (never counted here: the page
    /// doesn't wait for it) and shows once it arrived. The made-up machine counts its few files at once.
    pub(crate) fn game_size(&mut self, game: &crate::games::Game) -> Option<Size3> {
        let id = game.id();
        if let Some(known) = self.storage.sizes.get(&id) {
            return *known;
        }
        if self.sample {
            let here = sample_here(self);
            let size = Size3::of(&storage::game_size(&here.disk, &here.places, game, &Limits::default()));
            self.storage.sizes.insert(id, Some(size));
            return Some(size);
        }
        self.storage.size_wanted = Some(id);
        None
    }

    /// The storage saver's step of a confirmed dialog on the made-up machine: it runs for real on the made-up
    /// folders, and the made-up drives get the room it freed.
    pub(crate) fn storage_sample_step(&mut self, job: &Job) -> Result<(), String> {
        let here = sample_here(self);
        let steam = steam_now(self);
        let mut ask = |line: &[String]| {
            if *line == *Steam::question() {
                match steam {
                    Steam::Closed => Some((1, "0\n".to_string())),
                    Steam::Running => Some((0, "1\n".to_string())),
                    Steam::Unknown => None,
                }
            } else {
                // No game runs on the made-up machine (pgrep ends with 1 when it finds none).
                Some((1, String::new()))
            }
        };
        // What is erased gives its room back at once; what goes to the Recycle Bin doesn't.
        let mut freed: Vec<(PathBuf, u64)> = Vec::new();
        if let (Job::Clean(items), Some(report)) = (job, &self.storage.report) {
            for f in report.findings.iter().filter(|f| f.how != How::Bin && items.iter().any(|i| i.id == f.id)) {
                if let Some(d) = report.drives.get(f.drive) {
                    freed.push((d.mount.clone(), f.size.bytes));
                }
            }
        }
        storage::run(job, &here.site(sample_when()), &|_| {}, &mut ask)?;
        for (mount, bytes) in freed {
            match self.storage.rooms.iter_mut().find(|(m, _)| *m == mount) {
                Some((_, room)) => room.free = (room.free + bytes).min(room.total),
                None => self.storage.rooms.push((mount, Space { free: (SAMPLE_ROOM.free + bytes).min(SAMPLE_ROOM.total), total: SAMPLE_ROOM.total })),
            }
        }
        Ok(())
    }

    /// A cleaning or its Undo ended: what the last look found is no longer what is there, so a new one is taken.
    pub(crate) fn storage_changed(&mut self) {
        self.storage.report = None;
        self.storage.sizes.clear();
        if self.sample {
            sample_look(self);
        } else {
            self.storage.wanted = true;
        }
    }

    /// A sample run with the storage saver in one of its states: "full" (leftovers of removed games on both
    /// drives and an SD card that is nearly full), "cleaned" (the same after a cleaning, so there is something to
    /// undo), "looking" (the look is still running) and "empty" (nothing left to clear).
    pub fn sample_storage(&mut self, state: &str) {
        match state {
            "looking" => {
                self.storage.report = None;
                self.storage.looking = true;
            }
            "empty" => {
                // Whatever the made-up machine has to offer is taken away, until a look finds nothing.
                for _ in 0..3 {
                    sample_look(self);
                    let here = sample_here(self);
                    let offered: Vec<PathBuf> = self.storage.report.iter().flat_map(|r| r.findings.iter()).filter(|f| f.group != Group::Never).flat_map(|f| f.paths.clone()).collect();
                    for p in offered.iter().flat_map(|p| here.disk.disk(p).into_iter().chain([p.clone()])).filter(|p| p.starts_with(&self.ctx.root) || p.starts_with(&self.data)) {
                        let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                    }
                }
                sample_look(self);
            }
            "full" | "cleaned" => {
                let (root, home) = (self.ctx.root.clone(), self.ctx.home.clone());
                crate::fake::storage(&root, &home);
                let steam = home.join(".local/share/Steam");
                let card = root.join(crate::fake::FILES_DRIVE.trim_start_matches('/'));
                // The launch tool's choices stay as the made-up machine had them, with the one game on a Proton
                // of its own added.
                super::sample::steam_config(&steam, &[("1091500", "GE-Proton10-12")]);
                let _ = std::fs::create_dir_all(card.join("steamapps/downloading/632470")).and_then(|_| std::fs::write(card.join("steamapps/downloading/632470/depot_632471.pkg"), "pkg"));
                // Turbine Hub remembers the games that are gone from when they were played.
                for (id, name) in [("1145360", "Hades II"), ("632470", "Disco Elysium")] {
                    crate::history::add_session(&self.data, &crate::history::Session { game: format!("steam:{id}"), name: name.into(), start: crate::fake::STORAGE_NOW - 40 * 86_400, end: crate::fake::STORAGE_NOW - 40 * 86_400 + 3600, asleep: 0, before: None, after: None, on_battery: false });
                }
                // Sizes a real machine would have (the files are only said to be this big).
                let gb = |v: f64| (v * (1u64 << 30) as f64) as u64;
                let sized: [(&Path, &str, u64); 12] = [
                    (&steam, "steamapps/shadercache/1145360/fozpipelinesv6/steam_pipeline_cache.foz", gb(3.4)),
                    (&steam, "steamapps/shadercache/570/fozmediav1/video.foz", gb(1.1)),
                    (&steam, "steamapps/downloading/1145360/Content/Game.pkg", gb(6.8)),
                    (&card, "steamapps/shadercache/632470/fozpipelinesv6/steam_pipeline_cache.foz", gb(4.6)),
                    (&card, "steamapps/downloading/632470/depot_632471.pkg", gb(7.4)),
                    (&steam, "steamapps/compatdata/1145360/pfx/drive_c/users/steamuser/Saved Games/Hades II/Profile1.sav", gb(1.3)),
                    (&steam, "steamapps/compatdata/1091500/pfx.turbinehub-backup-1794000000/drive_c/users/steamuser/Saved Games/CD Projekt Red/Cyberpunk 2077/ManualSave-1/sav.dat", gb(2.1)),
                    (&steam, "steamapps/common/Proton 9.0/files/bin/wine64", gb(1.4)),
                    (&steam, "compatibilitytools.d/GE-Proton10-4/files/bin/wine64", gb(1.2)),
                    (&steam, "steamapps/shadercache/1086940/fozpipelinesv6/steam_pipeline_cache.foz", gb(2.6)),
                    (&home, ".var/app/com.example.OldLauncher/data/games/save.dat", gb(5.2)),
                    (&home, "Downloads/Old game installer.iso", gb(4.7)),
                ];
                for (base, rel, bytes) in sized {
                    if let Ok(f) = std::fs::File::options().write(true).open(base.join(rel)) {
                        let modified = f.metadata().and_then(|m| m.modified()).ok();
                        let _ = f.set_len(bytes);
                        // The file's age is part of what it is offered as.
                        if let Some(at) = modified {
                            let _ = f.set_modified(at);
                        }
                    }
                }
                // The card's Recycle Bin holds something worth a look too.
                for bin in std::fs::read_dir(&card).into_iter().flatten().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(".Trash-")) {
                    if let Ok(f) = std::fs::File::options().write(true).open(bin.path().join("files/old-rom.zip")) {
                        let _ = f.set_len(gb(0.9));
                    }
                }
                self.storage.rooms = vec![(PathBuf::from(crate::fake::FILES_DRIVE), Space { free: 13 << 30, total: 238 << 30 })];
                self.storage.sizes.clear();
                sample_look(self);
                if state == "cleaned" {
                    // The safe group and one thing that goes to the Recycle Bin, so there is something to undo.
                    if let Some(id) = self.storage.report.as_ref().and_then(|r| r.findings.iter().find(|f| f.kind == storage::Kind::Compatdata)).map(|f| f.id.clone()) {
                        self.storage.ticks.push(id);
                    }
                    let plan = self.storage.report.as_ref().and_then(|r| storage::plan(r, &self.storage.ticks, &sample_host(self)).ok());
                    if let Some(p) = plan {
                        let _ = self.play_sample(&p);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The plan's host: Steam as the app heard last, and the running games as the last look heard them (the cleaning
/// asks both again right before anything is touched).
fn host_now(ui: &Ui) -> Host {
    let steam = steam_now(ui);
    let running = match steam {
        Steam::Closed => Some(Vec::new()),
        Steam::Unknown => None,
        Steam::Running if ui.sample => Some(Vec::new()),
        Steam::Running => ui.storage.host.running.clone(),
    };
    Host { steam, running, ..ui.storage.host.clone() }
}

/// Is this page the Storage section itself (and not a look into it from a card or a link)?
pub(crate) fn is_page(name: &str, arg: &str) -> bool {
    (name == "module:storage" && (arg.is_empty() || arg == "storage")) || (name == "machine" && arg == "storage")
}

/// The Storage section is being shown, or one of its buttons pressed: the made-up machine has its look at once;
/// a real one asks for it and shows "Looking" until it arrives.
pub(crate) fn ensure(ui: &mut Ui) {
    if ui.storage.report.is_some() || ui.storage.looking {
        return;
    }
    if ui.sample {
        sample_look(ui);
    } else {
        ui.storage.wanted = true;
    }
}

fn meter(d: &storage::DriveUse, safe: Size, check: Size) -> Json {
    let room = if d.total == 0 { "Its room couldn't be read".to_string() } else { format!("{} free of {}", size_words(d.free), size_words(d.total)) };
    let line = if d.read_only {
        "A Windows drive. It is only read here, never changed.".to_string()
    } else {
        match (safe.bytes > 0 || safe.at_least, check.bytes > 0 || check.at_least) {
            (true, true) => format!("{} can be freed, and up to {} more after a look at what it is.", cap(&safe.words()), check.words()),
            (true, false) => format!("{} can be freed.", cap(&safe.words())),
            (false, true) => format!("Up to {} could be freed, after a look at what it is.", check.words()),
            (false, false) => "Nothing to clear here.".to_string(),
        }
    };
    let mut r = with(with(row("none", &d.mount.to_string_lossy(), "meter", &d.label, &line), "value", s(room)), "icon", s("drive"));
    r = with(r, "part", n(d.percent_full().map(|p| p as f64 / 100.0).unwrap_or(0.0)));
    r = with(r, "partWords", s(d.percent_full().map(|p| format!("{p}% full")).unwrap_or_default()));
    if d.nearly_full() && !d.read_only {
        r = with(with(r, "badge", s("Nearly full")), "tone", s("warn"));
    }
    r
}

fn cap(words: &str) -> String {
    let mut c = words.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// One leftover as a tick row: its name, its size, why it is there and what happens to it.
fn finding_row(ui: &Ui, f: &storage::Finding, ticked: bool) -> Json {
    let mut line = f.why.clone();
    // Inside a Steam library and not a removed game's leftover: it waits for Steam to be closed.
    if f.steam && !f.orphan() && ui.steam_running {
        line.push_str(" Needs Steam closed.");
    }
    if ui.veteran() && let Some(p) = f.paths.first() {
        line.push_str(&format!(" {}{}", p.display(), if f.paths.len() > 1 { format!(" and {} more", f.paths.len() - 1) } else { String::new() }));
    }
    let mut r = with(with(row("storage-tick", &f.id, "check", &f.title, &line), "on", b(ticked)), "value", s(f.size.words()));
    if f.group == Group::Check {
        // What may hold saves says what happens to it, before anything is ticked.
        let (words, tone) = match f.how {
            How::Bin => ("Goes to the Recycle Bin", "muted"),
            How::Erase | How::EmptyBin => ("Erased for good", "warn"),
            How::Flatpak => ("Flatpak removes it", "muted"),
        };
        r = with(with(r, "badge", s(words)), "tone", s(tone));
    }
    r
}

/// The Storage section of My machine: (its line, its rows, the main button, its own actions).
pub(crate) fn module(ui: &mut Ui) -> (String, Vec<Json>, Json, Vec<Json>) {
    let Some(report) = ui.storage.report.clone() else {
        let rows = vec![with(with(with(row("none", "storage-looking", "info", "Looking", "Counting what is on each drive and what of it can go. On a full card this can take a few seconds. Nothing is changed."), "icon", s("search")), "badge", s("Working")), "tone", s("muted"))];
        return ("What is using the room on each drive, and what of it can go.".into(), vec![section("", rows)], Json::Null, Vec::new());
    };
    let ticks = ui.storage.ticks.clone();
    let mut sections = Vec::new();
    let drives: Vec<Json> = report.drives.iter().enumerate().map(|(i, d)| {
        let (safe, check) = report.can_free(i);
        meter(d, safe, check)
    }).collect();
    sections.push(section("Drives", drives));
    // Right under the drives, so it is in view when a cleaning has just ended.
    if let Some(undo) = storage::undo_plan(&disk(ui), &ui.folder()) {
        let c = undo.items.len();
        let line = format!("Puts back {} to the Recycle Bin, to where it was. What was erased for good can't come back.", if c == 1 { "the 1 thing it moved".to_string() } else { format!("the {c} things it moved") });
        sections.push(section("Last cleaning", vec![with(with(row("storage-undo", "", "button", "Undo the last cleaning", &line), "button", s("Undo")), "icon", s("restore"))]));
    }
    let mut any = false;
    for group in [Group::Safe, Group::Check] {
        let rows: Vec<Json> = report.group(group).map(|f| finding_row(ui, f, ticks.contains(&f.id))).collect();
        if rows.is_empty() {
            continue;
        }
        any = true;
        let title = format!("{} \u{b7} {}", group.title(), report.total(group).words());
        let mut sec = section(&title, rows);
        sec = with(sec, "note", s(group.line()));
        sections.push(sec);
    }
    if !any {
        sections.push(with(section("Nothing to clear", vec![]), "empty", s("No leftovers were found. What takes the room is in use: the games below, and your own files.")));
    }
    let games: Vec<Json> = report.games.iter().filter(|g| g.total().bytes > 0).take(BIGGEST).map(|g| {
        let mut parts = vec![format!("Game {}", g.files.words())];
        if g.shaders.bytes > 0 {
            parts.push(format!("shader cache {}", g.shaders.words()));
        }
        if g.proton.bytes > 0 {
            parts.push(format!("Windows files {}", g.proton.words()));
        }
        let line = format!("{} \u{b7} on {}", parts.join(", "), report.drives.get(g.drive).map(|d| d.label.as_str()).unwrap_or("this machine"));
        with(with(with(row("open", &format!("games|{}", g.id), "button", &g.name, &line), "value", s(g.total().words())), "button", s("")), "icon", s("games"))
    }).collect();
    if !games.is_empty() {
        sections.push(section("Biggest games", games));
    }
    if !report.problems.is_empty() {
        sections.push(section("Couldn't be looked at", report.problems.iter().enumerate().map(|(i, p)| row("none", &format!("storage-problem-{i}"), "info", "Left alone", p)).collect()));
    }
    let chosen = report.chosen(&ticks).count();
    let primary = if chosen > 0 { obj! {"label": s(report.free_words(&ticks)), "act": s("storage-free"), "arg": s("")} } else { Json::Null };
    let looked = match super::ago(crate::now(), ui.storage.looked).as_str() {
        "now" => "Looked just now".to_string(),
        t => format!("Looked {t} ago"),
    };
    let looked = if ui.storage.looking { "Looking again, this list is from the last look".to_string() } else { looked };
    let line = format!("{looked}. {}", if chosen == 0 { "Tick what should go.".to_string() } else if chosen == 1 { "1 thing is ticked.".to_string() } else { format!("{chosen} things are ticked.") });
    (line, sections, primary, vec![super::controls::page_button("Look again", "storage-look", "")])
}

/// What the Overview's card says: the room that can be freed, once a look was taken.
pub(crate) fn card_value(ui: &Ui) -> Option<String> {
    if ui.storage.looking {
        return Some("Looking".into());
    }
    let r = ui.storage.report.as_ref()?;
    let safe = r.total(Group::Safe);
    Some(if safe.bytes > 0 {
        format!("{} can be freed", cap(&safe.words()))
    } else {
        match r.drives.first().filter(|d| d.total > 0) {
            Some(d) => format!("{} free", size_words(d.free)),
            None => "Nothing to clear".into(),
        }
    })
}

/// A game's real size for its page: "Takes 42 GB" (Veteran: with what it is made of). None while it isn't known.
pub(crate) fn game_words(ui: &mut Ui, game: &crate::games::Game) -> Option<String> {
    let size = ui.game_size(game)?;
    let total = size.total();
    if total.bytes == 0 && !total.at_least {
        return None;
    }
    let mut words = format!("Takes {}", total.words());
    if ui.veteran() && (size.shaders.bytes > 0 || size.proton.bytes > 0) {
        let mut parts = vec![format!("game {}", size.files.words())];
        if size.shaders.bytes > 0 {
            parts.push(format!("shader cache {}", size.shaders.words()));
        }
        if size.proton.bytes > 0 {
            parts.push(format!("Windows files {}", size.proton.words()));
        }
        words.push_str(&format!(" ({})", parts.join(", ")));
    }
    Some(words)
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        "storage-tick" => {
            ensure(ui);
            let offered = ui.storage.report.as_ref().is_some_and(|r| r.findings.iter().any(|f| f.id == arg && f.group != Group::Never));
            if !offered {
                return Some(refresh());
            }
            match ui.storage.ticks.iter().position(|t| t == arg) {
                Some(i) => {
                    ui.storage.ticks.remove(i);
                }
                None => ui.storage.ticks.push(arg.to_string()),
            }
            Some(refresh())
        }
        "storage-look" => {
            if ui.storage.looking {
                return Some(toast("Still looking. It shows here when it is done."));
            }
            if ui.sample {
                sample_look(ui);
            } else {
                ui.storage.wanted = true;
            }
            Some(refresh())
        }
        "storage-free" => {
            ensure(ui);
            let Some(report) = ui.storage.report.clone() else { return Some(toast("Still looking. Try again when the list is here.")) };
            let mut p: Plan = match storage::plan(&report, &ui.storage.ticks, &host_now(ui)) {
                Ok(p) => p,
                Err(why) => return Some(toast(&why)),
            };
            p.what = "Storage cleaning".into();
            // Never beside a copy from the Windows drive or a Proton being fetched: both write where it clears.
            if let Some(why) = ui.work_refusal(&p) {
                return Some(toast(&why));
            }
            Some(dialog(ui, p))
        }
        "storage-undo" => Some(match storage::undo_plan(&disk(ui), &ui.folder()) {
            Some(mut p) => {
                p.what = "Storage cleaning".into();
                dialog(ui, p)
            }
            None => toast("There is nothing left in the Recycle Bin to put back."),
        }),
        _ => None,
    }
}

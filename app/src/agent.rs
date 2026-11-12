//! `turbinehub --agent`: the background helper's side of the app. It only looks (which game runs, the battery, the
//! screens), runs the commands the engine hands it and brings the answers back. Every decision is in the engine's
//! `agent`, `lights`, `history` and `snapshots`.
//!
//! Inside the Flatpak the sandbox shows neither the host's processes nor a writable /sys, so the list of running
//! games comes from the host (`pgrep`) and the lights are written through one long-lived `tee` per file on the host.
//!
//!   turbinehub --agent                          run until stopped (the systemd user unit does this)
//!   turbinehub --agent --once                   one look at everything, printing what it would run instead of
//!                                               running it
//!   turbinehub --agent --once --machine=handheld|desktop|tv
//!                                               the same on a made-up machine in the middle of a game (the build's
//!                                               checks run this)
//!   turbinehub --agent --once --root=<dir> --home=<dir> --config=<dir> --data=<dir>
//!                                               the same on a machine made up by hand

use crate::backend::{cache_dir, data_dir, option, real_ctx};
use crate::run;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use turbinehub_engine::agent::{self, Agent, Do, Found, Paths};
use turbinehub_engine::fake::Preset;
use turbinehub_engine::hardware::{self, Machine};
use turbinehub_engine::lights::{self, ChipLook, Led, Scene, Shown};
use turbinehub_engine::lights_hid::{Out as HidOut, Session};
use turbinehub_engine::settings::Settings;
use turbinehub_engine::snapshots::Snapshot;
use turbinehub_engine::ui::lighting::{self, HostLights, Lighting, Tried};
use turbinehub_engine::{helper, now, sensors};

/// A wall clock this far ahead of the steady one since the last pass means the machine slept in between.
const SLEEP_GAP_SECS: u64 = 30;

/// How this run reads and acts.
struct Mode {
    paths: Paths,
    config_home: PathBuf,
    cache: PathBuf,
    /// Print what would be run instead of running it.
    dry: bool,
    /// A made-up machine under `paths.root`: its files are read and written directly.
    fake: bool,
    /// The folder of a machine made up for this run, removed at the end.
    made_up: Option<PathBuf>,
}

impl Mode {
    fn from_args() -> Mode {
        if let Some(kind) = option("machine") {
            let preset = match kind.as_str() {
                "desktop" => Preset::Desktop,
                "tv" => Preset::TvPc,
                _ => Preset::Handheld,
            };
            let dir = std::env::temp_dir().join(format!("turbinehub-agent-{}", std::process::id()));
            let (paths, config_home) = agent::sample(&dir, preset);
            return Mode { paths, config_home, cache: dir.join("cache"), dry: true, fake: true, made_up: Some(dir) };
        }
        let ctx = real_ctx();
        let root = option("root").map(PathBuf::from);
        let dir = |name: &str, real: PathBuf| option(name).map(PathBuf::from).unwrap_or(real);
        let config_home = dir("config", ctx.config_home.clone());
        Mode {
            paths: Paths { root: root.clone().unwrap_or(ctx.root), home: dir("home", ctx.home), folder: config_home.join("turbinehub"), data: dir("data", data_dir()) },
            config_home,
            cache: cache_dir(),
            dry: option("once").is_some(),
            fake: root.is_some(),
            made_up: None,
        }
    }

    /// Reads the host's files directly: a made-up machine, or a run outside the Flatpak.
    fn direct(&self) -> bool {
        self.fake || !run::in_flatpak()
    }

    fn host_file(&self, path: &str) -> Option<String> {
        if self.direct() {
            return std::fs::read_to_string(self.paths.root.join(path.trim_start_matches('/'))).ok();
        }
        run::host_text(&["cat", path])
    }

    /// The Steam app ids of the running games. None when the host couldn't be asked: that is never "no game".
    fn games(&self) -> Option<Vec<String>> {
        if self.direct() {
            return Some(agent::running_games(&agent::processes(&self.paths.root)));
        }
        // pgrep ends with 1 when nothing matches; anything else means it didn't run.
        let o = run::run(&agent::games_question(), None).ok().filter(|o| o.code == 0 || o.code == 1)?;
        Some(agent::running_games(&o.stdout))
    }

    fn machine(&self) -> Machine {
        Machine::read(&self.paths.root, "", &if self.fake { String::new() } else { hardware::device_of(&self.paths.home) })
    }
}

/// Work that takes a while, done off the main loop so the lights keep moving.
enum Job {
    Host(Vec<String>),
    Sinks,
    Verify,
    Snapshot { game: String, steam_id: String, name: String },
    Updates { test_builds: bool },
}

enum Answer {
    /// A command through the privileged helper that didn't work, and why.
    HostFailed(Vec<String>, String),
    Sinks(String, String),
    Verified(Option<Vec<String>>),
    Snapshot(String, Result<Option<Snapshot>, String>),
    Updates(Found),
}

struct Worker {
    data: PathBuf,
    home: PathBuf,
    cache: PathBuf,
}

impl Worker {
    fn work(&self, job: Job) -> Option<Answer> {
        let host = |line: &[String]| run::run(line, None).ok().map(|o| (o.code, o.stdout));
        match job {
            Job::Host(line) => {
                let o = run::run(&line, None).ok()?;
                let through_helper = line.iter().any(|a| a == helper::BIN);
                through_helper.then(|| run::helper_result(&o).err()).flatten().map(|why| Answer::HostFailed(line, why))
            }
            Job::Sinks => Some(Answer::Sinks(run::host_text(&["pactl", "list", "sinks"])?, run::host_text(&["pactl", "get-default-sink"])?)),
            Job::Verify => Some(Answer::Verified(run::verify())),
            Job::Snapshot { game, steam_id, name } => {
                let result = agent::take_snapshot(&self.data, &game, &steam_id, &name, now(), &mut |line: &[String]| host(line));
                Some(Answer::Snapshot(name, result))
            }
            Job::Updates { test_builds } => Some(Answer::Updates(run::look_for_updates(&self.home, &self.cache, crate::VERSION, test_builds).0)),
        }
    }
}

/// Starts a thread that does the jobs sent to it, one after the other.
fn worker(w: Worker, back: Sender<Answer>) -> Sender<Job> {
    let (tx, rx): (Sender<Job>, Receiver<Job>) = mpsc::channel();
    std::thread::spawn(move || {
        for job in rx {
            if let Some(a) = w.work(job) {
                let _ = back.send(a);
            }
        }
    });
    tx
}

/// Writes the lights' files. On a made-up machine and outside the Flatpak the files are written as they are; inside
/// it each file gets one `tee` on the host that stays open, and every frame is a line typed into it (the kernel
/// takes each write to these files as a new value).
struct LightWriter {
    direct: Option<PathBuf>,
    pipes: BTreeMap<String, ChildStdin>,
}

impl LightWriter {
    fn write(&mut self, path: &str, value: &str) {
        if let Some(root) = &self.direct {
            let _ = std::fs::write(root.join(path.trim_start_matches('/')), format!("{value}\n"));
            return;
        }
        if !self.pipes.contains_key(path) {
            let child = Command::new("flatpak-spawn").args(["--host", "tee", path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
            let Some(pipe) = child.ok().and_then(|mut c| c.stdin.take()) else { return };
            self.pipes.insert(path.to_string(), pipe);
        }
        let sent = self.pipes.get_mut(path).is_some_and(|p| p.write_all(format!("{value}\n").as_bytes()).and_then(|_| p.flush()).is_ok());
        if !sent {
            // The tee is gone (the light went away, or the session's helper restarted): a new one next time.
            self.pipes.remove(path);
        }
    }

    /// What a light's file holds now. /sys is readable inside the sandbox, so it is read directly.
    fn read(&self, root: &Path, path: &str) -> String {
        std::fs::read_to_string(root.join(path.trim_start_matches('/'))).unwrap_or_default().trim().to_string()
    }
}

unsafe extern "C" {
    fn ioctl(fd: std::ffi::c_int, request: std::ffi::c_ulong, ...) -> std::ffi::c_int;
}

/// The kernel's number for "send this feature report" on a hidraw node (HIDIOCSFEATURE of this many bytes).
fn feature_request(len: usize) -> std::ffi::c_ulong {
    ((3 << 30) | ((len as u64) << 16) | ((b'H' as u64) << 8) | 0x06) as std::ffi::c_ulong
}

/// Sends reports to the lighting chip's hidraw interfaces. The app's sandbox sees the host's devices, so each
/// interface is opened directly once the helper's lighting access rule let this user write to it: for writing
/// only, which is all an output report (a write) and a feature report (an ioctl) need. On a made-up machine each
/// report is a line of hex added to the stand-in file.
struct ChipWriter {
    direct: Option<PathBuf>,
    open: BTreeMap<String, File>,
}

impl ChipWriter {
    fn feature(f: &File, data: &[u8]) -> Result<(), String> {
        use std::os::fd::AsRawFd;
        // The report is only read by the kernel, for as many bytes as the request says.
        let sent = unsafe { ioctl(f.as_raw_fd(), feature_request(data.len()), data.as_ptr()) };
        if sent < 0 { Err(std::io::Error::last_os_error().to_string()) } else { Ok(()) }
    }

    /// One report: an output report (a plain write; when the interface refuses it the same bytes go as a feature
    /// report, as some firmware only takes those), or a feature report from the start.
    fn send(&mut self, node: &str, data: &[u8], feature: bool) -> Result<(), String> {
        if let Some(root) = &self.direct {
            let line = data.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            let path = root.join(node.trim_start_matches('/'));
            let mut f = File::options().append(true).create(true).open(&path).map_err(|e| e.to_string())?;
            return writeln!(f, "{line}").map_err(|e| e.to_string());
        }
        if !self.open.contains_key(node) {
            let f = File::options().write(true).open(node).map_err(|e| format!("{node} can't be opened ({e})"))?;
            self.open.insert(node.to_string(), f);
        }
        let f = self.open.get_mut(node).ok_or("gone")?;
        let result = if feature {
            Self::feature(f, data).map_err(|e| format!("{node} didn't take the feature report ({e})"))
        } else {
            match f.write(data) {
                Ok(n) if n == data.len() => Ok(()),
                other => {
                    let why = other.map(|n| format!("{n} bytes")).unwrap_or_else(|e| e.to_string());
                    Self::feature(f, data).map_err(|e| format!("{node} didn't take the report ({why}), nor as a feature report ({e})"))
                }
            }
        };
        if result.is_err() {
            // The controller went away (a sleep) or the access changed: opened again next time.
            self.open.remove(node);
        }
        result
    }
}

/// What happened on the way to the lights, for the lighting check.
#[derive(Default)]
struct Notes {
    sent: u64,
    failed: u64,
    /// Per light: the last report the chip didn't take.
    errors: BTreeMap<String, String>,
    /// Per light: a report that is only tried (Dynamic Lighting off) and wasn't taken.
    soft: BTreeMap<String, String>,
    /// Per light: what was written to its color file and what the file held afterwards.
    readback: BTreeMap<String, String>,
}

/// Plays the look on every light: through the files (only what changed), or through the lighting chip.
struct Player {
    files: LightWriter,
    chip: ChipWriter,
    shown: Shown,
    sessions: BTreeMap<String, Session>,
    /// Packed lights whose first write was read back, to see whether the kernel caps their colors.
    checked: BTreeSet<String>,
    /// Lights whose color file was written a frame ago and is read back at the next one: (light, what was written).
    pending: BTreeMap<String, String>,
    /// Lights that are closed to this user right now: left alone until they open.
    shut: BTreeSet<String>,
    /// Nothing was written since the last start over: the lights' own effect files are looked at first.
    fresh: bool,
    notes: Notes,
    /// Print what would be written instead of writing (a dry run on the real machine).
    print: bool,
    root: PathBuf,
}

impl Player {
    fn new(direct: Option<PathBuf>, print: bool, root: PathBuf) -> Player {
        Player { files: LightWriter { direct: direct.clone(), pipes: BTreeMap::new() }, chip: ChipWriter { direct, open: BTreeMap::new() }, shown: Shown::default(), sessions: BTreeMap::new(), checked: BTreeSet::new(), pending: BTreeMap::new(), shut: BTreeSet::new(), fresh: true, notes: Notes::default(), print, root }
    }

    /// Start over everywhere: every file and the chip's whole sequence again (after a sleep, or a new set of lights).
    fn forget(&mut self) {
        self.shown.forget();
        self.sessions.clear();
        self.pending.clear();
        self.fresh = true;
    }

    fn file(&mut self, path: &str, value: &str) {
        if self.print {
            println!("would write: {path} = {value}");
        } else {
            self.files.write(path, value);
        }
    }

    /// Carries out the chip's steps. An error when a report that has to arrive didn't: the session starts over next
    /// time. A report that is only tried (Dynamic Lighting off) never stops the colors.
    fn chip_steps(&mut self, led: &str, outs: Vec<HidOut>) -> Result<(), String> {
        for o in outs {
            match o {
                HidOut::Send { node, data, feature, .. } if self.print => println!("would send: {node} {}{}", data.iter().take(13).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "), if feature { " (feature report)" } else { "" }),
                HidOut::Send { node, data, feature, must } => match self.chip.send(&node, &data, feature) {
                    Ok(()) => self.notes.sent += 1,
                    Err(e) if must => {
                        self.sessions.remove(led);
                        self.notes.failed += 1;
                        self.notes.errors.insert(led.to_string(), e.clone());
                        return Err(e);
                    }
                    Err(e) => drop(self.notes.soft.insert(led.to_string(), e)),
                },
                HidOut::File { path, value } => self.file(&path, &value),
                HidOut::Pause(ms) if !self.print => std::thread::sleep(Duration::from_millis(ms)),
                HidOut::Pause(_) => {}
            }
        }
        Ok(())
    }

    /// One frame of the look, on the lights that are open.
    fn frame(&mut self, scene: &Scene, leds: &mut [Led], t_ms: u64) {
        // What was written a frame ago has reached the file by now: read it back once. A kernel that cut a packed
        // color down to 255 can only show blue that way, so that light moves to the lighting chip when there is
        // one, unless the player's test (or Ally Hub's choice) picked the way.
        for (name, written) in std::mem::take(&mut self.pending) {
            let path = format!("/sys/class/leds/{name}/multi_intensity");
            let now = self.files.read(&self.root, &path);
            self.notes.readback.insert(name.clone(), format!("wrote [{written}], the file held [{now}] a frame later"));
            // A packed color of 255 or less says nothing about a cap: looked at again with the next colors.
            let big = lights::numbers(&written).is_some_and(|v| v.iter().any(|x| *x > 255));
            if big || leds.iter().any(|l| l.name == name && !l.packed()) {
                self.checked.insert(name.clone());
            }
            if let Some(led) = leds.iter_mut().find(|l| l.name == name)
                && !led.fixed
                && led.packed()
                && lights::capped_by_kernel(&written, &now)
                && lights::to_chip(&self.root, led)
            {
                self.shown.forget();
            }
        }
        let open: Vec<Led> = leds.iter().filter(|l| !self.shut.contains(&l.name)).cloned().collect();
        if self.fresh {
            self.fresh = false;
            for (path, value) in lights::mode_writes(&self.root, &open) {
                self.file(&path, &value);
            }
        }
        for (path, value) in self.shown.changed(lights::frame(scene, &open, t_ms)) {
            self.file(&path, &value);
            if let Some(name) = path.strip_prefix("/sys/class/leds/").and_then(|p| p.strip_suffix("/multi_intensity"))
                && !self.print
                && !self.checked.contains(name)
            {
                self.pending.insert(name.to_string(), value.clone());
            }
        }
        for led in open.iter().filter(|l| l.uses_chip()) {
            let Some(chip) = led.chip.clone() else { continue };
            let session = self.sessions.entry(led.name.clone()).or_default();
            let outs = match lights::chip_frame(scene, led.method, t_ms) {
                ChipLook::Zones(colors) => session.zones(led.method, &chip, &led.name, &colors),
                ChipLook::Own(show) => session.show(led.method, &chip, &led.name, &show),
            };
            if self.chip_steps(&led.name, outs).is_ok() {
                self.notes.errors.remove(&led.name);
            }
        }
    }

    /// One way of the guided test: the main light red, driven the way `route` says. Answers whether it could be
    /// sent at all, and what happened in a few lines.
    fn try_route(&mut self, leds: &[Led], route: &str) -> (bool, Vec<String>) {
        let Some(mut led) = lights::main_light(leds).cloned() else { return (false, vec!["No lights found.".into()]) };
        lights::via(&mut led, route);
        if led.route() != route {
            return (false, vec![format!("This light can't be driven that way (it is on {}).", led.route())]);
        }
        let shut = lights::closed(&self.root, &led);
        if !shut.is_empty() {
            return (false, vec![format!("Closed to Turbine Hub: {}.", shut.join(", "))]);
        }
        // Every way starts from nothing, as after a sleep.
        self.forget();
        if let Some(chip) = led.chip.clone() {
            let outs = Session::default().test(led.method, &chip, &led.name, lights::TEST_COLOR);
            let count = outs.iter().filter(|o| matches!(o, HidOut::Send { must: true, .. })).count();
            self.notes.soft.remove(&led.name);
            return match self.chip_steps(&led.name, outs) {
                Ok(()) => {
                    let soft = self.notes.soft.get(&led.name).map(|e| format!(" Dynamic Lighting off wasn't taken ({e}), which is fine.")).unwrap_or_default();
                    (true, vec![format!("{count} reports sent to {} on route {}.{soft}", chip.lighting, led.method.key)])
                }
                Err(e) => (false, vec![e]),
            };
        }
        let dir = format!("/sys/class/leds/{}", led.name);
        for (path, value) in lights::mode_writes(&self.root, std::slice::from_ref(&led)) {
            self.file(&path, &value);
        }
        let text = lights::color_text(&led, &vec![lights::TEST_COLOR; led.zones]);
        self.file(&format!("{dir}/multi_intensity"), &text);
        self.file(&format!("{dir}/brightness"), &led.max.to_string());
        if self.print {
            return (true, vec![format!("Would write [{text}].")]);
        }
        std::thread::sleep(Duration::from_millis(300));
        let now = self.files.read(&self.root, &format!("{dir}/multi_intensity"));
        let kept = lights::numbers(&text).is_some() && lights::numbers(&text) == lights::numbers(&now);
        (true, vec![format!("Wrote [{text}], the file holds [{now}]{}.", if kept { "" } else { ": the system changed it" })])
    }

    /// The background helper's notes for the lighting check.
    fn lines(&self, leds: &[Led], route: &str, why_off: &str) -> Vec<String> {
        let mut out = vec![format!("driving the lights: {}", if why_off.is_empty() { "yes".to_string() } else { format!("no ({why_off})") }), format!("way asked for: {}", if route.is_empty() { "none (decided from the light's files)" } else { route })];
        for led in leds {
            let shut = lights::closed(&self.root, led);
            out.push(format!("{}: {}, {}", led.name, led.route(), if shut.is_empty() { "open".to_string() } else { format!("closed: {}", shut.join(", ")) }));
        }
        out.push(format!("chip reports sent: {}, not taken: {}", self.notes.sent, self.notes.failed));
        out.extend(self.notes.errors.iter().map(|(led, e)| format!("{led}: last report not taken: {e}")));
        out.extend(self.notes.soft.iter().map(|(led, e)| format!("{led}: Dynamic Lighting off not taken (the colors go out anyway): {e}")));
        out.extend(self.notes.readback.iter().map(|(led, r)| format!("{led}: {r}")));
        out
    }
}

/// Runs something every so many seconds.
struct Every(u64, u64);

impl Every {
    fn due(&mut self, now: u64) -> bool {
        let due = now < self.1 || now - self.1 >= self.0;
        if due {
            self.1 = now;
        }
        due
    }
}

pub fn main() -> i32 {
    let mode = Mode::from_args();
    // One background helper at a time: a second one would fight the first over the lights and the processor.
    let lock_file = mode.paths.data.join("agent/lock");
    let _ = std::fs::create_dir_all(mode.paths.data.join("agent"));
    let lock = File::options().create(true).truncate(false).write(true).open(&lock_file);
    let Ok(lock) = lock else {
        eprintln!("Couldn't open {}", lock_file.display());
        return 1;
    };
    if lock.try_lock().is_err() {
        eprintln!("The background helper is running already.");
        return if mode.dry { 1 } else { 0 };
    }

    let mut m = mode.machine();
    let mut core = Agent::start(mode.paths.clone(), crate::VERSION, &m);
    let (back, answers) = mpsc::channel();
    let w = || Worker { data: mode.paths.data.clone(), home: mode.paths.home.clone(), cache: mode.cache.clone() };
    // Quick things (the processor setting, the sound output) never wait behind a snapshot or an update check.
    let (quick, slow) = (worker(w(), back.clone()), worker(w(), back));
    let direct = mode.direct().then(|| mode.paths.root.clone());
    let mut player = Player::new(direct, mode.dry && !mode.fake, mode.paths.root.clone());
    let mut leds: Vec<Led> = Vec::new();
    // A light test asked for before this start is old news.
    let mut tested = lighting::test_asked(&mode.paths.folder).map(|(at, _)| at).unwrap_or(0);
    // While the guided test shows a way, the look stays off the lights: until this time at the latest.
    let mut testing_until = 0u64;
    // The way the lights are driven: the test's answer, or Ally Hub's saved choice on the same machine.
    let mut route = String::new();
    let mut noted = String::new();

    let read_count = |mode: &Mode| mode.host_file(agent::SUSPEND_COUNT).as_deref().and_then(agent::suspend_count);
    core.woke(read_count(&mode));

    let started = Instant::now();
    let (mut last_wall, mut last_steady) = (now(), 0u64);
    let (mut every_battery, mut every_games, mut every_screens, mut every_minute, mut every_machine, mut every_second) = (Every(15, 0), Every(4, 0), Every(3, 0), Every(60, 0), Every(600, 0), Every(1, 0));
    let mut lighting = Lighting::load(&mode.paths.folder);
    let mut lights_on = false;
    let mut passes = 0;

    loop {
        let wall = now();
        let steady = started.elapsed().as_secs();
        // The steady clock stands still while the machine sleeps and the wall clock doesn't.
        let away = wall.saturating_sub(last_wall);
        let slept = away >= steady.saturating_sub(last_steady) + SLEEP_GAP_SECS;
        (last_wall, last_steady) = (wall, steady);
        let mut todo: Vec<Do> = Vec::new();

        if every_second.due(wall) || slept {
            let st = Settings::load(&mode.config_home);
            lighting = Lighting::load(&mode.paths.folder);
            let helper_there = helper::installed(&mode.paths.root);
            let way = lighting::route_in_use(&lighting, &HostLights::load(&mode.paths.folder));
            let look_again = every_machine.due(wall) || slept;
            if look_again {
                m = mode.machine();
            }
            if look_again || way != route {
                let fresh = lights::driven(&mode.paths.root, &m.rgb_leds, &way);
                // The same lights stay on the chip once a write showed their files can't take the colors; a way
                // that was picked (or changed) always counts.
                let same = |a: &Led, b: &Led| a.name == b.name && a.zones == b.zones && a.fixed == b.fixed && (!a.fixed || a.route() == b.route());
                if way != route || fresh.len() != leds.len() || !fresh.iter().zip(&leds).all(|(a, b)| same(a, b)) || fresh.iter().zip(&leds).any(|(a, b)| a.chip.is_some() && a.chip != b.chip) {
                    leds = fresh;
                    player.forget();
                }
                route = way;
            }
            if slept && core.woke(read_count(&mode)) {
                let r = sensors::read(&mode.paths.root);
                core.slept(wall, away, &r, agent::wake_counts(&mode.paths.root, &m));
                // Lights often come back from a sleep in their own state: every file is written again, and the
                // lighting chip gets its whole sequence (Dynamic Lighting comes back after a sleep).
                player.forget();
                every_battery.1 = wall;
            } else if every_battery.due(wall) {
                core.battery(wall, &sensors::read(&mode.paths.root), agent::wake_counts(&mode.paths.root, &m));
            }
            // While a game runs the only news is its end, so the host is asked less often.
            every_games.0 = if core.game().is_empty() { 4 } else { 10 };
            if every_games.due(wall)
                && let Some(ids) = mode.games()
            {
                todo.extend(core.games(wall, &ids, &st, helper_there));
            }
            if every_screens.due(wall) {
                todo.extend(core.screens(wall, agent::external_screen(&mode.paths.root), &lighting));
            }
            if every_minute.due(wall) {
                todo.extend(core.steamos(wall, &agent::steamos(&mode.paths.root), helper_there));
                todo.extend(core.daily(wall, &st));
                core.heartbeat(wall);
            }
            // The guided test: the page asks for one way at a time, and for the look to come back at the end.
            if let Some((at, what)) = lighting::test_asked(&mode.paths.folder).filter(|(at, _)| *at > tested) {
                tested = at;
                if what == lighting::TEST_END {
                    testing_until = 0;
                } else {
                    let (sent, lines) = player.try_route(&leds, &what);
                    lighting::save_step(&mode.paths.data, &Tried { at, route: what, sent, lines });
                    testing_until = wall + 120;
                }
                player.forget();
            }
            if testing_until != 0 && wall >= testing_until {
                // Nobody answered: the look comes back.
                testing_until = 0;
                player.forget();
            }
            // The lights are driven once the player has set them up: their own choices saved, and what the way in
            // use needs open to this user (the helper's lighting access rule). Each light by itself: one that stays
            // closed never keeps the others dark. Left alone while another app runs them.
            player.shut = leds.iter().filter(|l| !lights::writable(&mode.paths.root, l)).map(|l| l.name.clone()).collect();
            let why_off = if !Lighting::saved(&mode.paths.folder) {
                "no lighting choices saved yet".to_string()
            } else if let Some(name) = lighting.left_to() {
                format!("left to {name}")
            } else if leds.is_empty() {
                "no lights found".into()
            } else if player.shut.len() == leds.len() {
                "closed to Turbine Hub".into()
            } else if testing_until != 0 {
                "the light test is showing a way".into()
            } else {
                String::new()
            };
            lights_on = why_off.is_empty();
            let lines = player.lines(&leds, &route, &why_off);
            if lines.join("\n") != noted && !mode.dry {
                noted = lines.join("\n");
                lighting::save_state(&mode.paths.data, wall, &lines);
            }
        }

        while let Ok(a) = answers.try_recv() {
            match a {
                Answer::HostFailed(line, why) => core.host_failed(wall, &line, &why),
                Answer::Sinks(list, default) => todo.extend(core.sinks_listed(wall, &list, &default)),
                Answer::Verified(drift) => core.verified(wall, drift),
                Answer::Snapshot(name, result) => core.snapshot_taken(wall, &name, &result),
                Answer::Updates(found) => core.updates_found(wall, found),
            }
        }

        for d in todo {
            if mode.dry || mode.fake {
                match &d {
                    Do::Host(line) => println!("would run: {}", line.join(" ")),
                    other => println!("would do: {other:?}"),
                }
                continue;
            }
            let _ = match d {
                Do::Host(line) => quick.send(Job::Host(line)),
                Do::Sinks => quick.send(Job::Sinks),
                Do::Verify => quick.send(Job::Verify),
                Do::Snapshot { game, steam_id, name } => slow.send(Job::Snapshot { game, steam_id, name }),
                Do::CheckUpdates { test_builds } => slow.send(Job::Updates { test_builds }),
            };
        }

        let scene = core.scene(&lighting);
        if lights_on {
            player.frame(&scene, &mut leds, started.elapsed().as_millis() as u64);
        }

        passes += 1;
        if mode.dry {
            if passes >= 2 {
                println!("status: {}", std::fs::read_to_string(mode.paths.data.join("agent/status.json")).unwrap_or_default().trim());
                for led in &leds {
                    let file = |f: &str| std::fs::read_to_string(mode.paths.root.join("sys/class/leds").join(&led.name).join(f)).unwrap_or_default().trim().to_string();
                    if mode.fake {
                        println!("light: {} shows {} at {}", led.name, file("multi_intensity"), file("brightness"));
                    }
                }
                if let Some(dir) = &mode.made_up {
                    let _ = std::fs::remove_dir_all(dir);
                }
                return 0;
            }
            continue;
        }
        std::thread::sleep(Duration::from_millis(if lights_on { lights::frame_ms(&scene) } else { 1000 }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("turbinehub-agent-app-{tag}-{}", std::process::id()))
    }

    #[test]
    fn lights_are_written_as_they_are_on_a_made_up_machine() {
        let root = test_dir("lights");
        let led = root.join("sys/class/leds/a:rgb:b");
        std::fs::create_dir_all(&led).unwrap();
        let mut w = LightWriter { direct: Some(root.clone()), pipes: BTreeMap::new() };
        w.write("/sys/class/leds/a:rgb:b/brightness", "128");
        assert_eq!(std::fs::read_to_string(led.join("brightness")).unwrap(), "128\n");
        assert!(w.pipes.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_lighting_chip_gets_its_sequence_once_then_only_changes() {
        let root = test_dir("chip");
        std::fs::create_dir_all(root.join("dev")).unwrap();
        std::fs::create_dir_all(root.join("sys/class/leds/ally:rgb:joystick_rings")).unwrap();
        let chip = turbinehub_engine::lights_hid::Chip { lighting: "/dev/hidraw3".into(), dynamic: Some("/dev/hidraw2".into()) };
        let led = Led { name: "ally:rgb:joystick_rings".into(), zones: 4, max: 255, channels: vec!["rgb".into(); 4], channel_max: vec![255; 4], capped: true, chip: Some(chip.clone()), found: Some(chip), method: turbinehub_engine::lights_hid::auto(), encoding: lights::Encoding::Packed, fixed: false, modes: Vec::new() };
        let mut leds = vec![led];
        let mut p = Player::new(Some(root.clone()), false, root.clone());
        let l = Lighting { brightness: 100, color: "#FF0000".into(), ..Lighting::default() };
        let scene = Scene { lighting: &l, game: "", battery: None, on_battery: false, charging: false, docked: false };
        p.frame(&scene, &mut leds, 0);
        let sent = |n: &str| std::fs::read_to_string(root.join("dev").join(n)).unwrap_or_default();
        // Dynamic Lighting off, the kernel's own light off, then the chip's whole first sequence.
        assert_eq!(sent("hidraw2"), "06 01\n");
        assert_eq!(std::fs::read_to_string(root.join("sys/class/leds/ally:rgb:joystick_rings/brightness")).unwrap(), "0\n");
        assert_eq!(sent("hidraw3").lines().count(), 8);
        assert!(sent("hidraw3").lines().nth(2).unwrap().starts_with("5a b3 01 00 ff 00 00"));
        // Still: nothing more. After a sleep: the whole sequence again.
        p.frame(&scene, &mut leds, 1000);
        assert_eq!(sent("hidraw3").lines().count(), 8);
        p.forget();
        p.frame(&scene, &mut leds, 2000);
        assert_eq!((sent("hidraw2").lines().count(), sent("hidraw3").lines().count()), (2, 16));
        // The guided test on Ally Hub's default route: the handshake first, the four halves red, set and apply.
        assert_eq!(p.try_route(&leds, "chip:m3"), (false, vec!["Closed to Turbine Hub: /dev/hidraw3 (mode 644).".to_string()]));
        std::fs::set_permissions(root.join("dev/hidraw3"), <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o602)).unwrap();
        let (ok, lines) = p.try_route(&leds, "chip:m3");
        assert!(ok && lines[0].starts_with("9 reports sent to /dev/hidraw3 on route m3"), "{lines:?}");
        let all = sent("hidraw3");
        let last: Vec<&str> = all.lines().rev().take(9).collect();
        assert!(last[8].starts_with("5a 41 53 55 53 20 54 65 63 68 2e 49 6e 63 2e 00") && last[5].starts_with("5a b3 01 00 ff 00 00") && last[0].starts_with("5a b4"), "{last:?}");
        // A way this light doesn't have, and a closed light, are said plainly and nothing is sent.
        assert_eq!(p.try_route(&leds, "files"), (false, vec!["This light can't be driven that way (it is on chip:m6).".to_string()]));
        p.shut = BTreeSet::from(["ally:rgb:joystick_rings".to_string()]);
        let before = sent("hidraw3").lines().count();
        p.forget();
        p.frame(&scene, &mut leds, 3000);
        assert_eq!(sent("hidraw3").lines().count(), before);
        assert!(p.lines(&leds, "chip:m6", "").iter().any(|l| l.starts_with("chip reports sent: ")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_light_with_a_picked_way_is_written_in_its_encoding_and_read_back() {
        let root = test_dir("hex");
        let dir = root.join("sys/class/leds/ally:rgb:joystick_rings");
        std::fs::create_dir_all(&dir).unwrap();
        for (f, v) in [("multi_index", "rgb rgb rgb rgb\n"), ("multi_intensity", "0 0 0 0\n"), ("brightness", "0\n"), ("max_brightness", "255\n")] {
            std::fs::write(dir.join(f), v).unwrap();
            std::fs::set_permissions(dir.join(f), <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o666)).unwrap();
        }
        let names = vec!["ally:rgb:joystick_rings".to_string()];
        let mut leds = lights::driven(&root, &names, "files:hex");
        let mut p = Player::new(Some(root.clone()), false, root.clone());
        // The test's red, as hex text, kept by the file.
        let (ok, lines) = p.try_route(&leds, "files:hex");
        assert_eq!((ok, lines), (true, vec!["Wrote [0xff0000 0xff0000 0xff0000 0xff0000], the file holds [0xff0000 0xff0000 0xff0000 0xff0000].".to_string()]));
        // The look goes out the same way, and the first write is read back a frame later for the lighting check.
        let l = Lighting { brightness: 100, color: "#00FF00".into(), ..Lighting::default() };
        let scene = Scene { lighting: &l, game: "", battery: None, on_battery: false, charging: false, docked: false };
        p.frame(&scene, &mut leds, 0);
        assert_eq!(std::fs::read_to_string(dir.join("multi_intensity")).unwrap(), "0x00ff00 0x00ff00 0x00ff00 0x00ff00\n");
        p.frame(&scene, &mut leds, 50);
        assert!(p.lines(&leds, "files:hex", "").iter().any(|l| l.contains("wrote [0x00ff00 0x00ff00 0x00ff00 0x00ff00], the file held [0x00ff00")), "{:?}", p.lines(&leds, "files:hex", ""));
        // A picked way is never moved to the chip by itself.
        assert!(!leds[0].uses_chip() && leds[0].fixed);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn timers_fire_at_once_then_at_their_pace_and_survive_a_clock_set_back() {
        let mut e = Every(15, 0);
        assert!(e.due(1000));
        assert!(!e.due(1014));
        assert!(e.due(1015));
        assert!(e.due(500));
        assert!(!e.due(510));
    }

    #[test]
    fn a_made_up_machine_is_never_asked_through_the_host() {
        let dir = test_dir("mode");
        let p = Paths { root: dir.join("root"), home: dir.join("home"), folder: dir.join("config/turbinehub"), data: dir.join("data") };
        let mode = Mode { paths: p.clone(), config_home: dir.join("config"), cache: dir.join("cache"), dry: true, fake: true, made_up: None };
        assert!(mode.direct());
        std::fs::create_dir_all(p.root.join("proc/77")).unwrap();
        std::fs::write(p.root.join("proc/77/cmdline"), "/x/reaper\0SteamLaunch\0AppId=1145360\0--\0game\0").unwrap();
        std::fs::create_dir_all(p.root.join("sys/power/suspend_stats")).unwrap();
        std::fs::write(p.root.join("sys/power/suspend_stats/success"), "4\n").unwrap();
        assert_eq!(mode.games(), Some(vec!["1145360".to_string()]));
        assert_eq!(mode.host_file(agent::SUSPEND_COUNT).as_deref().and_then(agent::suspend_count), Some(4));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

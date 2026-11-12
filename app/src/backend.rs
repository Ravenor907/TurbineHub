//! The bridge between the QML interface and the engine: one object, `Hub`, available to QML as a singleton.
//!
//! QML asks for a page and gets JSON, sends a press and gets what to show next. Anything slow (the steps of a
//! confirmed dialog, questions to the host or the internet) runs on its own thread and comes back through `poll`,
//! which QML calls on a timer together with the controller.
//!
//! A confirmed dialog runs one of two ways, and the engine says which (ui/queue.rs): behind the cover, alone and
//! with nothing else to press (`busy`: anything with the password, the helper, Steam's files, Turbine Hub's own
//! update), or in the line of work, one job at a time while the player keeps browsing (installs, updates,
//! removals, copies). `status` carries the line to the top bar, and `poll` hands each job's ending to the window.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// The mode, look and machine name as JSON (the engine's `Ui::state`).
        #[qproperty(QString, state)]
        #[qproperty(bool, gamemode)]
        /// A confirmed dialog's steps are running.
        #[qproperty(bool, busy)]
        #[qproperty(QString, busy_words, cxx_name = "busyWords")]
        /// What the top bar shows beside the tabs, as JSON (the engine's `Ui::status`): the battery, the clock,
        /// the work in the line, the updates that are ready.
        #[qproperty(QString, status)]
        /// Where the fonts and pictures are.
        #[qproperty(QString, share)]
        /// A controller is there, and its name (the footer's glyphs follow it).
        #[qproperty(bool, pads)]
        #[qproperty(QString, pad_name, cxx_name = "padName")]
        type Hub = super::HubRust;

        /// One page as JSON.
        #[qinvokable]
        fn page(self: Pin<&mut Self>, name: &QString, arg: &QString) -> QString;

        /// One press: answers what to show next as JSON.
        #[qinvokable]
        fn act(self: Pin<&mut Self>, action: &QString, arg: &QString, value: &QString) -> QString;

        /// The player said yes to the dialog. `password` and `again` are what was typed, when it asked.
        #[qinvokable]
        fn confirm(self: Pin<&mut Self>, password: &QString, again: &QString) -> QString;

        /// The player closed the dialog.
        #[qinvokable]
        fn cancel(self: Pin<&mut Self>);

        /// Controller presses and finished work since the last call, as JSON ("" when nothing happened).
        #[qinvokable]
        fn poll(self: Pin<&mut Self>) -> QString;

        /// Turbine Hub is closing: what waits in the line of work is dropped and the job being worked on is
        /// stopped cleanly. Comes back once it has stopped, or after a few seconds at most.
        #[qinvokable]
        #[cxx_name = "stopWork"]
        fn stop_work(self: Pin<&mut Self>);

        /// The value of a `--name=value` argument ("" when not given); "1" for a bare `--name`.
        #[qinvokable]
        fn option(self: &Self, name: &QString) -> QString;

        /// Prints one line on standard output (the screenshot run's check results).
        #[qinvokable]
        fn say(self: &Self, line: &QString);

        /// The window came to the front (true) or went behind another (false): the controller's presses only count
        /// in front, and not for a moment after coming back.
        #[qinvokable]
        fn front(self: Pin<&mut Self>, active: bool);

        /// The controller pointer's click, move or wheel step, sent into the window like a real mouse's (Main.qml
        /// makes them from what `poll` says about the pointer and R3). kind: 0 move, 1 press, 2 release, 3 wheel.
        #[qinvokable]
        #[cxx_name = "pointerEvent"]
        fn pointer_event(self: &Self, kind: i32, x: f64, y: f64, button: i32, buttons: i32, wheel: i32);

        /// The pointer: 0 hides it (a key was used), 1 pauses it and the left stick (a text box has the keyboard,
        /// or work runs), 2 lets them go on, 3 a button is held at it (it stays), 4 the button was let go.
        #[qinvokable]
        fn pointer(self: Pin<&mut Self>, what: i32);

        /// Shows the pointer at a place in the window (R3 held with the pointer hidden: at the highlight).
        #[qinvokable]
        #[cxx_name = "pointerPlace"]
        fn pointer_place(self: Pin<&mut Self>, x: f64, y: f64);

        /// For the build's checks (screenshot runs only): the sticks held at these places (each -1 to 1, y down)
        /// for `ms`, looked at every 16 ms as the real ones are. Answers what `poll` would have: the pointer and
        /// the left stick's directions.
        #[qinvokable]
        #[cxx_name = "testSticks"]
        fn test_sticks(self: Pin<&mut Self>, lx: f64, ly: f64, rx: f64, ry: f64, ms: i32) -> QString;

        /// For the build's checks (screenshot runs only): a keyboard key with Qt's code and modifiers, delivered
        /// to the window like a real one. `repeat`: the key is held down and this is the keyboard repeating it.
        #[qinvokable]
        #[cxx_name = "testKey"]
        fn test_key(self: &Self, key: i32, mods: i32, pressed: bool, repeat: bool);

        /// For the build's checks (screenshot runs only): a real mouse's move (kind 0), press (1), release (2) or
        /// wheel step (3) at a place in the window, with Qt's button and modifiers.
        #[qinvokable]
        #[cxx_name = "testMouse"]
        fn test_mouse(self: &Self, kind: i32, x: f64, y: f64, button: i32, mods: i32, wheel: i32);

        /// For the build's checks (screenshot runs only): a finger put down (kind 0), moved (1) or lifted (2) at a
        /// place in the window.
        #[qinvokable]
        #[cxx_name = "testTouch"]
        fn test_touch(self: &Self, kind: i32, x: f64, y: f64);

        /// For the build's checks (screenshot runs only): what a screen reader is told about the window, counted
        /// from Qt's accessibility tree. `what`: 0 the controls it finds, 1 those of them with no name, 2 those
        /// that say the highlight is on them, 3 the plain texts, 4 as 0 with every control printed. -1 outside a
        /// screenshot run.
        #[qinvokable]
        #[cxx_name = "testSpoken"]
        fn test_spoken(self: &Self, what: i32) -> i32;
    }
}

unsafe extern "C" {
    // qtglue.cpp
    fn turbinehub_test_key(key: i32, mods: i32, pressed: i32, repeat: i32) -> i32;
    fn turbinehub_test_mouse(kind: i32, x: f64, y: f64, button: i32, mods: i32, wheel: i32) -> i32;
    fn turbinehub_test_touch(kind: i32, x: f64, y: f64) -> i32;
    fn turbinehub_test_spoken(what: i32) -> i32;
    fn turbinehub_pointer_event(kind: i32, x: f64, y: f64, button: i32, buttons: i32, wheel: i32) -> i32;
    fn turbinehub_window_size(w: *mut f64, h: *mut f64) -> i32;
    fn turbinehub_copy_text(text: *const std::os::raw::c_char) -> i32;
    fn turbinehub_open_with(path: *const std::os::raw::c_char) -> i32;
}

use crate::pad::Pads;
use crate::run::{self, Event, Facts, Job};
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use turbinehub_engine::cli::Ctx;
use turbinehub_engine::fake::Preset;
use turbinehub_engine::json::{Json, quote};
use turbinehub_engine::settings::Mode;
use turbinehub_engine::sources;
use turbinehub_engine::ui::plan::Ask;
use turbinehub_engine::ui::{self, Plan, Ui};

pub struct HubRust {
    state: QString,
    gamemode: bool,
    busy: bool,
    busy_words: QString,
    status: QString,
    share: QString,
    pads: bool,
    pad_name: QString,
    ui: Ui,
    reader: Option<Pads>,
    /// When the controllers were first asked for, and whether the checkup was told that none showed.
    asked_pads: Option<std::time::Instant>,
    said_no_pad: bool,
    /// What the window last said about being in front (nothing yet: in front).
    in_front: bool,
    /// Releases found while the window changed places, for the next `poll`.
    let_go: Vec<(&'static str, bool)>,
    /// The controller pointer and the left stick, when they were last looked at, and whether they rest (a text box
    /// has the keyboard, or work runs).
    pointer: crate::pointer::Pointer,
    pointer_at: Option<std::time::Instant>,
    pointer_paused: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    /// The plan whose steps are running behind the cover.
    running: Option<Plan>,
    /// Stops the job of the line of work that is running now (set when Turbine Hub closes).
    stop: Arc<AtomicBool>,
    /// When the top bar's status was last worked out.
    status_at: std::time::Instant,
    /// When a made-up machine's job started (a trial run ends it after a moment).
    pretend: Option<std::time::Instant>,
    /// When the background helper's files were last looked at.
    looked: std::time::Instant,
}

/// The value of `--name=value` among the program's arguments; "1" for a bare `--name`.
pub fn option(name: &str) -> Option<String> {
    let flag = format!("--{name}");
    std::env::args().find_map(|a| if a == flag { Some("1".to_string()) } else { a.strip_prefix(&format!("{flag}=")).map(String::from) })
}

fn dir_from(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| turbinehub_engine::home_dir().join(fallback))
}

/// Turbine Hub's data folder: the background helper's history and reports, and the save snapshots.
pub fn data_dir() -> PathBuf {
    dir_from("XDG_DATA_HOME", ".local/share").join("turbinehub")
}

/// Turbine Hub's cache folder: the last copy of everything fetched.
pub fn cache_dir() -> PathBuf {
    dir_from("XDG_CACHE_HOME", ".cache").join("turbinehub")
}

/// Where the real machine is read from.
pub fn real_ctx() -> Ctx {
    Ctx { root: "/".into(), home: turbinehub_engine::home_dir(), config_home: dir_from("XDG_CONFIG_HOME", ".config"), efibootmgr: String::new(), version: crate::VERSION.into() }
}

/// The made-up machine's folder of this run, removed again when the window closes.
static SAMPLE_DIR: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// A new folder for this run's made-up machine. Inside the Flatpak every run has the same process id, so the name
/// also carries the time and a random number, and the folder is only taken when it didn't exist yet: runs side by
/// side never share one.
fn sample_dir() -> PathBuf {
    use std::hash::{BuildHasher, Hasher};
    let base = std::env::temp_dir();
    let _ = std::fs::create_dir_all(&base);
    loop {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(nanos);
        let dir = base.join(format!("turbinehub-sample-{}-{nanos}-{:016x}", std::process::id(), h.finish()));
        match std::fs::create_dir(&dir) {
            Ok(()) => {
                if let Ok(mut d) = SAMPLE_DIR.lock() {
                    *d = Some(dir.clone());
                }
                return dir;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            // Nowhere to make it: the engine makes the folders it needs itself.
            Err(_) => return dir,
        }
    }
}

/// Removes the made-up machine's folder of this run, if it made one.
pub fn remove_sample_dir() {
    if let Some(dir) = SAMPLE_DIR.lock().ok().and_then(|mut d| d.take()) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The engine for this run: a made-up machine for pictures (`--screenshot`) and trial runs (`--machine`), else the
/// real one.
fn make_ui() -> Ui {
    let preset = match option("machine").as_deref() {
        Some("desktop") => Some(Preset::Desktop),
        Some("tv") => Some(Preset::TvPc),
        // A picture of this very machine, to see the real readings in place.
        Some("real") => None,
        Some(_) => Some(ui::HANDHELD),
        None if option("screenshot").is_some() => Some(ui::HANDHELD),
        None => None,
    };
    let Some(preset) = preset else {
        let mut ui = Ui::new(real_ctx(), dir_from("XDG_CACHE_HOME", ".cache").join("turbinehub"));
        ui.data = data_dir();
        // The helper this Turbine Hub came with, to tell when the one installed on the machine is older.
        ui.helper_bundled = Some(PathBuf::from("/app/bin/turbinehub-helper")).filter(|p| p.is_file());
        // The lists that came with the app stand in until the first fetch, and what the background helper found
        // shows before the app has asked anything itself.
        for list in [&sources::ANTICHEAT, &sources::FIXES] {
            sources::seed(list, &ui.cache);
        }
        ui.decky_tag = sources::release_tag(&sources::local(&sources::DECKY_RELEASE, &ui.cache));
        ui.nsl_tag = sources::release_tag(&sources::local(&sources::NSL_RELEASE, &ui.cache));
        ui.absorb();
        ui.window = !run::in_game_mode();
        return ui;
    };
    let mode = match option("mode").as_deref() {
        Some("guided") => Some(Mode::Guided),
        Some("veteran") => Some(Mode::Veteran),
        _ => None,
    };
    let dir = sample_dir();
    let mut ui = Ui::sample(&dir, preset, mode);
    ui.window = option("window").is_some();
    if option("offline").is_some() {
        ui.sample_offline();
    }
    if option("no-password").is_some() {
        ui.facts.has_password = Some(false);
    }
    if option("helper").is_some() {
        ui.sample_with_helper();
    }
    if option("steam-running").is_some() {
        ui.steam_running = true;
    }
    // The host couldn't say whether Steam runs, a Steam library Turbine Hub may not read, a fresh update.
    if option("steam-unsure").is_some() {
        ui.sample_steam_unsure();
    }
    if option("unseen-library").is_some() {
        ui.sample_with_unseen_library();
    }
    if option("updated").is_some() {
        ui.sample_updated();
    }
    // The lights in one of their states: another app setting them, closed again, open and ready for the test.
    if let Some(state) = option("lights") {
        ui.sample_lights(&state);
    }
    if option("no-tool").is_some() {
        ui.sample_without_tool();
    }
    // Decky Loader gone after a SteamOS update, and the machine in one of Home's power modes.
    if option("decky-stopped").is_some() {
        ui.sample_decky_stopped();
    }
    if let Some(mode) = option("power-mode") {
        ui.sample_power_mode(&mode);
    }
    // NonSteamLaunchers set up, with two launchers ticked.
    if option("nsl").is_some() {
        ui.sample_with_nsl();
    }
    // Work in the line: Discord being installed and this many more waiting ("--queue=2").
    if let Some(waiting) = option("queue") {
        ui.sample_queue(waiting.parse().unwrap_or(0));
    }
    // Just updated, with the new version's notes fetched.
    if option("whats-new").is_some() {
        ui.sample_whats_new();
    }
    // An app the made-up machine has as well ("--installed=<app id>").
    if let Some(id) = option("installed") {
        ui.installed.push(id);
    }
    let mut st = ui.settings();
    if let Some(v) = option("scale").and_then(|v| v.parse::<u32>().ok()) {
        st.ui_scale = v.clamp(50, 150);
        st.scale_chosen = true;
    }
    if let Some(t) = option("theme").filter(|t| turbinehub_engine::settings::THEMES.contains(&t.as_str())) {
        st.theme = t;
    }
    if let Some(a) = option("accent").filter(|a| turbinehub_engine::settings::ACCENTS.contains(&a.as_str())) {
        st.accent = a;
    }
    if let Some(v) = option("vision").filter(|v| turbinehub_engine::settings::VISIONS.contains(&v.as_str())) {
        st.vision = v;
    }
    // Pictures never race an animation: they are taken with animations off, unless the picture is there to see
    // that a page worked while things move comes to rest where it would without them ("--motion=full").
    if option("screenshot").is_some() {
        st.animations = option("motion").filter(|m| turbinehub_engine::settings::MOTIONS.contains(&m.as_str())).unwrap_or_else(|| "off".into());
    }
    if option("arg").is_some() && option("page").as_deref() == Some("setup") {
        st.setup_done.clear();
    }
    let _ = st.save(&ui.ctx.config_home);
    ui
}

impl Default for HubRust {
    fn default() -> Self {
        let ui = make_ui();
        let (tx, rx) = mpsc::channel();
        let share = std::env::var("TURBINEHUB_SHARE").unwrap_or_else(|_| "/app/share/turbinehub".into());
        let hub = HubRust { state: QString::from(ui.state().as_str()), gamemode: (run::in_game_mode() && option("screenshot").is_none()) || option("gamemode").is_some(), busy: false, busy_words: QString::default(), status: QString::from(ui.status().as_str()), stop: Arc::new(AtomicBool::new(false)), status_at: std::time::Instant::now(), pretend: None, share: QString::from(share.as_str()), pads: false, pad_name: QString::default(), ui, reader: None, asked_pads: None, said_no_pad: false, in_front: true, let_go: Vec::new(), pointer: crate::pointer::Pointer::default(), pointer_at: None, pointer_paused: false, tx, rx, running: None, looked: std::time::Instant::now() };
        hub.gather(false);
        hub
    }
}

impl HubRust {
    /// Asks the host and the internet again (never in a sample run).
    fn gather(&self, check_now: bool) {
        if self.ui.sample {
            return;
        }
        let st = self.ui.settings();
        let g = run::Gather { home: self.ui.ctx.home.clone(), config_home: self.ui.ctx.config_home.clone(), cache: self.ui.cache.clone(), version: self.ui.ctx.version.clone(), test_builds: st.test_builds, want_notes: self.ui.whats_new_wanted(), helper_there: turbinehub_engine::helper::installed(&self.ui.ctx.root), check_now };
        run::gather(g, self.tx.clone());
    }

    fn take(&mut self, f: Facts) {
        let ui = &mut self.ui;
        if let Some(v) = f.installed {
            ui.installed = v;
        }
        if let Some(v) = f.updates {
            ui.facts.updates = ui::store::APPS.iter().filter(|a| v.iter().any(|u| u == a.id)).map(|a| a.name.to_string()).collect();
            ui.updates = v;
            ui.updates_own = true;
        }
        if let Some(v) = f.plugins {
            ui.facts.plugin_updates = v;
        }
        if f.local {
            ui.facts.has_password = f.has_password;
            // No answer counts as running: nothing is written to Steam's list then.
            let steam = f.steam.unwrap_or(turbinehub_engine::shortcuts::Steam::Unknown);
            ui.steam_running = steam.in_the_way();
            ui.steam_unsure = steam == turbinehub_engine::shortcuts::Steam::Unknown;
            ui.steam_started = f.steam_started;
            ui.sound = f.sound;
            ui.sound_now = f.sound_now;
            ui.free_bytes = f.free_bytes;
            if let Some(drift) = f.drift {
                // The background helper reads the same note, so it knows what the app found (and put back).
                turbinehub_engine::agent::save_drift(&ui.data, turbinehub_engine::now(), &turbinehub_engine::agent::steamos(&ui.ctx.root), &drift);
                ui.facts.drift = drift;
                ui.drift_own = true;
            }
            if f.agent_on.is_some() {
                ui.agent_on = f.agent_on;
            }
            if let Some(lights) = f.lights {
                ui.set_host_lights(lights);
            }
            if let Some(e) = f.efibootmgr {
                ui.ctx.efibootmgr = e;
            }
            if let Some(screens) = f.screens {
                // The main outside screen's refresh now, for the checkup's "runs below what it can" tip.
                ui.facts.current_hz = screens.iter().find(|x| !x.name.starts_with("eDP")).and_then(|x| x.hz);
                ui.facts.screens = screens;
            }
            let windows_here = ui.machine().drives.iter().any(|d| d.role == turbinehub_engine::hardware::DriveRole::Windows);
            ui.facts.not_in_library = ui::selftile::missing(ui);
            ui.facts.windows_not_brought = windows_here && !ui.settings().brought.iter().any(|b| b == "windows");
        }
        if let Some(online) = f.online {
            ui.offline = !online;
        }
        if f.decky_tag.is_some() {
            ui.decky_tag = f.decky_tag;
        }
        if f.nsl_tag.is_some() {
            ui.nsl_tag = f.nsl_tag;
        }
        if let Some(v) = f.update {
            ui.update = Some(v).filter(|v| !v.is_empty());
            ui.updates_own = true;
        }
    }

    /// The steps that need the engine's own state, done once everything else worked, and what the screens know
    /// about the host brought up to date (the engine decides both).
    fn finish(&mut self, plan: &Plan) -> Result<(), String> {
        self.ui.finish(plan)
    }

    /// What a plan's steps need to run, as job number `id` (0: behind the cover).
    fn job(&self, id: u64, plan: &Plan, password: String, stop: Arc<AtomicBool>) -> Job {
        let st = self.ui.settings();
        Job { id, stop, steps: plan.steps.clone(), password, home: self.ui.ctx.home.clone(), folder: self.ui.folder(), data: self.ui.data.clone(), version: self.ui.ctx.version.clone(), cache: self.ui.cache.clone(), test_builds: st.test_builds }
    }

    /// The next job of the line of work starts, when nothing of it runs. On a made-up machine it only counts as
    /// running: nothing is ever run there.
    fn pump(&mut self) {
        let Some((id, plan)) = self.ui.queue_next() else { return };
        crate::log::line("queue", &format!("Started: {} ({} steps)", plan.title, plan.steps.len()));
        if self.ui.sample {
            self.pretend = Some(std::time::Instant::now());
            return;
        }
        // The line's own stop for this job: Stop (the engine's `work-stop`) and closing set it, the job's thread
        // looks at it between its pieces of work.
        self.stop = self.ui.queue_running_stop().unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        run::start(self.job(id, &plan, String::new(), self.stop.clone()), self.tx.clone());
    }
}

/// How long a job of the line takes on a made-up machine that is tried by hand (pictures end it themselves).
const PRETEND_FOR: Duration = Duration::from_secs(4);
/// How often the top bar's battery and clock are looked at again.
const STATUS_EVERY: Duration = Duration::from_secs(5);
/// How long closing waits at most for the job being worked on to stop. A job stops between two pieces of its work,
/// in well under a second; taking away a big game that was half copied is what can take longer, and closing
/// waits for that, so nothing half made stays.
const STOP_WITHIN: Duration = Duration::from_secs(45);

/// How long the controller's presses are ignored after the window comes back to the front.
const QUIET_AFTER_BACK: Duration = Duration::from_millis(250);
/// How long a Desktop Mode window waits for a controller before the checkup says none was found.
const NO_PAD_AFTER: Duration = Duration::from_secs(10);

/// The help page with one more section, for a Desktop Mode window where no controller was found.
fn with_pad_hint(page: &str) -> String {
    let Ok(Json::Obj(mut fields)) = turbinehub_engine::json::parse(page) else { return page.to_string() };
    let text = |v: &str| Json::Str(v.to_string());
    let row = Json::Obj(vec![
        ("id".into(), text("no-controller")),
        ("kind".into(), text("info")),
        ("title".into(), text("No controller found")),
        ("line".into(), text("Keyboard and mouse work everywhere. For a controller in Desktop Mode, start Turbine Hub from Steam: Settings, My setup adds it to your library.")),
        ("act".into(), text("none")),
        ("arg".into(), text("")),
        ("icon".into(), text("games")),
    ]);
    let section = Json::Obj(vec![("title".into(), text("Controller")), ("rows".into(), Json::Arr(vec![row]))]);
    for (k, v) in fields.iter_mut() {
        if k == "sections"
            && let Json::Arr(list) = v
        {
            list.insert(list.len().min(1), section);
            break;
        }
    }
    Json::Obj(fields).to_text()
}

/// The window's size, when it is there.
fn window_size() -> Option<(f64, f64)> {
    let (mut w, mut h) = (0.0f64, 0.0f64);
    // SAFETY: a plain call into qtglue.cpp on the interface thread; it only reads the window's size into the two
    // numbers handed to it.
    let ok = unsafe { turbinehub_window_size(&mut w, &mut h) } == 1;
    (ok && w > 0.0 && h > 0.0).then_some((w, h))
}

impl HubRust {
    /// One look at the sticks, `dt_ms` after the last: the pointer's news as JSON (None when nothing changed) and
    /// the directions the left stick let go and pressed.
    fn look(&mut self, sticks: &crate::pointer::Sticks, dt_ms: f64, w: f64, h: f64) -> (Option<String>, Vec<(&'static str, bool)>) {
        let t = self.pointer.tick(sticks, dt_ms, w, h);
        let dirs: Vec<(&'static str, bool)> = t.release.map(|d| (d, false)).into_iter().chain(t.press.map(|d| (d, true))).collect();
        let news = (t.moved || t.hid || t.wheel != 0).then(|| self.pointer_json(t.moved, t.wheel));
        (news, dirs)
    }

    fn pointer_json(&self, moved: bool, wheel: i32) -> String {
        format!("{{\"x\":{:.1},\"y\":{:.1},\"shown\":{},\"moved\":{moved},\"wheel\":{wheel}}}", self.pointer.x, self.pointer.y, self.pointer.shown)
    }
}

fn answer(pairs: &[(&str, String)]) -> QString {
    let body: Vec<String> = pairs.iter().map(|(k, v)| format!("{}:{v}", quote(k))).collect();
    QString::from(format!("{{{}}}", body.join(",")).as_str())
}

impl qobject::Hub {
    fn refresh_state(mut self: Pin<&mut Self>) {
        let text = self.rust().ui.state();
        self.as_mut().set_state(QString::from(text.as_str()));
    }

    /// The top bar's status again, when it changed.
    fn refresh_status(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().status_at = std::time::Instant::now();
        let text = self.rust().ui.status();
        if self.status().to_string() != text {
            self.as_mut().set_status(QString::from(text.as_str()));
        }
    }

    pub fn stop_work(mut self: Pin<&mut Self>) {
        let running = self.as_mut().rust_mut().ui.queue_stop();
        self.rust().stop.store(true, Ordering::SeqCst);
        if self.rust().ui.sample {
            return;
        }
        crate::log::line("queue", "Closing: what waited was dropped, what ran is being stopped");
        let started = std::time::Instant::now();
        // The running plan's thread says when it has stopped; a copy of Files says so through its own look.
        let mut plan_left = running.is_some();
        let mut copy_left = true;
        while (plan_left || copy_left) && started.elapsed() < STOP_WITHIN {
            while let Ok(e) = self.rust().rx.try_recv() {
                if matches!(e, Event::Done(id, _) if Some(id) == running) {
                    plan_left = false;
                }
            }
            copy_left = match self.as_mut().rust_mut().ui.files_tick() {
                Some(news) => !news.contains("\"done\""),
                None => self.rust().ui.files_working(),
            };
            if plan_left || copy_left {
                std::thread::sleep(Duration::from_millis(30));
            }
        }
    }

    pub fn page(mut self: Pin<&mut Self>, name: &QString, arg: &QString) -> QString {
        let name = name.to_string();
        let mut text = self.as_mut().rust_mut().ui.page(&name, &arg.to_string());
        // Help in a Desktop Mode window with no controller seen says how to get one working.
        let no_pad = !*self.gamemode() && !self.rust().ui.sample && self.rust().reader.as_ref().is_some_and(|r| !r.connected());
        if name == "help" && no_pad {
            text = with_pad_hint(&text);
        }
        QString::from(text.as_str())
    }

    pub fn front(mut self: Pin<&mut Self>, active: bool) {
        self.as_mut().rust_mut().in_front = active;
        let mut this = self.as_mut().rust_mut();
        // A stick still pushed from the game presses nothing until it has been back in the middle, and the pointer
        // doesn't stay on screen behind a game.
        let stick = this.pointer.let_go();
        this.pointer.hide();
        this.pointer.holding = false;
        let Some(r) = this.reader.as_mut() else { return };
        if active {
            // No controller yet: ask SDL again now (after leaving Game Mode, or one switched on meanwhile).
            r.rescan(Duration::from_secs(2));
        }
        let mut ups = r.set_front(active, QUIET_AFTER_BACK);
        ups.extend(stick.and_then(|dir| r.stick_event(dir, false)));
        this.let_go.extend(ups);
    }

    pub fn pointer_event(&self, kind: i32, x: f64, y: f64, button: i32, buttons: i32, wheel: i32) {
        // SAFETY: a plain call into qtglue.cpp on the interface thread; it only sends an event to this window.
        unsafe { turbinehub_pointer_event(kind, x, y, button, buttons, wheel) };
    }

    pub fn pointer(mut self: Pin<&mut Self>, what: i32) {
        let mut this = self.as_mut().rust_mut();
        match what {
            0 => {
                this.pointer.hide();
            }
            1 => {
                this.pointer_paused = true;
                this.pointer.hide();
                this.pointer.holding = false;
                // A direction the stick holds is let go, so it doesn't go on repeating under the keyboard.
                if let Some(dir) = this.pointer.let_go()
                    && let Some(up) = this.reader.as_mut().and_then(|r| r.stick_event(dir, false))
                {
                    this.let_go.push(up);
                }
            }
            2 => this.pointer_paused = false,
            3 => this.pointer.holding = true,
            _ => this.pointer.holding = false,
        }
    }

    pub fn pointer_place(mut self: Pin<&mut Self>, x: f64, y: f64) {
        self.as_mut().rust_mut().pointer.place(x, y);
    }

    pub fn test_sticks(mut self: Pin<&mut Self>, lx: f64, ly: f64, rx: f64, ry: f64, ms: i32) -> QString {
        if option("screenshot").is_none() {
            return QString::default();
        }
        let Some((w, h)) = window_size() else { return QString::default() };
        let sticks = crate::pointer::Sticks { lx, ly, rx, ry };
        let (mut moved, mut wheel, mut keys) = (false, 0, Vec::new());
        let mut this = self.as_mut().rust_mut();
        for _ in 0..(ms / 16).max(1) {
            let t = this.pointer.tick(&sticks, 16.0, w, h);
            moved |= t.moved;
            wheel += t.wheel;
            keys.extend(t.release.map(|d| format!("[{},false]", quote(d))));
            keys.extend(t.press.map(|d| format!("[{},true]", quote(d))));
            // As in `poll`: the D-pad (the left stick is one) sends the pointer away.
            if t.press.is_some() {
                this.pointer.hide();
            }
        }
        let ptr = this.pointer_json(moved, wheel);
        answer(&[("ptr", ptr), ("keys", format!("[{}]", keys.join(",")))])
    }

    pub fn test_key(&self, key: i32, mods: i32, pressed: bool, repeat: bool) {
        if option("screenshot").is_some() {
            // SAFETY: a plain call into qtglue.cpp on the interface thread; it only sends an event to this window.
            unsafe { turbinehub_test_key(key, mods, i32::from(pressed), i32::from(repeat)) };
        }
    }

    pub fn test_spoken(&self, what: i32) -> i32 {
        if option("screenshot").is_none() {
            return -1;
        }
        // SAFETY: a plain call into qtglue.cpp on the interface thread; it only reads the window's accessibility tree.
        unsafe { turbinehub_test_spoken(what) }
    }

    pub fn test_mouse(&self, kind: i32, x: f64, y: f64, button: i32, mods: i32, wheel: i32) {
        if option("screenshot").is_some() {
            // SAFETY: a plain call into qtglue.cpp on the interface thread; it only sends an event to this window.
            unsafe { turbinehub_test_mouse(kind, x, y, button, mods, wheel) };
        }
    }

    pub fn test_touch(&self, kind: i32, x: f64, y: f64) {
        if option("screenshot").is_some() {
            // SAFETY: a plain call into qtglue.cpp on the interface thread; it only sends an event to this window.
            unsafe { turbinehub_test_touch(kind, x, y) };
        }
    }

    pub fn act(mut self: Pin<&mut Self>, action: &QString, arg: &QString, value: &QString) -> QString {
        let action = action.to_string();
        // What was typed (a search, a name) stays out of the log: only that something was.
        crate::log::line("act", &format!("{action} {arg}{}", if value.is_empty() { "" } else { " (with a value)" }));
        // A restart that was offered and not confirmed is forgotten with the next press.
        crate::RESTART.store(false, std::sync::atomic::Ordering::Relaxed);
        if action == "update-check" {
            // The player's own press always asks, also within six hours of the last answer.
            self.rust().gather(true);
            return answer(&[("toast", quote("Checking for updates"))]);
        }
        let text = self.as_mut().rust_mut().ui.act(&action, &arg.to_string(), &value.to_string());
        if let Ok(j) = turbinehub_engine::json::parse(&text) {
            // The dialog that closes Turbine Hub to start it again: main starts the new one once the window is gone.
            if j.get("restart").and_then(Json::as_bool) == Some(true) && !self.rust().ui.sample {
                crate::RESTART.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if j.get("state").is_some() {
                self.as_mut().refresh_state();
            }
            // Files: a path for the clipboard (Copy as path), and a file for the system's own list of apps (Open
            // with, in Desktop Mode). Neither on a made-up machine, whose paths lead nowhere.
            if let Some(text) = j.str_at("copy").and_then(|t| std::ffi::CString::new(t).ok()) {
                // SAFETY: a plain call into qtglue.cpp on the interface thread with a string that lives through it.
                unsafe { turbinehub_copy_text(text.as_ptr()) };
            }
            if let Some(path) = j.str_at("openWith").filter(|_| !self.rust().ui.sample).and_then(|t| std::ffi::CString::new(t).ok()) {
                // SAFETY: as above; the call only hands the open file to the desktop's portal.
                unsafe { turbinehub_open_with(path.as_ptr()) };
            }
            // A harmless host command the engine wants run right away.
            let line: Vec<String> = j.get("run").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect();
            if !line.is_empty() && !self.rust().ui.sample {
                run::quick(line);
            }
        }
        // A press may have put a copy of Files into the line, or ended a made-up machine's job.
        self.as_mut().rust_mut().pump();
        self.as_mut().refresh_status();
        QString::from(text.as_str())
    }

    pub fn confirm(mut self: Pin<&mut Self>, password: &QString, again: &QString) -> QString {
        let (password, again) = (password.to_string(), again.to_string());
        if *self.busy() {
            return answer(&[("error", quote("Something is still running. Give it a moment."))]);
        }
        let Some(plan) = self.as_mut().rust_mut().ui.confirm() else { return answer(&[]) };
        // Asked the same on a made-up machine: an empty password never goes ahead.
        let problem = match plan.ask {
            Ask::None => None,
            Ask::Current if password.is_empty() => Some("Type the password first."),
            Ask::New if password.is_empty() => Some("Type a password first."),
            Ask::New if password != again => Some("The two don't match. Type them again."),
            _ => None,
        };
        if let Some(p) = problem {
            // The dialog stays open with its plan.
            self.as_mut().rust_mut().ui.put_back(plan);
            return answer(&[("error", quote(p))]);
        }
        // Work that may run while the player browses joins the line; the engine says what that is, and refuses
        // what touches the same thing as a job already there.
        if !self.rust().ui.runs_covered(&plan) {
            crate::log::line("plan", &format!("Confirmed for the line of work: {} ({} steps)", plan.title, plan.steps.len()));
            let text = self.as_mut().rust_mut().ui.enqueue(plan).to_text();
            self.as_mut().rust_mut().pump();
            self.as_mut().refresh_status();
            return QString::from(text.as_str());
        }
        if let Some(why) = self.rust().ui.work_refusal(&plan) {
            return answer(&[("toast", quote(&why))]);
        }
        if self.rust().ui.sample {
            // A made-up machine: only what touches its own folders runs, and the screens carry on from there.
            let result = self.as_mut().rust_mut().ui.play_sample(&plan);
            self.as_mut().refresh_state();
            return match result {
                Ok(()) => answer(&[("toast", quote(&plan.done)), ("open", quote(&plan.then_open)), ("refresh", "true".into())]),
                Err(why) => answer(&[("toast", quote(&why)), ("refresh", "true".into())]),
            };
        }
        // Behind the cover nothing stops it: it runs to its end or to its time limit.
        let job = self.rust().job(0, &plan, password, Arc::new(AtomicBool::new(false)));
        crate::log::line("plan", &format!("Confirmed: {} ({} steps)", plan.title, plan.steps.len()));
        run::start(job, self.rust().tx.clone());
        self.as_mut().rust_mut().running = Some(plan);
        self.as_mut().refresh_status();
        self.as_mut().set_busy_words(QString::from("Working"));
        self.as_mut().set_busy(true);
        answer(&[("busy", "true".into())])
    }

    pub fn cancel(mut self: Pin<&mut Self>) {
        crate::RESTART.store(false, std::sync::atomic::Ordering::Relaxed);
        self.as_mut().rust_mut().ui.cancel();
    }

    pub fn poll(mut self: Pin<&mut Self>) -> QString {
        let mut out: Vec<(&str, String)> = Vec::new();
        let mut tick = false;
        if self.rust().reader.is_none() && crate::backend::option("screenshot").is_none() {
            let mut r = Pads::open();
            if !self.rust().in_front {
                r.set_front(false, QUIET_AFTER_BACK);
            }
            self.as_mut().rust_mut().reader = Some(r);
            self.as_mut().rust_mut().asked_pads = Some(std::time::Instant::now());
        }
        // What was let go while the window changed places comes first.
        let mut events = std::mem::take(&mut self.as_mut().rust_mut().let_go);
        let mut seen: Option<(bool, String, bool)> = None;
        let mut sticks = None;
        if let Some(p) = self.as_mut().rust_mut().reader.as_mut() {
            p.rescan(Duration::from_secs(10));
            events.extend(p.poll());
            seen = Some((p.connected(), p.name(), p.take_lost()));
            sticks = Some(p.sticks());
        }
        // The sticks: the right one moves the pointer, the left one is a D-pad when pushed and a wheel when swept.
        // Only in front (the sticks belong to the game otherwise) and not while a text box has the keyboard.
        let now = std::time::Instant::now();
        let dt = self.rust().pointer_at.map(|t| now.duration_since(t).as_secs_f64() * 1000.0).unwrap_or(16.0);
        self.as_mut().rust_mut().pointer_at = Some(now);
        let was_shown = self.rust().pointer.shown;
        let mut news = None;
        if let Some(sticks) = sticks.filter(|_| self.rust().in_front && !self.rust().pointer_paused)
            && let Some((w, h)) = window_size()
        {
            let mut this = self.as_mut().rust_mut();
            let (ptr, dirs) = this.look(&sticks, dt, w, h);
            news = ptr;
            for (dir, pressed) in dirs {
                events.extend(this.reader.as_mut().and_then(|r| r.stick_event(dir, pressed)));
            }
        }
        // Any button but the pointer's own (R3, and the triggers while it shows) means the buttons are in use: the
        // pointer goes, out of the highlight's way.
        if events.iter().any(|(k, down)| *down && !["r3", "lt", "rt"].contains(k)) && !self.rust().pointer.holding && self.as_mut().rust_mut().pointer.hide() {
            news = Some(self.rust().pointer_json(false, 0));
        }
        if news.is_none() && was_shown != self.rust().pointer.shown {
            news = Some(self.rust().pointer_json(false, 0));
        }
        if let Some(ptr) = news {
            out.push(("ptr", ptr));
        }
        if let Some((connected, name, lost)) = seen {
            if *self.pads() != connected || self.pad_name().to_string() != name {
                self.as_mut().set_pad_name(QString::from(name.as_str()));
                self.as_mut().set_pads(connected);
            }
            // A Desktop Mode window where no controller has shown for a while: the checkup says how to get one
            // working, and stops saying it the moment one shows.
            let none = !connected && !*self.gamemode() && self.rust().asked_pads.is_some_and(|t| t.elapsed() >= NO_PAD_AFTER);
            if none != self.rust().said_no_pad {
                self.as_mut().rust_mut().said_no_pad = none;
                self.as_mut().rust_mut().ui.facts.no_controller = none;
                crate::log::line("pad", if none { "No controller after ten seconds: the checkup says how to get one" } else { "A controller is there now" });
                tick = true;
            }
            // A controller went away: whatever it held in the interface stops.
            if lost {
                out.push(("lost", "true".into()));
            }
        }
        if !events.is_empty() {
            let keys: Vec<String> = events.into_iter().map(|(k, down)| format!("[{},{down}]", quote(k))).collect();
            out.push(("keys", format!("[{}]", keys.join(","))));
        }
        // A second launch asked this window to come to the front.
        if let Some(page) = crate::single::take_show() {
            out.push(("show", if page.is_empty() { "true".into() } else { quote(&page) }));
        }
        // Files: a copy's progress or its end, a search below a folder finding more.
        if let Some(news) = self.as_mut().rust_mut().ui.files_tick() {
            out.push(("files", news));
        }
        // What the background helper wrote since the last look (a sleep, updates it found), every half minute.
        if !self.rust().ui.sample && self.rust().looked.elapsed() >= Duration::from_secs(30) {
            self.as_mut().rust_mut().looked = std::time::Instant::now();
            tick |= self.as_mut().rust_mut().ui.absorb();
        }
        let mut status = false;
        let mut events = Vec::new();
        while let Ok(e) = self.rust().rx.try_recv() {
            events.push(e);
        }
        for e in events {
            match e {
                Event::Progress(0, words, _) => {
                    if !words.is_empty() {
                        self.as_mut().set_busy_words(QString::from(words.as_str()));
                    }
                }
                Event::Progress(id, words, part) => {
                    self.as_mut().rust_mut().ui.queue_say(id, &words, part);
                    status = true;
                }
                // What's new can show now (the window asks when it has no dialog open).
                Event::Notes => out.push(("whatsnew", "true".into())),
                // A job of the line of work ended: its note, the engine's lists brought up to date, the next one.
                Event::Done(id, result) if id != 0 => {
                    crate::log::line("queue", &match &result {
                        Ok(()) => format!("Job {id} done"),
                        Err(why) => format!("Job {id} stopped: {why}"),
                    });
                    if let Some(j) = self.as_mut().rust_mut().ui.queue_done(id, result) {
                        out.push(("job", j.to_text()));
                    }
                    self.rust().gather(false);
                    status = true;
                }
                Event::Facts(f) => {
                    self.as_mut().rust_mut().take(*f);
                    tick = true;
                }
                Event::Done(_, result) => {
                    let plan = self.as_mut().rust_mut().running.take();
                    let result = match (&plan, result) {
                        (Some(p), Ok(())) => self.as_mut().rust_mut().finish(p),
                        (_, r) => r,
                    };
                    crate::log::line("plan", &match &result {
                        Ok(()) => "Done".to_string(),
                        Err(why) => format!("Stopped: {why}"),
                    });
                    self.as_mut().set_busy(false);
                    self.as_mut().refresh_state();
                    self.rust().gather(false);
                    // A cleaning says what each drive really had free before and has now.
                    let folder = self.rust().ui.folder();
                    let (done, open) = plan.map(|p| (turbinehub_engine::storage::done_words(&p.steps, &folder, &turbinehub_engine::files::Disk::here()).unwrap_or(p.done), p.then_open)).unwrap_or_default();
                    out.push(("done", match result {
                        Ok(()) => format!("{{\"ok\":true,\"toast\":{},\"open\":{}}}", quote(&done), quote(&open)),
                        Err(why) => format!("{{\"ok\":false,\"toast\":{},\"open\":\"\"}}", quote(&why)),
                    }));
                }
            }
        }
        // A made-up machine tried by hand: its job ends after a moment, as a real one's would (also one that was
        // in the line from the start: --queue).
        if self.rust().ui.sample && self.rust().pretend.is_none() && self.rust().ui.queue_working() {
            self.as_mut().rust_mut().pretend = Some(std::time::Instant::now());
        }
        if self.rust().ui.sample && self.rust().pretend.is_some_and(|t| t.elapsed() >= PRETEND_FOR) {
            self.as_mut().rust_mut().pretend = None;
            let text = self.as_mut().rust_mut().ui.act("queue-finish", "", "");
            out.push(("job", text));
            status = true;
        }
        // The next job's turn, and the top bar: at once when the line changed, else every few seconds.
        self.as_mut().rust_mut().pump();
        if status || tick || self.rust().status_at.elapsed() >= STATUS_EVERY || out.iter().any(|(k, _)| *k == "files") {
            self.as_mut().refresh_status();
        }
        if tick {
            out.push(("tick", "true".into()));
        }
        if out.is_empty() { QString::default() } else { answer(&out) }
    }

    pub fn option(&self, name: &QString) -> QString {
        QString::from(option(&name.to_string()).unwrap_or_default().as_str())
    }

    pub fn say(&self, line: &QString) {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_without_a_controller_says_how_to_get_one_working() {
        let page = r#"{"title":"Home","sections":[{"title":"","rows":[]},{"title":"Buttons","rows":[]}],"hints":[]}"#;
        let j = turbinehub_engine::json::parse(&with_pad_hint(page)).unwrap();
        let sections = j.get("sections").unwrap().as_array();
        assert_eq!(sections.len(), 3);
        // Right under Where's my...?, before the list of buttons.
        assert_eq!(sections[1].str_at("title"), Some("Controller"));
        let row = &sections[1].get("rows").unwrap().as_array()[0];
        assert_eq!(row.str_at("kind"), Some("info"));
        assert!(row.str_at("line").unwrap().contains("start Turbine Hub from Steam"));
        assert_eq!(sections[2].str_at("title"), Some("Buttons"));
        // Something that isn't a page comes back as it was.
        assert_eq!(with_pad_hint("not a page"), "not a page");
    }
}

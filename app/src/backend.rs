//! The bridge between the QML interface and the engine: one object, `Hub`, available to QML as a singleton.
//!
//! QML asks for a page and gets JSON, sends a press and gets what to show next. Anything slow (the steps of a
//! confirmed dialog, questions to the host or the internet) runs on its own thread and comes back through `poll`,
//! which QML calls on a timer together with the controller.

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
        /// Where the fonts and pictures are.
        #[qproperty(QString, share)]
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

        /// The value of a `--name=value` argument ("" when not given); "1" for a bare `--name`.
        #[qinvokable]
        fn option(self: &Self, name: &QString) -> QString;

        /// Prints one line on standard output (the screenshot run's check results).
        #[qinvokable]
        fn say(self: &Self, line: &QString);
    }
}

use crate::pad::Pads;
use crate::run::{self, Event, Facts, Job};
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::path::PathBuf;
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
    share: QString,
    ui: Ui,
    pads: Option<Pads>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    /// The plan whose steps are running.
    running: Option<Plan>,
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
    if option("steam-running").is_some() {
        ui.steam_running = true;
    }
    // The lights in one of their states: another app setting them, closed again, open and ready for the test.
    if let Some(state) = option("lights") {
        ui.sample_lights(&state);
    }
    if option("no-tool").is_some() {
        ui.sample_without_tool();
    }
    // NonSteamLaunchers set up, with two launchers ticked.
    if option("nsl").is_some() {
        ui.sample_with_nsl();
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
    // Pictures never race an animation.
    st.animations = option("screenshot").is_none();
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
        let hub = HubRust { state: QString::from(ui.state().as_str()), gamemode: (run::in_game_mode() && option("screenshot").is_none()) || option("gamemode").is_some(), busy: false, busy_words: QString::default(), share: QString::from(share.as_str()), ui, pads: None, tx, rx, running: None, looked: std::time::Instant::now() };
        hub.gather();
        hub
    }
}

impl HubRust {
    /// Asks the host and the internet again (never in a sample run).
    fn gather(&self) {
        if self.ui.sample {
            return;
        }
        let st = self.ui.settings();
        let g = run::Gather { home: self.ui.ctx.home.clone(), config_home: self.ui.ctx.config_home.clone(), cache: self.ui.cache.clone(), version: self.ui.ctx.version.clone(), test_builds: st.test_builds, helper_there: turbinehub_engine::helper::installed(&self.ui.ctx.root) };
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
            ui.steam_running = f.steam_running;
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

    pub fn page(mut self: Pin<&mut Self>, name: &QString, arg: &QString) -> QString {
        let text = self.as_mut().rust_mut().ui.page(&name.to_string(), &arg.to_string());
        QString::from(text.as_str())
    }

    pub fn act(mut self: Pin<&mut Self>, action: &QString, arg: &QString, value: &QString) -> QString {
        let action = action.to_string();
        if action == "update-check" {
            self.rust().gather();
            return answer(&[("toast", quote("Checking for updates"))]);
        }
        let text = self.as_mut().rust_mut().ui.act(&action, &arg.to_string(), &value.to_string());
        if let Ok(j) = turbinehub_engine::json::parse(&text) {
            if j.get("state").is_some() {
                self.as_mut().refresh_state();
            }
            // A harmless host command the engine wants run right away.
            let line: Vec<String> = j.get("run").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect();
            if !line.is_empty() && !self.rust().ui.sample {
                run::quick(line);
            }
        }
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
        if self.rust().ui.sample {
            // A made-up machine: only what touches its own folders runs, and the screens carry on from there.
            let result = self.as_mut().rust_mut().ui.play_sample(&plan);
            self.as_mut().refresh_state();
            return match result {
                Ok(()) => answer(&[("toast", quote(&plan.done)), ("open", quote(&plan.then_open)), ("refresh", "true".into())]),
                Err(why) => answer(&[("toast", quote(&why)), ("refresh", "true".into())]),
            };
        }
        let st = self.rust().ui.settings();
        let job = Job { steps: plan.steps.clone(), password, home: self.rust().ui.ctx.home.clone(), folder: self.rust().ui.folder(), data: self.rust().ui.data.clone(), steam_running: self.rust().ui.steam_running, version: self.rust().ui.ctx.version.clone(), cache: self.rust().ui.cache.clone(), test_builds: st.test_builds };
        run::start(job, self.rust().tx.clone());
        self.as_mut().rust_mut().running = Some(plan);
        self.as_mut().set_busy_words(QString::from("Working"));
        self.as_mut().set_busy(true);
        answer(&[("busy", "true".into())])
    }

    pub fn cancel(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().ui.cancel();
    }

    pub fn poll(mut self: Pin<&mut Self>) -> QString {
        let mut out: Vec<(&str, String)> = Vec::new();
        let mut tick = false;
        if self.rust().pads.is_none() && crate::backend::option("screenshot").is_none() {
            self.as_mut().rust_mut().pads = Some(Pads::open());
        }
        if let Some(p) = self.as_mut().rust_mut().pads.as_mut() {
            p.rescan(Duration::from_secs(10));
            let keys: Vec<String> = p.poll().into_iter().map(|(k, down)| format!("[{},{down}]", quote(k))).collect();
            if !keys.is_empty() {
                out.push(("keys", format!("[{}]", keys.join(","))));
            }
        }
        // What the background helper wrote since the last look (a sleep, updates it found), every half minute.
        if !self.rust().ui.sample && self.rust().looked.elapsed() >= Duration::from_secs(30) {
            self.as_mut().rust_mut().looked = std::time::Instant::now();
            tick |= self.as_mut().rust_mut().ui.absorb();
        }
        let mut events = Vec::new();
        while let Ok(e) = self.rust().rx.try_recv() {
            events.push(e);
        }
        for e in events {
            match e {
                Event::Progress(words) => self.as_mut().set_busy_words(QString::from(words.as_str())),
                Event::Facts(f) => {
                    self.as_mut().rust_mut().take(*f);
                    tick = true;
                }
                Event::Done(result) => {
                    let plan = self.as_mut().rust_mut().running.take();
                    let result = match (&plan, result) {
                        (Some(p), Ok(())) => self.as_mut().rust_mut().finish(p),
                        (_, r) => r,
                    };
                    self.as_mut().set_busy(false);
                    self.as_mut().refresh_state();
                    self.rust().gather();
                    let (done, open) = plan.map(|p| (p.done, p.then_open)).unwrap_or_default();
                    out.push(("done", match result {
                        Ok(()) => format!("{{\"ok\":true,\"toast\":{},\"open\":{}}}", quote(&done), quote(&open)),
                        Err(why) => format!("{{\"ok\":false,\"toast\":{},\"open\":\"\"}}", quote(&why)),
                    }));
                }
            }
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

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
use turbinehub_engine::ui::plan::Ask;
use turbinehub_engine::ui::{self, Plan, Step, Ui};

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
}

/// The value of `--name=value` among the program's arguments; "1" for a bare `--name`.
pub fn option(name: &str) -> Option<String> {
    let flag = format!("--{name}");
    std::env::args().find_map(|a| if a == flag { Some("1".to_string()) } else { a.strip_prefix(&format!("{flag}=")).map(String::from) })
}

fn dir_from(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| turbinehub_engine::home_dir().join(fallback))
}

/// Where the real machine is read from.
pub fn real_ctx() -> Ctx {
    Ctx { root: "/".into(), home: turbinehub_engine::home_dir(), config_home: dir_from("XDG_CONFIG_HOME", ".config"), efibootmgr: String::new(), version: crate::VERSION.into() }
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
        ui.decky_tag = std::fs::read_to_string(ui.cache.join("decky-tag")).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        ui.window = !run::in_game_mode();
        return ui;
    };
    let mode = match option("mode").as_deref() {
        Some("guided") => Some(Mode::Guided),
        Some("veteran") => Some(Mode::Veteran),
        _ => None,
    };
    let dir = std::env::temp_dir().join(format!("turbinehub-sample-{}", std::process::id()));
    let mut ui = Ui::sample(&dir, preset, mode);
    ui.window = option("window").is_some();
    if option("offline").is_some() {
        ui.sample_offline();
    }
    if option("no-password").is_some() {
        ui.facts.has_password = Some(false);
    }
    let mut st = ui.settings();
    if let Some(v) = option("scale").and_then(|v| v.parse::<u32>().ok()) {
        st.ui_scale = v.clamp(50, 150);
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
        let hub = HubRust { state: QString::from(ui.state().as_str()), gamemode: run::in_game_mode() && option("screenshot").is_none(), busy: false, busy_words: QString::default(), share: QString::from(share.as_str()), ui, pads: None, tx, rx, running: None };
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
        run::gather(self.ui.ctx.home.clone(), self.ui.cache.clone(), self.ui.ctx.version.clone(), st.test_builds, turbinehub_engine::helper::installed(&self.ui.ctx.root), ui::games::rating_downloads(&self.ui), self.tx.clone());
    }

    fn take(&mut self, f: Facts) {
        let ui = &mut self.ui;
        if let Some(v) = f.installed {
            ui.installed = v;
        }
        if let Some(v) = f.updates {
            ui.facts.updates = ui::store::APPS.iter().filter(|a| v.iter().any(|u| u == a.id)).map(|a| a.name.to_string()).collect();
            ui.updates = v;
        }
        if f.local {
            ui.facts.has_password = f.has_password;
            ui.steam_running = f.steam_running;
            ui.sound = f.sound;
            ui.sound_now = f.sound_now;
            ui.free_bytes = f.free_bytes;
            ui.facts.drift = f.drift;
            if let Some(e) = f.efibootmgr {
                ui.ctx.efibootmgr = e;
            }
            let windows_here = ui.machine().drives.iter().any(|d| d.role == turbinehub_engine::hardware::DriveRole::Windows);
            ui.facts.windows_not_brought = windows_here && !ui.settings().brought.iter().any(|b| b == "windows");
        }
        if let Some(online) = f.online {
            ui.offline = !online;
        }
        if f.decky_tag.is_some() {
            ui.decky_tag = f.decky_tag;
        }
        if let Some(v) = f.update {
            ui.update = Some(v).filter(|v| !v.is_empty());
        }
    }

    /// The steps that need the engine's own state, done here once everything else worked.
    fn finish(&mut self, plan: &Plan) -> Result<(), String> {
        let now = turbinehub_engine::now();
        for st in &plan.steps {
            match st {
                Step::Report { game } => {
                    let name = turbinehub_engine::report::file_name(&now.to_string());
                    let text = ui::games::report_text(&self.ui, game, &now.to_string());
                    let file = self.ui.folder().join("reports").join(name);
                    std::fs::create_dir_all(self.ui.folder().join("reports")).and_then(|_| std::fs::write(&file, text)).map_err(|e| format!("Couldn't write the report: {e}"))?;
                }
                Step::SaveSetup => {
                    let file = ui::settings::setup_file(&self.ui);
                    std::fs::create_dir_all(self.ui.folder()).and_then(|_| std::fs::write(&file, ui::settings::setup_json(&self.ui))).map_err(|e| format!("Couldn't save the setup: {e}"))?;
                }
                Step::Done { setup, brought } => {
                    let mut s = self.ui.settings();
                    if let Some(x) = setup.filter(|x| !s.setup_done.iter().any(|d| d == x)) {
                        s.setup_done.push(x.to_string());
                    }
                    if let Some(x) = brought.filter(|x| !s.brought.iter().any(|d| d == x)) {
                        s.brought.push(x.to_string());
                    }
                    let _ = s.save(&self.ui.ctx.config_home);
                }
                Step::SetupFinished { boost } => {
                    let mut s = self.ui.settings();
                    s.setup_done = turbinehub_engine::settings::SETUP_STEPS.iter().map(|x| x.to_string()).collect();
                    s.game_boost = s.game_boost || *boost;
                    let _ = s.save(&self.ui.ctx.config_home);
                }
                _ => {}
            }
        }
        if !plan.done.is_empty() {
            ui::note_activity(&self.ui.folder(), now, &plan.done);
        }
        Ok(())
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
        let problem = match plan.ask {
            Ask::None => None,
            _ if self.rust().ui.sample => None,
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
            // A made-up machine: nothing runs, the screens just carry on.
            return answer(&[("toast", quote(&plan.done)), ("open", quote(&plan.then_open)), ("refresh", "true".into())]);
        }
        let st = self.rust().ui.settings();
        let job = Job { steps: plan.steps.clone(), password, home: self.rust().ui.ctx.home.clone(), steam_running: self.rust().ui.steam_running, version: self.rust().ui.ctx.version.clone(), cache: self.rust().ui.cache.clone(), test_builds: st.test_builds };
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
        let mut events = Vec::new();
        while let Ok(e) = self.rust().rx.try_recv() {
            events.push(e);
        }
        for e in events {
            match e {
                Event::Progress(words) => self.as_mut().set_busy_words(QString::from(words.as_str())),
                Event::Facts(f) => {
                    self.as_mut().rust_mut().take(*f);
                    out.push(("tick", "true".into()));
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
        if out.is_empty() { QString::default() } else { answer(&out) }
    }

    pub fn option(&self, name: &QString) -> QString {
        QString::from(option(&name.to_string()).unwrap_or_default().as_str())
    }

    pub fn say(&self, line: &QString) {
        println!("{line}");
    }
}

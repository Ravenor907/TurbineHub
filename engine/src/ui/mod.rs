//! What every screen shows and what every press does, decided here so the interface only draws it. Each page is one
//! JSON object built from the machine, the player's settings and the helper's record; each action answers with what
//! to show next (a note, another page, or the one dialog with the exact changes) and never changes the system by
//! itself: the app runs the commands of a confirmed dialog.

mod activity;
mod after;
pub mod bring;
pub mod games;
mod help;
mod home;
pub mod launch;
pub mod lighting;
pub mod machine;
pub mod nsl;
pub mod plan;
mod sample;
pub mod saves;
pub mod selftile;
pub mod settings;
mod setup;
pub mod store;

pub use activity::{ago, note as note_activity, read as read_activity};
pub use games::Switches;
pub use plan::{Plan, Step};
pub use sample::HANDHELD;

use crate::checkup::Facts;
use crate::cli::Ctx;
use crate::hardware::{GpuVendor, Kind, Machine};
use crate::helper::Ledger;
use crate::json::Json;
use crate::sensors::Readings;
use crate::settings::{Mode, Settings};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(crate) fn s(v: impl Into<String>) -> Json {
    Json::Str(v.into())
}
pub(crate) fn n(v: f64) -> Json {
    Json::Num(v)
}
pub(crate) fn b(v: bool) -> Json {
    Json::Bool(v)
}
pub(crate) fn arr(v: impl IntoIterator<Item = Json>) -> Json {
    Json::Arr(v.into_iter().collect())
}
macro_rules! obj {
    ($($k:literal : $v:expr),* $(,)?) => { $crate::json::Json::Obj(vec![$(($k.to_string(), $v)),*]) };
}
pub(crate) use obj;

/// Adds or replaces one key of an object.
pub(crate) fn with(mut j: Json, k: &str, v: Json) -> Json {
    if let Json::Obj(e) = &mut j {
        e.retain(|(key, _)| key != k);
        e.push((k.to_string(), v));
    }
    j
}

/// A footer hint: the button and its words.
pub(crate) fn hint(k: &str, label: &str) -> Json {
    obj! {"k": s(k), "label": s(label)}
}

/// One row of a rows page. `kind`: toggle, check, slider, choice, button, info or input. A press sends the row's
/// `act` with its `arg` (and the new value for a slider or a choice).
pub(crate) fn row(act: &str, arg: &str, kind: &str, title: &str, line: &str) -> Json {
    obj! {"id": s(if arg.is_empty() { act.to_string() } else { format!("{act}:{arg}") }), "kind": s(kind), "title": s(title), "line": s(line), "act": s(act), "arg": s(arg)}
}
pub(crate) fn section(title: &str, rows: Vec<Json>) -> Json {
    obj! {"title": s(title), "rows": arr(rows)}
}

/// The interface's state between presses: where it reads from, what the player ticked on the picking screens, and the
/// dialog waiting for an answer.
pub struct Ui {
    pub ctx: Ctx,
    /// A screenshot or test run on a made-up machine: nothing is ever run, and sample facts stand in for what only
    /// a real machine can say (updates, the last sleep).
    pub sample: bool,
    /// A Desktop Mode window: hints name keys, tips mention the mouse.
    pub window: bool,
    /// What the player used last, when the window knows: keys (true) or the controller (false). Help names the
    /// buttons from it; unknown, a Desktop Mode window means keys.
    pub keys: Option<bool>,
    pub facts: Facts,
    /// App ids installed on the host (Flathub), and the ones with an update.
    pub installed: Vec<String>,
    pub updates: Vec<String>,
    /// Where the Windows drive is mounted, when it is.
    pub windows_mount: Option<PathBuf>,
    /// Where ratings, the anti-cheat list and fixes are kept between runs.
    pub cache: PathBuf,
    /// Readings to show instead of the sensors' (sample runs).
    pub readings: Option<Readings>,
    /// The sound outputs as (name, words) and the one in use.
    pub sound: Vec<(String, String)>,
    pub sound_now: String,
    /// Decky Loader's newest release tag, the last one that was found.
    pub decky_tag: Option<String>,
    /// NonSteamLaunchers' newest release tag, the last one that was found.
    pub nsl_tag: Option<String>,
    /// Steam is running, or the host couldn't say (then it counts as running: see `shortcuts::Steam`).
    pub steam_running: bool,
    /// The host couldn't say whether Steam is running.
    pub steam_unsure: bool,
    /// The newer Turbine Hub this run installed: it is used once Turbine Hub starts again.
    pub update_installed: Option<String>,
    /// Steam library folders the player allowed in this run: read from Turbine Hub's next start.
    pub allowed: Vec<PathBuf>,
    /// Free space where Steam's main library lives.
    pub free_bytes: Option<u64>,
    /// The last try to reach the internet failed: screens keep showing what was fetched before, and say so.
    pub offline: bool,
    /// A newer Turbine Hub, when the last check found one.
    pub update: Option<String>,
    /// When the running Steam started, in seconds since 1970 (None: not running, or not known). Steam finds a new
    /// launch tool only when it starts.
    pub steam_started: Option<u64>,
    /// Turbine Hub's data folder: the background helper's history and reports, and the save snapshots.
    pub data: PathBuf,
    /// The background helper is set to start with the session (the host says). None until it was asked.
    pub agent_on: Option<bool>,
    /// The app looked for updates itself in this run, so its lists are newer than the background helper's.
    pub updates_own: bool,
    /// The app asked the helper about drift itself in this run.
    pub drift_own: bool,
    /// What the host says about the lights: Ally Hub's saved choice, who else is setting them, the helper's rule.
    pub host_lights: lighting::HostLights,
    /// The helper that came with this Turbine Hub (inside the Flatpak), to tell when the installed one is older.
    pub helper_bundled: Option<PathBuf>,
    /// The guided light test while it runs.
    pub light_test: Option<lighting::LightTest>,
    pub(crate) picked: BTreeMap<String, BTreeSet<String>>,
    pub(crate) pending: Option<Plan>,
    pub(crate) home_all: bool,
    pub(crate) filter: BTreeMap<String, String>,
}

impl Ui {
    pub fn new(ctx: Ctx, cache: PathBuf) -> Ui {
        let data = ctx.config_home.join("turbinehub/data");
        let host_lights = lighting::HostLights::load(&ctx.config_home.join("turbinehub"));
        Ui { keys: None, host_lights, helper_bundled: None, light_test: None, data, steam_unsure: false, update_installed: None, allowed: Vec::new(), steam_started: None, agent_on: None, updates_own: false, drift_own: false, ctx, sample: false, window: false, facts: Facts::default(), installed: Vec::new(), updates: Vec::new(), windows_mount: None, cache, readings: None, sound: Vec::new(), sound_now: String::new(), decky_tag: None, nsl_tag: None, steam_running: false, free_bytes: None, offline: false, update: None, picked: BTreeMap::new(), pending: None, home_all: false, filter: BTreeMap::new() }
    }

    pub fn machine(&self) -> Machine {
        Machine::read(&self.ctx.root, &self.ctx.efibootmgr, &self.home_dev())
    }
    fn home_dev(&self) -> String {
        if self.sample { crate::fake::HOME_DEV.into() } else { crate::hardware::device_of(&self.ctx.home) }
    }
    pub fn settings(&self) -> Settings {
        Settings::load(&self.ctx.config_home)
    }
    pub(crate) fn save(&self, st: &Settings) {
        let _ = st.save(&self.ctx.config_home);
    }
    pub fn ledger(&self) -> Ledger {
        crate::helper::ledger(&self.ctx.root)
    }
    pub(crate) fn readings(&self) -> Readings {
        self.readings.clone().unwrap_or_else(|| crate::sensors::read(&self.ctx.root))
    }
    pub(crate) fn veteran(&self) -> bool {
        self.settings().mode == Some(Mode::Veteran)
    }
    /// The player's own folder for Turbine Hub's files (per-game switches, lighting, the activity list).
    pub fn folder(&self) -> PathBuf {
        self.ctx.config_home.join("turbinehub")
    }

    /// What the background helper last wrote, when it is running now.
    pub fn agent(&self) -> crate::agent::Report {
        crate::agent::Report::load(&self.data)
    }
    /// Is the background helper on? What the host said about its unit, or until then whether it wrote a moment ago.
    pub fn agent_running(&self) -> bool {
        self.agent_on.unwrap_or_else(|| self.agent().alive(crate::now()))
    }

    /// Takes in what the background helper wrote since the last look: the last sleep, and (until the app has asked
    /// for itself in this run) the updates it found and what a SteamOS update reset. Answers whether anything
    /// the screens show changed.
    pub fn absorb(&mut self) -> bool {
        let before = (self.facts.clone(), self.updates.clone(), self.update.clone());
        let report = self.agent();
        self.facts.last_sleep = crate::history::last_sleep(&self.data);
        if !self.updates_own && report.checked > 0 {
            self.facts.updates = store::APPS.iter().filter(|a| report.updates.iter().any(|u| u == a.id)).map(|a| a.name.to_string()).collect();
            self.updates = report.updates.clone();
            self.facts.plugin_updates = report.plugins.clone();
            if let Some(v) = &report.update {
                self.update = Some(v.clone()).filter(|v| !v.is_empty());
            }
        }
        if !self.drift_own {
            self.facts.drift = crate::agent::drift(&self.data);
        }
        before != (self.facts.clone(), self.updates.clone(), self.update.clone())
    }

    /// A newer Turbine Hub that isn't installed yet (one this run installed only waits for a restart).
    pub(crate) fn update_ready(&self) -> Option<String> {
        self.update.clone().filter(|v| self.update_installed.as_ref() != Some(v))
    }

    /// Steam libraries in folders Turbine Hub isn't allowed to read, without the ones allowed in this run (those
    /// only wait for Turbine Hub's next start).
    pub(crate) fn unseen_libraries(&self) -> Vec<PathBuf> {
        crate::steam::unseen_libraries(&self.ctx.home).into_iter().filter(|p| !self.allowed.contains(p)).collect()
    }

    /// The facts for the checkup, with the newer Turbine Hub and the state of the lights among them.
    pub(crate) fn facts_now(&self) -> Facts {
        let shut = lighting::locked(self, &self.machine());
        let words = |v: &[PathBuf]| v.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>();
        let l = lighting::Lighting::load(&self.folder());
        // Another app setting the lights only matters once Turbine Hub drives them too.
        let rival = if lighting::Lighting::saved(&self.folder()) && l.left_to().is_none() { lighting::rivals(&self.host_lights, &self.ctx.home).first().map(|r| r.name.to_string()) } else { None };
        Facts { app_update: self.update_ready(), unseen_libraries: words(&self.unseen_libraries()), allowed_libraries: words(&self.allowed), window: self.window, lights_locked: shut.is_some(), lights_stuck: shut.as_ref().is_some_and(|s| s.stuck), lights_note: shut.as_ref().map(|s| lighting::shut_words(self, s)).unwrap_or_default(), lights_rival: rival, ..self.facts.clone() }
    }

    /// Is the helper on this machine older than the one this Turbine Hub came with? Its file differs then. The
    /// app update never replaces it by itself: a new helper only arrives with the player's password.
    pub fn helper_outdated(&self) -> bool {
        let Some(new) = &self.helper_bundled else { return false };
        let installed = self.ctx.root.join(crate::helper::BIN.trim_start_matches('/'));
        match (std::fs::read(new), std::fs::read(installed)) {
            (Ok(a), Ok(b)) => a != b,
            _ => false,
        }
    }

    /// Takes in what the host said about the lights, and keeps it for the background helper.
    pub fn set_host_lights(&mut self, host: lighting::HostLights) {
        host.save(&self.folder());
        self.host_lights = host;
    }

    /// What the top bar and the window need at every start and after every change.
    pub fn state(&self) -> String {
        let st = self.settings();
        let m = self.machine();
        obj! {
            "version": s(&self.ctx.version),
            "mode": s(match st.mode { Some(Mode::Guided) => "guided", Some(Mode::Veteran) => "veteran", None => "" }),
            "scale": n(st.scale_for(m.on_tv()) as f64),
            "animations": s(&st.animations),
            "theme": s(&st.theme),
            "accent": s(&st.accent),
            "vision": s(&st.vision),
            "machine": s(top_name(&m)),
            "kind": s(kind_key(&m)),
            "setupDone": b(st.setup_finished()),
            "sample": b(self.sample),
            "window": b(self.window),
        }
        .to_text()
    }

    /// One page as JSON. `arg` is the page's own (a section, a game, a search).
    pub fn page(&mut self, name: &str, arg: &str) -> String {
        let j = match name {
            "home" => home::page(self),
            "machine" => machine::cards_page(self),
            "games" => games::page(self, arg),
            "library" => games::library(self),
            "store" => store::page(self, arg),
            "store-item" => store::item(self, arg),
            "tool" => launch::tool_page(self),
            "proton" => launch::proton_page(self, arg),
            "saves" => saves::page(self, arg),
            "settings" => settings::page(self, arg),
            "bring" => bring::hub(self),
            "windrive" => bring::drive(self),
            "winapps" => bring::apps(self),
            "wheres" => help::page(self, arg),
            "help" => help::page_help(self, arg),
            "changes" => settings::changes(self),
            "pick" => setup::pick(),
            "setup" => setup::page(self, arg),
            "quick" => setup::quick(self),
            _ => match name.strip_prefix("module:") {
                Some(id) => machine::module(self, id),
                None => obj! {"title": s("Not found"), "sections": arr([])},
            },
        };
        j.to_text()
    }

    /// One press. Answers JSON: `toast` (a note), `open` (a page), `back`, `dialog` (the one dialog), `refresh`
    /// (the page changed), `state` (the look or mode changed).
    pub fn act(&mut self, action: &str, arg: &str, value: &str) -> String {
        plan::act(self, action, arg, value).to_text()
    }

    /// The dialog the player just confirmed, with its commands in order. None when nothing waits.
    pub fn confirm(&mut self) -> Option<Plan> {
        self.pending.take()
    }
    /// Keeps the dialog's plan after all (what was typed into it wasn't usable yet).
    pub fn put_back(&mut self, plan: Plan) {
        self.pending = Some(plan);
    }
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// The names of the pictures the screenshot run takes, each with its arguments.
    pub fn shots() -> Vec<(&'static str, &'static str)> {
        sample::SHOTS.to_vec()
    }
    /// A made-up machine under `dir`, with games, a Windows drive and sample facts, for pictures and tests.
    pub fn sample(dir: &std::path::Path, preset: crate::fake::Preset, mode: Option<Mode>) -> Ui {
        sample::build(dir, preset, mode)
    }
}

pub(crate) fn kind_key(m: &Machine) -> &'static str {
    if m.on_tv() { "tv" } else { m.kind.key() }
}

/// The word for this machine in sentences ("this handheld").
pub(crate) fn kind_word(m: &Machine) -> &'static str {
    match m.kind {
        Kind::Handheld => "handheld",
        Kind::Laptop => "laptop",
        Kind::Desktop => "PC",
    }
}

/// The processor's name without the maker's boilerplate.
pub fn cpu_short(cpu: &str) -> String {
    let mut t = cpu.replace("(R)", "").replace("(TM)", "").replace("  ", " ");
    if let Some(i) = t.find("Gen ")
        && t[..i].chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        t = t[i + 4..].to_string();
    }
    for p in ["AMD ", "Intel "] {
        if let Some(r) = t.strip_prefix(p) {
            t = r.to_string();
        }
    }
    t.split(" with ").next().unwrap_or("").split(" w/ ").next().unwrap_or("").trim().to_string()
}

/// A graphics card's name from its PCI id, for the cards Turbine Hub knows; else the maker's name.
pub fn gpu_name(m: &Machine) -> String {
    const NAMES: &[(&str, &str)] = &[("1002:7550", "RX 9070 XT"), ("1002:7590", "RX 9060 XT"), ("1002:744c", "RX 7900 XT"), ("1002:747e", "RX 7800 XT"), ("1002:7480", "RX 7600"), ("1002:73bf", "RX 6800 XT"), ("1002:73df", "RX 6700 XT"), ("1002:150e", "Radeon 890M"), ("1002:15bf", "Radeon 780M")];
    let Some(g) = m.main_gpu() else { return String::new() };
    let id = g.pci_id.to_ascii_lowercase();
    match NAMES.iter().find(|(k, _)| *k == id) {
        Some((_, name)) => (*name).into(),
        None => match g.vendor {
            GpuVendor::Amd => "AMD graphics".into(),
            GpuVendor::Nvidia => "Nvidia graphics".into(),
            GpuVendor::Intel => "Intel graphics".into(),
            GpuVendor::Other => "Graphics card".into(),
        },
    }
}

/// AMD's RDNA 4 graphics (Radeon RX 9000), by PCI id: the only cards where Proton's FSR 4 upgrade does anything.
const RDNA4: &[&str] = &["1002:7550", "1002:7551", "1002:7590", "1002:7591"];

/// Does the FSR 4 upgrade work on this machine? Only an RDNA 4 card runs FSR 4; everywhere else the switch would
/// change nothing, so it isn't offered.
pub fn fsr4_works(m: &Machine) -> bool {
    m.gpus.iter().any(|g| RDNA4.contains(&g.pci_id.to_ascii_lowercase().as_str()))
}

/// The machine's name in the top bar: a handheld or laptop by its own name, a PC with its main parts.
pub fn top_name(m: &Machine) -> String {
    if m.kind != Kind::Desktop {
        return m.name();
    }
    let gpu = gpu_name(m);
    if m.on_tv() { format!("Living room PC \u{b7} {gpu}") } else { format!("Desktop PC \u{b7} {} \u{b7} {gpu}", cpu_short(&m.cpu)) }
}

/// When the online lists were last fetched, in the cache folder's own stamp file (seconds since 1970).
pub fn checked_at(cache: &std::path::Path) -> Option<u64> {
    std::fs::read_to_string(cache.join("checked")).ok()?.trim().parse().ok()
}
pub fn set_checked(cache: &std::path::Path, now: u64) {
    let _ = crate::write_atomic(&cache.join("checked"), now.to_string().as_bytes());
}

/// How fresh what's shown is, in plain words. Screens that use online lists always show what was fetched last and
/// add this line, so being offline never empties a screen.
pub fn checked_words(now: u64, at: Option<u64>, offline: bool) -> String {
    let when = match at {
        None => "Not checked yet".to_string(),
        Some(t) => match now.saturating_sub(t) / 86_400 {
            0 => "Checked today".into(),
            1 => "Checked yesterday".into(),
            d => format!("Checked {d} days ago"),
        },
    };
    if offline { format!("{when}, offline now") } else { when }
}

/// A size in plain words (GB under a terabyte, TB above).
pub(crate) fn size_words(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.0} TB", gb / 1000.0)
    } else if gb >= 1.0 {
        format!("{gb:.0} GB")
    } else {
        format!("{:.0} MB", (bytes as f64 / 1e6).max(1.0))
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_play;
#[cfg(test)]
mod tests_background;
#[cfg(test)]
mod tests_lights;
#[cfg(test)]
mod tests_nsl;
#[cfg(test)]
mod tests_app;

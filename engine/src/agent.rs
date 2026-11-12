//! The background helper: `turbinehub --agent`, the same app run without a window by a systemd user unit. Every
//! decision it makes is here. It watches which game runs (Game Boost, save snapshots, battery use per game), keeps
//! battery and sleep history, follows the dock, checks for updates once a day and looks at the helper's record after
//! a SteamOS update. The app's side only looks (processes, sensors, screens), runs the commands this module hands
//! it (`Do`) and brings the answers back.
//!
//! It talks to the app through files in the data folder's `agent`: `status.json` (still running, the game, updates
//! found), `drift.json` (what a SteamOS update reset) and its own `state.json`. History is in `crate::history`,
//! snapshots in `crate::snapshots`.

use crate::hardware::Machine;
use crate::history::{self, Sample, Session, Sleep};
use crate::json::{self, Json};
use crate::lights::Scene;
use crate::sensors::Readings;
use crate::settings::Settings;
use crate::snapshots::{self, Kind, Snapshot};
use crate::ui::machine::Lighting;
use crate::ui::{Step, arr, n, obj, s};
use crate::{helper, modules};
use std::fs;
use std::path::{Path, PathBuf};

pub const UNIT: &str = "turbinehub-agent.service";
const FLATPAK_ID: &str = "io.github.ravenor907.TurbineHub";
const DAY: u64 = 24 * 3600;
/// A status file older than this means the background helper isn't running.
pub const ALIVE_SECS: u64 = 180;
/// A battery reading goes into the history this often.
const SAMPLE_SECS: u64 = 60;
/// How many times the sound outputs are asked for after docking (the TV's output takes a moment to appear).
const SINK_TRIES: u32 = 10;

/// The unit that starts the background helper with the player's session. It lives in the player's own systemd
/// folder, so it needs no password and SteamOS updates leave it alone.
pub fn unit_text() -> String {
    format!("[Unit]\nDescription=Turbine Hub background helper\n\n[Service]\nExecStart=/usr/bin/flatpak run --command=turbinehub {FLATPAK_ID} --agent\nRestart=on-failure\nRestartSec=30\nNice=5\n\n[Install]\nWantedBy=default.target\n")
}

pub fn unit_file(home: &Path) -> PathBuf {
    home.join(".config/systemd/user").join(UNIT)
}

fn host(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host"].iter().chain(args).map(|x| x.to_string()).collect()
}

/// Turning the background helper on: write the unit, then start it now and with every session.
pub fn turn_on(home: &Path) -> Vec<Step> {
    let file = unit_file(home);
    let dir = file.parent().unwrap_or(home).to_string_lossy().into_owned();
    vec![Step::Host(host(&["mkdir", "-p", &dir])), Step::HostWrite { path: file.to_string_lossy().into_owned(), value: unit_text() }, Step::Host(host(&["systemctl", "--user", "daemon-reload"])), Step::Host(host(&["systemctl", "--user", "enable", "--now", UNIT]))]
}

/// Turning it off: stop it, and remove the unit so nothing of it is left.
pub fn turn_off(home: &Path) -> Vec<Step> {
    vec![Step::Host(host(&["systemctl", "--user", "disable", "--now", UNIT])), Step::Host(host(&["rm", "-f", &unit_file(home).to_string_lossy()])), Step::Host(host(&["systemctl", "--user", "daemon-reload"]))]
}

/// Starts the background helper again when it is running (after Turbine Hub updated itself).
pub fn restart() -> Vec<String> {
    host(&["systemctl", "--user", "try-restart", UNIT])
}

/// Asks the host whether the unit is on. Its answer goes to `unit_enabled`.
pub fn unit_question() -> Vec<String> {
    host(&["systemctl", "--user", "is-enabled", UNIT])
}

pub fn unit_enabled(stdout: &str) -> bool {
    stdout.trim() == "enabled"
}

/// Lists the games Steam is running, from the host. Steam starts every game under its `reaper` program with the
/// game's id in the arguments; reading that changes nothing about Steam or the game.
pub fn games_question() -> Vec<String> {
    host(&["pgrep", "-af", "reaper SteamLaunch AppId="])
}

/// The Steam app ids of the running games in a process list (one process per line), in the order found.
pub fn running_games(processes: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in processes.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        for w in words.windows(3) {
            let reaper = w[0].rsplit('/').next() == Some("reaper");
            if let (true, "SteamLaunch", Some(id)) = (reaper, w[1], w[2].strip_prefix("AppId="))
                && !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_digit())
                && id != "0"
                && !out.iter().any(|x| x == id)
            {
                out.push(id.to_string());
            }
        }
    }
    out
}

/// What Steam is running under one app id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// A game: Turbine Hub's id for it ("steam:1145360", or "shortcut:<id>" for a non-Steam game) and its name,
    /// empty when nothing here knows it.
    Game { game: String, name: String },
    /// Something the player added to Steam that isn't a game: Turbine Hub itself, a browser tile, the mod manager.
    NotAGame,
}

/// Tells what an app id Steam started is. Steam runs everything in the library the same way, non-Steam entries
/// too, and Turbine Hub itself is usually one of those: opening it must not count as playing.
pub fn identify(home: &Path, id: &str) -> Started {
    let game = format!("steam:{id}");
    if let Some(g) = crate::scan_games(home).games.into_iter().find(|g| g.id() == game) {
        return Started::Game { game, name: g.name };
    }
    // A non-Steam entry is started under its own id, which Steam also writes as the top half of a longer one.
    let short = id.parse::<u64>().ok().map(|v| if v > u32::MAX as u64 { v >> 32 } else { v }).unwrap_or(0) as u32;
    for file in crate::shortcuts::shortcut_files(home) {
        let Some(list) = fs::read(&file).ok().filter(|b| b.len() <= 16 << 20).and_then(|b| crate::shortcuts::parse(&b).ok()) else { continue };
        if let Some(s) = list.iter().find(|s| s.appid == short) {
            let hay = format!("{} {}", s.exe, s.launch_options).to_lowercase();
            let tool = crate::shortcuts::is_self(s) || crate::shortcuts::browser_of(s).is_some() || hay.contains(&crate::ui::store::TURBINE.to_lowercase());
            return if tool { Started::NotAGame } else { Started::Game { game: format!("shortcut:{}", s.appid), name: s.name.clone() } };
        }
    }
    Started::Game { game, name: String::new() }
}

/// The process list read straight from /proc, for a run that isn't sandboxed (and for tests).
pub fn processes(root: &Path) -> String {
    let mut out = String::new();
    for e in fs::read_dir(root.join("proc")).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.bytes().all(|b| b.is_ascii_digit())
            && let Ok(bytes) = fs::read(e.path().join("cmdline"))
        {
            out.push_str(&format!("{name} {}\n", String::from_utf8_lossy(&bytes).replace('\0', " ")));
        }
    }
    out
}

/// Is a screen other than the built-in one connected?
pub fn external_screen(root: &Path) -> bool {
    fs::read_dir(root.join("sys/class/drm")).into_iter().flatten().flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        let kind = name.split_once('-').map(|(_, k)| k).unwrap_or("");
        name.starts_with("card") && !kind.is_empty() && !name.contains("Writeback") && !["eDP", "LVDS", "DSI"].iter().any(|k| kind.starts_with(k)) && fs::read_to_string(e.path().join("status")).is_ok_and(|t| t.trim() == "connected")
    })
}

/// The SteamOS version, from the host's os-release (inside the sandbox it is under /run/host).
pub fn steamos(root: &Path) -> String {
    let os = ["run/host/os-release", "run/host/etc/os-release", "etc/os-release"].iter().filter_map(|p| fs::read_to_string(root.join(p)).ok()).find(|t| !t.is_empty()).unwrap_or_default();
    os.lines().find_map(|l| l.strip_prefix("VERSION_ID=")).unwrap_or("").trim_matches('"').to_string()
}

/// Where the kernel counts the sleeps that worked.
pub const SUSPEND_COUNT: &str = "/sys/power/suspend_stats/success";

/// The number in `SUSPEND_COUNT`'s text.
pub fn suspend_count(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// How often each USB device that may wake the machine has done so: (the device's files, its name, the count).
pub fn wake_counts(root: &Path, m: &Machine) -> Vec<(String, String, u64)> {
    m.wake_devices
        .iter()
        .filter_map(|d| {
            let file = format!("{}_count", d.path);
            let count = fs::read_to_string(root.join(file.trim_start_matches('/'))).ok()?.trim().parse().ok()?;
            Some((d.path.clone(), if d.name.is_empty() { "A USB device".to_string() } else { d.name.clone() }, count))
        })
        .collect()
}

/// The sound outputs in `pactl list sinks`: each one's name and what a person calls it.
pub fn sinks(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for l in text.lines().map(str::trim) {
        if let Some(name) = l.strip_prefix("Name: ") {
            out.push((name.to_string(), name.to_string()));
        } else if let (Some(words), Some(last)) = (l.strip_prefix("Description: "), out.last_mut()) {
            last.1 = words.to_string();
        }
    }
    out
}

/// The output that goes to a TV or monitor, when there is one.
pub fn screen_sink(sinks: &[(String, String)]) -> Option<&(String, String)> {
    sinks.iter().find(|(name, words)| name.to_ascii_lowercase().contains("hdmi") || words.contains("HDMI") || words.contains("DisplayPort"))
}

/// The numbers of a version like "v1.4.2-beta" (1, 4, 2).
fn version_parts(v: &str) -> Option<Vec<u64>> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let v = v.split(['-', '+']).next().unwrap_or("");
    let parts: Option<Vec<u64>> = v.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty())
}

/// The installed Decky plugins the plugin store has a newer version of, by name. `store` is the store's list.
pub fn plugin_updates(home: &Path, store: &str) -> Vec<String> {
    let Ok(list) = json::parse(store) else { return Vec::new() };
    let mut out = Vec::new();
    for e in fs::read_dir(home.join("homebrew/plugins")).into_iter().flatten().flatten() {
        let read = |f: &str| fs::read_to_string(e.path().join(f)).ok().and_then(|t| json::parse(&t).ok());
        let (Some(plugin), Some(package)) = (read("plugin.json"), read("package.json")) else { continue };
        let (Some(name), Some(have)) = (plugin.str_at("name"), package.str_at("version").and_then(version_parts)) else { continue };
        let newest = list.as_array().iter().find(|p| p.str_at("name") == Some(name)).and_then(|p| p.get("versions").map(Json::as_array).unwrap_or(&[]).iter().filter_map(|v| version_parts(v.str_at("name")?)).max());
        if newest.is_some_and(|n| n > have) {
            out.push(name.to_string());
        }
    }
    out.sort_by_key(|x| x.to_lowercase());
    out
}

/// Where the background helper reads and writes.
#[derive(Debug, Clone)]
pub struct Paths {
    /// "/" for real, a made-up machine in tests.
    pub root: PathBuf,
    pub home: PathBuf,
    /// The player's Turbine Hub folder (settings beside it, lighting, the activity list).
    pub folder: PathBuf,
    /// Turbine Hub's data folder (history, snapshots, the files for the app).
    pub data: PathBuf,
}

fn dir(data: &Path) -> PathBuf {
    data.join("agent")
}

fn strings(j: &Json, k: &str) -> Vec<String> {
    j.get(k).map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect()
}

/// What the background helper tells the app.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// When it last wrote this, seconds since 1970.
    pub at: u64,
    pub version: String,
    /// The running game: id ("steam:1145360"), name, since when.
    pub game: Option<(String, String, u64)>,
    /// A newer Turbine Hub's version. None before the first check, Some("") when this one is the newest.
    pub update: Option<String>,
    /// Flathub app ids with an update.
    pub updates: Vec<String>,
    /// Decky plugins with an update, by name.
    pub plugins: Vec<String>,
    /// When the updates were last checked.
    pub checked: u64,
}

impl Report {
    pub fn load(data: &Path) -> Report {
        let Some(j) = fs::read_to_string(dir(data).join("status.json")).ok().and_then(|t| json::parse(&t).ok()) else { return Report::default() };
        let num = |k: &str| j.get(k).and_then(Json::as_f64).unwrap_or(0.0) as u64;
        let game = j.get("game").and_then(|g| Some((g.str_at("id")?.to_string(), g.str_at("name").unwrap_or("").to_string(), g.get("since").and_then(Json::as_f64)? as u64)));
        Report { at: num("at"), version: j.str_at("version").unwrap_or("").into(), game, update: j.str_at("update").map(String::from), updates: strings(&j, "updates"), plugins: strings(&j, "plugins"), checked: num("checked") }
    }
    pub fn save(&self, data: &Path) {
        let game = self.game.as_ref().map(|(id, name, since)| obj! {"id": s(id), "name": s(name), "since": n(*since as f64)}).unwrap_or(Json::Null);
        let j = obj! {"at": n(self.at as f64), "version": s(&self.version), "game": game, "update": self.update.as_ref().map(s).unwrap_or(Json::Null), "updates": arr(self.updates.iter().map(s)), "plugins": arr(self.plugins.iter().map(s)), "checked": n(self.checked as f64)};
        let _ = crate::write_atomic(&dir(data).join("status.json"), (j.to_text() + "\n").as_bytes());
    }
    /// Did the background helper write this a moment ago?
    pub fn alive(&self, now: u64) -> bool {
        self.at > 0 && now.saturating_sub(self.at) <= ALIVE_SECS
    }
    /// The game running now, when the report is fresh.
    pub fn game_now(&self, now: u64) -> Option<&(String, String, u64)> {
        self.game.as_ref().filter(|_| self.alive(now))
    }
}

/// What the helper's check found reset after a SteamOS update, as last written by the background helper or the app.
pub fn drift(data: &Path) -> Vec<String> {
    fs::read_to_string(dir(data).join("drift.json")).ok().and_then(|t| json::parse(&t).ok()).map(|j| strings(&j, "drift")).unwrap_or_default()
}

pub fn save_drift(data: &Path, now: u64, steamos: &str, drift: &[String]) {
    let j = obj! {"at": n(now as f64), "steamos": s(steamos), "drift": arr(drift.iter().map(s))};
    let _ = crate::write_atomic(&dir(data).join("drift.json"), (j.to_text() + "\n").as_bytes());
}

/// What the background helper remembers between its own runs.
#[derive(Debug, Clone, Default, PartialEq)]
struct State {
    /// The SteamOS version it last saw.
    steamos: String,
    /// When updates were last checked, and when that was last tried.
    checked: u64,
    tried: u64,
    /// Game Boost is on: the values to put back when the game ends.
    boost_before: Vec<(String, String)>,
    /// Sound went to the TV when docked: the output to go back to.
    sink_before: String,
}

impl State {
    fn load(data: &Path) -> State {
        let Some(j) = fs::read_to_string(dir(data).join("state.json")).ok().and_then(|t| json::parse(&t).ok()) else { return State::default() };
        let num = |k: &str| j.get(k).and_then(Json::as_f64).unwrap_or(0.0) as u64;
        let boost_before = j.get("boostBefore").map(Json::as_array).unwrap_or(&[]).iter().filter_map(|p| Some((p.str_at("path")?.to_string(), p.str_at("value")?.to_string()))).collect();
        State { steamos: j.str_at("steamos").unwrap_or("").into(), checked: num("checked"), tried: num("tried"), boost_before, sink_before: j.str_at("sinkBefore").unwrap_or("").into() }
    }
    fn save(&self, data: &Path) {
        let j = obj! {"steamos": s(&self.steamos), "checked": n(self.checked as f64), "tried": n(self.tried as f64), "boostBefore": arr(self.boost_before.iter().map(|(p, v)| obj! {"path": s(p), "value": s(v)})), "sinkBefore": s(&self.sink_before)};
        let _ = crate::write_atomic(&dir(data).join("state.json"), (j.to_text() + "\n").as_bytes());
    }
}

/// Something for the app's side to run. Everything else the background helper does, it does to its own files.
#[derive(Debug, Clone, PartialEq)]
pub enum Do {
    /// A command on the host whose answer isn't needed (the helper's set-now, the sound output).
    Host(Vec<String>),
    /// Snapshot this game's saves with `take_snapshot`, then tell `snapshot_taken`.
    Snapshot { game: String, steam_id: String, name: String },
    /// Look for updates, then tell `updates_found`.
    CheckUpdates { test_builds: bool },
    /// Run the helper's `verify`, then tell `verified`.
    Verify,
    /// Ask the host for its sound outputs and the one in use, then tell `sinks_listed`.
    Sinks,
}

/// What an update check found. A part that couldn't be checked is None and keeps what was known.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    /// The newer Turbine Hub's version, or "" when this one is the newest.
    pub update: Option<String>,
    pub apps: Option<Vec<String>>,
    pub plugins: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
struct Running {
    steam_id: String,
    game: String,
    name: String,
    since: u64,
    /// The charge when it started.
    pct: Option<u32>,
    on_battery: bool,
    /// Seconds asleep with the game open, and the charge those sleeps used.
    asleep: u64,
    asleep_pct: u32,
}

pub struct Agent {
    pub paths: Paths,
    state: State,
    report: Report,
    running: Option<Running>,
    /// The last battery reading and when.
    last: Option<(u64, Readings)>,
    last_sample: u64,
    wake_counts: Vec<(String, String, u64)>,
    handheld: bool,
    docked: bool,
    sound_wanted: bool,
    sink_tries: u32,
    checking: bool,
    verifying: bool,
    verify_after: u64,
    boost_problem_told: bool,
    /// App ids running now that turned out not to be games, so they aren't looked up again at every look.
    not_games: Vec<String>,
    /// The SteamOS version being checked after an update.
    new_os: String,
    /// How many times the kernel says the machine has slept, as last read.
    suspends: Option<u64>,
}

impl Agent {
    /// Starts where the last run left off. `m` is the machine as read at the start.
    pub fn start(paths: Paths, version: &str, m: &Machine) -> Agent {
        snapshots::clear_unfinished(&paths.data);
        let state = State::load(&paths.data);
        let before = Report::load(&paths.data);
        let report = Report { at: 0, version: version.into(), game: None, ..before };
        Agent { paths, state, report, running: None, last: None, last_sample: 0, wake_counts: Vec::new(), handheld: m.kind == crate::hardware::Kind::Handheld, docked: false, sound_wanted: false, sink_tries: 0, checking: false, verifying: false, verify_after: 0, boost_problem_told: false, not_games: Vec::new(), new_os: String::new(), suspends: None }
    }

    fn note(&self, now: u64, text: &str) {
        crate::ui::note_activity(&self.paths.folder, now, text);
    }

    /// Writes the status file, so the app sees the background helper is running and what it knows.
    pub fn heartbeat(&mut self, now: u64) {
        self.report.at = now;
        self.report.game = self.running.as_ref().map(|r| (r.game.clone(), r.name.clone(), r.since));
        self.report.save(&self.paths.data);
    }

    /// The running game's id, or empty.
    pub fn game(&self) -> &str {
        self.running.as_ref().map(|r| r.game.as_str()).unwrap_or("")
    }

    /// The games Steam runs now (`ids`, from `running_games`). A game starting turns Game Boost on and snapshots
    /// its saves, when the player asked for those; a game ending puts the processor back and writes the session
    /// down. `helper` says whether the privileged helper is installed: Game Boost needs it.
    ///
    /// Game Boost is a processor setting, not a change to any game, so it is the same for every game, including
    /// the ones whose anti-cheat runs online. Snapshots only read a game's save files.
    pub fn games(&mut self, now: u64, ids: &[String], st: &Settings, helper: bool) -> Vec<Do> {
        let mut out = Vec::new();
        if let Some(r) = self.running.clone().filter(|r| !ids.contains(&r.steam_id)) {
            self.running = None;
            let after = self.last.as_ref().and_then(|(_, x)| x.battery_pct);
            let session = Session { game: r.game.clone(), name: r.name.clone(), start: r.since, end: now, asleep: r.asleep, before: r.pct.map(|p| p.saturating_sub(r.asleep_pct)), after, on_battery: r.on_battery && r.pct.is_some() };
            history::add_session(&self.paths.data, &session);
            if !r.name.is_empty() {
                let played = history::time_words(session.played());
                self.note(now, &match (r.on_battery, session.before, after) {
                    (true, Some(b), Some(a)) if b > a => format!("{}: {played}, {}% of the battery", r.name, b - a),
                    _ => format!("{}: {played}", r.name),
                });
            }
            self.heartbeat(now);
        }
        self.not_games.retain(|id| ids.contains(id));
        let mut started = None;
        if self.running.is_none() {
            for id in ids {
                if self.not_games.contains(id) {
                    continue;
                }
                match identify(&self.paths.home, id) {
                    Started::Game { game, name } => {
                        started = Some((id.clone(), game, name));
                        break;
                    }
                    Started::NotAGame => self.not_games.push(id.clone()),
                }
            }
        }
        if let Some((id, game, name)) = started {
            let reading = self.last.as_ref().map(|(_, x)| x);
            self.running = Some(Running { steam_id: id.clone(), game: game.clone(), name: name.clone(), since: now, pct: reading.and_then(|x| x.battery_pct), on_battery: reading.is_some_and(|x| x.battery_pct.is_some() && !x.charging), asleep: 0, asleep_pct: 0 });
            // Only games Steam lists as installed: Ludusavi is asked by Steam's id, and anything else has none.
            if st.save_snapshots && !name.is_empty() && game.starts_with("steam:") && snapshots::due(&self.paths.data, &game, now) {
                out.push(Do::Snapshot { game, steam_id: id, name });
            }
            self.heartbeat(now);
        }
        let want = self.running.is_some() && st.game_boost && helper;
        if want && self.state.boost_before.is_empty() {
            let root = &self.paths.root;
            let read = |path: &str| fs::read_to_string(root.join(path.trim_start_matches('/'))).map(|t| t.trim().to_string()).unwrap_or_default();
            let (on, off) = (modules::game_boost(root, true), modules::game_boost(root, false));
            // What each processor group is set to now goes back at the end; a value the helper wouldn't take back
            // becomes the usual one.
            self.state.boost_before = off.into_iter().map(|(path, usual)| {
                let now_value = read(&path);
                let keep = ["default", "performance", "balance_performance", "balance_power", "power"].contains(&now_value.as_str());
                (path, if keep { now_value } else { usual })
            }).collect();
            if !on.is_empty() {
                self.boost_problem_told = false;
                self.state.save(&self.paths.data);
                out.push(Do::Host(helper::set_now_many(&on)));
                let name = self.running.as_ref().map(|r| r.name.clone()).filter(|x| !x.is_empty());
                self.note(now, &match name {
                    Some(name) => format!("Game Boost on for {name}"),
                    None => "Game Boost on".into(),
                });
            }
        } else if !want && !self.state.boost_before.is_empty() {
            let before = std::mem::take(&mut self.state.boost_before);
            self.state.save(&self.paths.data);
            if helper {
                out.push(Do::Host(helper::set_now_many(&before)));
            }
        }
        out
    }

    /// A `Do::Host` command through the privileged helper didn't work (`why` in plain words). Said once per game
    /// in the activity list, so "Game Boost on" isn't left standing when it never took effect.
    pub fn host_failed(&mut self, now: u64, line: &[String], why: &str) {
        if line.iter().any(|a| a == "set-now") && !self.boost_problem_told {
            self.boost_problem_told = true;
            self.note(now, &format!("Game Boost couldn't be set: {why}"));
        }
    }

    /// A battery reading, every few seconds. Once a minute it goes into the history.
    pub fn battery(&mut self, now: u64, r: &Readings, wake_counts: Vec<(String, String, u64)>) {
        if let Some(run) = &mut self.running {
            run.on_battery &= r.battery_pct.is_some() && !r.charging;
        }
        if let Some(pct) = r.battery_pct
            && now.saturating_sub(self.last_sample) >= SAMPLE_SECS
        {
            self.last_sample = now;
            history::add_sample(&self.paths.data, &Sample { at: now, pct, watts: r.battery_w, charging: r.charging, game: self.game().to_string() });
        }
        self.last = Some((now, r.clone()));
        self.wake_counts = wake_counts;
    }

    /// The machine just woke from a sleep of `secs` seconds. `r` and `wake_counts` are read after waking; what was
    /// read before the sleep is what `battery` was last given.
    pub fn slept(&mut self, now: u64, secs: u64, r: &Readings, wake_counts: Vec<(String, String, u64)>) {
        let before = self.last.as_ref().and_then(|(_, x)| x.battery_pct);
        // A game left open over the sleep: the sleep isn't play, and what it used isn't the game's.
        if let Some(run) = &mut self.running {
            run.asleep += secs;
            match (before, r.battery_pct) {
                (Some(b), Some(a)) if !r.charging && a <= b => run.asleep_pct += b - a,
                _ => run.on_battery = false,
            }
        }
        let mut wakers: Vec<String> = wake_counts.iter().filter(|(path, _, count)| self.wake_counts.iter().any(|(p, _, c)| p == path && count > c)).map(|(_, name, _)| name.clone()).collect();
        wakers.dedup();
        if let (Some(before), Some(after)) = (before, r.battery_pct) {
            let sleep = Sleep { at: now, secs, before, after, charging: r.charging || after > before, wakers };
            self.note(now, &format!("Slept {}: {}", history::time_words(secs), sleep.words()));
            history::add_sleep(&self.paths.data, &sleep);
        }
        self.last_sample = 0;
        self.battery(now, r, wake_counts);
    }

    /// The clock jumped ahead as it does over a sleep. `count` is how many times the kernel says the machine has
    /// slept (`suspend_count`), when that can be read: a jump without a new sleep in the count is only the clock
    /// being set, and isn't written down as a sleep. Called once at the start too, to learn the count.
    pub fn woke(&mut self, count: Option<u64>) -> bool {
        let before = self.suspends;
        self.suspends = count.or(before);
        !matches!((before, count), (Some(a), Some(b)) if a == b)
    }

    /// Whether a screen other than the built-in one is connected, looked at every few seconds. On a handheld that
    /// is the dock: sound goes to the TV when the player asked for it, and back when the screen is gone. (The
    /// lights going dark while docked is part of every lighting frame, see `scene`.)
    pub fn screens(&mut self, now: u64, external: bool, l: &Lighting) -> Vec<Do> {
        let docked = self.handheld && external;
        let wanted = docked && l.tv_sound_when_docked;
        let mut out = Vec::new();
        if wanted && (!self.sound_wanted || !self.docked) {
            self.sink_tries = SINK_TRIES;
        }
        if !wanted && !self.state.sink_before.is_empty() {
            let before = std::mem::take(&mut self.state.sink_before);
            self.state.save(&self.paths.data);
            out.push(Do::Host(host(&["pactl", "set-default-sink", &before])));
            self.note(now, "Sound is back on this machine's own output");
        }
        self.docked = docked;
        self.sound_wanted = wanted;
        if wanted && self.state.sink_before.is_empty() && self.sink_tries > 0 {
            self.sink_tries -= 1;
            out.push(Do::Sinks);
        }
        out
    }

    /// The host's sound outputs (`pactl list sinks`) and the one in use (`pactl get-default-sink`), after `Do::Sinks`.
    pub fn sinks_listed(&mut self, now: u64, list: &str, default: &str) -> Vec<Do> {
        let default = default.trim();
        if !self.sound_wanted || !self.state.sink_before.is_empty() || default.is_empty() {
            return Vec::new();
        }
        let all = sinks(list);
        let Some((name, words)) = screen_sink(&all) else { return Vec::new() };
        self.sink_tries = 0;
        if name == default {
            return Vec::new();
        }
        self.state.sink_before = default.to_string();
        self.state.save(&self.paths.data);
        self.note(now, &format!("Docked: sound goes to {words}"));
        vec![Do::Host(host(&["pactl", "set-default-sink", name]))]
    }

    /// Once a day, and never while a game runs: look for updates. A check that failed is tried again in an hour.
    pub fn daily(&mut self, now: u64, st: &Settings) -> Vec<Do> {
        let due = |last: u64, every: u64| now < last || now - last >= every;
        if self.running.is_some() || self.checking || !due(self.state.checked, DAY) || !due(self.state.tried, 3600) {
            return Vec::new();
        }
        self.checking = true;
        self.state.tried = now;
        self.state.save(&self.paths.data);
        vec![Do::CheckUpdates { test_builds: st.test_builds }]
    }

    /// The answer to `Do::CheckUpdates`. The app shows what is found; nothing is ever installed from here.
    pub fn updates_found(&mut self, now: u64, found: Found) {
        self.checking = false;
        if found == Found::default() {
            return;
        }
        if let Some(v) = found.update {
            if !v.is_empty() && self.report.update.as_deref() != Some(v.as_str()) {
                self.note(now, &format!("Turbine Hub {v} is out"));
            }
            self.report.update = Some(v);
        }
        let before = self.report.updates.len() + self.report.plugins.len();
        if let Some(v) = found.apps {
            self.report.updates = v;
        }
        if let Some(v) = found.plugins {
            self.report.plugins = v;
        }
        let ready = self.report.updates.len() + self.report.plugins.len();
        if ready > 0 && ready != before {
            self.note(now, &if ready == 1 { "1 update ready".to_string() } else { format!("{ready} updates ready") });
        }
        self.state.checked = now;
        self.state.save(&self.paths.data);
        self.report.checked = now;
        self.heartbeat(now);
    }

    /// The SteamOS version now (`steamos`). When it changed since the last look, the helper's record is checked
    /// against the machine.
    pub fn steamos(&mut self, now: u64, version: &str, helper: bool) -> Vec<Do> {
        if version.is_empty() || version == self.state.steamos || self.verifying || now < self.verify_after {
            return Vec::new();
        }
        if self.state.steamos.is_empty() || !helper {
            if !self.state.steamos.is_empty() {
                self.note(now, &format!("SteamOS updated to {version}"));
            }
            self.state.steamos = version.to_string();
            self.state.save(&self.paths.data);
            return Vec::new();
        }
        self.verifying = true;
        self.new_os = version.to_string();
        vec![Do::Verify]
    }

    /// The answer to `Do::Verify`: what differs between the helper's record and the machine, or None when the
    /// helper couldn't be asked (it is asked again in ten minutes).
    pub fn verified(&mut self, now: u64, drift: Option<Vec<String>>) {
        self.verifying = false;
        let Some(drift) = drift else {
            self.verify_after = now + 600;
            return;
        };
        save_drift(&self.paths.data, now, &self.new_os, &drift);
        self.note(now, &match drift.len() {
            0 => format!("SteamOS updated to {}: every setting is still in place", self.new_os),
            1 => format!("SteamOS updated to {}: 1 setting needs putting back", self.new_os),
            c => format!("SteamOS updated to {}: {c} settings need putting back", self.new_os),
        });
        self.state.steamos = std::mem::take(&mut self.new_os);
        self.state.save(&self.paths.data);
    }

    /// The answer to `Do::Snapshot`.
    pub fn snapshot_taken(&mut self, now: u64, name: &str, result: &Result<Option<Snapshot>, String>) {
        match result {
            Ok(Some(_)) => self.note(now, &format!("Saves of {name} snapshotted")),
            Ok(None) => {}
            Err(why) => self.note(now, &format!("Couldn't snapshot the saves of {name}: {why}")),
        }
    }

    /// What the lights should show now, for `lights::frame`.
    pub fn scene<'a>(&'a self, l: &'a Lighting) -> Scene<'a> {
        let reading = self.last.as_ref().map(|(_, x)| x);
        Scene { lighting: l, game: self.game(), battery: reading.and_then(|x| x.battery_pct), on_battery: reading.is_some_and(|x| x.battery_pct.is_some() && !x.charging), charging: reading.is_some_and(|x| x.charging), docked: self.docked }
    }
}

/// Runs one command on the host: its exit code and what it printed, or None when it couldn't be started.
pub type Runner<'a> = dyn FnMut(&[String]) -> Option<(i32, String)> + 'a;

/// Takes a launch snapshot of one Steam game's saves: asks Ludusavi for its name of the game, backs the saves up
/// into a new snapshot folder and completes it. Ok(None) when Ludusavi isn't installed or found nothing to save.
pub fn take_snapshot(data: &Path, game: &str, steam_id: &str, name: &str, now: u64, run: &mut Runner) -> Result<Option<Snapshot>, String> {
    let installed = run(&host(&["flatpak", "info", snapshots::LUDUSAVI])).is_some_and(|(code, _)| code == 0);
    if !installed {
        return Ok(None);
    }
    let title = snapshots::find_args(steam_id).and_then(|a| run(&a)).and_then(|(_, out)| snapshots::title_from_find(&out)).unwrap_or_else(|| name.to_string());
    let dir = snapshots::begin(data, game, now);
    let args = snapshots::backup_args(&dir, &title).ok_or("The game's name can't be given to Ludusavi.")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Couldn't make the snapshot folder: {e}"))?;
    let Some((_, out)) = run(&args) else {
        let _ = fs::remove_dir_all(&dir);
        return Err("Ludusavi couldn't be started.".into());
    };
    snapshots::finish(data, game, Kind::Launch, now, &dir, &out, snapshots::KEEP)
}

/// A made-up machine in the middle of a game, under `dir`, for trying the background helper without a real one
/// (`turbinehub --agent --once --machine=handheld`, which the build's checks run): Game Boost and the save time
/// machine are on, the privileged helper is there, the lights are set to an effect and open to write, an outside
/// screen is connected and Hades II is running. Answers the paths and the config folder the settings are in.
pub fn sample(dir: &Path, preset: crate::fake::Preset) -> (Paths, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::remove_dir_all(dir);
    let config_home = dir.join("config");
    let p = Paths { root: dir.join("root"), home: dir.join("home"), folder: config_home.join("turbinehub"), data: dir.join("data") };
    crate::fake::machine(&p.root, preset);
    let put = |path: PathBuf, text: &str| {
        if let Some(d) = path.parent() {
            let _ = fs::create_dir_all(d);
        }
        let _ = fs::write(path, text);
    };
    let root = |rel: &str| p.root.join(rel);
    put(root("proc/4242/cmdline"), "/home/deck/.local/share/Steam/ubuntu12_32/reaper\0SteamLaunch\0AppId=1145360\0--\0game\0");
    put(root(helper::BIN.trim_start_matches('/')), "");
    put(root(SUSPEND_COUNT.trim_start_matches('/')), "3\n");
    for led in fs::read_dir(root("sys/class/leds")).into_iter().flatten().flatten() {
        for f in ["multi_intensity", "brightness"] {
            let _ = fs::set_permissions(led.path().join(f), fs::Permissions::from_mode(0o666));
        }
    }
    if preset == crate::fake::Preset::Handheld {
        put(root("sys/class/drm/card1-DP-1/status"), "connected\n");
        put(root("sys/class/power_supply/BAT0/status"), "Discharging\n");
        put(root("sys/class/power_supply/BAT0/power_now"), "15000000\n");
        put(root("sys/class/power_supply/BAT0/energy_now"), "40000000\n");
    }
    let steam = p.home.join(".local/share/Steam");
    put(steam.join("steamapps/libraryfolders.vdf"), &format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n}}\n", steam.display()));
    put(steam.join("steamapps/appmanifest_1145360.acf"), "\"AppState\"\n{\n\t\"appid\"\t\t\"1145360\"\n\t\"name\"\t\t\"Hades II\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"Hades II\"\n}\n");
    put(steam.join("steamapps/common/Hades II/game.exe"), "MZ");
    let _ = Settings { game_boost: true, save_snapshots: true, ..Settings::default() }.save(&config_home);
    Lighting { effect: "Wave".into(), brightness: 80, tv_sound_when_docked: true, ..Lighting::default() }.save(&p.folder);
    (p, config_home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{self, Preset};

    struct Fixture {
        dir: PathBuf,
        agent: Agent,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn w(p: PathBuf, text: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn paths(dir: &Path) -> Paths {
        Paths { root: dir.join("root"), home: dir.join("home"), folder: dir.join("config/turbinehub"), data: dir.join("data") }
    }

    fn machine(p: &Paths) -> Machine {
        Machine::read(&p.root, "", "")
    }

    fn fixture(tag: &str, preset: Preset) -> Fixture {
        let dir = std::env::temp_dir().join(format!("turbinehub-agent-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let p = paths(&dir);
        fake::machine(&p.root, preset);
        let steam = p.home.join(".local/share/Steam");
        w(steam.join("steamapps/libraryfolders.vdf"), &format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n}}\n", steam.display()));
        w(steam.join("steamapps/appmanifest_1145360.acf"), "\"AppState\"\n{\n\t\"appid\"\t\t\"1145360\"\n\t\"name\"\t\t\"Hades II\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"Hades II\"\n}\n");
        w(steam.join("steamapps/common/Hades II/game.exe"), "MZ");
        let m = machine(&p);
        Fixture { agent: Agent::start(p, "0.1", &m), dir }
    }

    fn reading(pct: u32, charging: bool) -> Readings {
        Readings { battery_pct: Some(pct), battery_w: (!charging).then_some(15.0), charging, ..Readings::default() }
    }

    fn activity(a: &Agent) -> Vec<String> {
        crate::ui::read_activity(&a.paths.folder, 20).into_iter().map(|(_, words)| words).collect()
    }

    const REAPER: &str = "4242 /home/deck/.local/share/Steam/ubuntu12_32/reaper SteamLaunch AppId=1145360 -- /home/deck/.local/share/Steam/ubuntu12_32/steam-launch-wrapper -- proton waitforexitandrun Hades2.exe\n";

    #[test]
    fn the_made_up_machine_gives_the_background_helper_everything_to_do() {
        let dir = std::env::temp_dir().join(format!("turbinehub-agent-sample-{}", std::process::id()));
        let (p, config_home) = sample(&dir, Preset::Handheld);
        let m = machine(&p);
        let st = Settings::load(&config_home);
        let l = Lighting::load(&p.folder);
        let mut a = Agent::start(p.clone(), "0.1", &m);
        a.battery(100, &crate::sensors::read(&p.root), wake_counts(&p.root, &m));
        let ids = running_games(&processes(&p.root));
        let start = a.games(100, &ids, &st, helper::installed(&p.root));
        assert!(matches!(&start[0], Do::Snapshot { name, .. } if name == "Hades II"));
        assert_eq!(start.iter().filter(|d| matches!(d, Do::Host(line) if line.iter().any(|x| x == "set-now"))).count(), 1);
        assert_eq!(a.screens(100, external_screen(&p.root), &l), vec![Do::Sinks]);
        let leds = crate::lights::leds(&p.root, &m.rgb_leds);
        assert!(!leds.is_empty() && leds.iter().all(|led| crate::lights::writable(&p.root, led)));
        let scene = a.scene(&l);
        assert!(scene.docked && scene.on_battery && scene.game == "steam:1145360");
        assert_eq!(crate::lights::frame_ms(&scene), 125);
        assert_eq!(fs::read_to_string(p.root.join(SUSPEND_COUNT.trim_start_matches('/'))).ok().as_deref().and_then(suspend_count), Some(3));
        // A desktop gets the game and Game Boost too, and no dock.
        let (p, _) = sample(&dir, Preset::Desktop);
        let mut a = Agent::start(p.clone(), "0.1", &machine(&p));
        assert_eq!(a.games(100, &running_games(&processes(&p.root)), &st, true).len(), 2);
        assert_eq!(a.screens(100, external_screen(&p.root), &l), vec![]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_unit_and_its_commands_are_fixed() {
        let unit = unit_text();
        assert!(unit.contains("ExecStart=/usr/bin/flatpak run --command=turbinehub io.github.ravenor907.TurbineHub --agent\n"));
        assert!(unit.contains("[Install]\nWantedBy=default.target\n") && unit.contains("Restart=on-failure"));
        let home = Path::new("/home/deck");
        let on = turn_on(home);
        assert_eq!(on[0], Step::Host(host(&["mkdir", "-p", "/home/deck/.config/systemd/user"])));
        assert_eq!(on[1], Step::HostWrite { path: "/home/deck/.config/systemd/user/turbinehub-agent.service".into(), value: unit });
        assert_eq!(on[3], Step::Host(host(&["systemctl", "--user", "enable", "--now", "turbinehub-agent.service"])));
        let off = turn_off(home);
        assert_eq!(off[0], Step::Host(host(&["systemctl", "--user", "disable", "--now", "turbinehub-agent.service"])));
        assert_eq!(off[1], Step::Host(host(&["rm", "-f", "/home/deck/.config/systemd/user/turbinehub-agent.service"])));
        // Nothing is ever run as root, and nothing of Steam's is touched.
        for st in on.iter().chain(&off) {
            if let Step::Host(line) = st {
                assert!(!line.iter().any(|a| a == "sudo" || a == "pkexec" || a.contains("steam")), "{line:?}");
            }
        }
        assert!(unit_enabled("enabled\n") && !unit_enabled("disabled\n") && !unit_enabled(""));
        assert_eq!(restart()[2..], ["systemctl", "--user", "try-restart", "turbinehub-agent.service"]);
    }

    #[test]
    fn finds_running_games_in_a_process_list() {
        assert_eq!(running_games(REAPER), vec!["1145360"]);
        let many = format!("{REAPER}77 /usr/bin/reaper SteamLaunch AppId=1145360 -- x\n78 reaper SteamLaunch AppId=2379780 -- y\n");
        assert_eq!(running_games(&many), vec!["1145360", "2379780"]);
        // The question itself, a game's own arguments and odd ids aren't games.
        for not in ["99 flatpak-spawn --host pgrep -af reaper SteamLaunch AppId=", "5 /bin/game SteamLaunch AppId=12", "6 reaper SteamLaunch AppId=0 -- x", "7 reaper SteamLaunch AppId=12;rm -- x", "8 steam -- reaper", ""] {
            assert!(running_games(not).is_empty(), "{not}");
        }
        let f = fixture("proc", Preset::Desktop);
        w(f.agent.paths.root.join("proc/4242/cmdline"), "/x/reaper\0SteamLaunch\0AppId=1145360\0--\0game\0");
        w(f.agent.paths.root.join("proc/1/cmdline"), "init\0");
        w(f.agent.paths.root.join("proc/self/cmdline"), "reaper\0SteamLaunch\0AppId=5\0");
        assert_eq!(running_games(&processes(&f.agent.paths.root)), vec!["1145360"]);
    }

    #[test]
    fn opening_turbine_hub_or_a_tile_from_steam_is_not_playing() {
        use crate::shortcuts::{NewShortcut, add_shortcut};
        let mut f = fixture("notgame", Preset::Handheld);
        let a = &mut f.agent;
        let file = a.paths.home.join(".local/share/Steam/userdata/1/config/shortcuts.vdf");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let add = |name: &str, exe: &str, options: &str| {
            let s = NewShortcut { name: name.into(), exe: exe.into(), start_dir: "\"/usr/bin/\"".into(), launch_options: options.into(), flatpak_id: String::new(), icon: String::new() };
            add_shortcut(&file, &s, "no marker", crate::shortcuts::Steam::Closed).unwrap();
            s.appid().to_string()
        };
        let hub = add("Turbine Hub", "\"/usr/bin/flatpak\"", "run io.github.ravenor907.TurbineHub");
        let tile = add("Netflix", "\"/usr/bin/flatpak\"", "run --command=/app/bin/chrome com.google.Chrome --kiosk \"https://www.netflix.com\"");
        let mods = add("Turbine", "\"/usr/bin/flatpak\"", "run io.github.ravenor907.TurbineModManager");
        let heron = add("Heron's Flight", "\"/home/deck/Games/heron/run.sh\"", "");
        for id in [&hub, &tile, &mods] {
            assert_eq!(identify(&a.paths.home, id), Started::NotAGame, "{id}");
        }
        assert_eq!(identify(&a.paths.home, &heron), Started::Game { game: format!("shortcut:{heron}"), name: "Heron's Flight".into() });
        // Steam also writes a non-Steam entry's id as the top half of a longer number.
        let long = ((heron.parse::<u64>().unwrap() << 32) | 0x0200_0000).to_string();
        assert!(matches!(identify(&a.paths.home, &long), Started::Game { name, .. } if name == "Heron's Flight"));
        assert_eq!(identify(&a.paths.home, "1145360"), Started::Game { game: "steam:1145360".into(), name: "Hades II".into() });
        assert_eq!(identify(&a.paths.home, "424242"), Started::Game { game: "steam:424242".into(), name: String::new() });
        // Turbine Hub open from Steam: no Game Boost, no session, nothing in the status.
        let on = Settings { game_boost: true, save_snapshots: true, ..Settings::default() };
        assert_eq!(a.games(10, std::slice::from_ref(&hub), &on, true), vec![]);
        assert_eq!((a.game(), Report::load(&a.paths.data).game), ("", None));
        // A game started while Turbine Hub stays open is found behind it; a non-Steam game gets Game Boost but no
        // snapshot (Ludusavi is asked by Steam's id).
        let both = [hub.clone(), heron.clone()];
        assert_eq!(a.games(14, &both, &on, true).len(), 1);
        assert_eq!(a.game(), format!("shortcut:{heron}"));
        assert_eq!(a.games(100, std::slice::from_ref(&hub), &on, true).len(), 1);
        assert_eq!(a.game(), "");
        assert_eq!(history::sessions(&a.paths.data)[0].name, "Heron's Flight");
    }

    #[test]
    fn game_boost_goes_on_at_the_start_and_back_at_the_end() {
        let mut f = fixture("boost", Preset::Handheld);
        let a = &mut f.agent;
        let on = Settings { game_boost: true, ..Settings::default() };
        w(a.paths.root.join("sys/devices/system/cpu/cpufreq/policy1/energy_performance_preference"), "balance_power\n");
        // Without the privileged helper nothing can be set, so nothing is tried.
        assert_eq!(a.games(100, &["1145360".into()], &on, false), vec![]);
        let start = a.games(104, &["1145360".into()], &on, true);
        let epp = |n: u32, v: &str| (format!("/sys/devices/system/cpu/cpufreq/policy{n}/energy_performance_preference"), v.to_string());
        assert_eq!(start, vec![Do::Host(helper::set_now_many(&[epp(0, "performance"), epp(1, "performance")]))]);
        // Still running: nothing more to do.
        assert_eq!(a.games(108, &["1145360".into()], &on, true), vec![]);
        assert_eq!(Report::load(&a.paths.data).game, Some(("steam:1145360".into(), "Hades II".into(), 100)));
        // A second run of the background helper picks the game up where the first left it, and still knows what to
        // put back.
        let p = a.paths.clone();
        let mut again = Agent::start(p.clone(), "0.1", &machine(&p));
        assert_eq!(again.games(4000, &["1145360".into()], &on, true), vec![]);
        let end = again.games(4004, &[], &on, true);
        assert_eq!(end, vec![Do::Host(helper::set_now_many(&[epp(0, "balance_performance"), epp(1, "balance_power")]))]);
        assert_eq!(again.games(4008, &[], &on, true), vec![]);
        assert_eq!(Report::load(&p.data).game, None);
        assert!(activity(&again).iter().any(|l| l == "Game Boost on for Hades II"));
        // The helper refusing is said once, not once per processor group.
        let line = helper::set_now("/sys/x", "performance");
        again.host_failed(4005, &line, "Turbine Hub's helper isn't set up yet.");
        again.host_failed(4006, &line, "Turbine Hub's helper isn't set up yet.");
        again.host_failed(4007, &host(&["pactl", "set-default-sink", "x"]), "no such sink");
        assert_eq!(activity(&again).iter().filter(|l| l.starts_with("Game Boost couldn't be set")).count(), 1);
        // Turning the setting off in the middle of a game puts things back right away.
        again.games(5000, &["1145360".into()], &on, true);
        assert_eq!(again.games(5004, &["1145360".into()], &Settings::default(), true).len(), 1);
        assert_eq!(again.games(5008, &["1145360".into()], &Settings::default(), true), vec![]);
        // Off: a game changes nothing.
        let mut f = fixture("boost-off", Preset::Desktop);
        assert_eq!(f.agent.games(1, &["1145360".into()], &Settings::default(), true), vec![]);
    }

    #[test]
    fn a_game_start_snapshots_saves_when_the_player_asked() {
        let mut f = fixture("snap", Preset::Handheld);
        let a = &mut f.agent;
        let st = Settings { save_snapshots: true, ..Settings::default() };
        assert_eq!(a.games(1000, &["1145360".into()], &st, false), vec![Do::Snapshot { game: "steam:1145360".into(), steam_id: "1145360".into(), name: "Hades II".into() }]);
        assert_eq!(a.games(1004, &["1145360".into()], &st, false), vec![]);
        a.games(1100, &[], &st, false);
        // Something Steam runs that isn't an installed game has no saves to keep.
        assert_eq!(a.games(1200, &["228980".into()], &st, false), vec![]);
        assert_eq!(a.game(), "steam:228980");
        a.games(1300, &[], &st, false);
        // The app's side runs Ludusavi; here a stand-in does what Ludusavi does.
        let data = a.paths.data.clone();
        let mut asked = Vec::new();
        let mut ludusavi = |line: &[String]| {
            asked.push(line[2..].join(" "));
            Some(match line.iter().map(String::as_str).collect::<Vec<_>>()[..] {
                [.., "flatpak", "info", _] => (0, String::new()),
                [.., "find", "--api", "--steam-id", _] => (0, r#"{"games":{"Hades II":{}}}"#.to_string()),
                [.., "--path", dir, "--", title] => {
                    w(Path::new(dir).join(title).join("drive-0/save.sav"), "progress");
                    (0, format!(r#"{{"games":{{"{title}":{{"files":{{"/saves/save.sav":{{"bytes":8}}}}}}}}}}"#))
                }
                _ => (2, String::new()),
            })
        };
        let taken = take_snapshot(&data, "steam:1145360", "1145360", "Hades 2", 1000, &mut ludusavi);
        assert_eq!(asked, vec!["flatpak info com.github.mtkennerly.ludusavi".to_string(), "flatpak run com.github.mtkennerly.ludusavi find --api --steam-id 1145360".to_string(), format!("flatpak run com.github.mtkennerly.ludusavi backup --api --force --path {} -- Hades II", data.join("snapshots/steam_1145360/1000.part").display())]);
        let snap = taken.clone().unwrap().unwrap();
        assert_eq!((snap.name.as_str(), snap.bytes, snap.kind), ("Hades II", 8, Kind::Launch));
        a.snapshot_taken(1001, "Hades II", &taken);
        assert_eq!(activity(a)[0], "Saves of Hades II snapshotted");
        // The same game again within ten minutes: no second snapshot. Later: yes.
        assert_eq!(a.games(1400, &["1145360".into()], &st, false), vec![]);
        a.games(1500, &[], &st, false);
        assert_eq!(a.games(1000 + snapshots::EVERY_SECS, &["1145360".into()], &st, false).len(), 1);
        // Without Ludusavi nothing is tried and nothing is left behind.
        assert_eq!(take_snapshot(&data, "steam:1", "1", "X", 5, &mut |_| Some((1, String::new()))), Ok(None));
        assert_eq!(take_snapshot(&data, "steam:1", "1", "X", 5, &mut |_| None), Ok(None));
        assert!(!snapshots::game_dir(&data, "steam:1").exists());
    }

    #[test]
    fn sessions_and_readings_become_history() {
        let mut f = fixture("history", Preset::Handheld);
        let a = &mut f.agent;
        let st = Settings::default();
        a.battery(1000, &reading(90, false), vec![]);
        a.battery(1015, &reading(90, false), vec![]);
        a.games(1020, &["1145360".into()], &st, false);
        a.battery(1060, &reading(89, false), vec![]);
        a.battery(4600, &reading(60, false), vec![]);
        a.games(4620, &[], &st, false);
        let all = history::samples(&a.paths.data, 0);
        assert_eq!(all.iter().map(|x| (x.at, x.pct, x.game.as_str())).collect::<Vec<_>>(), vec![(1000, 90, ""), (1060, 89, "steam:1145360"), (4600, 60, "steam:1145360")]);
        assert_eq!(history::sessions(&a.paths.data), vec![Session { game: "steam:1145360".into(), name: "Hades II".into(), start: 1020, end: 4620, asleep: 0, before: Some(90), after: Some(60), on_battery: true }]);
        assert_eq!(activity(a)[0], "Hades II: 1 h, 30% of the battery");
        // Plugged in part of the way: the session is kept but says nothing about battery use.
        a.games(5000, &["1145360".into()], &st, false);
        a.battery(5100, &reading(61, true), vec![]);
        a.battery(5200, &reading(60, false), vec![]);
        a.games(6000, &[], &st, false);
        assert!(!history::sessions(&a.paths.data)[1].on_battery);
        assert_eq!(history::use_by_game(&history::sessions(&a.paths.data)).len(), 1);
        assert_eq!(a.scene(&Lighting::default()).battery, Some(60));
        // The handheld put to sleep for the night with the game open: the night isn't play, and what it used isn't
        // the game's.
        a.battery(10_000, &reading(80, false), vec![]);
        a.games(10_000, &["1145360".into()], &st, false);
        a.battery(13_600, &reading(60, false), vec![]);
        a.slept(13_600 + 8 * 3600, 8 * 3600, &reading(56, false), vec![]);
        a.battery(13_600 + 9 * 3600, &reading(40, false), vec![]);
        a.games(13_600 + 9 * 3600, &[], &st, false);
        let last = history::sessions(&a.paths.data).pop().unwrap();
        assert_eq!((last.played(), last.before, last.after, last.on_battery), (7200, Some(76), Some(40), true));
        assert_eq!(activity(a)[0], "Hades II: 2 h, 36% of the battery");
        // Charged while asleep: nothing can be said about what the game used.
        a.games(100_000, &["1145360".into()], &st, false);
        a.slept(130_000, 20_000, &reading(100, true), vec![]);
        a.games(131_000, &[], &st, false);
        assert!(!history::sessions(&a.paths.data).pop().unwrap().on_battery);
    }

    #[test]
    fn a_sleep_is_written_down_with_what_woke_the_machine() {
        let mut f = fixture("sleep", Preset::Handheld);
        let a = &mut f.agent;
        let m = machine(&a.paths);
        w(a.paths.root.join("sys/bus/usb/devices/3-1/power/wakeup_count"), "4\n");
        let before = wake_counts(&a.paths.root, &m);
        assert_eq!(before, vec![("/sys/bus/usb/devices/3-1/power/wakeup".to_string(), "Controller".to_string(), 4)]);
        a.battery(1000, &reading(80, false), before);
        w(a.paths.root.join("sys/bus/usb/devices/3-1/power/wakeup_count"), "5\n");
        a.slept(1000 + 8 * 3600, 8 * 3600, &reading(40, false), wake_counts(&a.paths.root, &m));
        assert_eq!(history::last_sleep(&a.paths.data), Some((5.0, vec!["Controller".to_string()])));
        assert_eq!(activity(a)[0], "Slept 8 h: 40% used, 5.0% an hour \u{b7} woken by Controller");
        // Woken by the power button (no device counted a wake), on the charger.
        a.slept(100_000, 3600, &reading(55, true), wake_counts(&a.paths.root, &m));
        let last = &history::sleeps(&a.paths.data, 1)[0];
        assert!(last.charging && last.wakers.is_empty());
        assert_eq!(history::last_sleep(&a.paths.data).unwrap().0, 5.0);
        // The clock being set looks like a sleep from the inside; the kernel's count of sleeps tells them apart.
        assert_eq!(suspend_count("12\n"), Some(12));
        assert!(a.woke(suspend_count("12\n")));
        assert!(!a.woke(Some(12)));
        assert!(a.woke(Some(13)));
        assert!(a.woke(None));
        assert!(!a.woke(Some(13)));
        // A desktop has no battery: nothing to write down.
        let mut f = fixture("sleep-pc", Preset::Desktop);
        f.agent.battery(10, &Readings::default(), vec![]);
        f.agent.slept(5000, 4000, &Readings::default(), vec![]);
        assert!(history::sleeps(&f.agent.paths.data, 5).is_empty());
    }

    const SINKS: &str = "Sink #56\n\tName: alsa_output.pci-0000_c4_00.1.hdmi-stereo\n\tDescription: Radeon HDMI Digital Stereo\nSink #71\n\tName: alsa_output.speakers\n\tDescription: Speakers\n";

    #[test]
    fn reads_sound_outputs_with_their_plain_names() {
        let text = "Sink #56\n\tState: RUNNING\n\tName: alsa_output.pci-0000_03_00.1.hdmi-stereo\n\tDescription: Navi 48 HDMI Digital Stereo\n\tDriver: PipeWire\nSink #71\n\tName: alsa_output.usb-headset.analog-stereo\n\tDescription: Arctis Nova 7\n";
        let all = sinks(text);
        assert_eq!(all, vec![("alsa_output.pci-0000_03_00.1.hdmi-stereo".to_string(), "Navi 48 HDMI Digital Stereo".to_string()), ("alsa_output.usb-headset.analog-stereo".to_string(), "Arctis Nova 7".to_string())]);
        assert_eq!(screen_sink(&all), Some(&all[0]));
        assert_eq!(screen_sink(&all[1..]), None);
        assert!(sinks("").is_empty());
    }

    #[test]
    fn docking_a_handheld_moves_sound_to_the_tv_and_back() {
        let mut f = fixture("dock", Preset::Handheld);
        let a = &mut f.agent;
        assert!(!external_screen(&a.paths.root));
        let on = Lighting { tv_sound_when_docked: true, off_when_docked: true, ..Lighting::default() };
        assert_eq!(a.screens(10, false, &on), vec![]);
        w(a.paths.root.join("sys/class/drm/card1-DP-1/status"), "connected\n");
        assert!(external_screen(&a.paths.root));
        assert_eq!(a.screens(13, true, &on), vec![Do::Sinks]);
        assert!(a.scene(&on).docked);
        // The TV's output isn't there yet: asked again at the next look, but not forever.
        assert_eq!(a.sinks_listed(14, "Sink #71\n\tName: alsa_output.speakers\n\tDescription: Speakers\n", "alsa_output.speakers\n"), vec![]);
        assert_eq!(a.screens(16, true, &on), vec![Do::Sinks]);
        assert_eq!(a.sinks_listed(17, SINKS, "alsa_output.speakers\n"), vec![Do::Host(host(&["pactl", "set-default-sink", "alsa_output.pci-0000_c4_00.1.hdmi-stereo"]))]);
        assert_eq!(a.screens(19, true, &on), vec![]);
        assert_eq!(activity(a)[0], "Docked: sound goes to Radeon HDMI Digital Stereo");
        // Undocked: back to what it was, once.
        assert_eq!(a.screens(100, false, &on), vec![Do::Host(host(&["pactl", "set-default-sink", "alsa_output.speakers"]))]);
        assert_eq!(a.screens(103, false, &on), vec![]);
        assert!(!a.scene(&on).docked);
        // Never more than a few tries, and nothing at all when the player didn't ask.
        let tries = (0..30).filter(|i| !a.screens(200 + i, true, &on).is_empty()).count();
        assert_eq!(tries, SINK_TRIES as usize);
        assert_eq!(a.screens(300, false, &on), vec![]);
        assert_eq!(a.screens(301, true, &Lighting::default()), vec![]);
        // Sound already on the TV: left alone, and nothing to put back later.
        a.screens(400, false, &on);
        a.screens(401, true, &on);
        assert_eq!(a.sinks_listed(402, SINKS, "alsa_output.pci-0000_c4_00.1.hdmi-stereo"), vec![]);
        assert_eq!(a.screens(500, false, &on), vec![]);
        // A desktop with a monitor isn't docked.
        let mut f = fixture("dock-pc", Preset::Desktop);
        assert!(external_screen(&f.agent.paths.root));
        assert_eq!(f.agent.screens(1, true, &on), vec![]);
        assert!(!f.agent.scene(&on).docked);
    }

    #[test]
    fn updates_are_looked_for_once_a_day_and_never_during_a_game() {
        let mut f = fixture("updates", Preset::Desktop);
        let a = &mut f.agent;
        let st = Settings { test_builds: true, ..Settings::default() };
        let now = 1_800_000_000;
        a.games(now, &["1145360".into()], &st, false);
        assert_eq!(a.daily(now + 5, &st), vec![]);
        a.games(now + 10, &[], &st, false);
        assert_eq!(a.daily(now + 20, &st), vec![Do::CheckUpdates { test_builds: true }]);
        assert_eq!(a.daily(now + 21, &st), vec![]);
        // Nothing could be reached: what was known stays, and the next try is in an hour.
        a.updates_found(now + 30, Found::default());
        assert_eq!(Report::load(&a.paths.data).update, None);
        assert_eq!(a.daily(now + 600, &st), vec![]);
        assert_eq!(a.daily(now + 3700, &st).len(), 1);
        a.updates_found(now + 3710, Found { update: Some("0.2".into()), apps: Some(vec!["com.heroicgameslauncher.hgl".into()]), plugins: None });
        let r = Report::load(&a.paths.data);
        assert_eq!((r.update.as_deref(), r.updates.len(), r.checked), (Some("0.2"), 1, now + 3710));
        assert!(r.alive(now + 3800) && !r.alive(now + 3710 + ALIVE_SECS + 1));
        assert_eq!(activity(a)[..2], ["1 update ready".to_string(), "Turbine Hub 0.2 is out".to_string()]);
        assert_eq!(a.daily(now + 3710 + DAY - 1, &st), vec![]);
        assert_eq!(a.daily(now + 3710 + DAY, &st).len(), 1);
        // The same findings again aren't news; Flathub being unreachable keeps the last list.
        a.updates_found(now + 2 * DAY, Found { update: Some("0.2".into()), apps: None, plugins: Some(vec![]) });
        assert_eq!(activity(a).len(), 3);
        assert_eq!(Report::load(&a.paths.data).updates.len(), 1);
        // A new run remembers when it last checked and what it found.
        let p = a.paths.clone();
        let mut again = Agent::start(p.clone(), "0.1", &machine(&p));
        assert_eq!(again.daily(now + 2 * DAY + 100, &st), vec![]);
        again.heartbeat(now + 2 * DAY + 100);
        assert_eq!(Report::load(&p.data).update.as_deref(), Some("0.2"));
    }

    #[test]
    fn decky_plugins_with_a_newer_version_are_found() {
        let f = fixture("plugins", Preset::Handheld);
        let home = &f.agent.paths.home;
        for (dir, name, version) in [("decky-css", "CSS Loader", "2.1.0"), ("protondb", "ProtonDB Badges", "1.1.0"), ("mine", "My own plugin", "0.0.1"), ("odd", "Odd", "soon")] {
            w(home.join(format!("homebrew/plugins/{dir}/plugin.json")), &format!("{{\"name\":\"{name}\"}}"));
            w(home.join(format!("homebrew/plugins/{dir}/package.json")), &format!("{{\"version\":\"{version}\"}}"));
        }
        let store = r#"[{"name":"CSS Loader","versions":[{"name":"2.1.0"},{"name":"v2.10.0"},{"name":"2.9.9"}]},{"name":"ProtonDB Badges","versions":[{"name":"1.1.0"},{"name":"1.0.9"}]},{"name":"Odd","versions":[{"name":"1.0.0"}]}]"#;
        assert_eq!(plugin_updates(home, store), vec!["CSS Loader"]);
        assert!(plugin_updates(home, "<html>").is_empty());
        assert!(plugin_updates(Path::new("/nonexistent"), store).is_empty());
    }

    #[test]
    fn a_steamos_update_gets_the_record_checked() {
        let mut f = fixture("os", Preset::Handheld);
        let a = &mut f.agent;
        assert_eq!(steamos(&a.paths.root), "3.9.5");
        // The first look only remembers the version.
        assert_eq!(a.steamos(10, "3.9.5", true), vec![]);
        assert_eq!(a.steamos(20, "3.9.5", true), vec![]);
        assert_eq!(a.steamos(30, "3.9.6", true), vec![Do::Verify]);
        assert_eq!(a.steamos(31, "3.9.6", true), vec![]);
        // The helper couldn't be asked: again in ten minutes, not at once.
        a.verified(40, None);
        assert_eq!(a.steamos(50, "3.9.6", true), vec![]);
        assert_eq!(a.steamos(650, "3.9.6", true), vec![Do::Verify]);
        a.verified(660, Some(vec!["Kernel setting vm.swappiness set to 100 isn't in effect".into()]));
        assert_eq!(drift(&a.paths.data).len(), 1);
        assert_eq!(activity(a)[0], "SteamOS updated to 3.9.6: 1 setting needs putting back");
        assert_eq!(a.steamos(700, "3.9.6", true), vec![]);
        // The app put things back and says so in the same file.
        save_drift(&a.paths.data, 800, "3.9.6", &[]);
        assert!(drift(&a.paths.data).is_empty());
        // Without the privileged helper there is no record to check.
        assert_eq!(a.steamos(900, "3.9.7", false), vec![]);
        assert_eq!(a.steamos(901, "3.9.7", true), vec![]);
        assert_eq!(a.steamos(902, "", true), vec![]);
    }
}

//! Moving over from AllyHub, once. Finds AllyHub's install, its settings and every system change it made, sorts the
//! changes into groups the player picks from (Turbine Hub takes the setting over, or it is undone), and reads
//! AllyHub's settings into Turbine Hub's. Nothing here changes anything: the app carries the plan out, the root
//! parts through the helper. After the import nothing of AllyHub's formats is read again.

use crate::helper::{Access, Setting};
use crate::json::{self, Json, quote};
use crate::settings::{Mode, Settings};
use crate::switches::{GameSwitches, Switch};
use crate::{Scan, Store};
use std::fs;
use std::path::Path;

const LEDS_RULE: &str = "/etc/udev/rules.d/99-allyhub-leds.rules";
const HID_RULE: &str = "/etc/udev/rules.d/70-allyhub-hid.rules";
const LIGHT_UNIT: &str = "/etc/systemd/system/allyhub-lighting-perms.service";
const LIGHT_SCRIPT: &str = "/etc/allyhub/lighting-perms.sh";
const CHARGE_RULE: &str = "/etc/udev/rules.d/99-allyhub-charge.rules";
const CHARGE_PERM_RULE: &str = "/etc/udev/rules.d/71-allyhub-charge-perm.rules";
const NOWAKE_RULE: &str = "/etc/udev/rules.d/71-allyhub-nowake.rules";
const NTSYNC_RULE: &str = "/etc/udev/rules.d/70-allyhub-ntsync.rules";
const NTSYNC_LOAD: &str = "/etc/modules-load.d/allyhub-ntsync.conf";
const SYSCTL: &str = "/etc/sysctl.d/99-zz-allyhub.conf";
const SYSCTL_ZRAM: &str = "/etc/sysctl.d/99-zz-allyhub-zram.conf";
const TUNEUP_TMPFILES: &str = "/etc/tmpfiles.d/allyhub-tuneup.conf";
const ZRAM_UNIT: &str = "/etc/systemd/system/allyhub-zram.service";
const ZRAM_SCRIPT: &str = "/etc/allyhub/zram.sh";
const BOOST_TMPFILES: &str = "/etc/tmpfiles.d/allyhub-boost.conf";
const DECKY_RULE: &str = "/etc/polkit-1/rules.d/49-allyhub-decky.rules";
const DECKY_MARK: &str = "/etc/allyhub/decky-restart-allowed";
const EDID_UNIT: &str = "/etc/systemd/system/allyhub-edid.service";
const EDID_SCRIPT: &str = "/etc/allyhub/edid-override.sh";
const EDID_FILE: &str = "/etc/allyhub/edid-144.bin";

/// Every root-owned file AllyHub may have written. The plan never names a system file outside this list.
pub const SYSTEM_FILES: &[&str] = &[
    LEDS_RULE, HID_RULE, LIGHT_UNIT, LIGHT_SCRIPT, CHARGE_RULE, CHARGE_PERM_RULE, NOWAKE_RULE, NTSYNC_RULE, NTSYNC_LOAD, SYSCTL, SYSCTL_ZRAM, TUNEUP_TMPFILES, ZRAM_UNIT, ZRAM_SCRIPT, BOOST_TMPFILES, DECKY_RULE, DECKY_MARK,
    EDID_UNIT, EDID_SCRIPT, EDID_FILE,
];

/// AllyHub's own folder under /etc, removed when it is empty.
pub const SYSTEM_DIR: &str = "/etc/allyhub";

/// AllyHub's quick panel, a root-owned folder in the player's home.
pub const PLUGIN_DIR: &str = "homebrew/plugins/AllyHub";

const AGENT_UNIT: &str = ".config/systemd/user/allyhub-agent.service";
const GAME_ENV: &str = ".config/environment.d/90-allyhub-games.conf";
const REFRESH_SCRIPT: &str = ".config/gamescope/scripts/90-allyhub-refresh.lua";
const CONFIG: &str = ".config/allyhub/config.json";
const APP_DIR: &str = ".local/share/allyhub";

/// AllyHub's own files and folders in the player's home (the backups in ~/AllyHub-Backups stay).
const APP_PATHS: &[&str] = &[
    AGENT_UNIT,
    APP_DIR,
    ".config/allyhub",
    ".local/share/allyhub-rescue",
    ".local/bin/allyhub",
    ".local/share/applications/allyhub.desktop",
    ".local/share/applications/allyhub-gamemode.desktop",
    ".local/share/applications/allyhub-testing-rescue.desktop",
    "Desktop/allyhub.desktop",
    "Desktop/allyhub-testing-rescue.desktop",
];

const TEMPLATES_DIR: &str = ".local/share/Steam/controller_base/templates";

/// What is on the machine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    /// AllyHub's program folder is there.
    pub app: bool,
    /// The text of AllyHub's settings file.
    pub config: Option<String>,
    pub system_files: Vec<&'static str>,
    /// Home-relative files and folders, AllyHub's own and the ones it added elsewhere.
    pub user_paths: Vec<String>,
    pub plugin: bool,
}

impl Found {
    pub fn any(&self) -> bool {
        self.app || self.config.is_some() || self.plugin || !self.system_files.is_empty() || !self.user_paths.is_empty()
    }

    fn has(&self, file: &str) -> bool {
        self.system_files.contains(&file)
    }

    fn has_user(&self, path: &str) -> bool {
        self.user_paths.iter().any(|p| p == path)
    }
}

fn under(root: &Path, abs: &str) -> std::path::PathBuf {
    root.join(abs.trim_start_matches('/'))
}

fn text(root: &Path, abs: &str) -> String {
    fs::read_to_string(under(root, abs)).unwrap_or_default()
}

/// Looks for AllyHub under `root` (the system) and `home`. Read only.
pub fn detect(root: &Path, home: &Path) -> Found {
    let mut user_paths: Vec<String> = APP_PATHS.iter().chain(&[GAME_ENV, REFRESH_SCRIPT]).filter(|p| home.join(p).symlink_metadata().is_ok()).map(|p| p.to_string()).collect();
    let mut templates: Vec<String> = fs::read_dir(home.join(TEMPLATES_DIR)).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("controller_allyhub_ally_x") && n.ends_with(".vdf")).map(|n| format!("{TEMPLATES_DIR}/{n}")).collect()).unwrap_or_default();
    templates.sort();
    user_paths.extend(templates);
    Found {
        app: home.join(APP_DIR).is_dir(),
        config: fs::read_to_string(home.join(CONFIG)).ok(),
        system_files: SYSTEM_FILES.iter().copied().filter(|f| under(root, f).symlink_metadata().is_ok()).collect(),
        user_paths,
        plugin: home.join(PLUGIN_DIR).is_dir(),
    }
}

// ---- launch options ----

/// One game whose Steam launch options carry AllyHub's switches.
#[derive(Debug, Clone, PartialEq)]
pub struct GameOptions {
    pub app_id: String,
    pub name: String,
    /// The switches AllyHub put in the launch options.
    pub switches: Vec<Switch>,
    /// The launch options without them ("" when nothing else is left).
    pub kept: String,
}

fn tokens(options: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut quote_ch) = (None, None);
    for (i, c) in options.char_indices() {
        match quote_ch {
            Some(q) if c == q => quote_ch = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => {
                quote_ch = Some(c);
                start = start.or(Some(i));
            }
            None if c.is_whitespace() => {
                if let Some(s) = start.take() {
                    out.push(&options[s..i]);
                }
            }
            None => start = start.or(Some(i)),
        }
    }
    if let Some(s) = start {
        out.push(&options[s..]);
    }
    out
}

fn switch_of(token: &str) -> Option<Switch> {
    if let Some(s) = Switch::ALL.iter().find(|s| s.env().is_some_and(|(k, v)| token == format!("{k}={v}"))) {
        return Some(*s);
    }
    let lsfg = token == "~/lsfg" || token == "$HOME/lsfg" || token.starts_with("/home/") && token.ends_with("/lsfg") && token.matches('/').count() == 3;
    lsfg.then_some(Switch::FrameGen)
}

/// Splits AllyHub's switches out of a game's launch options. Everything else stays as the player wrote it; a
/// lone `%command%` left over means no launch options at all.
pub fn split_launch_options(options: &str) -> (Vec<Switch>, String) {
    let mut switches = Vec::new();
    let mut kept = Vec::new();
    for t in tokens(options) {
        match switch_of(t) {
            Some(s) if !switches.contains(&s) => switches.push(s),
            Some(_) => {}
            None => kept.push(t),
        }
    }
    switches.sort();
    let kept = if kept == ["%command%"] { String::new() } else { kept.join(" ") };
    (switches, kept)
}

/// The installed Steam games whose launch options carry AllyHub's switches.
pub fn game_options(home: &Path, scan: &Scan) -> Vec<GameOptions> {
    let mut out = Vec::new();
    for g in scan.games.iter().filter(|g| g.store == Store::Steam) {
        let Some(options) = crate::steam::launch_options(home, &g.store_id) else { continue };
        let (switches, kept) = split_launch_options(&options);
        if !switches.is_empty() {
            out.push(GameOptions { app_id: g.store_id.clone(), name: g.name.clone(), switches, kept });
        }
    }
    out
}

// ---- groups ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Turbine Hub keeps the setting in effect through its own helper and files.
    TakeOver,
    /// The setting goes away with AllyHub.
    Undo,
}

/// One kind of change AllyHub made. Its files go either way; the choice is whether Turbine Hub keeps the effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: &'static str,
    pub title: &'static str,
    /// What is set now, in plain words.
    pub line: String,
    /// The helper settings that keep the same effect. Empty when Turbine Hub has no matching setting.
    pub take_over: Vec<Setting>,
    /// Per-game switches to move into Turbine Hub (the "games" group).
    pub games: Vec<GameOptions>,
    pub files: Vec<&'static str>,
    /// System services to turn off before their files go.
    pub units: Vec<&'static str>,
    pub user_paths: Vec<String>,
    pub plugin: bool,
    /// Parts Turbine Hub can't keep, in plain words.
    pub lost: Vec<String>,
}

impl Group {
    fn new(id: &'static str, title: &'static str) -> Group {
        Group { id, title, line: String::new(), take_over: Vec::new(), games: Vec::new(), files: Vec::new(), units: Vec::new(), user_paths: Vec::new(), plugin: false, lost: Vec::new() }
    }

    pub fn can_take_over(&self) -> bool {
        !self.take_over.is_empty() || !self.games.is_empty()
    }

    /// Take over where there is something to take over.
    pub fn default_choice(&self) -> Choice {
        if self.can_take_over() { Choice::TakeOver } else { Choice::Undo }
    }

    fn with_files(mut self, found: &Found, files: &[&'static str]) -> Group {
        self.files = files.iter().copied().filter(|f| found.has(f)).collect();
        self
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty() && self.user_paths.is_empty() && self.games.is_empty() && !self.plugin
    }

    fn keep(&mut self, s: Setting, lost: &str) {
        if s.check().is_ok() {
            if !self.take_over.contains(&s) {
                self.take_over.push(s);
            }
        } else {
            self.lost.push(lost.to_string());
        }
    }
}

fn unit_name(file: &'static str) -> &'static str {
    file.rsplit('/').next().unwrap_or(file)
}

fn config_json(found: &Found) -> Json {
    found.config.as_deref().and_then(|t| json::parse(t).ok()).unwrap_or(Json::Null)
}

fn at<'a>(j: &'a Json, path: &[&str]) -> Option<&'a Json> {
    path.iter().try_fold(j, |j, k| j.get(k))
}

/// A whole number that AllyHub may have saved as a number or as text.
fn number(j: Option<&Json>) -> Option<u64> {
    let j = j?;
    j.as_f64().filter(|v| v.fract() == 0.0 && *v >= 0.0).map(|v| v as u64).or_else(|| j.as_str()?.trim().trim_end_matches('%').parse().ok())
}

/// The value between the quotes after `key` on a rule line: `key=="value"` or `key="value"`.
fn rule_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = &line[line.find(key)? + key.len()..];
    let rest = rest.trim_start_matches('=').strip_prefix('"')?;
    rest.split('"').next()
}

fn charge_group(found: &Found, root: &Path) -> Group {
    let mut g = Group::new("battery", "Charge limit").with_files(found, &[CHARGE_RULE, CHARGE_PERM_RULE]);
    if g.files.is_empty() {
        return g;
    }
    let ps = under(root, "/sys/class/power_supply");
    let mut batteries: Vec<String> = fs::read_dir(&ps).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| ps.join(n).join("charge_control_end_threshold").exists()).collect()).unwrap_or_default();
    batteries.sort();
    let saved = number(at(&config_json(found), &["battery", "limit"]));
    let ruled = text(root, CHARGE_RULE).lines().find_map(|l| rule_value(l, "ATTR{charge_control_end_threshold}")?.parse::<u64>().ok());
    let live = batteries.first().and_then(|b| text(root, &format!("/sys/class/power_supply/{b}/charge_control_end_threshold")).trim().parse::<u64>().ok());
    match saved.or(ruled).or(live) {
        Some(limit) if limit < 100 && !batteries.is_empty() => {
            g.line = format!("Charging stops at {limit}%.");
            for b in &batteries {
                g.keep(Setting::Sysfs { path: format!("/sys/class/power_supply/{b}/charge_control_end_threshold"), value: limit.to_string() }, "This charge limit is outside what Turbine Hub sets.");
            }
        }
        _ => g.line = "The battery charges to 100%.".into(),
    }
    g
}

fn wake_group(found: &Found, root: &Path) -> Group {
    let mut g = Group::new("wake", "Wake devices").with_files(found, &[NOWAKE_RULE]);
    let mut unknown = 0;
    for line in text(root, NOWAKE_RULE).lines() {
        let Some(devpath) = rule_value(line, "DEVPATH") else { continue };
        let ok = devpath.starts_with("/devices/") && !devpath.split('/').any(|p| p == "..");
        let id = |f: &str| Some(text(root, &format!("/sys{devpath}/{f}")).trim().to_lowercase()).filter(|v| ok && !v.is_empty());
        match (id("idVendor"), id("idProduct")) {
            (Some(vendor), Some(product)) => g.keep(Setting::Wake { vendor, product, wake: false }, "One blocked device has an id Turbine Hub can't read."),
            _ => unknown += 1,
        }
    }
    let n = g.take_over.len();
    g.line = if n == 1 { "1 USB device can't wake the machine.".into() } else { format!("{n} USB devices can't wake the machine.") };
    if unknown > 0 {
        g.lost.push(format!("{unknown} blocked {} not plugged in now. Block {} again in My machine, Sleep.", if unknown == 1 { "device is" } else { "devices are" }, if unknown == 1 { "it" } else { "them" }));
    }
    g
}

fn tuneup_group(found: &Found, root: &Path) -> Group {
    let mut g = Group::new("tuneup", "Tune-up").with_files(found, &[SYSCTL, SYSCTL_ZRAM, TUNEUP_TMPFILES, ZRAM_UNIT, ZRAM_SCRIPT]);
    if found.has(ZRAM_UNIT) {
        g.units.push(unit_name(ZRAM_UNIT));
        g.keep(Setting::Zram { percent: 100, algorithm: "zstd".into() }, "");
    }
    for file in [SYSCTL_ZRAM, SYSCTL] {
        for line in text(root, file).lines().filter(|l| !l.trim_start().starts_with('#')) {
            let Some((key, value)) = line.split_once('=') else { continue };
            let key = key.trim();
            match value.trim().parse::<u64>() {
                Ok(value) => g.keep(Setting::Sysctl { key: key.to_string(), value }, &format!("{key} is not a setting Turbine Hub changes.")),
                Err(_) => g.lost.push(format!("{key} is not a setting Turbine Hub changes.")),
            }
        }
    }
    // tmpfiles lines: "w <path> - - - - <value>".
    for line in text(root, TUNEUP_TMPFILES).lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.len() == 7 && p[0] == "w" {
            g.keep(Setting::Sysfs { path: p[1].to_string(), value: p[6].to_string() }, &format!("{} is not a value Turbine Hub changes.", p[1].rsplit('/').next().unwrap_or(p[1])));
        }
    }
    let n = g.take_over.len();
    g.line = if n == 1 { "1 memory and kernel setting is on.".into() } else { format!("{n} memory and kernel settings are on.") };
    g
}

fn ntsync_group(found: &Found) -> Group {
    let mut g = Group::new("ntsync", "NTSync").with_files(found, &[NTSYNC_LOAD, NTSYNC_RULE]);
    if !g.files.is_empty() {
        g.line = "Games can use NTSync.".into();
        g.keep(Setting::Module { name: "ntsync".into() }, "");
        g.keep(Setting::Access { what: Access::Ntsync }, "");
    }
    g
}

fn lighting_group(found: &Found) -> Group {
    let mut g = Group::new("lighting", "Lighting").with_files(found, &[LEDS_RULE, HID_RULE, LIGHT_UNIT, LIGHT_SCRIPT]);
    if !g.files.is_empty() {
        g.line = "The ring lights change without a password.".into();
        g.keep(Setting::Access { what: Access::Lighting }, "");
        if found.has(LIGHT_UNIT) {
            g.units.push(unit_name(LIGHT_UNIT));
        }
    }
    g
}

fn boost_group(found: &Found) -> Group {
    let mut g = Group::new("boost", "Game Boost").with_files(found, &[BOOST_TMPFILES]);
    g.line = "Turbine Hub's helper switches the CPU for Game Boost, so this rule isn't needed.".into();
    g
}

fn panel_group(found: &Found) -> Group {
    let mut g = Group::new("panel", "Quick panel").with_files(found, &[DECKY_RULE, DECKY_MARK]);
    g.plugin = found.plugin;
    g.line = "AllyHub's panel in the Quick Access menu. Turbine Hub has its own in Store, Decky plugins.".into();
    g
}

fn display_group(found: &Found) -> Group {
    let mut g = Group::new("display", "Display").with_files(found, &[EDID_UNIT, EDID_SCRIPT, EDID_FILE]);
    if found.has(EDID_UNIT) {
        g.units.push(unit_name(EDID_UNIT));
    }
    if found.has_user(REFRESH_SCRIPT) {
        g.user_paths.push(REFRESH_SCRIPT.into());
    }
    g.line = "A higher refresh rate for the built-in screen.".into();
    if !g.is_empty() {
        g.lost.push("The higher refresh rate. Run the refresh test in My machine, Display.".into());
    }
    g
}

fn games_group(found: &Found, games: &[GameOptions]) -> Group {
    let mut g = Group::new("games", "Game switches");
    g.games = games.to_vec();
    let n = games.len();
    g.line = if n == 1 { "1 game has switches in its launch options.".into() } else { format!("{n} games have switches in their launch options.") };
    if found.has_user(GAME_ENV) {
        g.user_paths.push(GAME_ENV.into());
        g.lost.push("FSR 4 for every game at once. Turn it on per game in Games.".into());
        if n == 0 {
            g.line = "FSR 4 is on for every game.".into();
        }
    }
    g
}

/// The groups that have something on this machine, in the order they are shown.
pub fn groups(found: &Found, root: &Path, games: &[GameOptions]) -> Vec<Group> {
    let all = vec![charge_group(found, root), wake_group(found, root), tuneup_group(found, root), ntsync_group(found), lighting_group(found), boost_group(found), display_group(found), games_group(found, games), panel_group(found)];
    all.into_iter().filter(|g| !g.is_empty()).collect()
}

// ---- the plan ----

/// One thing the app does, in order. The first block needs the helper; the rest runs as the player.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// The helper's `apply`.
    Apply(Setting),
    DisableSystemUnit(&'static str),
    RemoveSystemFile(&'static str),
    /// Removes `SYSTEM_DIR` when it is empty.
    RemoveSystemDir,
    /// Removes `PLUGIN_DIR` in the player's home and restarts Decky.
    RemovePlugin,
    /// udev and systemd read their folders again.
    ReloadSystem,
    /// Sets a game's launch options through Steam.
    SetLaunchOptions { app_id: String, options: String },
    StopUserUnit(&'static str),
    /// A home-relative file or folder.
    RemoveUserPath(String),
    /// Takes AllyHub's own tile out of the Steam library.
    RemoveSteamEntry,
}

impl Step {
    /// Does this step take away part of AllyHub's own program (its service, files, settings or Steam tile)?
    pub fn removes_allyhub_itself(&self) -> bool {
        match self {
            Step::StopUserUnit(_) | Step::RemoveSteamEntry => true,
            Step::RemoveUserPath(p) => APP_PATHS.contains(&p.as_str()),
            _ => false,
        }
    }

    pub fn needs_helper(&self) -> bool {
        matches!(self, Step::Apply(_) | Step::DisableSystemUnit(_) | Step::RemoveSystemFile(_) | Step::RemoveSystemDir | Step::RemovePlugin | Step::ReloadSystem)
    }
}

/// The whole move, in order: Turbine Hub's settings go in first so nothing lapses, then AllyHub's system files
/// go, then its launch options, then AllyHub itself. A group without a choice gets its default.
pub fn plan(found: &Found, groups: &[Group], choices: &[(&str, Choice)]) -> Vec<Step> {
    let choice = |g: &Group| choices.iter().find(|(id, _)| *id == g.id).map(|(_, c)| *c).unwrap_or_else(|| g.default_choice());
    let mut steps = Vec::new();
    for g in groups.iter().filter(|g| choice(g) == Choice::TakeOver) {
        steps.extend(g.take_over.iter().cloned().map(Step::Apply));
    }
    for g in groups {
        steps.extend(g.units.iter().copied().map(Step::DisableSystemUnit));
    }
    for g in groups {
        steps.extend(g.files.iter().copied().map(Step::RemoveSystemFile));
    }
    let listed: Vec<&str> = groups.iter().flat_map(|g| g.files.iter().copied()).collect();
    steps.extend(found.system_files.iter().copied().filter(|f| !listed.contains(f)).map(Step::RemoveSystemFile));
    if !found.system_files.is_empty() {
        steps.push(Step::RemoveSystemDir);
        steps.push(Step::ReloadSystem);
    }
    if found.plugin {
        steps.push(Step::RemovePlugin);
    }
    for g in groups {
        steps.extend(g.games.iter().map(|o| Step::SetLaunchOptions { app_id: o.app_id.clone(), options: o.kept.clone() }));
    }
    // What AllyHub added for games first; its own program and settings last, so it works until the very end.
    let own = |p: &String| APP_PATHS.contains(&p.as_str());
    steps.extend(found.user_paths.iter().filter(|p| !own(p)).cloned().map(Step::RemoveUserPath));
    if found.has_user(AGENT_UNIT) {
        steps.push(Step::StopUserUnit("allyhub-agent.service"));
    }
    steps.extend(found.user_paths.iter().filter(|p| own(p)).cloned().map(Step::RemoveUserPath));
    if found.app {
        steps.push(Step::RemoveSteamEntry);
    }
    steps
}

// ---- when a step fails ----

/// Where the move stands after a step failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Stopped {
    /// Turbine Hub's settings that went in before the failure, newest first: the app undoes each through the
    /// helper, so the machine is as AllyHub left it. Empty once AllyHub's files started to go.
    pub undo: Vec<Setting>,
    /// AllyHub's program, settings and background service are all still in place.
    pub allyhub_runs: bool,
    /// None of AllyHub's system files is gone yet.
    pub allyhub_setup_whole: bool,
    /// The move can be started again from the top.
    pub retry: bool,
    pub words: &'static str,
}

/// What to do when step `failed` of `steps` didn't work: nothing after it runs. While Turbine Hub's settings are
/// going in, AllyHub's setup is untouched and the settings that went in are taken out again. Once AllyHub's files
/// started to go, the settings Turbine Hub took over stay (they keep the same effect), and AllyHub itself is still
/// installed because its own removal comes last. The import is marked done only after the last step.
pub fn stopped(steps: &[Step], failed: usize) -> Stopped {
    let own_removal = |s: &Step| s.removes_allyhub_itself();
    let done = &steps[..failed.min(steps.len())];
    let removing = done.iter().chain(steps.get(failed)).any(|s| !matches!(s, Step::Apply(_)));
    let allyhub_runs = !done.iter().chain(steps.get(failed)).any(own_removal);
    if !removing {
        let undo = done.iter().rev().filter_map(|s| if let Step::Apply(x) = s { Some(x.clone()) } else { None }).collect();
        return Stopped { undo, allyhub_runs: true, allyhub_setup_whole: true, retry: true, words: "Nothing moved. AllyHub's setup is as it was. Try again when you like." };
    }
    let words = if allyhub_runs { "The move stopped partway. What Turbine Hub took over is in effect, and AllyHub is still installed. Try again to finish." } else { "The move stopped while removing AllyHub. Its settings are in Turbine Hub. Try again to remove the rest." };
    Stopped { undo: Vec::new(), allyhub_runs, allyhub_setup_whole: false, retry: true, words }
}

// ---- settings ----

/// Something of AllyHub's that Turbine Hub doesn't carry over.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub what: &'static str,
    pub words: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub settings: Settings,
    pub switches: GameSwitches,
    pub left_behind: Vec<Note>,
}

fn note(out: &mut Vec<Note>, what: &'static str, words: &str) {
    out.push(Note { what, words: words.to_string() });
}

/// AllyHub's settings read into Turbine Hub's, once: when the import already ran, everything comes back unchanged.
/// `games` are the per-game switches to move (empty when the player chose to drop them).
pub fn import(config: Option<&str>, current: &Settings, switches: &GameSwitches, games: &[GameOptions]) -> Import {
    let mut out = Import { settings: current.clone(), switches: switches.clone(), left_behind: Vec::new() };
    if current.allyhub_imported {
        return out;
    }
    out.settings.allyhub_imported = true;
    for g in games {
        for s in &g.switches {
            out.switches.set(&format!("steam:{}", g.app_id), *s, true);
        }
    }
    let Some(j) = config.and_then(|t| json::parse(t).ok()) else { return out };
    let flag = |path: &[&str]| at(&j, path).and_then(Json::as_bool).unwrap_or(false);
    let s = &mut out.settings;
    if s.mode.is_none() {
        s.mode = Some(if flag(&["theme", "advanced"]) { Mode::Veteran } else { Mode::Guided });
    }
    if let Some(scale) = number(at(&j, &["theme", "scale_gamemode"])).or_else(|| number(at(&j, &["theme", "scale"]))) {
        s.ui_scale = scale.clamp(50, 150) as u32;
    }
    s.game_boost = s.game_boost || flag(&["performance", "boost"]);
    s.reports = s.reports || flag(&["updates", "reporting"]);
    s.test_builds = s.test_builds || at(&j, &["updates", "channel"]).and_then(Json::as_str) == Some("testing");

    let l = &mut out.left_behind;
    let own_lighting = at(&j, &["lighting", "controller"]).and_then(Json::as_str) == Some("allyhub");
    let presets = at(&j, &["lighting", "custom"]).map(|c| c.entries().len()).unwrap_or(0);
    if own_lighting || presets > 0 || at(&j, &["rgb"]).is_some_and(|v| *v != Json::Null) {
        note(l, "Lighting", &if presets > 0 { format!("The ring effect and {presets} custom {}. Pick an effect in My machine, Lighting.", if presets == 1 { "preset" } else { "presets" }) } else { "The ring effect and colors. Pick an effect in My machine, Lighting.".to_string() });
    }
    let colors = at(&j, &["game_colors"]).map(|c| c.entries().len()).unwrap_or(0);
    if colors > 0 {
        note(l, "Game colors", &format!("Ring colors for {colors} {}. Set them in My machine, Lighting.", if colors == 1 { "game" } else { "games" }));
    }
    if flag(&["agent", "battery_rings"]) {
        note(l, "Battery rings", "Rings that show the battery level. Turn them on in My machine, Lighting.");
    }
    if flag(&["agent", "dock_mode"]) {
        note(l, "Dock", "Lighting and HDMI sound when docked. Set them in My machine, Dock.");
    }
    if flag(&["saves", "time_machine"]) || flag(&["agent", "save_backup"]) {
        note(l, "Save backups", "Save snapshots. Turn them on in Games; the old backups stay in AllyHub-Backups.");
    }
    if flag(&["agent", "remote"]) {
        note(l, "Phone remote", "Turbine Hub has no phone remote yet.");
    }
    if at(&j, &["wol", "mac"]).and_then(Json::as_str).is_some_and(|m| !m.is_empty()) {
        note(l, "Wake a PC", "The saved PC to wake over the network. Turbine Hub has no page for it yet.");
    }
    let preset = at(&j, &["theme", "preset"]).and_then(Json::as_str).unwrap_or("ROG Crimson");
    if preset != "ROG Crimson" || at(&j, &["theme", "accent"]).is_some_and(|v| *v != Json::Null) {
        note(l, "Theme", "AllyHub's theme and colors. Pick a theme and accent in Settings.");
    }
    out
}

// ---- for the interface ----

fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(","))
}

/// The groups as JSON for the Moving from AllyHub screen.
pub fn groups_json(groups: &[Group]) -> String {
    list(groups.iter().map(|g| {
        format!(
            "{{\"id\":{},\"title\":{},\"line\":{},\"canTakeOver\":{},\"takeOver\":{},\"games\":{},\"lost\":{}}}",
            quote(g.id),
            quote(g.title),
            quote(&g.line),
            g.can_take_over(),
            list(g.take_over.iter().map(|s| quote(&s.words()))),
            list(g.games.iter().map(|o| format!("{{\"id\":{},\"name\":{},\"switches\":{}}}", quote(&o.app_id), quote(&o.name), list(o.switches.iter().map(|s| quote(s.title())))))),
            list(g.lost.iter().map(|s| quote(s)))
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helper_removes_exactly_the_files_this_plan_names() {
        let mut a: Vec<&str> = SYSTEM_FILES.to_vec();
        let mut b: Vec<&str> = turbinehub_helper::policy::ALLYHUB_FILES.to_vec();
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }
    use crate::fake::{self, Preset};
    use std::path::PathBuf;

    struct Dirs {
        root: PathBuf,
        home: PathBuf,
    }

    impl Drop for Dirs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.root.parent().unwrap());
        }
    }

    fn w(base: &Path, p: &str, t: &str) {
        let f = base.join(p.trim_start_matches('/'));
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, t).unwrap();
    }

    fn dirs(tag: &str) -> Dirs {
        let base = std::env::temp_dir().join(format!("turbinehub-migrate-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let d = Dirs { root: base.join("root"), home: base.join("home") };
        fake::machine(&d.root, Preset::AllyX);
        fs::create_dir_all(&d.home).unwrap();
        d
    }

    const CONFIG_TEXT: &str = r##"{"theme":{"preset":"ROG Crimson","scale":100,"scale_gamemode":125,"advanced":true,"accent":null},
        "rgb":null,"agent":{"enabled":true,"battery_rings":true,"dock_mode":false,"remote":false,"save_backup":false},
        "saves":{"time_machine":true,"keep":5},"game_colors":{"1245620":{"color":"#ff0000"},"489830":{"color":"#00ff00"}},
        "lighting":{"effect":"spiral","custom":{"Mine":{}},"controller":"allyhub"},
        "updates":{"reporting":true,"channel":"testing"},"performance":{"boost":true},"battery":{"limit":"80"},
        "wol":{"name":"Gaming PC","mac":""},"sleep":{"no_wake":["/devices/pci0000:00/usb3/3-1"]}}"##;

    /// An Ally X with everything AllyHub can set up.
    fn full(d: &Dirs) {
        let (root, home) = (&d.root, &d.home);
        w(root, LEDS_RULE, "ACTION==\"add|change\", SUBSYSTEM==\"leds\", KERNEL==\"*:rgb:*\", RUN+=\"/bin/sh -c 'chmod a+w /sys%p/brightness'\"\n");
        w(root, HID_RULE, "# allyhub-lighting v2\nKERNEL==\"hidraw*\", SUBSYSTEM==\"hidraw\", ATTRS{idVendor}==\"0b05\", ATTRS{idProduct}==\"1b4c\", MODE:=\"0666\"\n");
        w(root, LIGHT_UNIT, "[Unit]\n");
        w(root, LIGHT_SCRIPT, "#!/bin/sh\n");
        w(root, CHARGE_PERM_RULE, "ACTION==\"add|change\", SUBSYSTEM==\"power_supply\", KERNEL==\"BAT*\", RUN+=\"/bin/sh -c 'chgrp deck /sys%p/charge_control_end_threshold'\"\n");
        w(root, NOWAKE_RULE, "ACTION==\"add|bind|change\", SUBSYSTEM==\"usb\", DEVPATH==\"/devices/pci0000:00/usb3/3-1\", ATTR{power/wakeup}=\"disabled\"\nACTION==\"add|bind|change\", SUBSYSTEM==\"usb\", DEVPATH==\"/devices/pci0000:00/usb1/1-2\", ATTR{power/wakeup}=\"disabled\"\n");
        w(root, "/sys/devices/pci0000:00/usb3/3-1/idVendor", "0B05\n");
        w(root, "/sys/devices/pci0000:00/usb3/3-1/idProduct", "1b4c\n");
        w(root, SYSCTL, "# Ally Hub tune-up (Tools > Performance). Undo there, or delete this file.\nvm.vfs_cache_pressure = 50\nvm.dirty_bytes = 268435456\nvm.dirty_background_bytes = 67108864\nvm.compaction_proactiveness = 0\nkernel.split_lock_mitigate = 0\n");
        w(root, SYSCTL_ZRAM, "# Ally Hub tune-up: swap settings for zram\nvm.swappiness = 100\nvm.page-cluster = 0\n");
        w(root, TUNEUP_TMPFILES, "# Ally Hub tune-up: huge pages\nw /sys/kernel/mm/transparent_hugepage/enabled - - - - always\nw /sys/kernel/mm/transparent_hugepage/defrag - - - - defer+madvise\n");
        w(root, ZRAM_UNIT, "[Unit]\n");
        w(root, ZRAM_SCRIPT, "#!/bin/sh\n");
        w(root, NTSYNC_LOAD, "ntsync\n");
        w(root, NTSYNC_RULE, "KERNEL==\"ntsync\", MODE=\"0666\"\n");
        w(root, BOOST_TMPFILES, "z /sys/devices/system/cpu/cpufreq/policy*/energy_performance_preference 0664 root deck - -\n");
        w(root, DECKY_RULE, "polkit.addRule(function(a, s) {});\n");
        w(root, DECKY_MARK, "");
        w(home, CONFIG, CONFIG_TEXT);
        w(home, ".local/share/allyhub/core.py", "");
        w(home, AGENT_UNIT, "[Unit]\n");
        w(home, GAME_ENV, "PROTON_FSR4_UPGRADE=1\n");
        w(home, REFRESH_SCRIPT, "local MAX = 144\n");
        w(home, ".local/bin/allyhub", "");
        w(home, "Desktop/allyhub.desktop", "");
        w(home, &format!("{TEMPLATES_DIR}/controller_allyhub_ally_x.vdf"), "");
        w(home, &format!("{TEMPLATES_DIR}/controller_neptune.vdf"), "");
        w(home, &format!("{PLUGIN_DIR}/plugin.json"), "{}");
        w(home, "AllyHub-Backups/saves/x", "");
    }

    fn game(id: &str, switches: &[Switch], kept: &str) -> GameOptions {
        GameOptions { app_id: id.into(), name: format!("Game {id}"), switches: switches.to_vec(), kept: kept.into() }
    }

    #[test]
    fn a_machine_without_allyhub_has_nothing_to_move() {
        let d = dirs("none");
        let found = detect(&d.root, &d.home);
        assert!(!found.any());
        assert_eq!(found, Found::default());
        assert!(groups(&found, &d.root, &[]).is_empty());
        assert!(plan(&found, &[], &[]).is_empty());
    }

    #[test]
    fn finds_everything_allyhub_set_up() {
        let d = dirs("detect");
        full(&d);
        let found = detect(&d.root, &d.home);
        assert!(found.any() && found.app && found.plugin && found.config.is_some());
        assert_eq!(found.system_files.len(), 16);
        assert!(!found.has(CHARGE_RULE) && !found.has(EDID_UNIT));
        assert!(found.has_user(AGENT_UNIT) && found.has_user(GAME_ENV) && found.has_user(REFRESH_SCRIPT));
        assert!(found.has_user(".local/share/Steam/controller_base/templates/controller_allyhub_ally_x.vdf"));
        assert!(!found.user_paths.iter().any(|p| p.contains("neptune") || p.contains("Backups")));
    }

    #[test]
    fn each_group_maps_to_the_helper_settings_with_the_same_effect() {
        let d = dirs("groups");
        full(&d);
        let found = detect(&d.root, &d.home);
        let gs = groups(&found, &d.root, &[game("1245620", &[Switch::Fsr4], "")]);
        assert_eq!(gs.iter().map(|g| g.id).collect::<Vec<_>>(), vec!["battery", "wake", "tuneup", "ntsync", "lighting", "boost", "display", "games", "panel"]);
        let g = |id: &str| gs.iter().find(|g| g.id == id).unwrap();

        assert_eq!(g("battery").take_over, vec![Setting::Sysfs { path: "/sys/class/power_supply/BAT0/charge_control_end_threshold".into(), value: "80".into() }]);
        assert_eq!(g("battery").line, "Charging stops at 80%.");

        assert_eq!(g("wake").take_over, vec![Setting::Wake { vendor: "0b05".into(), product: "1b4c".into(), wake: false }]);
        assert_eq!(g("wake").line, "1 USB device can't wake the machine.");
        assert_eq!(g("wake").lost, vec!["1 blocked device is not plugged in now. Block it again in My machine, Sleep."]);

        let tune = g("tuneup");
        assert_eq!(tune.take_over.len(), 10);
        assert_eq!(tune.take_over[0], Setting::Zram { percent: 100, algorithm: "zstd".into() });
        assert!(tune.take_over.contains(&Setting::Sysctl { key: "vm.swappiness".into(), value: 100 }));
        assert!(tune.take_over.contains(&Setting::Sysfs { path: "/sys/kernel/mm/transparent_hugepage/defrag".into(), value: "defer+madvise".into() }));
        assert_eq!(tune.units, vec!["allyhub-zram.service"]);
        assert!(tune.lost.is_empty());

        assert_eq!(g("ntsync").take_over, vec![Setting::Module { name: "ntsync".into() }, Setting::Access { what: Access::Ntsync }]);
        assert_eq!(g("lighting").take_over, vec![Setting::Access { what: Access::Lighting }]);
        assert_eq!(g("lighting").units, vec!["allyhub-lighting-perms.service"]);

        for id in ["boost", "panel", "display"] {
            assert!(!g(id).can_take_over(), "{id}");
            assert_eq!(g(id).default_choice(), Choice::Undo);
        }
        assert!(g("panel").plugin);
        assert_eq!(g("display").user_paths, vec![REFRESH_SCRIPT]);
        assert_eq!(g("games").lost.len(), 1);
        assert!(g("games").can_take_over());

        for group in &gs {
            for s in &group.take_over {
                assert!(s.check().is_ok(), "{s:?}");
            }
            assert!(!group.line.is_empty(), "{}", group.id);
            for words in group.lost.iter().chain([&group.line]) {
                assert!(crate::glossary::jargon(words).is_empty() || group.id == "tuneup" || group.id == "ntsync", "{words}");
            }
        }
        let j = json::parse(&groups_json(&gs)).unwrap();
        assert_eq!(j.as_array().len(), 9);
        assert_eq!(j.as_array()[0].get("takeOver").unwrap().as_array()[0].as_str(), Some("charge_control_end_threshold set to 80"));
    }

    #[test]
    fn values_turbine_hub_does_not_set_are_reported_not_carried() {
        let d = dirs("odd");
        w(&d.root, SYSCTL, "vm.swappiness = 100\nkernel.core_pattern = |/bin/sh\nvm.max_map_count = 5\nnet.ipv4.ip_forward = 1\n");
        w(&d.root, TUNEUP_TMPFILES, "w /sys/kernel/mm/transparent_hugepage/enabled - - - - always\nw /etc/passwd - - - - x\nL /etc/x - - - - /bin/sh\n");
        w(&d.root, NOWAKE_RULE, "SUBSYSTEM==\"usb\", DEVPATH==\"/devices/../../etc\", ATTR{power/wakeup}=\"disabled\"\n");
        w(&d.root, "/etc/idVendor", "dead\n");
        w(&d.root, "/etc/idProduct", "beef\n");
        let found = detect(&d.root, &d.home);
        let gs = groups(&found, &d.root, &[]);
        let tune = gs.iter().find(|g| g.id == "tuneup").unwrap();
        assert_eq!(tune.take_over, vec![Setting::Sysctl { key: "vm.swappiness".into(), value: 100 }, Setting::Sysfs { path: "/sys/kernel/mm/transparent_hugepage/enabled".into(), value: "always".into() }]);
        assert_eq!(tune.lost.len(), 4);
        let wake = gs.iter().find(|g| g.id == "wake").unwrap();
        assert!(wake.take_over.is_empty() && wake.lost.len() == 1);
    }

    #[test]
    fn the_charge_limit_comes_from_the_settings_then_the_old_rule_then_the_battery() {
        let d = dirs("charge");
        w(&d.root, CHARGE_RULE, "ACTION==\"add\", SUBSYSTEM==\"power_supply\", KERNEL==\"BAT0\", ATTR{charge_control_end_threshold}=\"70\"\n");
        let limit = |d: &Dirs| groups(&detect(&d.root, &d.home), &d.root, &[])[0].take_over.clone();
        let at = |v: &str| vec![Setting::Sysfs { path: "/sys/class/power_supply/BAT0/charge_control_end_threshold".into(), value: v.into() }];
        assert_eq!(limit(&d), at("70"));
        w(&d.home, CONFIG, r#"{"battery":{"limit":60}}"#);
        assert_eq!(limit(&d), at("60"));
        w(&d.home, CONFIG, r#"{"battery":{"limit":""}}"#);
        fs::remove_file(under(&d.root, CHARGE_RULE)).unwrap();
        w(&d.root, CHARGE_PERM_RULE, "x\n");
        w(&d.root, "/sys/class/power_supply/BAT0/charge_control_end_threshold", "90\n");
        assert_eq!(limit(&d), at("90"));
        w(&d.root, "/sys/class/power_supply/BAT0/charge_control_end_threshold", "100\n");
        let gs = groups(&detect(&d.root, &d.home), &d.root, &[]);
        assert!(!gs[0].can_take_over());
        assert_eq!(gs[0].line, "The battery charges to 100%.");
        w(&d.home, CONFIG, r#"{"battery":{"limit":"5"}}"#);
        let gs = groups(&detect(&d.root, &d.home), &d.root, &[]);
        assert!(!gs[0].can_take_over() && gs[0].lost.len() == 1);
    }

    #[test]
    fn launch_options_lose_only_allyhubs_switches() {
        assert_eq!(split_launch_options("PROTON_FSR4_UPGRADE=1 SteamDeck=1 ~/lsfg %command%"), (vec![Switch::Fsr4, Switch::FrameGen, Switch::DeckMode], String::new()));
        assert_eq!(split_launch_options("PROTON_LOG=1 PROTON_USE_WINED3D=1 mangohud %command% -novid"), (vec![Switch::ProtonLog], "PROTON_USE_WINED3D=1 mangohud %command% -novid".into()));
        assert_eq!(split_launch_options("WINEDLLOVERRIDES=\"dinput8=n,b SteamDeck=1\" /home/deck/lsfg %command%"), (vec![Switch::FrameGen], "WINEDLLOVERRIDES=\"dinput8=n,b SteamDeck=1\" %command%".into()));
        assert_eq!(split_launch_options("SteamDeck=0 /home/deck/tools/lsfg %command%"), (vec![], "SteamDeck=0 /home/deck/tools/lsfg %command%".into()));
        assert_eq!(split_launch_options(""), (vec![], String::new()));
        assert_eq!(split_launch_options("-windowed"), (vec![], "-windowed".into()));
    }

    #[test]
    fn games_with_switches_are_read_from_steam() {
        let d = dirs("steam");
        fs::create_dir_all(d.home.join(".local/share/Steam/steamapps")).unwrap();
        w(&d.home, ".local/share/Steam/userdata/77/config/localconfig.vdf", "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"1245620\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"SteamDeck=1 %command%\"\n\t\t\t\t\t}\n\t\t\t\t\t\"489830\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"-windowed\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n");
        let g = |id: &str, name: &str| crate::Game { store: Store::Steam, store_id: id.into(), name: name.into(), install_dir: PathBuf::new(), library: PathBuf::new(), build: None, runtime: crate::Runtime::Unknown, cover: None, header: None, prefix: None };
        let scan = Scan { games: vec![g("1245620", "Elden Ring"), g("489830", "Skyrim"), g("5", "Other")], problems: vec![] };
        assert_eq!(game_options(&d.home, &scan), vec![GameOptions { app_id: "1245620".into(), name: "Elden Ring".into(), switches: vec![Switch::DeckMode], kept: String::new() }]);
    }

    #[test]
    fn the_plan_applies_first_then_removes_and_never_names_an_unlisted_system_file() {
        let d = dirs("plan");
        full(&d);
        let found = detect(&d.root, &d.home);
        let games = [game("1245620", &[Switch::Fsr4], "mangohud %command%")];
        let gs = groups(&found, &d.root, &games);
        let steps = plan(&found, &gs, &[("tuneup", Choice::Undo), ("boost", Choice::TakeOver)]);

        let applies: Vec<&Setting> = steps.iter().filter_map(|s| if let Step::Apply(x) = s { Some(x) } else { None }).collect();
        assert_eq!(applies.len(), 5);
        assert!(!applies.iter().any(|s| matches!(s, Setting::Sysctl { .. } | Setting::Zram { .. })));
        let first_removal = steps.iter().position(|s| !matches!(s, Step::Apply(_))).unwrap();
        assert_eq!(first_removal, 5);
        assert!(steps[first_removal..].iter().all(|s| !matches!(s, Step::Apply(_))));

        let removed: Vec<&str> = steps.iter().filter_map(|s| if let Step::RemoveSystemFile(f) = s { Some(*f) } else { None }).collect();
        assert_eq!(removed.len(), found.system_files.len());
        assert!(removed.iter().all(|f| SYSTEM_FILES.contains(f) && f.contains("allyhub")));
        let units: Vec<&Step> = steps.iter().filter(|s| matches!(s, Step::DisableSystemUnit(_))).collect();
        assert_eq!(units, vec![&Step::DisableSystemUnit("allyhub-zram.service"), &Step::DisableSystemUnit("allyhub-lighting-perms.service")]);
        let pos = |want: &Step| steps.iter().position(|s| s == want).unwrap();
        assert!(pos(&Step::DisableSystemUnit("allyhub-zram.service")) < pos(&Step::RemoveSystemFile(ZRAM_UNIT)));
        assert!(pos(&Step::RemoveSystemFile(DECKY_MARK)) < pos(&Step::RemoveSystemDir));
        assert!(pos(&Step::RemoveSystemDir) < pos(&Step::ReloadSystem));
        assert!(pos(&Step::ReloadSystem) < pos(&Step::RemovePlugin));

        let helper_end = steps.iter().rposition(Step::needs_helper).unwrap();
        assert!(steps[..=helper_end].iter().all(Step::needs_helper));
        assert!(steps.contains(&Step::SetLaunchOptions { app_id: "1245620".into(), options: "mangohud %command%".into() }));
        assert!(pos(&Step::StopUserUnit("allyhub-agent.service")) < pos(&Step::RemoveUserPath(AGENT_UNIT.into())));
        assert!(steps.contains(&Step::RemoveUserPath(APP_DIR.into())) && steps.contains(&Step::RemoveUserPath(GAME_ENV.into())));
        assert_eq!(steps.last(), Some(&Step::RemoveSteamEntry));
        assert!(!steps.iter().any(|s| matches!(s, Step::RemoveUserPath(p) if p.contains("Backups") || p.starts_with('/') || p.contains(".."))));
    }

    #[test]
    fn a_failed_step_stops_the_move_safely() {
        let d = dirs("stopped");
        full(&d);
        let found = detect(&d.root, &d.home);
        let gs = groups(&found, &d.root, &[game("1245620", &[Switch::Fsr4], "")]);
        let steps = plan(&found, &gs, &[]);
        let applies = steps.iter().filter(|s| matches!(s, Step::Apply(_))).count();
        assert_eq!(applies, 15);

        let s = stopped(&steps, 0);
        assert!(s.undo.is_empty() && s.allyhub_runs && s.allyhub_setup_whole && s.retry);
        let s = stopped(&steps, 3);
        assert_eq!(s.undo.len(), 3);
        assert_eq!(Step::Apply(s.undo[0].clone()), steps[2]);
        assert_eq!(Step::Apply(s.undo[2].clone()), steps[0]);
        assert!(s.allyhub_runs && s.allyhub_setup_whole);
        assert_eq!(stopped(&steps, applies - 1).undo.len(), applies - 1);

        for at in applies..steps.len() {
            let s = stopped(&steps, at);
            assert!(s.undo.is_empty() && !s.allyhub_setup_whole && s.retry, "{at}");
            assert_eq!(s.allyhub_runs, !steps[..=at].iter().any(Step::removes_allyhub_itself), "{at}: {:?}", steps[at]);
        }
        let first_own = steps.iter().position(Step::removes_allyhub_itself).unwrap();
        assert_eq!(steps[first_own], Step::StopUserUnit("allyhub-agent.service"));
        assert!(steps[first_own..].iter().all(Step::removes_allyhub_itself));
        assert!(steps[..first_own].contains(&Step::RemoveUserPath(GAME_ENV.into())));
    }

    #[test]
    fn a_stray_system_file_outside_every_group_still_goes() {
        let d = dirs("stray");
        w(&d.root, EDID_FILE, "x");
        w(&d.root, DECKY_MARK, "");
        let found = detect(&d.root, &d.home);
        let steps = plan(&found, &[], &[]);
        assert_eq!(steps, vec![Step::RemoveSystemFile(DECKY_MARK), Step::RemoveSystemFile(EDID_FILE), Step::RemoveSystemDir, Step::ReloadSystem]);
    }

    #[test]
    fn settings_come_over_once() {
        let games = [game("1245620", &[Switch::Fsr4, Switch::DeckMode], ""), game("489830", &[Switch::ProtonLog], "")];
        let got = import(Some(CONFIG_TEXT), &Settings::default(), &GameSwitches::default(), &games);
        let s = &got.settings;
        assert_eq!((s.mode, s.ui_scale, s.game_boost, s.reports, s.test_builds, s.allyhub_imported), (Some(Mode::Veteran), 125, true, true, true, true));
        assert_eq!((s.theme.as_str(), s.accent.as_str()), ("dark", "brass"));
        assert_eq!(got.switches.of("steam:1245620"), &[Switch::Fsr4, Switch::DeckMode]);
        assert_eq!(got.switches.of("steam:489830"), &[Switch::ProtonLog]);
        assert_eq!(got.left_behind.iter().map(|n| n.what).collect::<Vec<_>>(), vec!["Lighting", "Game colors", "Battery rings", "Save backups"]);
        assert_eq!(got.left_behind[0].words, "The ring effect and 1 custom preset. Pick an effect in My machine, Lighting.");
        assert_eq!(got.left_behind[1].words, "Ring colors for 2 games. Set them in My machine, Lighting.");

        let again = import(Some(CONFIG_TEXT), &got.settings, &got.switches, &games);
        assert_eq!((&again.settings, &again.switches), (&got.settings, &got.switches));
        assert!(again.left_behind.is_empty());
        assert_eq!(Settings::parse(&got.settings.to_json()), got.settings);
    }

    #[test]
    fn a_mode_already_picked_stays_and_a_missing_or_damaged_file_only_marks_the_import() {
        let mine = Settings { mode: Some(Mode::Guided), ui_scale: 90, ..Settings::default() };
        let got = import(Some(r##"{"theme":{"advanced":true,"scale":400,"preset":"Aura","accent":"#fff"},"agent":{"remote":true,"dock_mode":true},"wol":{"mac":"aa:bb"}}"##), &mine, &GameSwitches::default(), &[]);
        assert_eq!((got.settings.mode, got.settings.ui_scale), (Some(Mode::Guided), 150));
        assert_eq!(got.left_behind.iter().map(|n| n.what).collect::<Vec<_>>(), vec!["Dock", "Phone remote", "Wake a PC", "Theme"]);
        for text in [None, Some("not json"), Some("[]")] {
            let got = import(text, &Settings::default(), &GameSwitches::default(), &[]);
            let want = if text == Some("[]") { Settings { allyhub_imported: true, mode: Some(Mode::Guided), ..Settings::default() } } else { Settings { allyhub_imported: true, ..Settings::default() } };
            assert_eq!(got.settings, want, "{text:?}");
            assert!(got.left_behind.is_empty());
        }
    }

    #[test]
    fn words_follow_the_house_style() {
        let d = dirs("words");
        full(&d);
        let found = detect(&d.root, &d.home);
        let gs = groups(&found, &d.root, &[]);
        let got = import(Some(CONFIG_TEXT), &Settings::default(), &GameSwitches::default(), &[]);
        let all: Vec<String> = gs.iter().flat_map(|g| g.lost.iter().cloned().chain([g.line.clone(), g.title.to_string()])).chain(got.left_behind.iter().map(|n| n.words.clone())).collect();
        for words in &all {
            assert!(!words.contains('\u{2014}') && !words.contains('\u{2013}'), "{words}");
            assert!(!words.contains("TurbineHub") && !words.contains("you used") && !words.contains("Ally Hub"), "{words}");
        }
    }
}

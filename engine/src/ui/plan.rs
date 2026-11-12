//! The one dialog and what a confirmed dialog runs. Every change is shown first in plain words; the app runs the
//! steps in order only after the player says yes, and stops at the first one that fails.
use super::{Ui, arr, b, obj, s};
use crate::helper::{self, Setting};
use crate::json::Json;
use std::path::PathBuf;

/// One thing a confirmed dialog does.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// A command line run as it stands (host commands already start with flatpak-spawn --host).
    Host(Vec<String>),
    /// Installs or updates the privileged helper through sudo, with the dialog's password on stdin.
    HelperInstall,
    /// Sets the machine's first password: passwd, with the dialog's new password on stdin twice.
    SetPassword,
    /// Writes a file on the host (through tee, no shell): a lighting value, or the background helper's unit.
    HostWrite { path: String, value: String },
    /// Adds a tile to the Steam library that opens this address in the browser.
    Tile { name: String, url: String },
    /// Copies one game from the Windows drive into a SteamOS library: (from, to) pairs. The source is only read.
    CopyGame { name: String, pairs: Vec<(PathBuf, PathBuf)> },
    /// Moves a game's Windows files aside as a backup, and records the backup for Undo.
    ResetPrefix { pfx: PathBuf, app_id: String },
    /// Puts a game's Windows files back from the recorded backup; fresh ones made since are kept aside.
    ResetUndo { pfx: PathBuf, app_id: String },
    /// Writes a report about one game into Turbine Hub's folder.
    Report { game: String },
    /// Looks again after the helper opened the lights: these paths have to be open to the player now. Fails with
    /// what is still closed and why, in plain words (`exact`: with the paths, for Veteran mode).
    LightsCheck { need: Vec<String>, exact: bool },
    /// Saves the player's setup to the setup file.
    SaveSetup,
    /// Marks a first-run step or a Bring it over step as done once everything before it worked.
    Done { setup: Option<&'static str>, brought: Option<&'static str> },
    /// Marks the whole first run as done, and turns Game Boost on when it was ticked.
    SetupFinished { boost: bool },
    /// Turns one of the player's own settings on or off (see `set_choice`) once everything before it worked.
    Choice { key: &'static str, on: bool },
    /// Downloads the newer Turbine Hub, checks it against its published checksum and installs it.
    SelfUpdate,
    /// Writes the launch tool into Steam's folder for tools (or what differs of it), or takes it out again.
    Tool { install: bool },
    /// The Proton one game starts with through the launch tool: a Proton's folder, or None for Automatic.
    ProtonPick { app_id: String, dir: Option<PathBuf> },
    /// Puts the game's Proton pick from before the last change back.
    ProtonUndo { app_id: String },
    /// Ludusavi's work on a game's saves or on the Windows drive's: snapshot, restore, Undo.
    Saves(crate::saves::Job),
    /// Takes a tile out of the Steam library again.
    TileRemove { url: String },
    /// Adds Turbine Hub itself to the Steam library with its art, or takes that entry out again.
    SelfTile { add: bool },
    /// Writes Turbine Hub's own library art for its entry, never over the player's pictures.
    SelfArt,
    /// Adds Turbine Files (Turbine Hub opened straight on Files) to the Steam library with its art, or takes that
    /// entry out again.
    FilesTile { add: bool },
    /// Fetches NonSteamLaunchers' script (or its Decky plugin's installer) at this release tag and starts it on the
    /// host, where it shows its own windows. Not waited for: the player works in its window.
    Nsl { tag: String, plugin: bool },
    /// The storage saver's work: frees the ticked leftovers (each one looked at again first), or puts back what the
    /// last cleaning moved to the Recycle Bin.
    Storage(crate::storage::Job),
    /// Downloads a GE-Proton release ("" for the newest), checks it against its published SHA-512 and unpacks it
    /// into Steam's folder for tools.
    ProtonGet { tag: String },
    /// Takes a GE-Proton build Turbine Hub fetched out again, by its folder's name (refused while a game is set
    /// to use it).
    ProtonRemove { name: String },
    /// Draws the missing pictures of the library entries that have none, never over the player's own.
    TileArt,
    /// Removes exactly the pictures `TileArt` wrote, and the icons it set.
    TileArtUndo,
}

/// Which password the dialog asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    None,
    /// The machine's password, for the helper's one-time install.
    Current,
    /// A new password, typed twice.
    New,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub title: String,
    pub text: String,
    /// The exact changes: a few words each, and one line of why.
    pub items: Vec<(String, String)>,
    pub confirm: String,
    pub ask: Ask,
    /// Something is lost or replaced: the confirm button isn't the highlighted one at first.
    pub danger: bool,
    pub steps: Vec<Step>,
    /// The note when every step worked.
    pub done: String,
    /// The page to show afterwards, when not the current one.
    pub then_open: String,
    /// The name of the thing it is about ("Discord"), for the line of work in the top bar. "" takes the title.
    pub what: String,
}

impl Plan {
    pub fn new(title: &str, text: &str, confirm: &str) -> Plan {
        Plan { title: title.into(), text: text.into(), items: Vec::new(), confirm: confirm.into(), ask: Ask::None, danger: false, steps: Vec::new(), done: String::new(), then_open: String::new(), what: String::new() }
    }
    pub fn to_json(&self) -> Json {
        obj! {
            "title": s(&self.title),
            "text": s(&self.text),
            "items": arr(self.items.iter().map(|(t, l)| obj! {"title": s(t), "line": s(l)})),
            "confirm": s(&self.confirm),
            "cancel": s("Cancel"),
            "ask": s(match self.ask { Ask::None => "", Ask::Current => "current", Ask::New => "new" }),
            "danger": b(self.danger),
        }
    }
    /// Does any step download something (installs and updates from Flathub, Decky Loader, Turbine Hub itself,
    /// NonSteamLaunchers)? Removing apps works offline.
    pub fn needs_internet(&self) -> bool {
        self.steps.iter().any(|x| match x {
            Step::Host(c) => {
                let has = |w: &str| c.iter().any(|a| a == w);
                (has("flatpak") && (has("install") || has("update"))) || has("decky-install")
            }
            Step::SelfUpdate | Step::Nsl { .. } | Step::ProtonGet { .. } => true,
            _ => false,
        })
    }

    /// How many system changes the plan makes (helper jobs).
    pub fn helper_jobs(&self) -> usize {
        self.steps.iter().filter(|x| matches!(x, Step::Host(c) if c.iter().any(|a| a == helper::BIN))).count()
    }
}

pub(crate) fn toast(text: &str) -> Json {
    obj! {"toast": s(text)}
}
pub(crate) fn refresh() -> Json {
    obj! {"refresh": b(true)}
}
pub(crate) fn open(page: &str) -> Json {
    obj! {"open": s(page)}
}
pub(crate) fn state() -> Json {
    obj! {"state": b(true), "refresh": b(true)}
}

/// Shows the dialog and keeps the plan until the player answers.
pub(crate) fn dialog(ui: &mut Ui, plan: Plan) -> Json {
    // Downloads can't work offline: say so before anything is asked or changed.
    if ui.offline && plan.needs_internet() {
        return toast("You're offline. This needs the internet, so try again once you're connected.");
    }
    let j = plan.to_json();
    ui.pending = Some(plan);
    obj! {"dialog": j}
}

/// A dialog that only explains: steps to follow, one button to close.
pub(crate) fn guide(title: &str, text: &str, items: &[(&str, &str)]) -> Json {
    obj! {"dialog": obj! {
        "title": s(title),
        "text": s(text),
        "items": arr(items.iter().map(|(t, l)| obj! {"title": s(*t), "line": s(*l)})),
        "confirm": s(""),
        "cancel": s("Close"),
        "ask": s(""),
        "danger": b(false),
        // Steps for the player to follow: nothing in them is done yet, so they get no ticks.
        "guide": b(true),
    }}
}

/// Sets one of the player's own on or off settings by its key ("boost", "reports", "testbuilds").
pub fn set_choice(st: &mut crate::settings::Settings, key: &str, on: bool) {
    match key {
        "boost" => st.game_boost = on,
        "reports" => st.reports = on,
        "testbuilds" => st.test_builds = on,
        _ => {}
    }
}

/// Does the next system change ask for the password? Only while Turbine Hub's helper isn't on the machine, or is
/// the one of an older version. Nothing asks for it before that change, and nothing after it.
pub(crate) fn asks_password(ui: &Ui) -> bool {
    ui.helper_outdated() || !helper::installed(&ui.ctx.root)
}

/// How a dialog that asks for the password starts, when the plan doesn't say it in its own words.
const SYSTEM_CHANGE: &str = "This changes a system setting";

/// A plan that sends settings to the helper. When the helper isn't installed yet the plan installs it first and the
/// dialog asks for the password once; without a password on the machine the same dialog sets one first.
pub(crate) fn helper_plan(ui: &mut Ui, plan: Plan, settings: &[Setting]) -> Json {
    helper_plan_because(ui, plan, settings, SYSTEM_CHANGE)
}

/// The same, with the dialog's own first words about what in it is a system change ("3 of these 9 changes are
/// system settings").
pub(crate) fn helper_plan_because(ui: &mut Ui, mut plan: Plan, settings: &[Setting], lead: &str) -> Json {
    for st in settings {
        if let Err(e) = st.check() {
            return toast(&format!("That change isn't allowed: {e}"));
        }
        plan.steps.push(Step::Host(helper::apply(st)));
    }
    ask_for_helper(ui, plan, lead)
}

/// Puts the helper's install in front of a plan that needs it. A machine without a password gets one in the same
/// dialog: it is typed twice, set first, then used once for the helper.
pub(crate) fn with_helper(ui: &mut Ui, plan: Plan) -> Json {
    ask_for_helper(ui, plan, SYSTEM_CHANGE)
}

/// This is the only place Turbine Hub ever asks for the password, so the dialog's first lines say why it is asked
/// now, before the dialog's own words, and what is done with it is listed first.
fn ask_for_helper(ui: &mut Ui, mut plan: Plan, lead: &str) -> Json {
    let why = if ui.helper_outdated() {
        // The helper on the machine is from an older Turbine Hub: a new one only ever arrives with the password.
        plan.steps.insert(0, Step::HelperInstall);
        plan.ask = Ask::Current;
        plan.items.insert(0, ("Update Turbine Hub's helper".into(), "Asked once for each new helper. System changes after this ask for nothing.".into()));
        format!("{lead}, and Turbine Hub's helper on this machine is from an older version. Updating it takes your SteamOS password once more, like the admin prompt on Windows. Turbine Hub never stores it.")
    } else if !helper::installed(&ui.ctx.root) {
        plan.steps.insert(0, Step::HelperInstall);
        plan.items.insert(0, ("Set up Turbine Hub's helper".into(), "Once. It makes the system changes you approve and records each one. Later changes ask for no password.".into()));
        if ui.facts.has_password == Some(false) {
            plan.steps.insert(0, Step::SetPassword);
            plan.ask = Ask::New;
            plan.items.insert(0, ("Create this machine's password".into(), "Typed twice below. Write it down somewhere safe.".into()));
            format!("{lead}, so SteamOS asks for a password, like the admin prompt on Windows. This machine has none yet, so you create one now. SteamOS can't recover it if you forget it. Turbine Hub uses it once to set up its helper and never stores it.")
        } else {
            plan.ask = Ask::Current;
            format!("{lead}, so SteamOS asks for your password, like the admin prompt on Windows. Turbine Hub uses it once to set up its helper and never stores it.")
        }
    } else {
        String::new()
    };
    if !why.is_empty() {
        plan.text = if plan.text.is_empty() { why } else { format!("{why}\n\n{}", plan.text) };
    }
    dialog(ui, plan)
}

/// What a setting is on this machine now, as the kernel holds it. None when it can't be read from here.
fn live(ui: &Ui, st: &Setting) -> Option<String> {
    let read = |abs: &str| std::fs::read_to_string(ui.ctx.root.join(abs.trim_start_matches('/'))).ok().map(|t| turbinehub_helper::ops::live_word(&t)).filter(|t| !t.is_empty());
    match st {
        Setting::Sysctl { key, .. } => read(&format!("/proc/sys/{}", key.replace('.', "/"))),
        Setting::Sysfs { path, .. } => read(path),
        _ => None,
    }
}

/// A setting's name, with a USB device called by its own name when this machine knows it.
fn name_of(ui: &Ui, st: &Setting) -> String {
    if let Setting::Wake { vendor, product, .. } = st {
        let m = ui.machine();
        if let Some(d) = m.wake_devices.iter().find(|d| d.vendor == *vendor && d.product == *product && !d.name.is_empty()) {
            return format!("{} wakes the machine", d.name);
        }
    }
    st.name()
}

/// The exact key or path a setting changes and its raw values, for Veteran mode: "key: before → after".
fn exact_change(st: &Setting, before: Option<&str>, after: &str) -> String {
    let target = match st {
        Setting::Sysctl { key, .. } => key.clone(),
        Setting::Sysfs { path, .. } => path.clone(),
        _ => return st.exact(),
    };
    match before {
        Some(b) => format!("{target}: {b} \u{2192} {after}"),
        None => format!("{target} = {after}"),
    }
}

/// The raw value a setting sets, as the kernel takes it.
fn raw_value(st: &Setting) -> String {
    match st {
        Setting::Sysctl { value, .. } => value.to_string(),
        Setting::Sysfs { value, .. } => value.clone(),
        _ => String::new(),
    }
}

/// One line of a confirm dialog for a setting: its plain name, the value now and the value after; in Veteran mode
/// also exactly which key or file changes, with the raw values.
pub(crate) fn change_item(ui: &Ui, st: &Setting) -> (String, String) {
    let now = live(ui, st);
    let after = st.value_shown();
    let before = match st {
        Setting::Sysctl { .. } | Setting::Sysfs { .. } => now.as_deref().map(|v| st.shown(v)),
        // Offered as a switch: it is the other way now.
        Setting::Service { enabled, .. } => Some(if *enabled { "off" } else { "on" }.to_string()),
        Setting::Wake { vendor, product, .. } => ui.machine().wake_devices.iter().find(|d| d.vendor == *vendor && d.product == *product).map(|d| if d.enabled { "yes" } else { "no" }.to_string()),
        Setting::Zram { .. } => Some(format!("{}% of memory", crate::tuneup::zram_percent(&ui.ctx.root))),
        Setting::Module { .. } | Setting::Access { .. } => None,
    };
    let title = match before {
        Some(before) if before != after => format!("{}: {before} \u{2192} {after}", name_of(ui, st)),
        Some(_) => format!("{}: stays {after}", name_of(ui, st)),
        None => format!("{}: {after}", name_of(ui, st)),
    };
    (title, if ui.veteran() { exact_change(st, now.as_deref(), &raw_value(st)) } else { String::new() })
}

/// A recorded change taken apart for Undo: its name, the value it set, what Undo puts back (from the record's
/// "before"), and for Veteran mode the exact key or file with the raw values ("" in Guided mode).
pub(crate) struct Undone {
    pub name: String,
    pub now: String,
    pub back: String,
    pub exact: String,
}

/// One line of an Undo dialog: the value now and what Undo puts back.
pub(crate) fn undo_item(ui: &Ui, e: &helper::Entry) -> (String, String) {
    let u = undone(ui, e);
    (format!("{}: {} \u{2192} {}", u.name, u.now, u.back), u.exact)
}

pub(crate) fn undone(ui: &Ui, e: &helper::Entry) -> Undone {
    let st = match &e.change {
        helper::Change::Set(st) => st,
        helper::Change::Decky { tag } => return Undone { name: format!("Decky Loader {tag}"), now: "installed".into(), back: "removed, or the one from before".into(), exact: if ui.veteran() { "The loader and its unit from before go back, if there were any".into() } else { String::new() } },
    };
    let before = e.before("value").or(e.before("enabled"));
    let back = match (st, before) {
        (Setting::Sysctl { key, .. }, Some("0")) if turbinehub_helper::ops::ratio_of(key).is_some() => {
            let (_, default) = turbinehub_helper::ops::ratio_of(key).unwrap_or(("", "0"));
            format!("{}% of memory, as before", e.before("ratio").unwrap_or(default))
        }
        (Setting::Sysctl { .. } | Setting::Sysfs { .. } | Setting::Service { .. }, Some(v)) => st.shown(v),
        (Setting::Zram { .. }, _) => "SteamOS's own size".into(),
        (Setting::Wake { wake, .. }, _) => if *wake { "no, as before" } else { "yes, as before" }.into(),
        (Setting::Access { .. }, _) => "off".into(),
        (Setting::Module { .. }, _) => "off from the next start".into(),
        _ => "as it was before".into(),
    };
    let exact = if !ui.veteran() {
        String::new()
    } else if let (Setting::Sysctl { key, .. }, Some("0")) = (st, before) {
        match turbinehub_helper::ops::ratio_of(key) {
            Some((ratio, d)) => format!("{key}: {} \u{2192} off; {ratio} = {}", raw_value(st), e.before("ratio").unwrap_or(d)),
            None => exact_change(st, Some(&raw_value(st)), "0"),
        }
    } else {
        match st {
            Setting::Sysctl { .. } | Setting::Sysfs { .. } => match before {
                Some(b) => exact_change(st, Some(&raw_value(st)), b),
                None => format!("{}, nothing recorded to put back", st.exact()),
            },
            Setting::Service { unit, .. } => format!("systemctl {} --now {unit}", if before == Some("true") { "enable" } else { "disable" }),
            _ => match st.file() {
                Some(f) => format!("Taken out of {f}"),
                None => st.exact(),
            },
        }
    };
    Undone { name: name_of(ui, st), now: st.value_shown(), back, exact }
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Json {
    if action == "open" {
        return open(arg);
    }
    let found = super::files::act(ui, action, arg, value)
        .or_else(|| super::settings::act(ui, action, arg, value))
        .or_else(|| super::home::act(ui, action, arg, value))
        .or_else(|| super::games::act(ui, action, arg, value))
        .or_else(|| super::launch::act(ui, action, arg, value))
        .or_else(|| super::saves::act(ui, action, arg, value))
        .or_else(|| super::selftile::act(ui, action, arg, value))
        .or_else(|| super::onepress::act(ui, action, arg, value))
        .or_else(|| super::machine::act(ui, action, arg, value))
        .or_else(|| super::store::act(ui, action, arg, value))
        .or_else(|| super::nsl::act(ui, action, arg, value))
        .or_else(|| super::bring::act(ui, action, arg, value))
        .or_else(|| super::setup::act(ui, action, arg, value))
        .or_else(|| super::help::act(ui, action, arg, value))
        .or_else(|| super::queue::act(ui, action, arg, value))
        .or_else(|| super::whatsnew::act(ui, action, arg, value));
    found.unwrap_or_else(|| obj! {})
}

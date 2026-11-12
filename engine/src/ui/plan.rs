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
    /// Writes one value to a lighting file on the host (through tee, no shell).
    HostWrite { path: String, value: String },
    /// Adds a tile to the Steam library that opens this address in the browser.
    Tile { name: String, url: String },
    /// Copies one game from the Windows drive into a SteamOS library: (from, to) pairs. The source is only read.
    CopyGame { name: String, pairs: Vec<(PathBuf, PathBuf)> },
    /// Moves a game's Windows files aside as a backup.
    ResetPrefix { pfx: PathBuf },
    /// Writes a report about one game into Turbine Hub's folder.
    Report { game: String },
    /// Saves the player's setup to the setup file.
    SaveSetup,
    /// Marks a first-run step or a Bring it over step as done once everything before it worked.
    Done { setup: Option<&'static str>, brought: Option<&'static str> },
    /// Marks the whole first run as done, and turns Game Boost on when it was ticked.
    SetupFinished { boost: bool },
    /// Downloads the newer Turbine Hub, checks it against its published checksum and installs it.
    SelfUpdate,
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
}

impl Plan {
    pub fn new(title: &str, text: &str, confirm: &str) -> Plan {
        Plan { title: title.into(), text: text.into(), items: Vec::new(), confirm: confirm.into(), ask: Ask::None, danger: false, steps: Vec::new(), done: String::new(), then_open: String::new() }
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
    }}
}

/// A plan that sends settings to the helper. When the helper isn't installed yet the plan installs it first and the
/// dialog asks for the password once; without a password on the machine the player is sent to set one.
pub(crate) fn helper_plan(ui: &mut Ui, mut plan: Plan, settings: &[Setting]) -> Json {
    for st in settings {
        if let Err(e) = st.check() {
            return toast(&format!("That change isn't allowed: {e}"));
        }
        plan.steps.push(Step::Host(helper::apply(st)));
    }
    with_helper(ui, plan)
}

/// Puts the helper's install in front of a plan that needs it.
pub(crate) fn with_helper(ui: &mut Ui, mut plan: Plan) -> Json {
    if !helper::installed(&ui.ctx.root) {
        if ui.facts.has_password == Some(false) {
            return super::with(toast("Set a password first. System changes need one."), "open", s("password"));
        }
        plan.steps.insert(0, Step::HelperInstall);
        plan.ask = Ask::Current;
        plan.items.push(("Your SteamOS password, asked once".into(), "It sets up Turbine Hub's helper, which makes system changes and records each one. The password is never stored.".into()));
    }
    dialog(ui, plan)
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Json {
    if action == "open" {
        return open(arg);
    }
    let found = super::settings::act(ui, action, arg, value)
        .or_else(|| super::games::act(ui, action, arg, value))
        .or_else(|| super::machine::act(ui, action, arg, value))
        .or_else(|| super::store::act(ui, action, arg, value))
        .or_else(|| super::bring::act(ui, action, arg, value))
        .or_else(|| super::setup::act(ui, action, arg, value))
        .or_else(|| super::help::act(ui, action, arg, value));
    found.unwrap_or_else(|| obj! {})
}

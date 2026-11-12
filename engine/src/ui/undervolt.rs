//! Graphics card undervolting in My machine's Graphics card section: Veteran mode only, folded under Advanced, with
//! its warnings before anything else. A new value is only ever tried, and it has to pass two checks before it
//! counts (`crate::uvcheck`): a 10 second question, then 5 minutes of play and the question again. A value that
//! passed stays on until the machine restarts and is never turned on again by itself: nothing puts an offset back
//! at a start, a login or a wake (the owner's decision until the feature has been tested on real cards).
//!
//! The window calls `uv_tick` every frame and runs what `uv_take_run` hands it (only ever going back, or noting a
//! driver reset), and `uv_close` as it closes. The session lives in the data folder (`undervolt-session.json`)
//! with the boot it belongs to, so a window that was killed in the middle of a check goes back at its next start,
//! and a restart forgets everything but the note of the last value that passed (for "Use again").
use super::machine::marked;
use super::plan::{Plan, Step, dialog, go, helper_plan, refresh, toast, undo_item, with_helper};
use super::{Ui, arr, b, n, obj, row, s, section, with};
use crate::hardware::Machine;
use crate::helper;
use crate::json::{self, Json};
use crate::modules;
use crate::uvcheck::{self, Clock, Do, Look, Out, Session, Stage, Underway};
use std::path::Path;
use turbinehub_helper::gpu;
use turbinehub_helper::policy::mv_words;

/// The session's file in the data folder.
const FILE: &str = "undervolt-session.json";

/// The last value that passed both checks, and the kernel it passed on: "Use it again" may skip the 5 minutes on
/// the same kernel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Note {
    pub offset: i32,
    pub kernel: String,
}

/// The checks of this session and what the window has to run for them.
#[derive(Debug, Default)]
pub(crate) struct Watch {
    pub session: Session,
    pub note: Option<Note>,
    pub clock: Clock,
    /// The card's slot, for reading its voltage between looks.
    pub slot: String,
    /// The helper job waiting for the window to run it, and the one running.
    run: Option<Do>,
    running: Option<Do>,
    /// What to say at the next tick (a check that was underway when the window was killed).
    pending: Option<Out>,
    looked: Option<u64>,
    /// The countdown last told to the window.
    told: Option<u64>,
}

fn read(root: &Path, abs: &str) -> Option<String> {
    std::fs::read_to_string(root.join(abs.trim_start_matches('/'))).ok()
}

fn boot_id(root: &Path) -> String {
    read(root, gpu::BOOT_ID).unwrap_or_default().trim().to_string()
}

fn kernel(root: &Path) -> String {
    read(root, gpu::OSRELEASE).unwrap_or_default().trim().to_string()
}

fn od_path(slot: &str) -> String {
    format!("/sys/bus/pci/devices/{slot}/pp_od_clk_voltage")
}

impl Watch {
    /// The session as the window left it. From another boot only the note is kept: the card started at its own
    /// voltage. From this boot with a check underway, the window was killed in the middle of it: it goes back.
    pub fn load(data: &Path, root: &Path) -> Watch {
        let mut w = Watch::default();
        let Some(j) = std::fs::read_to_string(data.join(FILE)).ok().and_then(|t| json::parse(&t).ok()) else { return w };
        let int = |k: &str| j.get(k).and_then(Json::as_f64).map(|v| v as i32);
        w.note = int("last").map(|offset| Note { offset, kernel: j.str_at("lastKernel").unwrap_or("").to_string() });
        w.slot = j.str_at("slot").unwrap_or("").to_string();
        if j.str_at("boot") != Some(boot_id(root).as_str()) || boot_id(root).is_empty() {
            return w;
        }
        w.session.on = int("on");
        w.session.suspends = j.get("suspends").and_then(Json::as_f64).map(|v| v as u64);
        if let (Some(stage), Some(offset)) = (j.str_at("stage").and_then(Stage::from_key), int("offset")) {
            w.session.underway = Some(Underway { stage, offset, back: int("back").unwrap_or(0), until: 0, short: false });
            w.pending = Some(w.session.back("Turbine Hub closed during the check last time"));
        }
        w
    }

    fn save(&self, data: &Path, root: &Path) {
        let opt = |v: Option<i32>| v.map(|v| n(v as f64)).unwrap_or(Json::Null);
        let u = self.session.underway.as_ref();
        let j = obj! {
            "boot": s(boot_id(root)),
            "slot": s(&self.slot),
            "on": opt(self.session.on),
            "suspends": self.session.suspends.map(|v| n(v as f64)).unwrap_or(Json::Null),
            "stage": s(u.map(|u| u.stage.key()).unwrap_or("")),
            "offset": opt(u.map(|u| u.offset)),
            "back": opt(u.map(|u| u.back)),
            "last": opt(self.note.as_ref().map(|x| x.offset)),
            "lastKernel": s(self.note.as_ref().map(|x| x.kernel.as_str()).unwrap_or("")),
        };
        let _ = std::fs::create_dir_all(data);
        let _ = crate::write_atomic(&data.join(FILE), (j.to_text() + "\n").as_bytes());
    }
}

/// Where undervolting stands on this machine.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Uv {
    /// Nothing to show: not a desktop with an AMD card.
    Hidden,
    /// An AMD card that isn't an RX 7000 or RX 9000 (RX 6000 and older).
    Unsupported,
    /// Overdrive is off. `pending`: it was turned on and waits for the restart.
    Off { pending: bool },
    /// Overdrive is on and the card gives no voltage range. `leaving`: tuning was turned off, it goes at the restart.
    NoRange { leaving: bool },
    /// Ready: the offset the card holds now, the lowest one taken, the value that passed and is on until the
    /// restart, the one being checked, the note of the last one that passed, what tripped.
    Ready { now: i32, lo: i32, on: Option<i32>, underway: Option<Underway>, note: Option<Note>, tripped: Option<i32>, leaving: bool },
}

pub(crate) fn state(ui: &Ui, m: &Machine) -> Uv {
    let Some(card) = modules::undervolt_card(m) else {
        return if modules::undervolt_unsupported(m) { Uv::Unsupported } else { Uv::Hidden };
    };
    let (_, recorded) = modules::undervolt_recorded(m, &ui.ledger());
    if !m.overdrive_on() {
        return Uv::Off { pending: recorded };
    }
    let Some((lo, _)) = card.od.and_then(|od| od.allowed()) else { return Uv::NoRange { leaving: !recorded } };
    let tripped = read(&ui.ctx.root, gpu::TRIPPED).and_then(|t| gpu::parse_tripped(&t)).map(|t| t.0);
    let now = card.od.and_then(|od| od.offset).unwrap_or(0);
    let w = &ui.uv;
    Uv::Ready { now, lo, on: w.session.on.filter(|v| *v != 0 && *v == now), underway: w.session.underway.clone(), note: w.note.clone(), tripped, leaving: !recorded }
}

/// The words for what tripped.
pub(crate) const TRIPPED_WORDS: &str = "The graphics card's lower voltage was turned off because the machine stopped unexpectedly last time. Try a smaller step?";
/// What a value that passed is, said where it shows.
pub(crate) const UNTIL_RESTART: &str = "Lower voltage is on until the machine restarts. It is never turned on by itself at start.";

/// The fold's badge: (words, tone).
fn badge(st: &Uv) -> (String, &'static str) {
    match st {
        Uv::Off { pending: true } => ("Restart needed".into(), "warn"),
        Uv::Ready { underway: Some(u), .. } => (format!("Checking {}", mv_words(u.offset)), "warn"),
        Uv::Ready { tripped: Some(_), .. } => ("Turned off after a stop".into(), "stop"),
        Uv::Ready { leaving: true, .. } | Uv::NoRange { leaving: true } => ("Off after a restart".into(), "warn"),
        Uv::Ready { on: Some(v), .. } => (format!("{} until restart", mv_words(*v)), "ok"),
        _ => (String::new(), ""),
    }
}

/// The checkup's item: a trip to look at, or a restart that is due. (look, title, line).
pub(crate) fn checkup(ui: &Ui) -> Option<(bool, String, String)> {
    if !ui.veteran() {
        return None;
    }
    match state(ui, &ui.machine()) {
        Uv::Ready { tripped: Some(v), .. } => Some((true, "The graphics card's lower voltage was turned off".into(), format!("{} was on when the machine stopped unexpectedly. Try a smaller step?", mv_words(v)))),
        Uv::Off { pending: true } => Some((false, "Restart to turn on graphics card tuning".into(), "Undervolting can be set after the next start.".into())),
        _ => None,
    }
}

fn alert(id: &str, title: &str, line: &str, badge: &str) -> Json {
    with(with(with(with(row("none", id, "info", title, line), "alert", b(true)), "badge", s(badge)), "tone", s("stop")), "icon", s("warn"))
}

/// Whether "Use it again" may skip the 5 minutes: the same value passed on this same kernel, and nothing tripped.
fn short(ui: &Ui, offset: i32, tripped: Option<i32>) -> bool {
    tripped.is_none() && ui.uv.note.as_ref().is_some_and(|x| x.offset == offset && x.offset != 0 && x.kernel == kernel(&ui.ctx.root))
}

/// The Advanced section of the Graphics card page, in Veteran mode. Empty in Guided mode and where nothing applies.
pub(crate) fn section_rows(ui: &Ui, m: &Machine) -> Option<Json> {
    if !ui.veteran() {
        return None;
    }
    let st = state(ui, m);
    if st == Uv::Hidden {
        return None;
    }
    let (word, tone) = badge(&st);
    let open = ui.uv_open;
    let mut rows = vec![with(with(with(with(row("uv-fold", "", "button", "Undervolting", "A lower voltage for the graphics card: a few more frames, or a cooler, quieter card. Read the warnings first."), "button", s(if open { "Hide" } else { "Show" })), "badge", s(word)), "tone", s(tone)), "icon", s("gauge"))];
    if let Uv::Ready { tripped: Some(_), .. } = st {
        rows.push(alert("tripped", "Turned off after a stop", TRIPPED_WORDS, "Turned off"));
    }
    // A check underway shows folded too, with its Stop.
    if let Uv::Ready { underway: Some(u), .. } = &st {
        let words = ui.uv.session.words(ui.uv.clock.now());
        let line = match u.stage {
            Stage::Check => format!("The lower voltage stays on while you play. Stop goes back to {} at once.", uvcheck::mv(u.back)),
            _ => format!("Waiting for your answer. Stop goes back to {} at once.", uvcheck::mv(u.back)),
        };
        rows.push(with(with(with(with(with(row("uv-stop", "", "button", &words, &line), "button", s("Stop")), "icon", s("close")), "primary", b(true)), "liveTitle", s("check")), "tone", s("warn")));
    }
    if !open {
        return Some(section("Advanced", rows));
    }
    rows.push(alert(
        "warn-crash",
        "Undervolting can crash games",
        "Too low a voltage makes games crash, or the screen go black until a hard restart. Nothing is damaged, and if the machine stops, Turbine Hub turns the lower voltage back off by itself. How much it gains varies from card to card.",
        "Read first",
    ));
    rows.push(alert(
        "warn-tuning",
        "Radeon RX 7000 and RX 9000 only, after a restart",
        "Turning it on needs a restart and marks the system log as tuned. Turn tuning off before you report graphics problems to Valve. RX 6000 and older cards, handhelds and laptops aren't supported.",
        "Read first",
    ));
    let off = || with(with(row("uv-off", "", "button", "Turn tuning off", "Removes the driver option. The card runs as made from the next start."), "button", s("Turn off")), "icon", s("restore"));
    let restart = |title: &str, line: &str| with(with(with(with(row("uv-restart", "", "button", title, line), "button", s("Restart")), "icon", s("reboot")), "badge", s("Restart needed")), "tone", s("warn"));
    match &st {
        Uv::Hidden => {}
        Uv::Unsupported => rows.push(row("none", "unsupported", "info", "This card isn't supported", "Undervolting is for Radeon RX 7000 and RX 9000 cards. RX 6000 and older cards don't say how far their voltage may go, so Turbine Hub won't change it.")),
        Uv::Off { pending: false } => rows.push(marked(ui, with(with(row("uv-on", "", "button", "Turn on graphics card tuning", "Lets the driver take a lower voltage. Needs a restart. Asks twice first."), "button", s("Turn on")), "icon", s("bolt")))),
        Uv::Off { pending: true } => {
            rows.push(restart("Restart to finish turning it on", "Graphics card tuning starts with the next start."));
            rows.push(off());
        }
        Uv::NoRange { leaving } => {
            rows.push(row("none", "no-range", "info", "This card doesn't give a voltage range", gpu::NO_RANGE));
            if *leaving { rows.push(restart("Restart to finish turning tuning off", "The driver option is gone already.")) } else { rows.push(off()) }
        }
        Uv::Ready { now, lo, on, underway, note, tripped, leaving } => {
            let line = match (on, underway) {
                (_, Some(_)) => "Being checked: it counts only after the 5 minutes and your second Keep.".to_string(),
                (Some(_), None) => UNTIL_RESTART.to_string(),
                (None, None) if *now != 0 => "Not set by this session's checks. Safe reset puts the card back at its own voltage.".to_string(),
                _ => "The card's own voltage. Nothing turns a lower one on by itself at start.".to_string(),
            };
            let mut now_row = with(row("none", "now", "info", "Voltage offset now", &line), "value", s(mv_words(*now)));
            if on.is_some() && underway.is_none() {
                now_row = with(with(now_row, "badge", s("Until restart")), "tone", s("ok"));
            }
            rows.push(now_row);
            // The last value that passed, offered again (through Try) when it isn't on and didn't stop the machine.
            if let Some(x) = note.as_ref().filter(|x| x.offset != 0 && x.offset != *now && x.offset >= *lo && Some(x.offset) != *tripped)
                && underway.is_none()
            {
                let line = if short(ui, x.offset, *tripped) {
                    "It passed the 5 minute check on this system before, so only the 10 second question comes again."
                } else {
                    "It passed before on another kernel, so both checks come again: 10 seconds, then 5 minutes."
                };
                rows.push(marked(ui, with(with(row("uv-again", "", "button", &format!("Use {} again", mv_words(x.offset)), line), "button", s("Try")), "icon", s("gauge"))));
            }
            let notes = arr([(gpu::WARN_MV, "Below -50 mV many cards crash in some games. Watch a game you know before you keep it."), (gpu::DEEP_MV, "Below -100 mV is past what most cards take. Expect crashes and black screens.")].iter().map(|(below, text)| obj! {"below": n(*below as f64), "text": s(*text)}));
            let mut slider = row("uv-try", "", "slider", "Lower voltage", "Pick a step, then A tries it: 10 seconds to keep it, then 5 minutes of play and one more Keep.");
            for (k, v) in [("min", *lo), ("max", 0), ("value", *now), ("step", gpu::STEP_MV)] {
                slider = with(slider, k, n(v as f64));
            }
            rows.push(marked(ui, with(with(with(with(with(slider, "unit", s(" mV")), "notes", notes), "showFrom", b(true)), "low", s(format!("{lo} mV"))), "high", s("0 mV"))));
            rows.push(with(with(with(row("uv-reset", "", "button", "Safe reset", "Back to the card's own voltage at once. Asks nothing."), "button", s("Reset")), "icon", s("restore")), "primary", b(*now != 0)));
            if *leaving { rows.push(restart("Restart to finish turning tuning off", "The driver option is gone already.")) } else { rows.push(off()) }
        }
    }
    Some(section("Advanced", rows))
}

fn host(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host"].iter().chain(args).map(|x| x.to_string()).collect()
}

/// The question on screen while a value is being checked: the 10 second one after Try, or the 60 second one after
/// the 5 minutes. The engine's clock answers it when nobody does (`engine`): the window only counts down.
fn question(u: &Underway, left: u64) -> Json {
    let (title, text, confirm, cancel) = match u.stage {
        Stage::Again => (
            format!("Still running well at {}?", uvcheck::mv(u.offset)),
            format!("Going back to {} in {{n}} s. Keep it only if games ran with no crash, stutter or odd colors. It stays on until the machine restarts.", uvcheck::mv(u.back)),
            "Keep it for this session".to_string(),
            format!("Go back to {}", uvcheck::mv(u.back)),
        ),
        _ => (
            format!("Keep {}?", uvcheck::mv(u.offset)),
            format!(
                "Going back to {} in {{n}} s. {}",
                uvcheck::mv(u.back),
                if u.short { "It passed the 5 minute check on this system before, so Keep turns it on until the machine restarts." } else { "Keep starts 5 minutes of play with it; it counts only after that." }
            ),
            "Keep".to_string(),
            "Go back".to_string(),
        ),
    };
    obj! {
        "title": s(title),
        "text": s(text),
        "seconds": n(left as f64),
        "engine": b(true),
        "confirm": s(confirm),
        "cancel": s(cancel),
        "act": s("uv-back"),
        "arg": s(""),
        "value": s(""),
        "keepAct": s("uv-keep"),
        "keepValue": s(u.offset.to_string()),
    }
}

impl Ui {
    fn uv_save(&self) {
        self.uv.save(&self.data, &self.ctx.root);
    }

    /// The helper's command line for what the session asks.
    fn uv_line(&self, d: &Do) -> Option<Vec<String>> {
        let m = self.machine();
        match d {
            Do::Back { to } => modules::gpu_job(&m, "back", Some(*to)),
            Do::Trip { offset } => modules::gpu_job(&m, "trip", Some(*offset)),
        }
    }

    /// Takes in what a step of the session said: the job to run, the question that closes or opens. Answers the
    /// window's news ({toast, close, keep, refresh}).
    fn uv_out(&mut self, out: Out) -> Option<Json> {
        if out == Out::default() {
            return None;
        }
        if let Some(d) = out.run {
            self.uv.run = Some(d);
        }
        self.uv_save();
        let mut j = obj! {"refresh": b(true)};
        if let Some(say) = out.say {
            j = with(j, "toast", s(say));
        }
        if out.close {
            j = with(j, "close", b(true));
        }
        if out.again
            && let Some(u) = &self.uv.session.underway
        {
            j = with(j, "keep", question(u, uvcheck::AGAIN_MS / 1000));
        }
        Some(j)
    }

    /// Every frame: the clock and, every two seconds while a value is on, the card. Answers news for the window
    /// when there is any: a countdown that moved ({left, status}), a question that opens or closes, words to say.
    pub fn uv_tick(&mut self) -> Option<String> {
        let mut news = Vec::new();
        if let Some(out) = self.uv.pending.take() {
            news.extend(self.uv_out(out));
        }
        let s_ = &self.uv.session;
        if s_.underway.is_none() && s_.on.is_none_or(|v| v == 0) {
            if self.uv.told.take().is_some() {
                news.push(obj! {"status": b(true)});
            }
            return merge(news);
        }
        let now = self.uv.clock.now();
        let out = self.uv.session.tick(now);
        news.extend(self.uv_out(out));
        if self.uv.looked.is_none_or(|at| now >= at + uvcheck::LOOK_MS) && self.uv.running.is_none() && self.uv.run.is_none() {
            self.uv.looked = Some(now);
            let card = read(&self.ctx.root, &od_path(&self.uv.slot)).and_then(|t| gpu::parse_od(&t).offset);
            let suspends = read(&self.ctx.root, crate::agent::SUSPEND_COUNT).as_deref().and_then(crate::agent::suspend_count);
            let boot = boot_id(&self.ctx.root);
            let armed = read(&self.ctx.root, gpu::ARMED).and_then(|t| gpu::parse_armed(&t)).filter(|(b, ..)| *b == boot).map(|(_, _, v)| v);
            let before = self.uv.session.clone();
            let out = self.uv.session.look(Look { card, suspends, armed });
            if out == Out::default() && self.uv.session != before {
                self.uv_save();
            }
            news.extend(self.uv_out(out));
        }
        let left = self.uv.session.left(now);
        if left != self.uv.told {
            self.uv.told = left;
            news.push(obj! {"left": left.map(|l| n(l as f64)).unwrap_or(Json::Null), "status": b(true)});
        }
        merge(news)
    }

    /// The helper job the session asks for, for the window to run (on a made-up machine it plays the helper's
    /// part on the made-up card instead, and the window answers `uv_done` at once).
    pub fn uv_take_run(&mut self) -> Option<Vec<String>> {
        let d = self.uv.run.take()?;
        let line = self.uv_line(&d);
        if line.is_some() {
            self.uv.running = Some(d);
        }
        line
    }

    /// The job ended. Answers the window's news.
    pub fn uv_done(&mut self, result: Result<(), String>) -> Option<String> {
        let d = self.uv.running.take();
        self.uv.looked = None;
        if self.sample
            && let Some(Do::Back { to }) = d
        {
            crate::fake::undervolt(&self.ctx.root, "kept", to);
        }
        match result {
            Ok(()) => Some(obj! {"refresh": b(true)}.to_text()),
            Err(e) => {
                // A failed Back leaves the card where it was: nothing of this session holds it any more.
                if matches!(d, Some(Do::Back { .. })) {
                    self.uv.session.on = None;
                    self.uv_save();
                }
                Some(obj! {"refresh": b(true), "toast": s(format!("The graphics card couldn't go back ({e}). Press Safe reset in Graphics card."))}.to_text())
            }
        }
    }

    /// Turbine Hub is closing: a value still being checked goes back. The command to run before the window goes.
    pub fn uv_close(&mut self) -> Option<Vec<String>> {
        let out = self.uv.session.close();
        if out == Out::default() {
            return None;
        }
        self.uv_save();
        out.run.and_then(|d| self.uv_line(&d))
    }

    /// The words for the top bar while a value is being checked ("Checking -30 mV: 4:12 left"), else "".
    pub fn uv_words(&self) -> String {
        self.uv.session.words(self.uv.clock.now())
    }

    /// The same, short, for a narrow top bar: "4:12".
    pub fn uv_short(&self) -> String {
        let now = self.uv.clock.now();
        self.uv.session.underway.as_ref().map(|u| uvcheck::clock_words(u.until.saturating_sub(now))).unwrap_or_default()
    }

    /// Stops the checks' clock (the pictures, which must look the same every time).
    pub fn uv_freeze_clock(&mut self) {
        let now = self.uv.clock.now();
        self.uv.clock.set(now);
    }
}

fn merge(news: Vec<Json>) -> Option<String> {
    if news.is_empty() {
        return None;
    }
    let mut out = obj! {};
    for j in news {
        if let Json::Obj(e) = j {
            for (k, v) in e {
                out = with(out, &k, v);
            }
        }
    }
    Some(out.to_text())
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    if !action.starts_with("uv-") {
        return None;
    }
    let m = ui.machine();
    let offset = || value.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v.round() as i32);
    let st = state(ui, &m);
    let veteran = ui.veteran();
    let job = |what: &str, o: Option<i32>| modules::gpu_job(&m, what, o).map(Step::Host);
    let now_ms = ui.uv.clock.now();
    Some(match action {
        "uv-fold" => {
            ui.uv_open = !ui.uv_open;
            refresh()
        }
        "uv-on" => {
            let Some(setting) = modules::gpu_overdrive(&m) else { return Some(toast(if modules::undervolt_card(&m).is_some() { "The graphics driver's settings couldn't be read, so tuning can't be turned on right now." } else { "Graphics card tuning is for Radeon RX 7000 and RX 9000 cards." })) };
            let mut p = Plan::new(
                "Turn on graphics card tuning?",
                "It lets Turbine Hub give the graphics card a lower voltage. Too low makes games crash or the screen go black; nothing is damaged, and Turbine Hub turns it back off by itself if the machine stops.",
                "Turn on",
            );
            p.danger = true;
            p.items = vec![
                ("Writes one line for the graphics driver".into(), if veteran { setting.exact() } else { "Overdrive on from the next start. Turn tuning off removes it.".into() }),
                ("Needs a restart".into(), "Nothing changes until the machine starts again. Restart now or later is asked next.".into()),
                ("Marks the system log as tuned".into(), "The kernel notes that overdrive is on and sets its tuned flag (taint S). Turn tuning off before you report graphics problems to Valve.".into()),
            ];
            p.next = Some(("uv-restart".into(), "on".into(), String::new()));
            p.done = "Graphics card tuning turns on after a restart".into();
            helper_plan(ui, p, &[setting])
        }
        "uv-restart" => {
            let (title, text) = if arg == "off" || matches!(st, Uv::Ready { leaving: true, .. } | Uv::NoRange { leaving: true }) {
                ("Restart now to turn tuning off?", "Graphics card tuning goes with the next start. Close your game first: the machine restarts right away.")
            } else {
                ("Restart now?", "Graphics card tuning starts with the next start. Close your game first: the machine restarts right away.")
            };
            let mut p = Plan::new(title, text, "Restart now");
            p.cancel = "Later".into();
            p.steps.push(Step::Host(host(&["systemctl", "reboot"])));
            p.done = "Restarting".into();
            dialog(ui, p)
        }
        "uv-try" | "uv-again" => {
            let Uv::Ready { now, lo, underway, tripped, note, .. } = st else { return Some(toast("Graphics card tuning isn't on yet.")) };
            if underway.is_some() {
                return Some(toast("A value is being checked. Stop it first."));
            }
            let o = if action == "uv-again" { note.map(|x| x.offset)? } else { offset()? };
            if o == now {
                return Some(toast("That's already set."));
            }
            if o < lo || o > 0 || o % gpu::STEP_MV != 0 {
                return Some(toast(&format!("This card takes {lo} to 0 mV in steps of {} mV.", gpu::STEP_MV)));
            }
            let quick = short(ui, o, tripped);
            let mut p = Plan::new(
                &format!("Try {}?", mv_words(o)),
                &format!(
                    "The card runs at it at once. Keep it within {} seconds, or it goes back by itself. {} If the screen freezes or goes black, hold the power button: nothing turns it on again at the next start.",
                    uvcheck::ASK_MS / 1000,
                    if quick { "It passed the 5 minute check on this system before, so Keep turns it on until the machine restarts." } else { "Keep then starts 5 minutes of play with it, and asks once more at the end." }
                ),
                "Try it",
            );
            let setting = modules::gpu_voltage(&m, o)?;
            p.items.push((format!("Graphics card voltage: {} \u{2192} {}", mv_words(now), mv_words(o)), if veteran { setting.exact() } else { String::new() }));
            p.items.push(("Until the machine restarts at most".into(), "Nothing turns a lower voltage on by itself at start, login or wake.".into()));
            if o < gpu::WARN_MV {
                p.items.push(("Below -50 mV".into(), "Many cards crash in some games down here. Watch a game you know for a while before you keep it.".into()));
            }
            if o < gpu::DEEP_MV {
                p.items.push(("Below -100 mV".into(), "Past what most cards take. Expect crashes and black screens. Nothing is damaged.".into()));
            }
            p.danger = o < gpu::WARN_MV;
            p.steps.extend(job("try", Some(o)));
            p.next = Some(("uv-keep-ask".into(), if quick { "short".into() } else { String::new() }, o.to_string()));
            p.done = format!("Trying {}", mv_words(o));
            with_helper(ui, p)
        }
        // The try worked: the 10 second question.
        "uv-keep-ask" => {
            let o = offset()?;
            let slot = modules::undervolt_card(&m).map(|g| g.slot.clone()).unwrap_or_default();
            ui.uv.slot = slot;
            if ui.sample {
                crate::fake::undervolt(&ui.ctx.root, "kept", o);
            }
            ui.uv.session.tried(now_ms, o, arg == "short");
            // The sleeps counted when the try began, so one in the middle of the check is seen.
            if let Some(c) = read(&ui.ctx.root, crate::agent::SUSPEND_COUNT).as_deref().and_then(crate::agent::suspend_count) {
                ui.uv.session.suspends = Some(c);
            }
            ui.uv.looked = Some(now_ms);
            ui.uv.told = None;
            ui.uv_save();
            let u = ui.uv.session.underway.clone()?;
            obj! {"keep": question(&u, uvcheck::ASK_MS / 1000)}
        }
        // The question on screen again (the pictures, and a window that lost it).
        "uv-question" => match ui.uv.session.underway.clone() {
            Some(u) if u.stage != Stage::Check => obj! {"keep": question(&u, ui.uv.session.left(now_ms).unwrap_or(0))},
            _ => refresh(),
        },
        "uv-keep" => {
            let out = ui.uv.session.keep(now_ms);
            if out == Out::default() {
                return Some(obj! {"toast": s("That question is over already."), "refresh": b(true)});
            }
            // A value that passed both checks is noted for "Use it again", with the kernel it passed on.
            if ui.uv.session.underway.is_none()
                && let Some(v) = ui.uv.session.on
            {
                ui.uv.note = Some(Note { offset: v, kernel: kernel(&ui.ctx.root) });
            }
            ui.uv.told = None;
            ui.uv_out(out).unwrap_or_else(refresh)
        }
        "uv-back" | "uv-stop" => {
            let out = ui.uv.session.back(if action == "uv-stop" { "Stopped" } else { "Going back" });
            ui.uv.told = None;
            ui.uv_out(out).unwrap_or_else(refresh)
        }
        "uv-reset" => {
            let mut p = Plan::new("Safe reset", "", "Reset");
            p.steps.extend(job("reset", None));
            if p.steps.is_empty() {
                return Some(toast("This card has no voltage to reset."));
            }
            ui.uv.session.underway = None;
            ui.uv.session.on = None;
            ui.uv_save();
            p.done = "The graphics card is back at its own voltage".into();
            go(ui, p)
        }
        "uv-off" => {
            if matches!(st, Uv::Ready { underway: Some(_), .. }) {
                return Some(toast("A value is being checked. Stop it first."));
            }
            let ledger = ui.ledger();
            let mut p = Plan::new(
                "Turn graphics card tuning off?",
                "The driver option goes, and from the next start the card runs as made with overdrive off. The system log is no longer marked as tuned from then on.",
                "Turn off",
            );
            if let Uv::Ready { now, .. } = &st {
                p.items.push((format!("Graphics card voltage: {} \u{2192} {}", mv_words(*now), mv_words(0)), "At once.".into()));
                p.steps.extend(job("reset", None));
            }
            if let Some(e) = ledger.find_key("gpu-overdrive") {
                p.items.push(undo_item(ui, e));
                p.steps.push(Step::Host(helper::undo(e.id)));
            }
            if p.steps.is_empty() {
                return Some(toast("Graphics card tuning is off already. It goes with the next restart."));
            }
            p.items.push(("Needs a restart".into(), "Restart now or later is asked next.".into()));
            p.next = Some(("uv-restart".into(), "off".into(), String::new()));
            p.done = "Graphics card tuning goes off after a restart".into();
            with_helper(ui, p)
        }
        _ => return None,
    })
}

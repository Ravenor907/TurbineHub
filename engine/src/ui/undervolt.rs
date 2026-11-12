//! Graphics card undervolting in My machine's Graphics card section: Veteran mode only, folded under Advanced, with
//! its warnings before anything else. A new value is only ever tried: it runs for 15 seconds and goes back by itself
//! unless Keep is pressed while it counts down. Kept values are put back by the background helper a minute into
//! each session (never at boot), run on probation until they settle, and are turned off by themselves when the
//! machine stops while one is new (see `turbinehub_helper::gpu` and `agent::Agent::undervolt`).
use super::machine::marked;
use super::plan::{Plan, Step, dialog, go, helper_plan, refresh, toast, undo_item, with_helper};
use super::{Ui, arr, b, n, obj, row, s, section, with};
use crate::agent::UvProgress;
use crate::hardware::Machine;
use crate::helper;
use crate::json::Json;
use crate::modules;
use turbinehub_helper::gpu;
use turbinehub_helper::policy::mv_words;

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
    /// Ready: the offset the card holds now, the lowest one taken, what is kept, what tripped and how the kept one
    /// is doing.
    Ready { now: i32, lo: i32, kept: Option<i32>, tripped: Option<i32>, progress: UvProgress, leaving: bool },
}

pub(crate) fn state(ui: &Ui, m: &Machine) -> Uv {
    let Some(card) = modules::undervolt_card(m) else {
        return if modules::undervolt_unsupported(m) { Uv::Unsupported } else { Uv::Hidden };
    };
    let (kept, recorded) = modules::undervolt_recorded(m, &ui.ledger());
    if !m.overdrive_on() {
        return Uv::Off { pending: recorded };
    }
    let Some((lo, _)) = card.od.and_then(|od| od.allowed()) else { return Uv::NoRange { leaving: !recorded } };
    let tripped = std::fs::read_to_string(ui.ctx.root.join(gpu::TRIPPED.trim_start_matches('/'))).ok().and_then(|t| gpu::parse_tripped(&t)).map(|t| t.0);
    let progress = UvProgress::load(&ui.data);
    Uv::Ready { now: card.od.and_then(|od| od.offset).unwrap_or(0), lo, kept, tripped, progress, leaving: !recorded }
}

/// The words for what tripped.
pub(crate) const TRIPPED_WORDS: &str = "The graphics card's lower voltage was turned off because the machine stopped unexpectedly last time. Try a smaller step?";

/// How the kept value is doing, for the fold's badge and the checkup: (badge, tone).
fn badge(st: &Uv) -> (String, &'static str) {
    match st {
        Uv::Off { pending: true } => ("Restart needed".into(), "warn"),
        Uv::Ready { tripped: Some(_), .. } => ("Turned off after a stop".into(), "stop"),
        Uv::Ready { leaving: true, .. } | Uv::NoRange { leaving: true } => ("Off after a restart".into(), "warn"),
        Uv::Ready { kept: Some(k), progress, .. } if *k != 0 && progress.settled && progress.offset == *k => (format!("{} settled", mv_words(*k)), "ok"),
        Uv::Ready { kept: Some(k), .. } if *k != 0 => (format!("{} on probation", mv_words(*k)), "warn"),
        _ => (String::new(), ""),
    }
}

/// The checkup's item: a trip to look at, a value on probation, a restart that is due. (look, title, line).
pub(crate) fn checkup(ui: &Ui) -> Option<(bool, String, String)> {
    if !ui.veteran() {
        return None;
    }
    match state(ui, &ui.machine()) {
        Uv::Ready { tripped: Some(v), .. } => Some((true, "The graphics card's lower voltage was turned off".into(), format!("{} was on when the machine stopped unexpectedly. Try a smaller step?", mv_words(v)))),
        Uv::Ready { kept: Some(k), progress, .. } if k != 0 && !(progress.settled && progress.offset == k) => {
            Some((false, format!("Graphics card voltage on probation: {}", mv_words(k)), format!("Watched for 2 hours of play with a sleep and wake ({} so far). If games crash, Safe reset puts it back.", hours(progress.healthy))))
        }
        Uv::Off { pending: true } => Some((false, "Restart to turn on graphics card tuning".into(), "Undervolting can be set after the next start.".into())),
        _ => None,
    }
}

fn hours(secs: u64) -> String {
    match secs / 60 {
        m if m < 60 => format!("{m} min"),
        m => format!("{} h {} min", m / 60, m % 60),
    }
}

fn alert(id: &str, title: &str, line: &str, badge: &str) -> Json {
    with(with(with(with(row("none", id, "info", title, line), "alert", b(true)), "badge", s(badge)), "tone", s("stop")), "icon", s("warn"))
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
        Uv::Ready { now, lo, kept, progress, leaving, tripped } => {
            let kept_words = match kept {
                Some(k) if *k != 0 && tripped.is_some() => "Kept, but turned off: it isn't put back. Try a smaller step, or Safe reset.".to_string(),
                Some(k) if *k != 0 && progress.settled && progress.offset == *k => format!("Kept and settled: {} ran 2 hours with a sleep and wake and no trouble.", mv_words(*k)),
                Some(k) if *k != 0 => format!("Kept, on probation: {} is watched for 2 hours of play with a sleep and wake ({} so far).", mv_words(*k), hours(progress.healthy)),
                _ => "Nothing kept: the card starts at its own voltage.".into(),
            };
            let mut now_row = with(row("none", "now", "info", "Voltage offset now", &kept_words), "value", s(mv_words(*now)));
            if kept.is_some_and(|k| k != 0 && k != *now) && tripped.is_none() {
                now_row = with(with(now_row, "badge", s(if ui.agent_running() { "Back a minute into each start" } else { "Background helper off" })), "tone", s("warn"));
            }
            rows.push(now_row);
            let notes = arr([(gpu::WARN_MV, "Below -50 mV many cards crash in some games. Watch a game you know before you keep it."), (gpu::DEEP_MV, "Below -100 mV is past what most cards take. Expect crashes and black screens.")].iter().map(|(below, text)| obj! {"below": n(*below as f64), "text": s(*text)}));
            let mut slider = row("uv-try", "", "slider", "Lower voltage", "Pick a step, then A tries it for 15 seconds. Keep appears while it counts down.");
            for (k, v) in [("min", *lo), ("max", 0), ("value", *now), ("step", gpu::STEP_MV)] {
                slider = with(slider, k, n(v as f64));
            }
            rows.push(marked(ui, with(with(with(with(with(slider, "unit", s(" mV")), "notes", notes), "showFrom", b(true)), "low", s(format!("{lo} mV"))), "high", s("0 mV"))));
            rows.push(with(with(with(row("uv-reset", "", "button", "Safe reset", "Back to the card's own voltage at once, and nothing kept. Asks nothing."), "button", s("Reset")), "icon", s("restore")), "primary", b(*now != 0 || kept.is_some_and(|k| k != 0))));
            if *leaving { rows.push(restart("Restart to finish turning tuning off", "The driver option is gone already.")) } else { rows.push(off()) }
            if !ui.agent_running() && kept.is_some_and(|k| k != 0) {
                rows.extend(super::machine::agent_off_row(ui, "Putting a kept voltage back at each start needs"));
            }
        }
    }
    Some(section("Advanced", rows))
}

fn host(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host"].iter().chain(args).map(|x| x.to_string()).collect()
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
    Some(match action {
        "uv-fold" => {
            ui.uv_open = !ui.uv_open;
            refresh()
        }
        "uv-on" => {
            let Some(setting) = modules::gpu_overdrive(&m) else { return Some(toast("Graphics card tuning is for Radeon RX 7000 and RX 9000 cards only.")) };
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
        "uv-try" => {
            let Uv::Ready { now, lo, .. } = st else { return Some(toast("Graphics card tuning isn't on yet.")) };
            let o = offset()?;
            if o == now {
                return Some(toast("That's already set."));
            }
            if o < lo || o > 0 || o % gpu::STEP_MV != 0 {
                return Some(toast(&format!("This card takes {lo} to 0 mV in steps of {} mV.", gpu::STEP_MV)));
            }
            let mut p = Plan::new(
                &format!("Try {} for {} seconds?", mv_words(o), gpu::TRY_SECS),
                &format!("The card runs at it at once. Keep it within {} seconds, or it goes back by itself. If the screen freezes or goes black, hold the power button: Turbine Hub turns the lower voltage off at the next start.", gpu::TRY_SECS),
                "Try it",
            );
            let setting = modules::gpu_voltage(&m, o)?;
            p.items.push((format!("Graphics card voltage: {} \u{2192} {}", mv_words(now), mv_words(o)), if veteran { setting.exact() } else { String::new() }));
            if o < gpu::WARN_MV {
                p.items.push(("Below -50 mV".into(), "Many cards crash in some games down here. Watch a game you know for a while before you keep it.".into()));
            }
            if o < gpu::DEEP_MV {
                p.items.push(("Below -100 mV".into(), "Past what most cards take. Expect crashes and black screens. Nothing is damaged.".into()));
            }
            p.danger = o < gpu::WARN_MV;
            p.steps.extend(job("try", Some(o)));
            p.next = Some(("uv-keep-ask".into(), String::new(), o.to_string()));
            p.done = format!("Trying {}", mv_words(o));
            with_helper(ui, p)
        }
        "uv-keep-ask" => {
            let o = offset()?;
            let back = match st {
                Uv::Ready { kept: Some(k), tripped: None, .. } => k,
                _ => 0,
            };
            obj! {"keep": obj! {
                "title": s(format!("Keep {}?", mv_words(o))),
                "text": s(format!("Going back to {} in {{n}} s. Keep it only if everything still looks right.", mv_words(back))),
                "seconds": n(gpu::TRY_SECS as f64),
                "confirm": s("Keep"),
                "cancel": s("Go back"),
                "act": s("uv-back"),
                "arg": s(""),
                "value": s(""),
                "keepAct": s("uv-keep"),
                "keepValue": s(o.to_string()),
            }}
        }
        "uv-keep" => {
            let o = offset()?;
            let setting = modules::gpu_voltage(&m, o)?;
            let mut p = Plan::new("Keep this voltage", "", "Keep");
            p.steps.push(Step::Host(helper::apply(&setting)));
            p.done = if o == 0 { "The card's own voltage, kept".into() } else { format!("Kept {}. On probation for the next 2 hours of play", mv_words(o)) };
            go(ui, p)
        }
        "uv-back" => {
            let mut p = Plan::new("Go back", "", "Go back");
            p.steps.extend(job("back", None));
            p.done = match st {
                Uv::Ready { kept: Some(k), tripped: None, .. } if k != 0 => format!("Back to {}", mv_words(k)),
                _ => "Back to the card's own voltage".into(),
            };
            go(ui, p)
        }
        "uv-reset" => {
            let mut p = Plan::new("Safe reset", "", "Reset");
            p.steps.extend(job("reset", None));
            if p.steps.is_empty() {
                return Some(toast("This card has no voltage to reset."));
            }
            p.done = "The graphics card is back at its own voltage".into();
            go(ui, p)
        }
        "uv-off" => {
            let ledger = ui.ledger();
            let mut p = Plan::new(
                "Turn graphics card tuning off?",
                "The driver option goes, and from the next start the card runs as made with overdrive off. The system log is no longer marked as tuned from then on.",
                "Turn off",
            );
            if let Uv::Ready { now, kept, .. } = &st
                && (*now != 0 || kept.is_some())
            {
                p.items.push((format!("Graphics card voltage: {} \u{2192} {}", mv_words(*now), mv_words(0)), "At once, and nothing kept.".into()));
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

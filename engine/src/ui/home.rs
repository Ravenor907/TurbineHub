//! Home: Guided shows what needs a look first, then setup and a few readings; Veteran is a dashboard. Both have
//! the quick switches up front (the power mode, Game Boost, the lights, the charge limit, FSR 4 where it works),
//! each one press from opening the app, and Start fixes what the checkup can put right by itself.
use super::plan::{Plan, Step, dialog, toast, with_helper};
use super::{Ui, activity, arr, b, kind_word, n, obj, row, s, with};
use crate::checkup::{self, Item, Level};
use crate::hardware::Machine;
use crate::history::{self, Sample};
use crate::json::Json;
use crate::sensors::Readings;
use crate::settings::{SETUP_STEPS, Settings};

/// How many checkup cards Guided shows before "Show all".
const SHOWN: usize = 3;

pub(crate) fn step_title(id: &str) -> &'static str {
    match id {
        "machine" => "Your machine",
        "games" => "Your games",
        "bring" => "Bring it over",
        _ => "Picks",
    }
}

fn item_json(i: &Item) -> Json {
    obj! {"id": s(i.id), "look": b(i.level == Level::Look), "icon": s(i.icon), "title": s(&i.title), "line": s(&i.line), "button": s(i.action.0), "opens": s(i.action.1)}
}

fn thousands(v: u32) -> String {
    if v >= 1000 { format!("{},{:03}", v / 1000, v % 1000) } else { v.to_string() }
}

/// Seconds of battery left: from the background helper's readings of the last hours with the reading of this
/// moment at the end, so there is an answer (a rougher one) without the background helper too.
pub(crate) fn battery_left(ui: &Ui, r: &Readings, now: u64) -> Option<u64> {
    let mut run = history::samples(&ui.data, now.saturating_sub(3 * 3600));
    run.retain(|x| x.at <= now);
    run.push(Sample { at: now, pct: r.battery_pct?, watts: r.battery_w, charging: r.charging, game: String::new(), temp: r.cpu_c });
    history::time_left(&run, now, r.battery_wh)
}

/// The readings as tiles, only the ones this machine has: (id, big value, small words, icon). `left` is the
/// battery time left in seconds, when it can be told.
pub(crate) fn tiles(m: &Machine, r: &Readings, left: Option<u64>) -> Vec<(&'static str, String, String, &'static str)> {
    let mut out = Vec::new();
    if let Some(p) = r.battery_pct {
        let words = match left {
            _ if r.charging => "Battery, charging".to_string(),
            Some(secs) => format!("Battery, about {} left", history::time_words(secs)),
            None => "Battery".to_string(),
        };
        out.push(("battery", format!("{p}%"), words, "battery"));
    }
    if let Some(c) = r.cpu_c {
        out.push(("cpu", format!("{c:.0}\u{b0}C"), "CPU".into(), "temp"));
    }
    if let Some(c) = r.gpu_c
        && m.battery().is_none()
    {
        out.push(("gpu", format!("{c:.0}\u{b0}C"), "Graphics card".into(), "temp"));
    }
    if let Some(w) = r.battery_w.or(r.gpu_w) {
        out.push(("power", if w < 20.0 { format!("{w:.1} W") } else { format!("{w:.0} W") }, if r.battery_w.is_some() { "Power draw" } else { "Graphics power" }.to_string(), "bolt"));
    }
    if let Some(f) = r.fan_rpm
        && f > 0
    {
        out.push(("fan", format!("{} rpm", thousands(f)), "Fan".into(), "fan"));
    }
    if let Some(e) = m.connected_displays().find(|d| !d.internal).and_then(|d| d.edid.as_ref()) {
        let res = match e.best_mode.1 {
            2160 => "4K".to_string(),
            h => format!("{h}p"),
        };
        out.push(("screen", format!("{res} {} Hz", e.max_hz), if m.on_tv() { "TV" } else { "Monitor" }.to_string(), "display"));
    }
    out
}

fn setup_card(st: &Settings) -> Json {
    let left: Vec<&str> = SETUP_STEPS.iter().copied().filter(|x| !st.setup_done.iter().any(|d| d == x)).collect();
    obj! {
        "show": b(!left.is_empty()),
        "done": n((SETUP_STEPS.len() - left.len()) as f64),
        "total": n(SETUP_STEPS.len() as f64),
        "next": arr(left.iter().take(2).map(|x| s(step_title(x)))),
        "step": s(left.first().copied().unwrap_or("")),
    }
}

/// What Home offers besides its cards: every check, and setup while steps are left.
fn home_options(st: &Settings, all: bool, more: Option<bool>) -> Vec<Json> {
    let mut out = vec![super::controls::option(if all { "Show fewer checks" } else { "Show all checks" }, "home-all", "")];
    if let Some(open) = more {
        out.push(super::controls::option(if open { "Fewer switches" } else { "More switches" }, "home-more", ""));
    }
    if SETUP_STEPS.iter().any(|x| !st.setup_done.iter().any(|d| d == x)) {
        out.push(super::controls::option("Continue setup", "open", "setup"));
    }
    out
}

/// The quick switches: what is changed most, one press from opening the app. Each is a row like any other, so its
/// press is the one its own page makes. `.0`: the ones always shown (FSR 4 for every game only where the graphics
/// card runs it); `.1`: the ways to the other sections of My machine, which Guided folds under "More switches" and
/// Veteran shows at once.
fn switches(ui: &Ui, m: &Machine, st: &Settings) -> (Vec<Json>, Vec<Json>) {
    let needs = super::plan::asks_password(ui);
    let helper = crate::helper::installed(&ui.ctx.root);
    let icon = |r: Json, name: &str| with(r, "icon", s(name));
    let mut first = Vec::new();
    let mut rest = Vec::new();
    if let Some(r) = super::machine::mode_row(ui, m) {
        first.push(icon(with(with(r, "title", s("Power mode")), "wide", b(true)), "gauge"));
    }
    if !m.cpu_epp.is_empty() {
        let waits = st.game_boost && !(helper && ui.agent_running());
        let badge = if waits { "Not active yet" } else if !st.game_boost && needs { super::setup::NEEDS_PASSWORD } else { "" };
        first.push(icon(with(with(with(row("boost", "", "toggle", "Game Boost", "Full speed while a game runs"), "on", b(st.game_boost)), "badge", s(badge)), "tone", s(if waits { "warn" } else { "" })), "bolt"));
    }
    if !m.rgb_leds.is_empty() {
        let l = super::machine::Lighting::load(&ui.folder());
        if l.left_to().is_none() {
            let line = if l.brightness == 0 { "Off".to_string() } else { format!("{}, {}%", l.label(), l.brightness) };
            first.push(icon(with(row("light", "power", "toggle", "Lights", &line), "on", b(l.brightness > 0)), "light"));
        }
    }
    if let Some(limit) = m.battery().and_then(|b| b.charge_limit) {
        let line = if limit < 100 { format!("Stops at {limit}%, easy on the battery") } else { "Charges to 100% now".to_string() };
        first.push(icon(super::machine::marked(ui, with(row("charge-quick", "", "toggle", "Stop charging at 80%", &line), "on", b(limit < 100))), "battery"));
    }
    let fsr4_all = super::games::Switches::load(&ui.folder()).fsr4_all;
    // Only where FSR 4 runs, or while it is on so it can be turned off.
    if super::fsr4_works(m) || fsr4_all {
        first.push(icon(with(row("fsr4all", "", "toggle", "FSR 4, every game", "The sharper upscaling for Proton games"), "on", b(fsr4_all)), "spark"));
    }
    for c in crate::modules::cards(&ui.ctx.root, m, &ui.ledger(), st.game_boost) {
        // What has its switch here already needs no second way in.
        if matches!(c.id, "cpu" | "power" | "lighting" | "battery") {
            continue;
        }
        let value = match c.id {
            "dock" => super::machine::dock_words(&super::machine::Lighting::load(&ui.folder())),
            _ => c.value.clone(),
        };
        rest.push(icon(with(row("open", &format!("module:{}", c.id), "button", super::machine::section_title(c.id, m), &value), "button", s("")), c.icon));
    }
    (first, rest)
}

/// The checkup's fixes that need nothing from the player but a yes: Start on Home makes them all at once.
const FIXES: &[&str] = &["reapply", "update-all", "decky-repair"];

fn fixable(items: &[Item]) -> Vec<&Item> {
    items.iter().filter(|i| i.level == Level::Look && FIXES.contains(&i.action.1)).collect()
}

/// The battery's charge over the last twelve hours for the tile's small line, when there are readings.
fn spark(ui: &Ui, now: u64) -> Option<Json> {
    const SPAN: u64 = 12 * 3600;
    let run = history::samples(&ui.data, now.saturating_sub(SPAN));
    let line = history::trend(&run, now, SPAN, 24, |x| Some(x.pct as f64));
    (line.iter().flatten().count() >= 2).then(|| super::machine::chart_line(&line))
}

pub(crate) fn act(ui: &mut Ui, action: &str, _arg: &str, _value: &str) -> Option<Json> {
    match action {
        "home-more" => {
            ui.home_more = !ui.home_more;
            Some(super::plan::refresh())
        }
        "checkup-fix" => {
            let m = ui.machine();
            let items = checkup::items(&m, &ui.facts_now());
            let fixes = fixable(&items);
            if fixes.is_empty() {
                return Some(toast("Nothing here can be fixed in one go. Each card has its own button."));
            }
            if let [one] = fixes[..] {
                return super::help::act(ui, "checkup", one.action.1, "");
            }
            // Each fix as its own dialog would ask, put together: the helper's setup once, then every step in order.
            let mut all = Plan::new(&format!("Fix {} things?", fixes.len()), "Everything the checkup can put right by itself, one after the other. Each change is recorded and can be undone.", "Fix all");
            for i in &fixes {
                ui.pending = None;
                let _ = super::help::act(ui, "checkup", i.action.1, "");
                let Some(mut p) = ui.pending.take() else { continue };
                while matches!(p.steps.first(), Some(Step::HelperInstall | Step::SetPassword)) {
                    p.steps.remove(0);
                    if !p.items.is_empty() {
                        p.items.remove(0);
                    }
                }
                all.items.push((i.title.clone(), i.line.clone()));
                all.steps.extend(p.steps);
            }
            if all.steps.is_empty() {
                return Some(toast("Nothing here can be fixed right now."));
            }
            all.done = "Fixed".into();
            Some(if all.helper_jobs() > 0 { with_helper(ui, all) } else { dialog(ui, all) })
        }
        _ => None,
    }
}

pub(crate) fn page(ui: &mut Ui) -> Json {
    let m = ui.machine();
    let st = ui.settings();
    let items = checkup::items(&m, &ui.facts_now());
    let (headline, sub) = checkup::headline(&items);
    let r = ui.readings();
    let now = crate::now();
    let line = spark(ui, now);
    let tiles = arr(tiles(&m, &r, battery_left(ui, &r, now)).into_iter().map(|(id, v, l, icon)| {
        let tile = obj! {"id": s(id), "value": s(v), "label": s(l), "icon": s(icon)};
        match (&line, id) {
            (Some(points), "battery") => with(with(tile, "spark", points.clone()), "sparkWords", s("The charge over the last 12 hours")),
            _ => tile,
        }
    }));
    let (first, rest) = switches(ui, &m, &st);
    // Every check that found nothing, for "Show all checks".
    let passed = checkup::passed(&m, &ui.facts_now(), &items);
    let checks = match passed.len() {
        0 => String::new(),
        1 => "1 check passed.".into(),
        n => format!("{n} checks passed."),
    };
    let passed_json = arr(if ui.home_all { passed.iter().map(|(id, words)| obj! {"id": s(*id), "title": s(words)}).collect::<Vec<_>>() } else { Vec::new() });
    // Start fixes what needs nothing but a yes; with one such thing it says that thing's own words.
    let fixes = fixable(&items);
    let primary = match fixes[..] {
        [] => Json::Null,
        [one] => obj! {"label": s(one.action.0), "act": s("checkup-fix"), "arg": s("")},
        _ => obj! {"label": s(format!("Fix all {}", fixes.len())), "act": s("checkup-fix"), "arg": s("")},
    };
    if ui.veteran() {
        let mut all = first;
        all.extend(rest);
        let mut options = vec![super::controls::local("Refresh", "reload", "f5")];
        options.extend(home_options(&st, ui.home_all, None));
        return obj! {
            "view": s("veteran"),
            "tiles": tiles,
            "switches": arr(all),
            "items": arr(items.iter().map(item_json)),
            "fine": s(if items.iter().any(|i| i.level == Level::Look) { "" } else { "Nothing needs a look." }),
            "more": s(if ui.home_all { String::new() } else { checks }),
            "all": b(ui.home_all),
            "passed": passed_json,
            "activity": arr(activity::read(&ui.folder(), 8).into_iter().map(|(at, w)| obj! {"when": s(activity::ago(now, at)), "text": s(w)})),
            "primary": primary,
            "options": arr(options),
        };
    }
    let shown = if ui.home_all { items.len() } else { SHOWN.min(items.len()) };
    let more = items.len() - shown;
    let tip = if ui.window {
        ("Mouse and keyboard", "Everything works with a mouse and with keys. Right-click or R shows what a thing can do, F finds, Q and E switch tabs, Esc goes back.")
    } else {
        ("New here?", "X on anything shows what it can do, with Help for the page at the end. Looking for Task Manager or the C: drive? View opens the menu, with \"Where's my...?\"")
    };
    let tips = match more {
        0 => String::new(),
        1 => "1 more tip.".into(),
        n => format!("{n} more tips."),
    };
    obj! {
        "view": s("guided"),
        "headline": s(headline),
        "sub": s(sub),
        "setup": setup_card(&st),
        "items": arr(items.iter().take(shown).map(item_json)),
        "more": s(if ui.home_all { String::new() } else { [tips, checks].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ") }),
        "all": b(ui.home_all),
        "passed": passed_json,
        "tiles": tiles,
        "switches": arr(first),
        // The switches used less often are on the page too, one press away.
        "extra": arr(if ui.home_more { rest.clone() } else { Vec::new() }),
        "moreSwitches": s(if rest.is_empty() { "" } else if ui.home_more { "Fewer switches" } else { "More switches" }),
        "tip": obj! {"title": s(tip.0), "line": s(tip.1)},
        "kind": s(kind_word(&m)),
        "primary": primary,
        "options": arr(home_options(&st, ui.home_all, (!rest.is_empty()).then_some(ui.home_more))),
    }
}

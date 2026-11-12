//! My machine: one page with a section for each thing this machine has, picked with the chips across its top (the
//! Store's sections page draws it). The
//! first section, Overview, shows every part's state with its main switch on its card; every other section holds
//! that part's controls, on the page. Every control that changes the system becomes a dialog with the exact
//! settings the helper will write. `module:<id>` is the same page opened on that section.
use super::plan::{Plan, Step, asks_password, change_item, dialog, guide, helper_plan, refresh, toast, with_helper};
use super::setup::NEEDS_PASSWORD;
use super::{Ui, arr, b, cpu_short, gpu_name, kind_word, n, obj, row, s, section, size_words, with};
use crate::hardware::{Display, DriveRole, GpuVendor, Kind, Machine};
use crate::helper::{self, Setting};
use crate::json::{self, Json};
use crate::modules;
use crate::tuneup::{self, State};

const CHARGE: &[u32] = &[60, 70, 80, 90, 100];
const RATES: &[u32] = &[50, 60, 75, 90, 100, 120, 144, 165, 180, 240, 360];
const VRR: &[(&str, &str)] = &[("Off", "never"), ("Automatic", "automatic"), ("Always", "always")];

pub use super::lighting::{COLORS, EFFECTS, Lighting, dock_words, light_writes};

/// A row that says the background helper is off, on the pages whose controls only work with it. None while it runs.
pub(crate) fn agent_off_row(ui: &Ui, what: &str) -> Option<Json> {
    (!ui.agent_running()).then(|| with(with(with(with(row("open", "settings", "button", "The background helper is off", &format!("{what} it. Settings, General turns it on.")), "button", s("Settings")), "badge", s("Off")), "tone", s("warn")), "icon", s("external")))
}

/// The built-in screen's refresh range, from what the screen itself reports.
fn panel_range(m: &Machine) -> Option<(u32, u32)> {
    m.connected_displays().find(|d| d.internal).and_then(|d| d.edid.as_ref()).and_then(|e| e.range_hz)
}

fn spec(m: &Machine) -> String {
    let mut parts = vec![cpu_short(&m.cpu)];
    if m.kind == Kind::Desktop {
        parts.push(gpu_name(m));
    }
    parts.push(format!("{} GB", m.memory_gb));
    if let Some((low, high)) = panel_range(m) {
        parts.push(format!("{low} to {high} Hz"));
    } else if m.kind == Kind::Desktop {
        parts.push(match m.drives.len() { 1 => "1 drive".into(), c => format!("{c} drives") });
    }
    if !m.steamos.is_empty() {
        parts.push(format!("SteamOS {}", m.steamos));
    }
    parts.retain(|p| !p.is_empty());
    parts.join(" \u{b7} ")
}

pub(crate) fn machine_line(m: &Machine) -> String {
    spec(m)
}

/// A section's name on its chip and at the top of its page.
pub(crate) fn section_title(id: &str, m: &Machine) -> &'static str {
    match id {
        "overview" => "Overview",
        "power" => "Power",
        "cpu" => "Processor",
        "display" => "Screen",
        "monitors" if m.on_tv() => "TV",
        "monitors" => "Monitors",
        "lighting" => "Lighting",
        "battery" => "Battery",
        "sleep" => "Sleep and wake",
        "gpu" => "Graphics card",
        "drives" => "Drives",
        "sound" => "Sound",
        "dock" => "Dock",
        "tuneup" => "Tune-up",
        "windows" => "Windows",
        _ => "My machine",
    }
}

/// The long explanation of a section, one press away (Options, "About ..."): what it is, then what each control
/// does and what stays as it is.
pub(crate) fn about(id: &str) -> (&'static str, Vec<(&'static str, &'static str)>) {
    match id {
        "power" | "cpu" => (
            "How hard the processor may work. More power is faster, louder and warmer, and on a battery it empties sooner.",
            vec![
                ("Mode", "Quiet, Balanced and Boost set how eager the processor is and, where this machine has a power limit, the limit too. The same three are on Home."),
                ("Power limit", "The most the processor may draw, in watts (TDP). Shown only where the machine lets it be set, and not while Steam's own slider or a plugin sets it."),
                ("Game Boost", "The background helper sets the processor to full speed when a game starts and puts it back when the game ends. It changes no game, so it works with every game."),
                ("Undo", "Every limit and mode is recorded first. Settings, Review changes puts the old values back."),
            ],
        ),
        "display" => (
            "The built-in screen and how fast it can redraw.",
            vec![("Refresh range", "What the screen itself says it can do. A higher rate is smoother, a lower one saves battery."), ("In Game Mode", "Steam sets the rate for each game: its quick menu, Performance."), ("In Desktop Mode", "The choices here set the screen for the desktop.")],
        ),
        "monitors" => (
            "Your screens: refresh rate, variable refresh (VRR) and HDR.",
            vec![("Desktop Mode", "The choices here set the screens for the desktop, at once."), ("Game Mode", "Steam shows one screen and sets it itself: Steam's Settings, Display."), ("VRR", "The screen follows the game's frame rate, so motion stays smooth when it drops."), ("HDR", "Brighter highlights in games made for it, on screens that have it.")],
        ),
        "lighting" => (
            "The lights on this machine: a look, its brightness and its colors. The background helper plays the look.",
            vec![("Looks", "Ready-made effects with their colors. Pick one with A; it shows in the preview first."), ("Effect and colors", "Make your own from an effect, its speed and up to two colors."), ("A look per game", "Pick a look while a game runs and that game keeps it."), ("Test lights", "Run once: the lights turn red one way after another and you say when they do."), ("Other apps", "When HueSync or Ally Hub sets the lights too, pick who runs them. Turbine Hub never changes another app's settings.")],
        ),
        "battery" => (
            "The battery: where charging stops, how long the charge lasts and what used it.",
            vec![("Charge limit", "A battery that sits at 100% on the charger ages faster. Stopping at 80% keeps it healthy; charge to 100% before a trip."), ("The last day", "The charge and the processor's temperature over 24 hours, from the background helper's readings. A gap is a sleep."), ("Time left", "The energy left over the average draw of the last ten minutes."), ("Battery use by game", "From the sessions that ran on battery from start to end.")],
        ),
        "sleep" => (
            "Which devices may wake the machine, and what each sleep used.",
            vec![("Wake from", "Turn a device off when it wakes the machine by accident. Leave a controller's receiver on to wake it from the couch."), ("When it counts", "A change takes effect the next time the device is plugged in or the machine starts."), ("Last sleeps", "How long each sleep was, what it used and what woke the machine.")],
        ),
        "gpu" => (
            "The graphics card and how much power it may draw.",
            vec![("Power limit", "Lower is cooler and quieter and costs a little speed. The card's own range is the slider's."), ("Safe reset", "Puts the limit back to what it was before Turbine Hub changed it."), ("Nvidia", "Nvidia support on SteamOS is experimental, so its controls are few.")],
        ),
        "drives" => (
            "Every drive in this machine and what it is used for.",
            vec![("Game library", "Steam installs games there."), ("Ready to set up", "An empty drive. Steam formats it and adds it as a library; Turbine Hub shows the steps and never formats anything itself."), ("Windows drive", "Read only here. Games can't run from it, but they can be copied.")],
        ),
        "sound" => ("Where sound goes: the speakers, a headset or the TV.", vec![("Output", "Picked at once, no restart. Steam's quick menu has the same choice.")]),
        "dock" => (
            "What changes when a TV or monitor is connected to the handheld.",
            vec![("Lights off while docked", "No lights across the room while you play on the TV."), ("Sound to the TV", "Sound switches to the TV when it is connected, and back when it is gone."), ("How", "The background helper sees the screen come and go and does the switching.")],
        ),
        "tuneup" => (
            "Memory and kernel settings that make games smoother, the ones gaming systems like Bazzite and CachyOS ship with.",
            vec![("Each switch", "One setting, or a few that belong together. Its dialog shows the exact values before anything changes."), ("Turn on all", "Everything that is off, in one go. Start does it."), ("SteamOS does it", "Marked on a setting that SteamOS has made its own default."), ("Undo", "Turning a switch off puts back what was there before. The settings stay through SteamOS updates.")],
        ),
        "windows" => (
            "Windows is on this machine too. This starts it once.",
            vec![("Restart to Windows", "The next start only. The start after that is SteamOS again, and the boot order stays as it is."), ("Before", "Close your game first: the machine restarts right away.")],
        ),
        _ => (
            "Everything this machine has that Turbine Hub can set, a section each. Only what is really there is shown.",
            vec![("Overview", "Every part with its state and its main switch."), ("Sections", "The chips across the top, or LT and RT, go from one to the next."), ("System changes", "Each shows its exact changes first, is recorded and can be undone in Settings, Review changes.")],
        ),
    }
}

/// Marks a control whose press changes a system setting, while that still asks for the password (Turbine Hub's
/// helper isn't set up, or is an older one). A row that has something else to say keeps its own mark.
pub(crate) fn marked(ui: &Ui, r: Json) -> Json {
    if asks_password(ui) && r.str_at("badge").unwrap_or("").is_empty() { with(with(r, "badge", s(NEEDS_PASSWORD)), "tone", s("")) } else { r }
}

/// The first control of a section: where the highlight starts when the section opens.
fn first_control(page: &Json) -> String {
    let rows = || page.get("sections").map(Json::as_array).unwrap_or(&[]).iter().flat_map(|sec| sec.get("rows").map(Json::as_array).unwrap_or(&[]).iter());
    let press = |r: &&Json| !matches!(r.str_at("kind"), Some("info" | "preview" | "chart")) && r.str_at("act") != Some("none");
    // A thing to fix comes before the controls (a drive to set up, lights to open).
    rows().find(|r| press(r) && r.str_at("tone") == Some("warn")).or_else(|| rows().find(press)).or_else(|| rows().next()).and_then(|r| r.str_at("id")).unwrap_or("").to_string()
}

/// My machine. `arg` is the section ("" for Overview).
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    let m = ui.machine();
    let st = ui.settings();
    let ledger = ui.ledger();
    let cards = modules::cards(&ui.ctx.root, &m, &ledger, st.game_boost);
    let id = cards.iter().map(|c| c.id).find(|c| *c == arg).unwrap_or("overview");
    let mut chips = vec![obj! {"id": s("overview"), "title": s("Overview"), "icon": s("machine"), "on": b(id == "overview")}];
    chips.extend(cards.iter().map(|c| obj! {"id": s(c.id), "title": s(section_title(c.id, &m)), "icon": s(c.icon), "on": b(c.id == id)}));
    let mut j = if id == "overview" { overview(ui, &m, &cards) } else { module(ui, id) };
    let mut options: Vec<Json> = j.get("options").map(Json::as_array).unwrap_or(&[]).to_vec();
    if id == "overview" && !ledger.entries.is_empty() {
        options.push(super::controls::page_button("Undo all", "undo-all", ""));
    }
    options.push(super::controls::option(&if id == "overview" { "About My machine".to_string() } else { format!("About {}", section_title(id, &m)) }, "about", id));
    let focus = if id == "overview" { cards.first().map(|c| format!("card:{}", c.id)).unwrap_or_default() } else { first_control(&j) };
    // The line under the chips: what the section is (Overview: which machine this is).
    let line = j.str_at("intro").unwrap_or("").to_string();
    let at = if id == "overview" { 0 } else { cards.iter().position(|c| c.id == id).map(|i| i + 1).unwrap_or(0) };
    for (k, v) in [("chips", arr(chips)), ("index", n(at as f64)), ("section", s(id)), ("header", obj! {"title": s(""), "line": s(line)}), ("options", arr(options)), ("focus", s(focus))] {
        j = with(j, k, v);
    }
    // B on a section goes back to Overview before it leaves the tab.
    if id != "overview" {
        j = with(j, "backTo", s("overview"));
    }
    j
}

/// The Overview: every part of the machine as a card with its state, and its main switch or button on the card
/// where it has one (Game Boost, the lights, the charge limit, the tune-up, Restart to Windows). The other cards
/// open their section.
fn overview(ui: &mut Ui, m: &Machine, cards: &[modules::Card]) -> Json {
    let st = ui.settings();
    let lighting = Lighting::load(&ui.folder());
    let veteran = ui.veteran();
    let needs = asks_password(ui);
    let boost_waits = st.game_boost && !(helper::installed(&ui.ctx.root) && ui.agent_running());
    let mut out = Vec::new();
    for c in cards {
        let title = section_title(c.id, m);
        let mut value = c.value.clone();
        let mut line = if veteran { c.line.to_string() } else { c.line.replace(" (TDP)", "") };
        // (The switch's name; is it a switch, is it on; the button's words; the press; the mark.)
        let mut own: Option<(&str, bool, bool, String, &str, String, &str)> = None;
        match c.id {
            "power" | "cpu" => {
                let badge = if boost_waits { "Not active yet" } else if !st.game_boost && needs { NEEDS_PASSWORD } else { "" };
                own = Some(("Game Boost", true, st.game_boost, String::new(), "boost", String::new(), badge));
            }
            "lighting" => {
                line = "Effects, colors per game and the battery level.".into();
                match lighting.left_to() {
                    Some(name) => value = format!("{name} runs them"),
                    None => {
                        value = if lighting.brightness == 0 { "Off".to_string() } else { format!("{} \u{b7} {}%", lighting.label(), lighting.brightness) };
                        own = Some(("Lights", true, lighting.brightness > 0, String::new(), "light", "power".into(), ""));
                    }
                }
            }
            "battery" => {
                if let Some(limit) = m.battery().and_then(|b| b.charge_limit) {
                    own = Some(("Stop at 80%", true, limit < 100, String::new(), "charge-quick", String::new(), if needs { NEEDS_PASSWORD } else { "" }));
                }
            }
            "tuneup" => {
                let off = tuneup::plan(&ui.ctx.root, &ui.ledger()).iter().filter(|i| i.state == State::Off).count();
                if off > 0 {
                    own = Some(("", false, false, format!("Turn on all {off}"), "tuneup", "all".into(), if needs { NEEDS_PASSWORD } else { "" }));
                }
            }
            "windows" => own = Some(("", false, false, "Restart".into(), "windows-boot", String::new(), if needs { NEEDS_PASSWORD } else { "" })),
            "dock" => value = dock_words(&lighting),
            "sound" => value = ui.sound.iter().find(|(name, _)| *name == ui.sound_now).map(|(_, words)| words.clone()).unwrap_or(value),
            _ => {}
        }
        let target = format!("module:{}", c.id);
        let mut card = obj! {"id": s(c.id), "title": s(title), "icon": s(c.icon), "value": s(value), "line": s(line), "badge": s(if c.experimental { "Experimental" } else { "" }), "tone": s("warn"), "act": s("open"), "arg": s(&target), "button": s("")};
        if let Some((status, toggle, on, button, act, arg, badge)) = own {
            for (k, v) in [("status", s(status)), ("toggle", b(toggle)), ("on", b(on)), ("button", s(button)), ("act", s(act)), ("arg", s(arg))] {
                card = with(card, k, v);
            }
            if !badge.is_empty() {
                card = with(with(card, "badge", s(badge)), "tone", s(if badge == NEEDS_PASSWORD { "" } else { "warn" }));
            }
            // Its own press is the switch, so going to its section and what is there are in its Options.
            let mut offers = vec![super::controls::option(&format!("Go to {title}"), "open", &target)];
            offers.extend(super::controls::actions_of(&module(ui, c.id)));
            card = with(card, "offers", arr(offers));
        }
        out.push(card);
    }
    obj! {
        "title": s(m.name()),
        "intro": s(format!("{} \u{b7} {}. Only what this {} has is shown.", m.name(), spec(m), kind_word(m))),
        "columns": n(4.0),
        "cards": arr(out),
        "sections": arr([]),
    }
}

/// One screen's choices for Desktop Mode: refresh rate, variable refresh and HDR.
fn screen_rows(ui: &Ui, d: &Display, first: bool) -> Vec<Json> {
    let Some(e) = &d.edid else { return Vec::new() };
    let name = output_name(&d.connector).to_string();
    // What Desktop Mode has this screen set to now, when it could be asked.
    let now = ui.facts.screens.iter().find(|x| x.name == name);
    let mut rows = Vec::new();
    let list = rates(d);
    if list.len() > 1 {
        rows.push(with(with(with(row("monitor", &format!("hz|{name}"), "choice", "Refresh rate", &format!("This screen goes up to {} Hz", e.max_hz)), "options", arr(list.iter().map(|r| s(r.to_string())))), "index", n(now.and_then(|x| x.hz).or(ui.facts.current_hz.filter(|_| first)).and_then(|h| list.iter().position(|r| *r == h)).map(|p| p as f64).unwrap_or(-1.0))), "apply", b(true)));
    }
    if e.range_hz.is_some() {
        rows.push(with(with(with(row("monitor", &format!("vrr|{name}"), "choice", "Variable refresh (VRR)", "Smooth motion when fps drops"), "options", arr(VRR.iter().map(|(t, _)| s(*t)))), "index", n(now.and_then(|x| VRR.iter().position(|(_, k)| *k == x.vrr)).map(|p| p as f64).unwrap_or(-1.0))), "apply", b(true)));
    }
    rows.push(with(with(with(row("monitor", &format!("hdr|{name}"), "choice", "HDR", "For HDR games, on screens that have it"), "options", arr([s("Off"), s("On")])), "index", n(now.and_then(|x| x.hdr).map(|on| if on { 1.0 } else { 0.0 }).unwrap_or(-1.0))), "apply", b(true)));
    rows
}

/// The three power modes as one choice, where this machine has them.
pub(crate) fn mode_row(ui: &Ui, m: &Machine) -> Option<Json> {
    if !modules::has_power_modes(&ui.ctx.root, m) {
        return None;
    }
    let now = modules::power_mode_now(&ui.ctx.root, m);
    let words: Vec<String> = modules::PowerMode::ALL
        .iter()
        .map(|x| match x.watts(m) {
            Some(w) if ui.veteran() => format!("{} \u{b7} {w} W", x.title()),
            _ => x.title().to_string(),
        })
        .collect();
    let line = match now {
        Some(x) => x.line().to_string(),
        None => "Quiet lasts longest, Boost is fastest.".to_string(),
    };
    let r = with(with(row("power-mode", "", "choice", "Mode", &line), "options", arr(words.into_iter().map(s))), "index", n(now.and_then(|x| modules::PowerMode::ALL.iter().position(|y| *y == x)).map(|i| i as f64).unwrap_or(-1.0)));
    Some(marked(ui, r))
}

/// How much of the past a chart shows, and in how many parts.
const DAY: u64 = 24 * 3600;
const PARTS: usize = 48;

/// A chart's line as the page gets it: a number for each part, null where nothing was read.
pub(crate) fn chart_line(values: &[Option<f64>]) -> Json {
    arr(values.iter().map(|v| v.map(|x| n((x * 10.0).round() / 10.0)).unwrap_or(Json::Null)))
}

/// The charge and the processor's temperature over the last day as two small charts, from the background helper's
/// readings. Nothing where there are no readings to draw.
fn history_rows(ui: &Ui) -> Vec<Json> {
    let now = crate::now();
    let run = crate::history::samples(&ui.data, now.saturating_sub(DAY));
    let mut out = Vec::new();
    let pct = crate::history::trend(&run, now, DAY, PARTS, |x| Some(x.pct as f64));
    if let Some((lo, hi)) = crate::history::range(&pct) {
        // (The newest reading itself, not its half hour's average.)
        let last = run.last().map(|x| x.pct as f64).unwrap_or(hi);
        let r = with(row("none", "chart-charge", "chart", "Charge, last 24 hours", &format!("Between {lo:.0}% and {hi:.0}%. A gap is a sleep.")), "value", s(format!("{last:.0}%")));
        out.push(chart(r, chart_line(&pct), 0.0, 100.0, "0%", "100%"));
    }
    let temp = crate::history::trend(&run, now, DAY, PARTS, |x| x.temp);
    if let Some((lo, hi)) = crate::history::range(&temp) {
        let last = run.iter().rev().find_map(|x| x.temp).unwrap_or(hi);
        // The scale has room above and below what was read, in steps of ten.
        let (floor, top) = (((lo - 5.0) / 10.0).floor() * 10.0, ((hi + 5.0) / 10.0).ceil() * 10.0);
        let r = with(row("none", "chart-temp", "chart", "Processor temperature, last 24 hours", &format!("Between {lo:.0}\u{b0}C and {hi:.0}\u{b0}C.")), "value", s(format!("{last:.0}\u{b0}C")));
        out.push(chart(r, chart_line(&temp), floor, top, &format!("{floor:.0}\u{b0}C"), &format!("{top:.0}\u{b0}C")));
    }
    out
}

/// A chart row: its line, the scale's ends as numbers and as words, and the ends of its time.
fn chart(r: Json, points: Json, min: f64, max: f64, low: &str, high: &str) -> Json {
    let mut r = r;
    for (k, v) in [("points", points), ("min", n(min)), ("max", n(max)), ("low", s(low)), ("high", s(high)), ("from", s("24 hours ago")), ("to", s("Now"))] {
        r = with(r, k, v);
    }
    r
}

fn inches(d: &Display) -> String {
    match d.edid.as_ref().map(|e| e.size_cm) {
        Some((w, h)) if w > 0 && h > 0 => format!("{:.0}-inch", ((w * w + h * h) as f64).sqrt() / 2.54),
        _ => "Screen".into(),
    }
}

/// The connector's name as the desktop knows it ("card1-DP-4" is "DP-4").
fn output_name(connector: &str) -> &str {
    connector.split_once('-').map(|(_, r)| r).unwrap_or(connector)
}

fn rates(d: &Display) -> Vec<u32> {
    let Some(e) = &d.edid else { return Vec::new() };
    let low = e.range_hz.map(|r| r.0).unwrap_or(0);
    RATES.iter().copied().filter(|r| *r <= e.max_hz && *r >= low.min(60)).collect()
}

fn drive_row(d: &crate::hardware::Drive) -> Json {
    let title = if d.model.is_empty() { "Drive".to_string() } else { d.model.clone() };
    let (kind, badge, tone, line, button) = match d.role {
        DriveRole::System => ("info", "Game library", "ok", "The system drive. Steam installs games here.", ""),
        DriveRole::Empty => ("button", "Ready to set up", "warn", "Not set up yet. Steam formats it and adds it as a game library. Anything on it is erased.", "Set up for games"),
        DriveRole::Windows => ("button", "Windows drive", "muted", "Read only here. Games can't run from a Windows drive, but they can be copied.", "Copy games from it"),
        DriveRole::Installer => ("info", "SteamOS installer", "muted", "The stick SteamOS was installed from.", ""),
        DriveRole::Data if d.removable => ("info", "Removable", "muted", "Format it in Steam's Storage settings to use it for games.", ""),
        DriveRole::Data => ("info", "Other files", "muted", "Has files on it, so Turbine Hub leaves it alone.", ""),
    };
    let mut r = row("drive", &d.name, kind, &title, line);
    r = with(with(r, "badge", s(badge)), "tone", s(tone));
    r = with(r, "value", s(size_words(d.bytes)));
    with(with(r, "button", s(button)), "icon", s("drive"))
}

fn tuneup_rows(ui: &Ui) -> (Vec<Json>, usize) {
    let plan = tuneup::plan(&ui.ctx.root, &ui.ledger());
    let off = plan.iter().filter(|i| i.state == State::Off).count();
    let guided = !ui.veteran();
    let rows = plan
        .iter()
        .map(|i| (i, if guided { (i.plain_title, i.plain_why) } else { (i.title, i.why) }))
        .map(|(i, (title, why))| match i.state {
            State::SteamOsDoes => with(with(row("none", "", "info", title, why), "badge", s("SteamOS does it")), "tone", s("ok")),
            st => marked(ui, with(with(row("tuneup", i.id, "toggle", title, why), "on", b(st == State::On)), "badge", s(if i.restart && st == State::Off { "Next restart" } else { "" }))),
        })
        .collect();
    (rows, off)
}

/// One card's page of controls.
pub(crate) fn module(ui: &mut Ui, id: &str) -> Json {
    let m = ui.machine();
    let st = ui.settings();
    let veteran = ui.veteran();
    // Game Boost only happens with Turbine Hub's helper installed and the background helper running.
    let boost_waits = st.game_boost && !(helper::installed(&ui.ctx.root) && ui.agent_running());
    let boost_asks = !st.game_boost && asks_password(ui);
    let boost = with(with(with(row("boost", "", "toggle", "Game Boost", "Full processor speed while a game runs, quiet the rest of the time."), "on", b(st.game_boost)), "badge", s(if boost_waits { "Not active yet" } else if boost_asks { NEEDS_PASSWORD } else { "" })), "tone", s(if boost_waits { "warn" } else { "" }));
    let mut extra: Vec<(&str, Json)> = Vec::new();
    let (title, intro, sections): (String, String, Vec<Json>) = match id {
        "power" => {
            let mut rows = Vec::new();
            if let Some(p) = m.power_limits.first() {
                let mut r = row("tdp", "", "slider", if veteran { "TDP limit" } else { "Power limit" }, "Lower is quieter and lasts longer. Higher is faster. A applies it.");
                for (k, v) in [("min", p.min), ("max", p.max), ("value", p.current), ("step", 1)] {
                    r = with(r, k, n(v as f64));
                }
                rows.push(marked(ui, with(r, "unit", s(" W"))));
            }
            rows.push(boost);
            rows.extend(mode_row(ui, &m));
            ("Power".into(), "How much power the processor may use.".into(), vec![section("", rows)])
        }
        "cpu" => {
            let mut rows = vec![boost];
            rows.extend(mode_row(ui, &m));
            ("Processor".into(), "How eager the processor is.".into(), vec![section("", rows)])
        }
        "display" => {
            let mut rows = Vec::new();
            match panel_range(&m) {
                Some((low, high)) => rows.push(with(row("none", "", "info", "Refresh range", "Steam's quick menu, Performance, sets the refresh rate for each game."), "value", s(format!("{low} to {high} Hz")))),
                None => rows.push(row("none", "", "info", "Refresh rate", "Steam's quick menu, Performance, sets the refresh rate for each game.")),
            }
            // In a Desktop Mode window the built-in screen takes the same choices a monitor does; in Game Mode
            // Steam sets it, and the row says where.
            if ui.window {
                if let Some(d) = m.connected_displays().find(|d| d.internal) {
                    rows.extend(screen_rows(ui, d, true));
                }
            } else {
                rows.push(with(row("checkup", "refresh-rate", "button", "In Game Mode, Steam sets the screen", "Steam's Settings, Display. In Desktop Mode the choices are here."), "button", s("How")));
            }
            ("Screen".into(), "The built-in screen.".into(), vec![section("", rows)])
        }
        "lighting" => return super::lighting::page(ui, &m),
        "battery" => {
            let mut rows = Vec::new();
            if let Some(bat) = m.battery() {
                if let Some(limit) = bat.charge_limit {
                    let idx = CHARGE.iter().position(|c| *c == limit).map(|i| i as f64).unwrap_or(-1.0);
                    rows.push(marked(ui, with(with(row("charge", "", "choice", "Charge limit", "Stopping at 80% keeps the battery healthy when it's mostly plugged in."), "options", arr(CHARGE.iter().map(|c| s(format!("{c}%"))))), "index", n(idx))));
                }
                if let Some(c) = bat.capacity {
                    rows.push(with(row("none", "", "info", "Charge now", ""), "value", s(format!("{c}%"))));
                }
            }
            let now = crate::now();
            let r = ui.readings();
            if r.battery_pct.is_some() {
                let (value, line) = match super::home::battery_left(ui, &r, now) {
                    _ if r.charging => ("Charging".to_string(), "Shown while running on battery."),
                    Some(secs) => (format!("About {}", crate::history::time_words(secs)), "At the draw of the last few minutes."),
                    None => ("Not known yet".to_string(), "It shows after a few minutes on battery."),
                };
                rows.push(with(row("none", "", "info", "Time left", line), "value", s(value)));
            }
            let by_game: Vec<Json> = crate::history::use_by_game(&crate::history::sessions(&ui.data))
                .iter()
                .take(8)
                .map(|g| with(row("none", "", "info", if g.name.is_empty() { "A game" } else { &g.name }, &format!("{} on battery, {}", crate::history::time_words(g.secs), if g.sessions == 1 { "1 session".to_string() } else { format!("{} sessions", g.sessions) })), "value", s(format!("{} an hour", crate::history::percent(g.per_hour)))))
                .collect();
            let empty = if !by_game.is_empty() { "" } else if ui.agent_running() { "Nothing yet. It fills in as you play on battery." } else { "The background helper keeps this history. Settings, General turns it on." };
            let mut sections = vec![section("", rows), with(section("Battery use by game", by_game), "empty", s(empty))];
            let charts = history_rows(ui);
            if !charts.is_empty() {
                sections.insert(1, with(section("The last day", charts), "columns", n(2.0)));
            }
            ("Battery".into(), "Where charging stops, how long the charge lasts and what used it.".into(), sections)
        }
        "sleep" => {
            let ledger = ui.ledger();
            let rows: Vec<Json> = m
                .wake_devices
                .iter()
                .map(|d| {
                    let blocked = ledger.settings().iter().any(|x| matches!(x, Setting::Wake { vendor, product, wake: false } if *vendor == d.vendor && *product == d.product));
                    marked(ui, with(row("wake", &format!("{}:{}", d.vendor, d.product), "toggle", if d.name.is_empty() { "USB device" } else { &d.name }, if blocked { "Can't wake the machine" } else { "Can wake the machine" }), "on", b(d.enabled && !blocked)))
                })
                .collect();
            let empty = if rows.is_empty() { "No USB device can wake this machine." } else { "" };
            let mut sections = vec![with(section("Wake from", rows), "empty", s(empty))];
            // What each sleep used is a battery matter: machines without one have nothing to list.
            if m.battery().is_some() {
                let last: Vec<Json> = crate::history::sleeps(&ui.data, 6).iter().map(|x| with(row("none", "", "info", &format!("Asleep {}", crate::history::time_words(x.secs)), &x.words()), "value", s(format!("{} ago", super::ago(crate::now(), x.at))))).collect();
                let empty = if !last.is_empty() { "" } else if ui.agent_running() { "Nothing yet. Each sleep shows here once the machine wakes." } else { "The background helper keeps this history. Settings, General turns it on." };
                sections.push(with(section("Last sleeps", last), "empty", s(empty)));
            }
("Sleep and wake".into(), "Which devices may wake the machine. Turn one off when it wakes it by accident, leave a controller's receiver on to wake from the couch.".into(), sections)
        }
        "tuneup" => {
            let (rows, off) = tuneup_rows(ui);
            if off > 0 {
                extra.push(("primary", obj! {"label": s(format!("Turn on all {off}")), "act": s("tuneup"), "arg": s("all")}));
            }
            ("Tune-up".into(), "Memory and kernel settings that make games smoother. Each one is saved first and can be undone.".into(), vec![section("", rows)])
        }
        "dock" => {
            let l = Lighting::load(&ui.folder());
            let mut rows = vec![with(row("light", "dockoff", "toggle", "Lights off while docked", "No lights while a TV or monitor is connected"), "on", b(l.off_when_docked)), with(row("light", "docksound", "toggle", "Sound to the TV while docked", "Switches to the TV's sound when one is connected, and back when it's gone"), "on", b(l.tv_sound_when_docked))];
            if let Some(off) = agent_off_row(ui, "The dock options need") {
                rows.insert(0, off);
            }
            ("Dock".into(), "What changes when a TV or monitor is connected. The background helper does the switching.".into(), vec![section("", rows)])
        }
        "gpu" => {
            let mut rows = Vec::new();
            if let Some(g) = m.main_gpu() {
                rows.push(with(row("none", "", "info", "Graphics card", ""), "value", s(gpu_name(&m))));
                if g.vendor == GpuVendor::Nvidia {
                    rows.push(row("none", "", "info", "Nvidia on SteamOS is experimental", "Games run through Valve's and Nvidia's shared work. Game Mode, HDR and VRR can misbehave."));
                }
                if let Some((_, min, max, now)) = &g.power_cap {
                    let mut r = row("gpu-power", "", "slider", "Power limit", "Lower is cooler and quieter. A applies it.");
                    for (k, v) in [("min", min.div_ceil(1_000_000)), ("max", max / 1_000_000), ("value", (now + 500_000) / 1_000_000), ("step", 5)] {
                        r = with(r, k, n(v as f64));
                    }
                    rows.push(marked(ui, with(r, "unit", s(" W"))));
                    rows.push(with(with(row("gpu-reset", "", "button", "Safe reset", "Back to the card's own limit"), "button", s("Reset")), "icon", s("restore")));
                }
            }
            ("Graphics card".into(), String::new(), vec![section("", rows)])
        }
        "monitors" => {
            let mut out = Vec::new();
            // In Game Mode Steam sets the screen: say so before the controls, which are for Desktop Mode.
            if !ui.window {
                out.push(section("", vec![with(row("checkup", "refresh-rate", "button", "In Game Mode, Steam sets the screen", "Steam's Settings, Display. These controls set the screens for Desktop Mode."), "button", s("How"))]));
            }
            for (i, d) in m.connected_displays().enumerate() {
                let Some(e) = &d.edid else { continue };
                let mut rows = vec![with(row("none", "", "info", &format!("{} \u{b7} {}x{}", inches(d), e.best_mode.0, e.best_mode.1), if d.hdmi { "Connected over HDMI" } else { "Connected over DisplayPort" }), "value", s(format!("up to {} Hz", e.max_hz)))];
                rows.extend(screen_rows(ui, d, i == 0));
                out.push(section(&format!("{} {}", if m.on_tv() { "TV" } else { "Monitor" }, i + 1), rows));
            }
            if ui.window {
                out.push(section("Game Mode", vec![row("none", "", "info", "Game Mode shows one screen at a time", "Steam picks it: Settings, Display. These controls set the screens for Desktop Mode.")]));
            }
            (if m.on_tv() { "TV" } else { "Monitors" }.into(), String::new(), out)
        }
        "drives" => ("Drives".into(), "Steam can install games on any drive marked Game library.".into(), vec![section("", m.drives.iter().map(drive_row).collect())]),
        "sound" => {
            let idx = ui.sound.iter().position(|(name, _)| *name == ui.sound_now).map(|i| i as f64).unwrap_or(-1.0);
            let rows = if ui.sound.is_empty() {
                vec![row("none", "", "info", "No outputs found", "Steam's quick menu also picks the output.")]
            } else {
                vec![with(with(with(row("sound", "", "choice", "Output", "Speakers, a headset or the TV"), "options", arr(ui.sound.iter().map(|(_, words)| s(words)))), "index", n(idx)), "wide", b(true))]
            };
            ("Sound".into(), String::new(), vec![section("", rows)])
        }
        "windows" => {
            let rows = vec![marked(ui, with(with(row("windows-boot", "", "button", "Restart to Windows", "The next start only. After that the machine starts SteamOS again."), "button", s("Restart")), "icon", s("reboot")))];
            ("Windows".into(), "For games that only run there.".into(), vec![section("", rows)])
        }
        _ => ("Not found".into(), String::new(), Vec::new()),
    };
    let mut j = obj! {"title": s(title), "intro": s(intro), "sections": arr(sections)};
    for (k, v) in extra {
        j = with(j, k, v);
    }
    j
}

/// The helper commands that undo these settings, newest first (only the ones Turbine Hub recorded).
fn undo_steps(ui: &Ui, settings: &[Setting]) -> Vec<Step> {
    let ledger = ui.ledger();
    let mut ids: Vec<u64> = settings.iter().filter_map(|x| ledger.find_key(&x.key()).map(|e| e.id)).collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.dedup();
    ids.into_iter().map(|id| Step::Host(helper::undo(id))).collect()
}

/// The power limit dialog's lines: one for the limit itself, and one more when the short boost limits change with
/// it (they are set to the same value, so lowering the limit lowers them too).
fn tdp_items(m: &Machine, w: u64, veteran: bool) -> Vec<(String, String)> {
    let Some(main) = m.power_limits.first() else { return Vec::new() };
    let to = w.clamp(main.min, main.max);
    let mut out = vec![(format!("Power limit: {} W \u{2192} {to} W", main.current), if veteran { format!("{}: {} \u{2192} {to}", main.path, main.current) } else { "What the processor may use for as long as it runs.".into() })];
    let boosts: Vec<(&crate::hardware::PowerLimit, u64)> = m.power_limits.iter().skip(1).map(|p| (p, w.clamp(p.min, p.max))).filter(|(p, v)| p.current != *v).collect();
    if boosts.is_empty() {
        return out;
    }
    let lower = boosts.iter().any(|(p, v)| *v < p.current);
    let higher = boosts.iter().any(|(p, v)| *v > p.current);
    let words = |v: Vec<u64>| {
        let mut v = v;
        v.dedup();
        v.iter().map(|x| format!("{x} W")).collect::<Vec<_>>().join(" and ")
    };
    let from = words(boosts.iter().map(|(p, _)| p.current).collect());
    let new = words(boosts.iter().map(|(_, v)| *v).collect());
    let title = match (lower, higher) {
        (true, false) => format!("Boost limits lowered too: {from} \u{2192} {new}"),
        (false, true) => format!("Boost limits raised too: {from} \u{2192} {new}"),
        _ => format!("Boost limits changed too: {from} \u{2192} {new}"),
    };
    let line = if veteran {
        boosts.iter().map(|(p, v)| format!("{}: {} \u{2192} {v}", p.path, p.current)).collect::<Vec<_>>().join("; ")
    } else if lower {
        "Short bursts above the limit are capped at the new limit as well. Undo puts them back.".into()
    } else {
        "Short bursts may use up to the new limit as well. Undo puts them back.".into()
    };
    out.push((title, line));
    out
}

/// A tune-up item in the "Turn on all" dialog: its name and why, with the exact values in Veteran mode, and when
/// it waits for the next restart.
fn tuneup_item(ui: &Ui, i: &tuneup::Item) -> (String, String) {
    let mut line = if ui.veteran() { i.settings.iter().map(|x| change_item(ui, x).1).collect::<Vec<_>>().join("; ") } else { i.why.to_string() };
    if i.restart {
        line.push_str(if line.is_empty() { "Takes effect at the next restart." } else { " Takes effect at the next restart." });
    }
    (i.title.to_string(), line)
}

fn host(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host"].iter().chain(args).map(|x| x.to_string()).collect()
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    let m = ui.machine();
    let num: Option<u64> = value.parse::<f64>().ok().map(|v| v.round().max(0.0) as u64);
    match action {
        "boost" => {
            let mut st = ui.settings();
            let on = !st.game_boost;
            // Game Boost is the background helper setting the processor through Turbine Hub's helper: both must be
            // there, and turning it on says what is missing. Without the helper it only turns on once the helper's
            // setup in the dialog worked; cancelling leaves it off.
            if on && !helper::installed(&ui.ctx.root) {
                let mut p = Plan::new("Set up Turbine Hub's helper?", "Game Boost sets the processor to full speed while a game runs. That is a system setting, so it goes through Turbine Hub's helper.", "Set up");
                p.steps.push(Step::Choice { key: "boost", on: true });
                p.done = "Game Boost on".into();
                return Some(with(with_helper(ui, p), "refresh", b(true)));
            }
            st.game_boost = on;
            ui.save(&st);
            Some(if !st.game_boost {
                refresh()
            } else if !ui.agent_running() {
                with(with(refresh(), "toast", s("Game Boost starts once the background helper is on. Settings, General.")), "open", s("settings"))
            } else {
                refresh()
            })
        }
        "about" => {
            let (text, items) = about(arg);
            let title = if arg == "overview" || arg.is_empty() { "My machine".to_string() } else { section_title(arg, &m).to_string() };
            Some(guide(&title, text, &items))
        }
        // The charge limit's one switch on Overview and Home: 80%, or back to 100%.
        "charge-quick" => {
            let limit = m.battery().and_then(|b| b.charge_limit)?;
            let to = if limit < 100 { 100 } else { 80 };
            act(ui, "charge", "", &CHARGE.iter().position(|c| *c == to)?.to_string())
        }
        "power-mode" => {
            let mode = *modules::PowerMode::ALL.get(num? as usize)?;
            let settings = modules::power_mode(&ui.ctx.root, &m, mode);
            if settings.is_empty() {
                return Some(toast("This machine has no power modes."));
            }
            if modules::power_mode_now(&ui.ctx.root, &m) == Some(mode) {
                return Some(toast("That's already set."));
            }
            let mut p = Plan::new(&format!("Switch to {}?", mode.title()), &format!("{} Turbine Hub saves the current values first, and Undo puts them back. It stays this way after restarts.", mode.line()), "Switch");
            // One line for the processor, however many groups of cores it has, then the power limit's lines.
            let prefs: Vec<&Setting> = settings.iter().filter(|x| matches!(x, Setting::Sysfs { path, .. } if path.ends_with("energy_performance_preference"))).collect();
            if let Some(first) = prefs.first() {
                let (title, exact) = change_item(ui, first);
                let title = title.replace(" (core group 0)", "");
                let more = match prefs.len() - 1 {
                    0 => String::new(),
                    n => format!(", and the same for {n} more"),
                };
                p.items.push((title, if ui.veteran() { format!("{exact}{more}") } else { "How eager the processor is to speed up.".into() }));
            }
            if let Some(w) = mode.watts(&m) {
                p.items.extend(tdp_items(&m, w, ui.veteran()));
            }
            p.done = format!("{} mode on", mode.title());
            Some(helper_plan(ui, p, &settings))
        }
        "tdp" => {
            let w = num?;
            let settings = modules::tdp(&m, w);
            if settings.is_empty() {
                return Some(toast("This machine has no power limit to set."));
            }
            if m.power_limits.iter().all(|p| p.current == w.clamp(p.min, p.max)) {
                return Some(toast("That's already set."));
            }
            let mut p = Plan::new(&format!("Set the power limit to {w} W?"), "Turbine Hub saves the current limits first. Undo puts them back.", "Set it");
            p.items = tdp_items(&m, w, ui.veteran());
            p.done = format!("Power limit set to {w} W");
            Some(helper_plan(ui, p, &settings))
        }
        "gpu-power" => {
            let w = num?;
            let Some(setting) = modules::gpu_power(&m, w) else { return Some(toast("This graphics card has no power limit to set.")) };
            if let (Some(g), Setting::Sysfs { value, .. }) = (m.main_gpu(), &setting)
                && g.power_cap.as_ref().is_some_and(|(_, _, _, now)| now.to_string() == *value)
            {
                return Some(toast("That's already set."));
            }
            let mut p = Plan::new(&format!("Limit the graphics card to {w} W?"), "Turbine Hub saves the current limit first. Safe reset puts it back.", "Set it");
            p.items.push(change_item(ui, &setting));
            p.done = format!("Graphics power limit set to {w} W");
            Some(helper_plan(ui, p, &[setting]))
        }
        "gpu-reset" => {
            let steps = modules::gpu_power(&m, 0).map(|x| undo_steps(ui, &[x])).unwrap_or_default();
            if steps.is_empty() {
                return Some(toast("The card is at its own limit already."));
            }
            let mut p = Plan::new("Reset the graphics card's power limit?", "The limit goes back to what it was before Turbine Hub changed it.", "Reset");
            let ledger = ui.ledger();
            p.items = modules::gpu_power(&m, 0).and_then(|x| ledger.find_key(&x.key()).cloned()).map(|e| super::plan::undo_item(ui, &e)).into_iter().collect();
            p.steps = steps;
            p.done = "Graphics power limit reset".into();
            Some(with_helper(ui, p))
        }
        "charge" => {
            let pct = *CHARGE.get(num? as usize)?;
            let settings = modules::charge_limit(&m, pct);
            if settings.is_empty() {
                return Some(toast("This battery has no charge limit."));
            }
            if m.batteries.iter().filter_map(|b| b.charge_limit).all(|l| l == pct) {
                return Some(toast("That's already set."));
            }
            let mut p = Plan::new(&if pct == 100 { "Charge to 100%?".to_string() } else { format!("Stop charging at {pct}%?") }, "It stays this way after restarts and SteamOS updates. Undo puts the old limit back.", "Set it");
            p.items = settings.iter().map(|x| change_item(ui, x)).collect();
            p.done = format!("Charge limit set to {pct}%");
            Some(helper_plan(ui, p, &settings))
        }
        "wake" => {
            let (vendor, product) = arg.split_once(':')?;
            let d = m.wake_devices.iter().find(|d| d.vendor == vendor && d.product == product)?;
            let blocked = ui.ledger().settings().iter().any(|x| matches!(x, Setting::Wake { vendor: v, product: p, wake: false } if v == vendor && p == product));
            let allow = blocked || !d.enabled;
            let name = if d.name.is_empty() { "this device" } else { &d.name };
            // A device inside the case is never plugged in again: only the machine's next start counts for it.
            let when = if d.built_in { "It takes effect the next time the machine starts." } else { "It takes effect the next time the device is plugged in or the machine starts." };
            let mut p = Plan::new(&if allow { format!("Let {name} wake the machine?") } else { format!("Stop {name} from waking the machine?") }, when, if allow { "Allow" } else { "Stop it" });
            let setting = modules::usb_wake(vendor, product, allow);
            p.items.push(change_item(ui, &setting));
            p.done = if allow { format!("{name} can wake the machine") } else { format!("{name} can't wake the machine") };
            Some(helper_plan(ui, p, &[setting]))
        }
        "tuneup" => {
            let plan = tuneup::plan(&ui.ctx.root, &ui.ledger());
            if arg == "all" {
                let off: Vec<&tuneup::Item> = plan.iter().filter(|i| i.state == State::Off).collect();
                if off.is_empty() {
                    return Some(toast("Everything in the tune-up is on already."));
                }
                let mut p = Plan::new(&if off.len() == 1 { "Turn on 1 tune-up item?".to_string() } else { format!("Turn on {} tune-up items?", off.len()) }, "System settings for smoother games. Turbine Hub saves the current settings first, and Undo puts them back. They stay on after SteamOS updates. Items that are on already stay as they are.", "Turn on");
                p.items = off.iter().map(|i| tuneup_item(ui, i)).collect();
                p.done = "Tune-up on".into();
                let settings: Vec<Setting> = off.iter().flat_map(|i| i.settings.clone()).collect();
                return Some(helper_plan(ui, p, &settings));
            }
            let item = plan.iter().find(|i| i.id == arg)?;
            Some(match item.state {
                State::SteamOsDoes => toast("SteamOS does this already."),
                State::Off => {
                    let mut p = Plan::new(&format!("Turn on {}?", item.title), item.why, "Turn on");
                    p.items = item.settings.iter().map(|x| change_item(ui, x)).collect();
                    if item.restart {
                        p.items.push(("Takes effect at the next restart".into(), "Until then everything stays as it is.".into()));
                    }
                    p.done = format!("{} on", item.title);
                    helper_plan(ui, p, &item.settings)
                }
                State::On => {
                    let mut p = Plan::new(&format!("Turn off {}?", item.title), "The settings go back to what they were before.", "Turn off");
                    let ledger = ui.ledger();
                    p.items = item.settings.iter().filter_map(|x| ledger.find_key(&x.key())).map(|e| super::plan::undo_item(ui, e)).collect();
                    p.steps = undo_steps(ui, &item.settings);
                    p.done = format!("{} off", item.title);
                    with_helper(ui, p)
                }
            })
        }
        "light" | "light-test" | "light-report" | "light-answer" | "light-rival" => super::lighting::act(ui, action, arg, value),
        "monitor" => {
            let (what, name) = arg.split_once('|')?;
            let d = m.connected_displays().find(|d| output_name(&d.connector) == name)?;
            let e = d.edid.as_ref()?;
            let idx = num? as usize;
            let (title, setting, done) = match what {
                "hz" => {
                    let hz = *rates(d).get(idx)?;
                    (format!("Switch this screen to {hz} Hz?"), format!("output.{name}.mode.{}x{}@{hz}", e.best_mode.0, e.best_mode.1), format!("Screen set to {hz} Hz"))
                }
                "vrr" => {
                    let (words, key) = VRR.get(idx)?;
                    (format!("Set variable refresh to {words}?"), format!("output.{name}.vrrpolicy.{key}"), format!("Variable refresh: {words}"))
                }
                "hdr" => (if idx == 1 { "Turn HDR on?" } else { "Turn HDR off?" }.to_string(), format!("output.{name}.hdr.{}", if idx == 1 { "enable" } else { "disable" }), if idx == 1 { "HDR on" } else { "HDR off" }.to_string()),
                _ => return None,
            };
            let mut p = Plan::new(&title, "This changes the screen in Desktop Mode. In Game Mode, Steam's Display settings decide.", "Change");
            p.steps.push(Step::Host(host(&["kscreen-doctor", &setting])));
            p.done = done;
            Some(dialog(ui, p))
        }
        "drive" => {
            let d = m.drives.iter().find(|d| d.name == arg)?;
            Some(match d.role {
                DriveRole::Windows => super::plan::open("windrive"),
                DriveRole::Empty => guide("Set up this drive for games", "Steam does the formatting, so the drive ends up exactly the way SteamOS wants it. Anything on the drive is erased.", &[("1", "Press the Steam button and open Settings."), ("2", "Storage, then pick this drive."), ("3", "Format, then confirm."), ("4", "Steam adds it as a game library. Press Y on it to make it the default.")]),
                _ => return None,
            })
        }
        "sound" => {
            let (name, words) = ui.sound.get(num? as usize)?.clone();
            ui.sound_now = name.clone();
            Some(obj! {"refresh": b(true), "toast": s(format!("Sound goes to {words}")), "run": arr(host(&["pactl", "set-default-sink", &name]).iter().map(s))})
        }
        "windows-boot" => {
            let entry = m.windows_boot.clone()?;
            let mut p = Plan::new("Restart to Windows?", "The machine restarts into Windows once. The start after that is SteamOS again. Close your game first.", "Restart");
            p.danger = true;
            p.steps.push(Step::Host(helper::job("boot-next", Some(&format!("{{\"entry\":{}}}", json::quote(&entry))))));
            p.steps.push(Step::Host(host(&["systemctl", "reboot"])));
            p.done = "Restarting to Windows".into();
            Some(with_helper(ui, p))
        }
        _ => None,
    }
}

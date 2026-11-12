//! My machine: the cards for what this machine has, and one page of controls per card. Every control that changes
//! the system becomes a dialog with the exact settings the helper will write.
use super::plan::{Plan, Step, dialog, guide, helper_plan, refresh, toast, with_helper};
use super::{Ui, arr, b, cpu_short, gpu_name, hint, kind_word, n, obj, row, s, section, size_words, with};
use crate::hardware::{Display, DriveRole, GpuVendor, Kind, Machine};
use crate::helper::{self, Setting};
use crate::json::{self, Json};
use crate::modules;
use crate::tuneup::{self, State};
use std::path::{Path, PathBuf};

pub const EFFECTS: &[&str] = &["Static", "Breathe", "Heartbeat", "Color cycle", "Wave", "Spiral", "Candle", "Twinkle"];
pub const COLORS: &[(&str, &str)] = &[("Brass", "#C99E52"), ("Red", "#E0564A"), ("Orange", "#E08A3C"), ("Green", "#6FAE8C"), ("Blue", "#6E9CC8"), ("Purple", "#A58BC6"), ("White", "#FFFFFF")];
const CHARGE: &[u32] = &[60, 70, 80, 90, 100];
const RATES: &[u32] = &[50, 60, 75, 90, 100, 120, 144, 165, 180, 240, 360];
const VRR: &[(&str, &str)] = &[("Off", "never"), ("Automatic", "automatic"), ("Always", "always")];

/// The player's lighting and dock choices. The background helper plays the moving effects; Test lights shows the
/// color and brightness right away.
#[derive(Debug, Clone, PartialEq)]
pub struct Lighting {
    pub effect: String,
    /// 0 (slow) to 100 (fast).
    pub speed: u32,
    /// 0 to 100 percent.
    pub brightness: u32,
    /// One of `COLORS`, as "#RRGGBB".
    pub color: String,
    pub rainbow: bool,
    pub per_game: bool,
    pub battery_level: bool,
    pub off_when_docked: bool,
    pub tv_sound_when_docked: bool,
}

impl Default for Lighting {
    fn default() -> Self {
        Lighting { effect: "Static".into(), speed: 50, brightness: 60, color: COLORS[0].1.into(), rainbow: false, per_game: false, battery_level: false, off_when_docked: false, tv_sound_when_docked: false }
    }
}

impl Lighting {
    fn file(folder: &Path) -> PathBuf {
        folder.join("lighting.json")
    }
    pub fn parse(text: &str) -> Lighting {
        let d = Lighting::default();
        let Ok(j) = json::parse(text) else { return d };
        let flag = |k: &str| j.get(k).and_then(Json::as_bool).unwrap_or(false);
        let pct = |k: &str, def: u32| j.get(k).and_then(Json::as_f64).map(|v| (v.round() as i64).clamp(0, 100) as u32).unwrap_or(def);
        Lighting {
            effect: j.str_at("effect").filter(|e| EFFECTS.contains(e)).unwrap_or(&d.effect).to_string(),
            speed: pct("speed", d.speed),
            brightness: pct("brightness", d.brightness),
            color: j.str_at("color").filter(|c| COLORS.iter().any(|(_, h)| h == c)).unwrap_or(&d.color).to_string(),
            rainbow: flag("rainbow"),
            per_game: flag("perGame"),
            battery_level: flag("batteryLevel"),
            off_when_docked: flag("offWhenDocked"),
            tv_sound_when_docked: flag("tvSoundWhenDocked"),
        }
    }
    pub fn to_json(&self) -> String {
        obj! {"effect": s(&self.effect), "speed": n(self.speed as f64), "brightness": n(self.brightness as f64), "color": s(&self.color), "rainbow": b(self.rainbow), "perGame": b(self.per_game), "batteryLevel": b(self.battery_level), "offWhenDocked": b(self.off_when_docked), "tvSoundWhenDocked": b(self.tv_sound_when_docked)}.to_text() + "\n"
    }
    pub fn load(folder: &Path) -> Lighting {
        std::fs::read_to_string(Self::file(folder)).map(|t| Lighting::parse(&t)).unwrap_or_default()
    }
    pub fn save(&self, folder: &Path) {
        let _ = crate::write_atomic(&Self::file(folder), self.to_json().as_bytes());
    }
    /// The color as red, green and blue, 0 to 255.
    pub fn rgb(&self) -> (u8, u8, u8) {
        let h = self.color.trim_start_matches('#');
        let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
        (p(0), p(2), p(4))
    }
}

/// The lighting files to write for the still color and brightness: (path, value) per light. The color file keeps
/// as many numbers as it holds now (some lights take one color per part).
pub fn light_writes(root: &Path, m: &Machine, l: &Lighting) -> Vec<(String, String)> {
    let (r, g, bl) = l.rgb();
    let mut out = Vec::new();
    for led in &m.rgb_leds {
        if led.contains('/') || led.contains("..") {
            continue;
        }
        let dir = format!("/sys/class/leds/{led}");
        let now = std::fs::read_to_string(root.join(format!("{}/multi_intensity", dir.trim_start_matches('/')))).unwrap_or_default();
        let sets = (now.split_whitespace().count() / 3).max(1);
        out.push((format!("{dir}/multi_intensity"), vec![format!("{r} {g} {bl}"); sets].join(" ")));
        out.push((format!("{dir}/brightness"), (l.brightness * 255 / 100).to_string()));
    }
    out
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

pub(crate) fn cards_page(ui: &mut Ui) -> Json {
    let m = ui.machine();
    let st = ui.settings();
    let ledger = ui.ledger();
    let cards = modules::cards(&ui.ctx.root, &m, &ledger, st.game_boost);
    let lighting = Lighting::load(&ui.folder());
    let undo = !ledger.entries.is_empty();
    let veteran = ui.veteran();
    obj! {
        "title": s(m.name()),
        "line": s(spec(&m)),
        "note": s(format!("Only what this {} has is shown.", kind_word(&m))),
        "columns": n(4.0),
        "cards": arr(cards.iter().map(|c| {
            let value = if c.id == "lighting" { format!("{} \u{b7} {}%", lighting.effect, lighting.brightness) } else { c.value.clone() };
            obj! {"id": s(c.id), "title": s(c.title), "icon": s(c.icon), "value": s(value), "line": s(if c.id == "lighting" { "Effects, colors per game and the battery level.".to_string() } else if veteran { c.line.to_string() } else { c.line.replace(" (TDP)", "") }), "badge": s(if c.experimental { "Experimental" } else { "" }), "act": s("open"), "arg": s(format!("module:{}", c.id)), "button": s("")}
        })),
        "hints": arr(if undo { vec![hint("y", "Undo all")] } else { vec![] }),
        "y": if undo { obj! {"act": s("undo-all"), "arg": s("")} } else { Json::Null },
    }
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
    let rows = plan
        .iter()
        .map(|i| match i.state {
            State::SteamOsDoes => with(with(row("none", "", "info", i.title, i.why), "badge", s("SteamOS does it")), "tone", s("ok")),
            st => with(with(row("tuneup", i.id, "toggle", i.title, i.why), "on", b(st == State::On)), "badge", s(if i.restart && st == State::Off { "Next restart" } else { "" })),
        })
        .collect();
    (rows, off)
}

/// One card's page of controls.
pub(crate) fn module(ui: &mut Ui, id: &str) -> Json {
    let m = ui.machine();
    let st = ui.settings();
    let veteran = ui.veteran();
    let boost = with(row("boost", "", "toggle", "Game Boost", "Full processor speed while a game runs, quiet the rest of the time."), "on", b(st.game_boost));
    let mut hints: Vec<Json> = Vec::new();
    let mut extra: Vec<(&str, Json)> = Vec::new();
    let (title, intro, sections): (String, String, Vec<Json>) = match id {
        "power" => {
            let mut rows = Vec::new();
            if let Some(p) = m.power_limits.first() {
                let mut r = row("tdp", "", "slider", if veteran { "TDP limit" } else { "Power limit" }, "Lower is quieter and lasts longer. Higher is faster. A applies it.");
                for (k, v) in [("min", p.min), ("max", p.max), ("value", p.current), ("step", 1)] {
                    r = with(r, k, n(v as f64));
                }
                rows.push(with(r, "unit", s(" W")));
            }
            rows.push(boost);
            ("Power".into(), "How much power the processor may use.".into(), vec![section("", rows)])
        }
        "cpu" => ("Processor".into(), String::new(), vec![section("", vec![boost])]),
        "display" => {
            let mut rows = Vec::new();
            match panel_range(&m) {
                Some((low, high)) => rows.push(with(row("none", "", "info", "Refresh range", "Steam's quick menu, Performance, sets the refresh rate for each game."), "value", s(format!("{low} to {high} Hz")))),
                None => rows.push(row("none", "", "info", "Refresh rate", "Steam's quick menu, Performance, sets the refresh rate for each game.")),
            }
            ("Display".into(), String::new(), vec![section("", rows)])
        }
        "lighting" => {
            let l = Lighting::load(&ui.folder());
            let effect = with(with(row("light", "effect", "choice", "Effect", ""), "options", arr(EFFECTS.iter().map(|e| s(*e)))), "index", n(EFFECTS.iter().position(|e| *e == l.effect).unwrap_or(0) as f64));
            let slider = |key: &str, title: &str, v: u32, low: &str, high: &str| {
                let mut r = with(row("light", key, "slider", title, ""), "value", n(v as f64));
                for (k, x) in [("min", 0.0), ("max", 100.0), ("step", 10.0)] {
                    r = with(r, k, n(x));
                }
                with(with(with(r, "live", b(true)), "low", s(low)), "high", s(high))
            };
            let colors = with(with(with(row("light", "color", "choice", "Color", ""), "options", arr(COLORS.iter().map(|(name, _)| s(*name)))), "swatches", arr(COLORS.iter().map(|(_, h)| s(*h)))), "index", n(COLORS.iter().position(|(_, h)| *h == l.color).unwrap_or(0) as f64));
            let t = |key: &str, title: &str, line: &str, on: bool| with(row("light", key, "toggle", title, line), "on", b(on));
            hints.push(hint("x", "Test lights"));
            extra.push(("x", obj! {"act": s("light-test"), "arg": s("")}));
            (
                "Lighting".into(),
                "The background helper plays moving effects. Test lights shows the color and brightness now.".into(),
                vec![
                    section("Effect", vec![effect, slider("speed", "Speed", l.speed, "Slow", "Fast"), slider("brightness", "Brightness", l.brightness, "0%", "100%")]),
                    section("Colors", vec![colors, t("rainbow", "Rainbow", "Ignore the color above", l.rainbow), t("pergame", "Colors per game", "Each game keeps its own", l.per_game), t("level", "Battery level", "The lights show the charge level", l.battery_level)]),
                ],
            )
        }
        "battery" => {
            let mut rows = Vec::new();
            if let Some(bat) = m.battery() {
                if let Some(limit) = bat.charge_limit {
                    let idx = CHARGE.iter().position(|c| *c == limit).map(|i| i as f64).unwrap_or(-1.0);
                    rows.push(with(with(row("charge", "", "choice", "Charge limit", "Stopping at 80% keeps the battery healthy when it's mostly plugged in."), "options", arr(CHARGE.iter().map(|c| s(format!("{c}%"))))), "index", n(idx)));
                }
                if let Some(c) = bat.capacity {
                    rows.push(with(row("none", "", "info", "Charge now", ""), "value", s(format!("{c}%"))));
                }
            }
            ("Battery".into(), String::new(), vec![section("", rows)])
        }
        "sleep" => {
            let ledger = ui.ledger();
            let rows: Vec<Json> = m
                .wake_devices
                .iter()
                .map(|d| {
                    let blocked = ledger.settings().iter().any(|x| matches!(x, Setting::Wake { vendor, product, wake: false } if *vendor == d.vendor && *product == d.product));
                    with(row("wake", &format!("{}:{}", d.vendor, d.product), "toggle", if d.name.is_empty() { "USB device" } else { &d.name }, if blocked { "Can't wake the machine" } else { "Can wake the machine" }), "on", b(d.enabled && !blocked))
                })
                .collect();
            let empty = if rows.is_empty() { "No USB device can wake this machine." } else { "" };
            (if m.kind == Kind::Desktop { "Sleep and wake" } else { "Sleep" }.into(), "Which devices may wake the machine. Turn one off when it wakes it by accident, leave a controller's receiver on to wake from the couch.".into(), vec![with(section("Wake from", rows), "empty", s(empty))])
        }
        "tuneup" => {
            let (rows, off) = tuneup_rows(ui);
            if off > 0 {
                extra.push(("primary", obj! {"label": s(format!("Turn on all {off}")), "k": s("x"), "act": s("tuneup"), "arg": s("all")}));
                hints.push(hint("x", "Turn on all"));
            }
            ("Tune-up".into(), "Memory and kernel settings that make games smoother. Each one is saved first and can be undone.".into(), vec![section("", rows)])
        }
        "dock" => {
            let l = Lighting::load(&ui.folder());
            let rows = vec![with(row("light", "dockoff", "toggle", "Lights off while docked", "No lights when the handheld sits in its dock"), "on", b(l.off_when_docked)), with(row("light", "docksound", "toggle", "Sound to the TV while docked", "Switches to HDMI sound when a TV is connected"), "on", b(l.tv_sound_when_docked))];
            ("Dock".into(), "What changes when the handheld is docked to a TV. The background helper does the switching.".into(), vec![section("", rows)])
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
                    rows.push(with(r, "unit", s(" W")));
                    rows.push(with(with(row("gpu-reset", "", "button", "Safe reset", "Back to the card's own limit"), "button", s("Reset")), "icon", s("restore")));
                }
            }
            ("Graphics card".into(), String::new(), vec![section("", rows)])
        }
        "monitors" => {
            let mut out = Vec::new();
            for (i, d) in m.connected_displays().enumerate() {
                let Some(e) = &d.edid else { continue };
                let name = output_name(&d.connector).to_string();
                let mut rows = vec![with(row("none", "", "info", &format!("{} \u{b7} {}x{}", inches(d), e.best_mode.0, e.best_mode.1), if d.hdmi { "Connected over HDMI" } else { "Connected over DisplayPort" }), "value", s(format!("up to {} Hz", e.max_hz)))];
                let list = rates(d);
                if list.len() > 1 {
                    rows.push(with(with(with(row("monitor", &format!("hz|{name}"), "choice", "Refresh rate", &format!("This screen goes up to {} Hz", e.max_hz)), "options", arr(list.iter().map(|r| s(r.to_string())))), "index", n(ui.facts.current_hz.filter(|_| i == 0).and_then(|h| list.iter().position(|r| *r == h)).map(|p| p as f64).unwrap_or(-1.0))), "apply", b(true)));
                }
                if e.range_hz.is_some() {
                    rows.push(with(with(with(row("monitor", &format!("vrr|{name}"), "choice", "Variable refresh (VRR)", "Smooth motion when fps drops"), "options", arr(VRR.iter().map(|(t, _)| s(*t)))), "index", n(-1.0)), "apply", b(true)));
                }
                rows.push(with(with(with(row("monitor", &format!("hdr|{name}"), "choice", "HDR", "For HDR games, on screens that have it"), "options", arr([s("Off"), s("On")])), "index", n(-1.0)), "apply", b(true)));
                out.push(section(&format!("{} {}", if m.on_tv() { "TV" } else { "Monitor" }, i + 1), rows));
            }
            out.push(section("Game Mode", vec![row("none", "", "info", "Game Mode shows one screen at a time", "Steam picks it: Settings, Display. These controls set the screens for Desktop Mode.")]));
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
            let rows = vec![with(with(row("windows-boot", "", "button", "Restart to Windows", "The next start only. After that the machine starts SteamOS again."), "button", s("Restart")), "icon", s("reboot"))];
            ("Restart to Windows".into(), "For games that only run there.".into(), vec![section("", rows)])
        }
        _ => ("Not found".into(), String::new(), Vec::new()),
    };
    let mut j = obj! {"crumb": s("My machine"), "title": s(title), "intro": s(intro), "sections": arr(sections), "hints": arr(hints)};
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

fn host(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host"].iter().chain(args).map(|x| x.to_string()).collect()
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    let m = ui.machine();
    let num: Option<u64> = value.parse::<f64>().ok().map(|v| v.round().max(0.0) as u64);
    match action {
        "boost" => {
            let mut st = ui.settings();
            st.game_boost = !st.game_boost;
            ui.save(&st);
            Some(refresh())
        }
        "tdp" => {
            let w = num?;
            let settings = modules::tdp(&m, w);
            if settings.is_empty() {
                return Some(toast("This machine has no power limit to set."));
            }
            let mut p = Plan::new(&format!("Set the power limit to {w} W?"), "Turbine Hub saves the current limit first. Undo puts it back.", "Set it");
            p.items = settings.iter().map(|x| (x.words(), String::new())).collect();
            p.done = format!("Power limit set to {w} W");
            Some(helper_plan(ui, p, &settings))
        }
        "gpu-power" => {
            let w = num?;
            let Some(setting) = modules::gpu_power(&m, w) else { return Some(toast("This graphics card has no power limit to set.")) };
            let mut p = Plan::new(&format!("Limit the graphics card to {w} W?"), "Turbine Hub saves the current limit first. Safe reset puts it back.", "Set it");
            p.items.push((setting.words(), String::new()));
            p.done = format!("Graphics power limit set to {w} W");
            Some(helper_plan(ui, p, &[setting]))
        }
        "gpu-reset" => {
            let steps = modules::gpu_power(&m, 0).map(|x| undo_steps(ui, &[x])).unwrap_or_default();
            if steps.is_empty() {
                return Some(toast("The card is at its own limit already."));
            }
            let mut p = Plan::new("Reset the graphics card's power limit?", "The limit goes back to what it was before Turbine Hub changed it.", "Reset");
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
            let mut p = Plan::new(&if pct == 100 { "Charge to 100%?".to_string() } else { format!("Stop charging at {pct}%?") }, "It stays this way after restarts and SteamOS updates. Undo puts the old limit back.", "Set it");
            p.items = settings.iter().map(|x| (x.words(), String::new())).collect();
            p.done = format!("Charge limit set to {pct}%");
            Some(helper_plan(ui, p, &settings))
        }
        "wake" => {
            let (vendor, product) = arg.split_once(':')?;
            let d = m.wake_devices.iter().find(|d| d.vendor == vendor && d.product == product)?;
            let blocked = ui.ledger().settings().iter().any(|x| matches!(x, Setting::Wake { vendor: v, product: p, wake: false } if v == vendor && p == product));
            let allow = blocked || !d.enabled;
            let name = if d.name.is_empty() { "this device" } else { &d.name };
            let mut p = Plan::new(&if allow { format!("Let {name} wake the machine?") } else { format!("Stop {name} from waking the machine?") }, "It takes effect the next time the device is plugged in or the machine starts.", if allow { "Allow" } else { "Stop it" });
            let setting = modules::usb_wake(vendor, product, allow);
            p.items.push((setting.words(), String::new()));
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
                let mut p = Plan::new("Turn on the tune-up?", "System settings for smoother games. Turbine Hub saves the current settings first, and Undo puts them back. They stay on after SteamOS updates.", "Turn on");
                p.items = plan.iter().map(|i| (i.title.to_string(), match i.state { State::Off => i.why.to_string(), State::On => "On already".into(), State::SteamOsDoes => "SteamOS does this already: skipped".into() })).collect();
                p.done = "Tune-up on".into();
                let settings: Vec<Setting> = off.iter().flat_map(|i| i.settings.clone()).collect();
                return Some(helper_plan(ui, p, &settings));
            }
            let item = plan.iter().find(|i| i.id == arg)?;
            Some(match item.state {
                State::SteamOsDoes => toast("SteamOS does this already."),
                State::Off => {
                    let mut p = Plan::new(&format!("Turn on {}?", item.title), item.why, "Turn on");
                    p.items = item.settings.iter().map(|x| (x.words(), String::new())).collect();
                    if item.restart {
                        p.items.push(("Takes effect at the next restart".into(), String::new()));
                    }
                    p.done = format!("{} on", item.title);
                    helper_plan(ui, p, &item.settings)
                }
                State::On => {
                    let mut p = Plan::new(&format!("Turn off {}?", item.title), "The settings go back to what they were before.", "Turn off");
                    p.steps = undo_steps(ui, &item.settings);
                    p.done = format!("{} off", item.title);
                    with_helper(ui, p)
                }
            })
        }
        "light" => {
            let mut l = Lighting::load(&ui.folder());
            let idx = num.unwrap_or(0) as usize;
            match arg {
                "effect" => l.effect = EFFECTS.get(idx)?.to_string(),
                "color" => l.color = COLORS.get(idx)?.1.to_string(),
                "speed" => l.speed = (num? as u32).min(100),
                "brightness" => l.brightness = (num? as u32).min(100),
                "rainbow" => l.rainbow = !l.rainbow,
                "pergame" => l.per_game = !l.per_game,
                "level" => l.battery_level = !l.battery_level,
                "dockoff" => l.off_when_docked = !l.off_when_docked,
                "docksound" => l.tv_sound_when_docked = !l.tv_sound_when_docked,
                _ => return None,
            }
            l.save(&ui.folder());
            Some(refresh())
        }
        "light-test" => {
            let l = Lighting::load(&ui.folder());
            let writes = light_writes(&ui.ctx.root, &m, &l);
            if writes.is_empty() {
                return Some(toast("No lights found on this machine."));
            }
            let mut p = Plan::new("Show it on the lights?", "Sets the color and brightness now. Moving effects play once the background helper runs.", "Show");
            let access = modules::lighting_access();
            let has_access = ui.ledger().find_key(&access.key()).is_some();
            if !has_access {
                p.items.push((access.words(), "Asked once, so lighting never needs the password again.".into()));
                p.steps.push(Step::Host(helper::apply(&access)));
            }
            p.steps.extend(writes.into_iter().map(|(path, value)| Step::HostWrite { path, value }));
            p.done = "Lights set".into();
            Some(if has_access { dialog(ui, p) } else { with_helper(ui, p) })
        }
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

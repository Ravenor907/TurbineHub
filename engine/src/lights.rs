//! Lighting effects on the machine's multicolor lights: every LED under /sys/class/leds with a color file
//! (`multi_intensity`), whoever made it. The background helper asks for a frame a few times a second and writes
//! what changed. Nothing here knows a device by name: what a light can do is read from its own files.
//!
//! - `multi_index` names what each number in the color file is: red, green and blue per part of the light, or one
//!   whole color per part packed into one number (0xRRGGBB, as the ROG Xbox Ally X's rings take it).
//! - `multi_max_intensity`, where the kernel has it, says how big each number may be. Some kernels cap the packed
//!   numbers at 255, which leaves the file able to show blue only: then the colors go to the controller's lighting
//!   chip instead (`lights_hid`), when there is one.
//!
//! The effects and presets are the ones from Ally Hub, with its lessons kept: colors are corrected for how LEDs
//! give out light (`gamma`), the brightness slider follows how eyes see it (`dim`), and a one-color frame for the
//! chip is a single message.

use crate::lights_hid::{self, Chip, Method, Mode, Show};
use crate::ui::lighting::{self, Lighting, Look};
use std::collections::BTreeMap;
use std::path::Path;

pub type Rgb = (u8, u8, u8);

/// Names of a color-file number that carries a whole color packed as 0xRRGGBB.
const PACKED: &[&str] = &["rgb", "color", "colour"];

/// How a packed color is written to the color file (Ally Hub's `LIGHT_ENCODINGS`). The guided test finds the one
/// the real light shows right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// 0xRRGGBB as a plain number, whatever `multi_max_intensity` says.
    #[default]
    Packed,
    /// 0xBBGGRR as a plain number.
    PackedBgr,
    /// 0xRRGGBB written as hex text ("0xff0000"), the way HueSync writes these parts.
    Hex,
    /// 0xRRGGBB cut down to `multi_max_intensity` (known wrong on the Ally X; kept so Ally Hub's old choice loads).
    Clamped,
}

impl Encoding {
    pub fn key(self) -> &'static str {
        match self {
            Encoding::Packed => "packed",
            Encoding::PackedBgr => "packed_bgr",
            Encoding::Hex => "hex",
            Encoding::Clamped => "clamped",
        }
    }
    pub fn from_key(key: &str) -> Option<Encoding> {
        [Encoding::Packed, Encoding::PackedBgr, Encoding::Hex, Encoding::Clamped].into_iter().find(|e| e.key() == key)
    }
}

/// The encodings the guided test tries, in Ally Hub's order.
pub const TEST_ENCODINGS: &[Encoding] = &[Encoding::Hex, Encoding::Packed, Encoding::PackedBgr];

/// Is this a way to drive a light that Turbine Hub knows? "" (decide from the light's own files), "files",
/// "files:<encoding>" or "chip:<route>".
pub fn route_ok(route: &str) -> bool {
    match route.split_once(':') {
        None => route.is_empty() || route == "files",
        Some(("files", enc)) => Encoding::from_key(enc).is_some(),
        Some(("chip", m)) => lights_hid::method_of(m).is_some(),
        _ => false,
    }
}

/// One light as the kernel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Led {
    pub name: String,
    /// How many parts (zones) the light has, each with a color of its own.
    pub zones: usize,
    /// The brightest value its brightness file takes.
    pub max: u32,
    /// What each number in the color file is ("red", "green", "blue", or "rgb" for a packed color).
    pub channels: Vec<String>,
    /// The largest value of each number, when the kernel says (`multi_max_intensity`).
    pub channel_max: Vec<u32>,
    /// The kernel caps packed colors at 255: written through the files, every color comes out blue.
    pub capped: bool,
    /// The controller's lighting chip, used instead of the files: when they can't show the colors, or when the
    /// guided test (or Ally Hub's saved choice) says the chip is what works.
    pub chip: Option<Chip>,
    /// The lighting chip this light has, used or not.
    pub found: Option<Chip>,
    /// The chip route in use (`lights_hid::METHODS`).
    pub method: &'static Method,
    /// How packed colors are written to the color file.
    pub encoding: Encoding,
    /// The way was picked (the guided test, or Ally Hub's saved choice): it is never changed by itself.
    pub fixed: bool,
    /// The light's own effect files and the value that makes each show plain colors ("effect" to "static"): left
    /// in a mode of its own, a light ignores the colors written to it. Ally Hub's `static_mode`.
    pub modes: Vec<(String, String)>,
}

impl Led {
    pub fn packed(&self) -> bool {
        self.channels.iter().any(|c| PACKED.contains(&c.to_ascii_lowercase().as_str()))
    }
    /// The light is driven through the controller's lighting chip, not its files.
    pub fn uses_chip(&self) -> bool {
        self.chip.is_some()
    }
    fn dir(&self) -> String {
        format!("/sys/class/leds/{}", self.name)
    }
    /// The way this light is driven, as a route ("files:hex", "chip:m6").
    pub fn route(&self) -> String {
        match &self.chip {
            Some(_) => format!("chip:{}", self.method.key),
            None if self.packed() => format!("files:{}", self.encoding.key()),
            None => "files".into(),
        }
    }
}

fn read(root: &Path, abs: &str) -> String {
    std::fs::read_to_string(root.join(abs.trim_start_matches('/'))).unwrap_or_default()
}

/// Is this LED a multicolor one? Any LED with a color file is (the way Ally Hub finds them), whatever its name.
pub fn is_multicolor(root: &Path, name: &str) -> bool {
    !name.contains('/') && !name.contains("..") && root.join("sys/class/leds").join(name).join("multi_intensity").exists()
}

/// The lights among `names` (from `Machine::rgb_leds`) with what each can do, driven the way their own files say.
pub fn leds(root: &Path, names: &[String]) -> Vec<Led> {
    names.iter().filter(|n| !n.contains('/') && !n.contains("..")).map(|n| led(root, n)).collect()
}

/// The lights Turbine Hub drives: those among `names` that have a color file now, each driven by `route` (the
/// guided test's answer or Ally Hub's saved choice; "" leaves it to the light's own files). A name without a color
/// file is no light Turbine Hub can set, so it is never driven, checked or asked to be opened.
pub fn driven(root: &Path, names: &[String], route: &str) -> Vec<Led> {
    let mut out: Vec<Led> = names.iter().filter(|n| is_multicolor(root, n)).map(|n| led(root, n)).collect();
    for l in &mut out {
        via(l, route);
    }
    out
}

/// Sets the way a light is driven. A chip route only counts for a light that has a chip, an encoding only for a
/// light with packed colors.
pub fn via(led: &mut Led, route: &str) {
    match route.split_once(':') {
        Some(("chip", m)) => {
            if let (Some(m), Some(chip)) = (lights_hid::method_of(m), led.found.clone()) {
                (led.chip, led.method, led.fixed) = (Some(chip), m, true);
            }
        }
        Some(("files", enc)) => {
            if let Some(enc) = Encoding::from_key(enc).filter(|_| led.packed()) {
                (led.chip, led.encoding, led.fixed) = (None, enc, true);
            }
        }
        None if route == "files" && !led.packed() => (led.chip, led.fixed) = (None, true),
        _ => {}
    }
}

/// The light the page previews and the checkup answers for: the one with the most parts, the first of them.
pub fn main_light(leds: &[Led]) -> Option<&Led> {
    let most = leds.iter().map(|l| l.zones).max()?;
    leds.iter().find(|l| l.zones == most)
}

/// The ways the guided test tries on this light, in Ally Hub's order: the light's own files first (each encoding
/// of a packed color, unless the kernel says it caps them), then every chip route worth trying.
pub fn test_routes(led: &Led) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if !led.packed() {
        out.push("files".into());
    } else if !led.capped {
        out.extend(TEST_ENCODINGS.iter().map(|e| format!("files:{}", e.key())));
    }
    if led.found.is_some() {
        out.extend(lights_hid::TEST_METHODS.iter().map(|m| format!("chip:{m}")));
    }
    out
}

/// A route in a few words, for Veteran mode and reports.
pub fn route_words(route: &str) -> String {
    match route.split_once(':') {
        Some(("chip", m)) => match lights_hid::method_of(m) {
            Some(x) => format!("lighting chip, route {} ({})", x.key, x.name),
            None => format!("lighting chip, route {m}"),
        },
        Some(("files", enc)) => format!("the light's files, colors as {enc}"),
        _ if route == "files" => "the light's files".into(),
        _ => "decided from the light's files".into(),
    }
}

fn led(root: &Path, name: &str) -> Led {
    let dir = format!("/sys/class/leds/{name}");
    let text = |f: &str| read(root, &format!("{dir}/{f}"));
    let mut channels: Vec<String> = text("multi_index").split_whitespace().map(|c| c.trim_matches(['[', ']']).to_ascii_lowercase()).collect();
    if channels.is_empty() {
        let count = (text("multi_intensity").split_whitespace().count() / 3).max(1);
        channels = (0..count).flat_map(|_| ["red", "green", "blue"]).map(String::from).collect();
    }
    let channel_max: Vec<u32> = text("multi_max_intensity").split_whitespace().filter_map(|v| v.parse().ok()).collect();
    let mut l = Led { name: name.to_string(), zones: 1, max: text("max_brightness").trim().parse().ok().filter(|v| *v > 0).unwrap_or(255), channels, channel_max, capped: false, chip: None, found: None, method: lights_hid::auto(), encoding: Encoding::Packed, fixed: false, modes: Vec::new() };
    l.zones = if l.packed() { l.channels.len() } else { (l.channels.len() / 3).max(1) };
    l.capped = l.packed() && l.channel_max.len() == l.channels.len() && l.channels.iter().zip(&l.channel_max).any(|(c, m)| PACKED.contains(&c.as_str()) && (1..=255).contains(m));
    // Only a light with packed colors can be the rings behind an ASUS lighting chip.
    if l.packed() {
        l.found = lights_hid::find(root, name);
    }
    if l.capped {
        l.chip = l.found.clone();
    }
    for attr in ["effect", "mode"] {
        let options: Vec<String> = text(&format!("{attr}_index")).split_whitespace().map(|o| o.trim_matches(['[', ']']).to_string()).collect();
        if !root.join(dir.trim_start_matches('/')).join(attr).exists() {
            continue;
        }
        if let Some(plain) = STATIC_MODES.iter().find_map(|want| options.iter().find(|o| o.eq_ignore_ascii_case(want))) {
            l.modes.push((attr.to_string(), plain.clone()));
        }
    }
    l
}

/// Names a light's own effect file gives to "show the color that is written" (Ally Hub's `STATIC_MODE_NAMES`).
const STATIC_MODES: &[&str] = &["monocolor", "static", "solid", "direct", "custom", "single", "fixed", "steady"];

/// Use the lighting chip for this light from now on: the kernel was seen cutting the colors down after a write
/// (`capped_by_kernel`), on a machine whose files didn't say so. None when there is no chip to use.
pub fn to_chip(root: &Path, led: &mut Led) -> bool {
    led.capped = true;
    led.found = lights_hid::find(root, &led.name);
    led.chip = led.found.clone();
    led.chip.is_some()
}

/// The numbers in a color file's text, plain or hex. None when something in it isn't a number.
pub fn numbers(text: &str) -> Option<Vec<u32>> {
    text.split_whitespace().map(|v| if let Some(h) = v.strip_prefix("0x") { u32::from_str_radix(h, 16).ok() } else { v.parse().ok() }).collect()
}

/// Did the kernel keep what was written to a color file? False when it cut the packed colors down to 255.
pub fn capped_by_kernel(written: &str, now: &str) -> bool {
    match (numbers(written), numbers(now)) {
        (Some(w), Some(n)) => w.len() == n.len() && w.iter().any(|v| *v > 255) && n.iter().all(|v| *v <= 255) && w != n,
        _ => false,
    }
}

/// May the background helper drive this light? True once the helper's lighting access rule is in effect. Asked of
/// the files' own permissions, because inside the app's sandbox /sys is read only whatever they say.
pub fn writable(root: &Path, led: &Led) -> bool {
    closed(root, led).is_empty()
}

/// Exactly what this user has to be able to write for the way this light is driven: the chip's commands interface
/// when the chip is used, the light's color and brightness files otherwise. Nothing else is needed: not other
/// lights, not the chip's other interfaces (Dynamic Lighting off is best effort).
pub fn needs(led: &Led) -> Vec<String> {
    match &led.chip {
        Some(chip) => vec![chip.lighting.clone()],
        None => vec![format!("{}/multi_intensity", led.dir()), format!("{}/brightness", led.dir())],
    }
}

/// Those of `paths` that are closed to this user, each with its state in a few words ("/dev/hidraw3 (mode 600)").
pub fn closed_among(root: &Path, paths: &[String]) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    paths
        .iter()
        .filter_map(|path| match std::fs::metadata(root.join(path.trim_start_matches('/'))).ok().map(|m| m.permissions().mode() & 0o777) {
            None => Some(format!("{path} (missing)")),
            Some(m) if m & 0o002 == 0 => Some(format!("{path} (mode {m:o})")),
            Some(_) => None,
        })
        .collect()
}

/// What is still closed to this user for the way this light is driven.
pub fn closed(root: &Path, led: &Led) -> Vec<String> {
    closed_among(root, &needs(led))
}

/// Why things can still be closed right after the helper opened them, in plain words.
pub fn closed_why(closed: &[String]) -> &'static str {
    if closed.iter().any(|c| c.ends_with("(missing)")) {
        "The light isn't there right now (the controller may be asleep or restarting). Try again in a moment."
    } else {
        "Something else on this system closes them again right after Turbine Hub opens them, usually the system's own controller service. A lighting report shows what."
    }
}

/// What the lights should show now, besides the player's choices.
#[derive(Debug, Clone, Copy)]
pub struct Scene<'a> {
    pub lighting: &'a Lighting,
    /// The running game's id, or empty.
    pub game: &'a str,
    /// The charge, on a machine with a battery.
    pub battery: Option<u32>,
    /// Running on battery: frames come slower.
    pub on_battery: bool,
    /// Plugged in and charging.
    pub charging: bool,
    /// A handheld with an outside screen connected.
    pub docked: bool,
}

impl Scene<'_> {
    /// No light at all: brightness 0, lights off while docked, or HueSync runs them.
    fn dark(&self) -> bool {
        self.lighting.brightness == 0 || (self.docked && self.lighting.off_when_docked)
    }
    fn low_battery(&self) -> bool {
        self.lighting.low_flash && self.on_battery && self.battery.is_some_and(|b| b <= 15)
    }
    /// What shows now, with its brightness from 0 to 100: the low battery flash, the running game's own look, the
    /// charge, or the player's look (the order Ally Hub uses).
    pub fn current(&self) -> (Look, u32) {
        let l = self.lighting;
        if self.low_battery() {
            return (Look::effect("Strobe", &["#FF0000"], lighting::speed_of(0.5), 50), 100);
        }
        if l.per_game
            && let Some(look) = l.game_colors.get(self.game).and_then(|v| Look::from_ref(v, l))
        {
            return (look, l.brightness);
        }
        if let (true, Some(pct)) = (l.battery_level, self.battery) {
            let c = hex(battery_rgb(pct));
            return (if self.charging { Look::effect("Breathe", &[&c], lighting::speed_of(0.9), 45) } else { Look::effect("Static", &[&c], 50, 50) }, l.brightness);
        }
        (Look::of(l), l.brightness)
    }
    fn still(&self) -> bool {
        self.dark() || {
            let (look, _) = self.current();
            look.effect == "Static"
        }
    }
}

/// Milliseconds until the next frame: 20 frames a second on wall power and 8 on battery while something moves, once
/// a second otherwise (enough to follow the charge, a game starting or the dock).
pub fn frame_ms(scene: &Scene) -> u64 {
    match (scene.still(), scene.on_battery) {
        (true, _) => 1000,
        (false, false) => 50,
        (false, true) => 125,
    }
}

/// A color from the color wheel at full strength. `hue` in degrees.
pub fn hue_rgb(hue: f64) -> Rgb {
    hsv(hue / 360.0, 1.0)
}

fn hsv(h: f64, s: f64) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let c = |v: f64| ((1.0 - s + s * v) * 255.0).round() as u8;
    (c(r), c(g), c(b))
}

/// The charge as a color: red at 0%, yellow at half, green when full.
pub fn battery_rgb(pct: u32) -> Rgb {
    let p = pct.min(100);
    if p < 50 { (255, (255 * p / 50) as u8, 0) } else { ((255 * (100 - p) / 50) as u8, 255, 0) }
}

pub fn hex(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2)
}

pub fn hex_rgb(hex: &str) -> Option<Rgb> {
    let h = hex.strip_prefix('#').filter(|h| h.len() == 6 && h.is_ascii())?;
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some((p(0)?, p(2)?, p(4)?))
}

/// A number from 0 up to 1 that looks random and is the same every time for the same two inputs.
fn hash01(i: i64, seed: u64) -> f64 {
    let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed.wrapping_add(0x632B_E59B_D9B4_E019).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    (x % 10_000) as f64 / 10_000.0
}

/// Smooth noise: from one random number to the next.
fn noise(x: f64, seed: u64) -> f64 {
    let i = x.floor();
    let f = x - i;
    let f = f * f * (3.0 - 2.0 * f);
    hash01(i as i64, seed) * (1.0 - f) + hash01(i as i64 + 1, seed) * f
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t.clamp(0.0, 1.0)).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

fn smooth(f: f64) -> f64 {
    f * f * (3.0 - 2.0 * f)
}

/// One color and its strength (0 to 1) of a single-color effect, `t` seconds in. Ally Hub's `render_effect`.
fn render(look: &Look, t: f64) -> (Rgb, f64) {
    let (sp, p, cols) = (look.speed, look.param, &look.colors);
    let first = cols[0];
    match look.effect.as_str() {
        "Breathe" => {
            let ph = (t * sp / 4.0).rem_euclid(1.0);
            (first, p + (1.0 - p) * (0.5 - 0.5 * (std::f64::consts::TAU * ph).cos()))
        }
        "Heartbeat" => {
            let ph = (t * sp / 1.2).rem_euclid(1.0);
            let beat = (-((ph - 0.08) / 0.05).powi(2)).exp().max(0.7 * (-((ph - 0.3) / 0.05).powi(2)).exp());
            (first, 0.06 + 0.94 * beat)
        }
        "Color cycle" => (hsv(t * sp * 0.08, p.max(0.05)), 1.0),
        "Wave" => {
            let n = cols.len() as f64;
            let pos = (t * sp / 3.0).rem_euclid(n);
            let i = pos.floor() as usize;
            (mix(cols[i % cols.len()], cols[(i + 1) % cols.len()], smooth(pos - i as f64)), 1.0)
        }
        "Strobe" => {
            let rate = sp * 2.0;
            let on = (t * rate).rem_euclid(1.0) < if cols.len() == 1 { 0.5 } else { 0.85 };
            (cols[(t * rate).floor() as usize % cols.len()], if on { 1.0 } else { 0.0 })
        }
        "Candle" => {
            let n = 0.6 * noise(t * 7.0 * sp, 1) + 0.4 * noise(t * 19.0 * sp, 2);
            (mix(first, *cols.last().unwrap_or(&first), noise(t * 1.5 * sp, 3)), (1.0 - p) + p * n)
        }
        "Twinkle" => {
            let rate = 3.0 * sp;
            let k = (t * rate).floor();
            let spike = if hash01(k as i64, 7) < 0.15 + 0.7 * p { (std::f64::consts::PI * (t * rate).rem_euclid(1.0)).sin().powi(2) } else { 0.0 };
            (mix(first, *cols.get(1).unwrap_or(&(255, 255, 255)), spike), 0.75 + 0.25 * spike)
        }
        _ => (first, 1.0),
    }
}

/// Where each of four ring parts sits: (stick, half), in the order ASUS numbers the Ally's zones.
const ZONE_POS: [(usize, usize); 4] = [(0, 0), (0, 1), (1, 0), (1, 1)];

/// The spiral's colors per part, `t` seconds in. Ally Hub's `zone_frames`; with four parts they are taken as two
/// rings of two halves, otherwise as one ring.
fn spiral(look: &Look, zones: usize, t: f64) -> Vec<Rgb> {
    let turn = if look.ccw { -1.0 } else { 1.0 };
    let phase = t * look.speed * 0.18;
    let color = |u: f64| {
        if look.rainbow {
            return hsv(u, 1.0);
        }
        let n = look.colors.len();
        let x = u.rem_euclid(1.0) * n as f64;
        let i = x.floor() as usize;
        mix(look.colors[i % n], look.colors[(i + 1) % n], smooth(x - i as f64))
    };
    (0..zones)
        .map(|z| {
            let (mut d, mut pos) = (turn, z as f64 / zones as f64);
            if zones == 4 {
                let (stick, half) = ZONE_POS[z];
                pos = half as f64 / 2.0;
                if look.layout == "linked" {
                    pos = (stick * 2 + half) as f64 / 4.0;
                } else if look.layout == "mirror" && stick == 1 {
                    (d, pos) = (-turn, (1 - half) as f64 / 2.0);
                }
            }
            color(d * phase + pos * look.param)
        })
        .collect()
}

/// The brightness slider (0 to 100) as the share of the light's full drive, following how eyes see it: half on the
/// slider is about a fifth of the drive, so low settings get properly dim. Ally Hub's `led_dim`.
pub fn dim(pct: u32) -> f64 {
    if pct == 0 { 0.0 } else { (pct.min(100) as f64 / 100.0).powf(2.4) }
}

/// A screen color as LED drive values at strength `k` (0 to 1). LEDs give out light in proportion to what they get
/// while picked colors are gamma-encoded, so sent as they are the weaker channels glow too bright and every color
/// looks washed out. Only the mix is corrected: the strongest channel keeps its value. Ally Hub's `led_gamma`.
pub fn gamma(c: Rgb, k: f64) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    let top = c.0.max(c.1).max(c.2);
    if top == 0 {
        return (0, 0, 0);
    }
    let lin = |v: u8| (v as f64 / 255.0).powf(2.2);
    let base = lin(top);
    let ch = |v: u8| (top as f64 * (lin(v) / base) * k).round().clamp(0.0, 255.0) as u8;
    let mut out = (ch(c.0), ch(c.1), ch(c.2));
    if k > 0.0 && out == (0, 0, 0) {
        // Very dim but on: the strongest channel keeps glowing.
        if c.0 == top {
            out.0 = 1;
        } else if c.1 == top {
            out.1 = 1;
        } else {
            out.2 = 1;
        }
    }
    out
}

/// One light's colors (one per part, as picked) and its strength from 0 to 1 (the effect's pulse and the brightness
/// slider together), `t_ms` milliseconds into the effect.
pub fn look(scene: &Scene, zones: usize, t_ms: u64) -> (Vec<Rgb>, f64) {
    let zones = zones.max(1);
    if scene.dark() {
        return (vec![(0, 0, 0); zones], 0.0);
    }
    let (lk, brightness) = scene.current();
    let t = t_ms as f64 / 1000.0;
    let full = dim(brightness);
    if lk.effect == "Spiral" {
        return (spiral(&lk, zones, t), full);
    }
    let (color, f) = render(&lk, t);
    // The rainbow toggle: the parts take the color wheel, turning slowly, instead of the picked color.
    let single = !matches!(lk.effect.as_str(), "Color cycle" | "Wave");
    if lk.rainbow && single {
        let turn = t / 60.0;
        return ((0..zones).map(|z| hsv(z as f64 / zones as f64 + turn, 1.0)).collect(), full * f);
    }
    (vec![color; zones], full * f)
}

/// The colors to send for this moment, strength folded in and corrected for the LEDs.
pub fn drive(scene: &Scene, zones: usize, t_ms: u64) -> Vec<Rgb> {
    let (colors, level) = look(scene, zones, t_ms);
    colors.into_iter().map(|c| if level > 0.0 { gamma(c, level) } else { (0, 0, 0) }).collect()
}

/// The color file's text for these colors (one per part), in the light's own format.
pub fn color_text(led: &Led, colors: &[Rgb]) -> String {
    let packed = led.packed();
    let n = colors.len().max(1);
    led.channels
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let zone = if packed { i } else { i / 3 };
            let (r, g, b) = colors.get(zone.min(n - 1)).copied().unwrap_or((0, 0, 0));
            let max = if led.channel_max.len() == led.channels.len() { led.channel_max[i] } else { 0 };
            let scale = |v: u8| if max > 0 && max != 255 { (v as u32 * max + 127) / 255 } else { v as u32 };
            let rgb = (r as u32) << 16 | (g as u32) << 8 | b as u32;
            match ch.as_str() {
                c if PACKED.contains(&c) => match led.encoding {
                    Encoding::Packed => rgb.to_string(),
                    Encoding::PackedBgr => ((b as u32) << 16 | (g as u32) << 8 | r as u32).to_string(),
                    Encoding::Hex => format!("0x{rgb:06x}"),
                    Encoding::Clamped => if max > 0 { rgb.min(max) } else { rgb }.to_string(),
                },
                "red" => scale(r).to_string(),
                "green" => scale(g).to_string(),
                "blue" => scale(b).to_string(),
                _ => "0".into(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The files to write for this moment: (path, value) for the color file and brightness of every light that is
/// driven through its files. Lights driven through the lighting chip get theirs from `chip_frame`.
pub fn frame(scene: &Scene, leds: &[Led], t_ms: u64) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for led in leds.iter().filter(|l| !l.uses_chip()) {
        let colors = drive(scene, led.zones, t_ms);
        let lit = colors.iter().any(|c| *c != (0, 0, 0));
        out.push((format!("{}/multi_intensity", led.dir()), color_text(led, &colors)));
        out.push((format!("{}/brightness", led.dir()), if lit { led.max } else { 0 }.to_string()));
    }
    out
}

/// A light's own effect files that have to change for plain colors to show, as (path, value): only those this user
/// may write and that say something else now. Best effort, as in Ally Hub: the colors are written either way.
pub fn mode_writes(root: &Path, leds: &[Led]) -> Vec<(String, String)> {
    use std::os::unix::fs::PermissionsExt;
    let mut out = Vec::new();
    for led in leds.iter().filter(|l| !l.uses_chip()) {
        for (attr, value) in &led.modes {
            let path = format!("{}/{attr}", led.dir());
            let file = root.join(path.trim_start_matches('/'));
            let open = std::fs::metadata(&file).is_ok_and(|m| m.permissions().mode() & 0o002 != 0);
            let now = std::fs::read_to_string(&file).unwrap_or_default();
            if open && now.trim().trim_matches(['[', ']']) != value {
                out.push((path, value.clone()));
            }
        }
    }
    out
}

/// What the lighting chip should do now.
#[derive(Debug, Clone, PartialEq)]
pub enum ChipLook {
    /// These four colors, one per ring half, as a frame.
    Zones(Vec<Rgb>),
    /// One of the chip's own modes, which it then runs by itself.
    Own(Show),
}

/// A look as one of the chip's own modes (Ally Hub's `hid_effect_packets`): a plain color, its pulse for the
/// effects that breathe, beat, flash or blend two colors, its rainbow for the color cycle, its spiral.
fn own_show(lk: &Look, brightness: u32) -> Show {
    let cols: Vec<Rgb> = lk.colors.iter().map(|c| gamma(*c, 1.0)).collect();
    let first = cols.first().copied().unwrap_or((255, 255, 255));
    let show = |mode: Mode, color: Rgb, color2: Rgb, speed: f64| Show { mode, color, color2, speed, level: lights_hid::level(brightness), direction: 0, zones: None };
    match lk.effect.as_str() {
        "Spiral" => Show { direction: lk.ccw as u8, ..show(Mode::Spiral, (0, 0, 0), (0, 0, 0), lk.speed) },
        // The rainbow switch on a still light: the color wheel over the four halves.
        "Static" if lk.rainbow => Show { zones: Some((0..4).map(|z| gamma(hsv(z as f64 / 4.0, 1.0), 1.0)).collect()), ..show(Mode::Solid, first, (0, 0, 0), 1.0) },
        "Static" => show(Mode::Solid, first, (0, 0, 0), 1.0),
        "Color cycle" => show(Mode::Rainbow, (255, 255, 255), (0, 0, 0), lk.speed),
        "Breathe" | "Heartbeat" => show(Mode::Pulse, first, (0, 0, 0), lk.speed),
        "Strobe" => show(Mode::Pulse, first, cols.get(1).copied().unwrap_or((0, 0, 0)), 2.0),
        // Wave, candle and twinkle: the chip's two-color pulse is the closest match.
        _ => show(Mode::Pulse, first, cols.get(1).copied().unwrap_or(first), lk.speed),
    }
}

/// The chip's look for this moment on the route `m`. A route that can be animated gets frames, except for the
/// chip's own rainbow spiral at full brightness (smoother than frames; it has only three brightness steps, so below
/// full the spiral is drawn frame by frame, which dims smoothly). Any other route gets one of the chip's own modes.
pub fn chip_frame(scene: &Scene, m: &Method, t_ms: u64) -> ChipLook {
    let (lk, brightness) = scene.current();
    let dark = scene.dark();
    if !dark && lk.effect == "Spiral" && lk.chip && brightness >= 98 {
        return ChipLook::Own(Show { level: 3, ..own_show(&lk, brightness) });
    }
    if m.commit_first {
        return ChipLook::Zones(drive(scene, 4, t_ms));
    }
    if dark {
        return ChipLook::Own(Show { level: 0, ..Show::solid((0, 0, 0)) });
    }
    ChipLook::Own(own_show(&lk, brightness))
}

/// Remembers what each file was last given, so a frame only writes what changed.
#[derive(Debug, Default)]
pub struct Shown(BTreeMap<String, String>);

impl Shown {
    /// The writes of `frame` that differ from what is showing, which then counts as shown.
    pub fn changed(&mut self, frame: Vec<(String, String)>) -> Vec<(String, String)> {
        frame.into_iter().filter(|(path, value)| self.0.insert(path.clone(), value.clone()).as_ref() != Some(value)).collect()
    }
    /// Forget everything: the next frame writes every file (after a sleep, or when something else set the lights).
    /// What a file was last given.
    pub fn get(&self, path: &str) -> Option<&str> {
        self.0.get(path).map(String::as_str)
    }
    pub fn forget(&mut self) {
        self.0.clear();
    }
}

/// A live picture of the look for the Lighting page: `count` frames `step_ms` apart, each with one "#RRGGBB" per
/// part. The picture shows the effect's pulse, not the brightness slider (the page dims the whole picture).
pub fn preview(l: &Lighting, game: &str, zones: usize, count: usize, step_ms: u64) -> Vec<Vec<String>> {
    let full = Lighting { brightness: 100, off_when_docked: false, low_flash: false, battery_level: false, ..l.clone() };
    let scene = Scene { lighting: &full, game, battery: None, on_battery: false, charging: false, docked: false };
    (0..count)
        .map(|i| {
            let (colors, level) = look(&scene, zones, i as u64 * step_ms);
            let f = level.clamp(0.0, 1.0).powf(1.0 / 2.2);
            colors.into_iter().map(|c| hex(((c.0 as f64 * f).round() as u8, (c.1 as f64 * f).round() as u8, (c.2 as f64 * f).round() as u8))).collect()
        })
        .collect()
}

/// The guided test's color: red on every part.
pub const TEST_COLOR: Rgb = (255, 0, 0);

/// Everything about the machine's lights, for problem reports: every LED folder with its files, their modes and
/// what they hold, each ASUS HID interface with its usages and mode, and how Turbine Hub drives each light on
/// `route` (the guided test's answer, Ally Hub's saved choice, or "").
pub fn diagnostics(root: &Path, route: &str) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    let mut out = Vec::new();
    let mut names: Vec<String> = std::fs::read_dir(root.join("sys/class/leds")).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    names.sort();
    let mut plain = Vec::new();
    for name in &names {
        if !is_multicolor(root, name) {
            plain.push(name.clone());
            continue;
        }
        let mut l = led(root, name);
        via(&mut l, route);
        let f = |file: &str| read(root, &format!("/sys/class/leds/{name}/{file}")).trim().chars().take(120).collect::<String>();
        let how = match &l.chip {
            Some(c) => format!("lighting chip {} (Dynamic Lighting {}), route {} ({})", c.lighting, c.dynamic.as_deref().unwrap_or("none"), l.method.key, l.method.name),
            None if l.capped && !l.fixed => "files, capped at 255 and no lighting chip found".into(),
            None if l.packed() => format!("files, colors as {}", l.encoding.key()),
            None => "files".into(),
        };
        let shut = closed(root, &l);
        out.push(format!(
            "{name}: zones {} index [{}] intensity [{}] max_intensity [{}] brightness {} of {}, {}, driven by {how}{}",
            l.zones,
            f("multi_index"),
            f("multi_intensity"),
            f("multi_max_intensity"),
            f("brightness"),
            f("max_brightness"),
            if shut.is_empty() { "writable".to_string() } else { format!("not writable: {}", shut.join(", ")) },
            if l.fixed { ", way picked" } else { "" }
        ));
        // Every file in the light's folder: its mode and what it holds.
        let dir = root.join("sys/class/leds").join(name);
        let mut files: Vec<String> = std::fs::read_dir(&dir).map(|d| d.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        files.sort();
        for file in files.iter().filter(|x| x.as_str() != "uevent").take(30) {
            let mode = std::fs::metadata(dir.join(file)).map(|m| format!("{:o}", m.permissions().mode() & 0o777)).unwrap_or_else(|_| "?".into());
            let value = std::fs::read_to_string(dir.join(file)).map(|v| v.trim().chars().take(100).collect::<String>()).unwrap_or_else(|_| "(unreadable)".into());
            out.push(format!("  {name}/{file}: mode {mode}, holds [{value}]"));
        }
    }
    if out.is_empty() {
        out.push("No multicolor lights".into());
    }
    if !plain.is_empty() {
        out.push(format!("other LEDs (one color, never driven): {}", plain.join(", ")));
    }
    out.extend(lights_hid::describe(root));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::lighting::{EFFECTS, PRESETS};

    pub(crate) fn scene(l: &Lighting) -> Scene<'_> {
        Scene { lighting: l, game: "", battery: None, on_battery: false, charging: false, docked: false }
    }

    fn lit(effect: &str) -> Lighting {
        Lighting { effect: effect.into(), brightness: 100, color: "#E0564A".into(), ..Lighting::default() }
    }

    fn triple(name: &str, zones: usize, max: u32) -> Led {
        Led { name: name.into(), zones, max, channels: (0..zones).flat_map(|_| ["red", "green", "blue"]).map(String::from).collect(), channel_max: Vec::new(), capped: false, chip: None, found: None, method: lights_hid::auto(), encoding: Encoding::Packed, fixed: false, modes: Vec::new() }
    }

    #[test]
    fn every_effect_and_preset_gives_valid_frames_and_all_but_static_move() {
        let ring = triple("multicolor:rgb:ring", 4, 255);
        let dot = triple("x:rgb:power", 1, 100);
        let looks: Vec<Lighting> = EFFECTS.iter().map(|e| lit(e)).chain(PRESETS.iter().map(|p| Lighting { brightness: 100, ..p.apply(&Lighting::default()) })).collect();
        for l in &looks {
            let sc = scene(l);
            let mut seen = std::collections::BTreeSet::new();
            for t in (0..20_000).step_by(50) {
                let f = frame(&sc, &[ring.clone(), dot.clone()], t);
                assert_eq!(f.len(), 4);
                assert_eq!(f[0].0, "/sys/class/leds/multicolor:rgb:ring/multi_intensity");
                assert_eq!(f[0].1.split(' ').filter(|v| v.parse::<u8>().is_ok()).count(), 12, "{}", l.effect);
                assert_eq!(f[2].1.split(' ').count(), 3);
                assert!(f[1].1.parse::<u32>().unwrap() <= 255 && f[3].1.parse::<u32>().unwrap() <= 100, "{}", l.effect);
                seen.insert(f);
            }
            assert_eq!(seen.len() > 1, l.effect != "Static", "{} {}", l.effect, l.preset);
            assert_eq!(frame_ms(&sc), if l.effect == "Static" { 1000 } else { 50 });
            assert_eq!(frame_ms(&Scene { on_battery: true, ..sc }), if l.effect == "Static" { 1000 } else { 125 });
        }
    }

    #[test]
    fn static_shows_the_color_true_and_the_slider_follows_the_eye() {
        let l = lit("Static");
        let led = triple("a:rgb:b", 1, 255);
        // Full brightness: the strongest channel as picked, the others corrected so the color isn't washed out.
        assert_eq!(frame(&scene(&l), std::slice::from_ref(&led), 999), vec![("/sys/class/leds/a:rgb:b/multi_intensity".to_string(), "224 27 20".to_string()), ("/sys/class/leds/a:rgb:b/brightness".to_string(), "255".to_string())]);
        assert_eq!(gamma((0x0E, 0xA5, 0xE9), 1.0), (0, 109, 233));
        // Half on the slider is about a fifth of the light, and very low stays on.
        assert!((dim(50) - 0.19).abs() < 0.01 && dim(0) == 0.0 && dim(100) == 1.0);
        let low = Lighting { brightness: 5, ..lit("Static") };
        let c = drive(&scene(&low), 1, 0)[0];
        assert!(c.0 >= 1 && c.0 < 5, "{c:?}");
        // The rainbow spreads over the parts of a light and ignores the color.
        let l = Lighting { rainbow: true, ..lit("Static") };
        assert_eq!(look(&scene(&l), 3, 0).0, vec![(255, 0, 0), (0, 255, 0), (0, 0, 255)]);
    }

    #[test]
    fn breathing_stays_within_the_chosen_brightness_and_its_lowest_point() {
        let l = Lighting { brightness: 100, speed: 100, param: 20, ..lit("Breathe") };
        let levels: Vec<f64> = (0..8000).step_by(25).map(|t| look(&scene(&l), 1, t).1).collect();
        let (low, high) = (levels.iter().cloned().fold(1.0, f64::min), levels.iter().cloned().fold(0.0, f64::max));
        assert!((0.19..0.22).contains(&low) && high > 0.99, "{low} {high}");
        // Slower at a lower speed: a quarter of a second moves it less.
        let slow = Lighting { speed: 0, ..l.clone() };
        assert!((look(&scene(&slow), 1, 250).1 - look(&scene(&slow), 1, 0).1).abs() < (look(&scene(&l), 1, 250).1 - look(&scene(&l), 1, 0).1).abs());
    }

    #[test]
    fn the_spiral_runs_around_two_rings_and_other_effects_keep_the_parts_together() {
        let cycle = look(&scene(&lit("Color cycle")), 4, 1234).0;
        assert!(cycle.iter().all(|c| *c == cycle[0]));
        let wave = PRESETS.iter().find(|p| p.name == "Aurora").unwrap().apply(&Lighting::default());
        let w = look(&scene(&wave), 4, 4000).0;
        assert!(w.iter().all(|c| *c == w[0]));
        // RGB Spiral: four ring halves, four different colors, moving.
        let rgb = PRESETS.iter().find(|p| p.name == "RGB Spiral").unwrap().apply(&Lighting::default());
        let a = look(&scene(&rgb), 4, 0).0;
        assert_eq!(a.iter().collect::<std::collections::BTreeSet<_>>().len(), 4);
        assert_ne!(a, look(&scene(&rgb), 4, 1000).0);
        // Neon Vortex: its own colors, the sticks mirroring each other.
        let neon = PRESETS.iter().find(|p| p.name == "Neon Vortex").unwrap().apply(&Lighting::default());
        assert!(!neon.chip && neon.layout == "mirror");
        let n = look(&scene(&neon), 4, 0).0;
        assert_eq!((n[0], n[1]), (n[3], n[2]));
        // One part: a spiral is just a color moving through the palette.
        assert_eq!(look(&scene(&rgb), 1, 0).0.len(), 1);
    }

    #[test]
    fn low_battery_game_charge_and_dock_come_before_the_chosen_look() {
        let mut l = Lighting { battery_level: true, per_game: true, ..lit("Static") };
        l.game_colors.insert("steam:1145360".into(), "#6E9CC8".into());
        l.game_colors.insert("steam:42".into(), "preset:Ember".into());
        let sc = Scene { lighting: &l, game: "", battery: Some(81), on_battery: true, charging: false, docked: false };
        assert_eq!(look(&sc, 1, 0).0, vec![battery_rgb(81)]);
        assert_eq!((battery_rgb(0), battery_rgb(50), battery_rgb(100)), ((255, 0, 0), (255, 255, 0), (0, 255, 0)));
        // Charging: the charge's color breathes.
        assert_eq!(sc.current().0.effect, "Static");
        assert_eq!(Scene { charging: true, on_battery: false, ..sc }.current().0.effect, "Breathe");
        // A game's own look wins over the charge: a color, or a whole preset.
        assert_eq!(look(&Scene { game: "steam:1145360", ..sc }, 1, 0).0, vec![(0x6E, 0x9C, 0xC8)]);
        assert_eq!(Scene { game: "steam:42", ..sc }.current().0.effect, "Candle");
        // The low battery flash wins over everything but the dark.
        let flash = Lighting { low_flash: true, ..l.clone() };
        let low = Scene { lighting: &flash, battery: Some(12), game: "steam:42", ..sc };
        assert_eq!(low.current().0.effect, "Strobe");
        assert_eq!(Scene { on_battery: false, charging: true, ..low }.current().0.effect, "Candle");
        // Docked with lights off while docked: dark, and nothing to animate.
        let l = Lighting { off_when_docked: true, ..lit("Wave") };
        let docked = Scene { lighting: &l, game: "", battery: Some(50), on_battery: false, charging: false, docked: true };
        assert_eq!(look(&docked, 2, 500), (vec![(0, 0, 0); 2], 0.0));
        assert_eq!(frame_ms(&docked), 1000);
        assert_eq!(frame_ms(&Scene { docked: false, ..docked }), 50);
    }

    #[test]
    fn only_what_changed_is_written() {
        let l = lit("Breathe");
        let led = [triple("a:rgb:b", 1, 255)];
        let mut shown = Shown::default();
        assert_eq!(shown.changed(frame(&scene(&l), &led, 0)).len(), 2);
        // Breathing changes the color file only; the brightness stays.
        let next = shown.changed(frame(&scene(&l), &led, 400));
        assert_eq!(next.len(), 1);
        assert!(next[0].0.ends_with("/multi_intensity"));
        assert!(shown.changed(frame(&scene(&l), &led, 400)).is_empty());
        shown.forget();
        assert_eq!(shown.changed(frame(&scene(&l), &led, 400)).len(), 2);
    }

    fn fake_led(root: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = root.join("sys/class/leds").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for (f, t) in files {
            std::fs::write(dir.join(f), t).unwrap();
        }
    }

    #[test]
    fn lights_are_read_from_the_kernel_and_checked_for_access() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("turbinehub-lights-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fake_led(&root, "multicolor:rgb:ring", &[("multi_intensity", "0 0 0 0 0 0\n"), ("brightness", "0\n"), ("max_brightness", "3\n")]);
        let dir = root.join("sys/class/leds/multicolor:rgb:ring");
        let found = leds(&root, &["multicolor:rgb:ring".into(), "gone:rgb:x".into(), "../../etc".into()]);
        assert_eq!((found.len(), found[0].zones, found[0].max, found[1].zones, found[1].max), (2, 2, 3, 1, 255));
        assert!(is_multicolor(&root, "multicolor:rgb:ring") && !is_multicolor(&root, "gone:rgb:x"));
        let mode = |f: &str, m: u32| std::fs::set_permissions(dir.join(f), std::fs::Permissions::from_mode(m)).unwrap();
        mode("multi_intensity", 0o644);
        mode("brightness", 0o666);
        assert!(!writable(&root, &found[0]));
        mode("multi_intensity", 0o666);
        assert!(writable(&root, &found[0]));
        assert!(!writable(&root, &found[1]));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn packed_colors_are_written_whole_and_a_capped_kernel_is_seen() {
        let root = std::env::temp_dir().join(format!("turbinehub-lights-packed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // The ROG Xbox Ally X's rings: four parts, one packed color each.
        fake_led(&root, "ally:rgb:joystick_rings", &[("multi_index", "rgb rgb rgb rgb\n"), ("multi_intensity", "0 0 0 0\n"), ("multi_max_intensity", "16777215 16777215 16777215 16777215\n")]);
        let led = &leds(&root, &["ally:rgb:joystick_rings".into()])[0];
        assert!(led.packed() && !led.capped && led.zones == 4 && led.chip.is_none());
        assert_eq!(color_text(led, &[(255, 0, 0), (0, 255, 0), (0, 0, 255), (1, 2, 3)]), "16711680 65280 255 66051");
        // Per-channel files scale to their own maximum.
        let small = Led { channel_max: vec![100, 100, 100], ..triple("a", 1, 255) };
        assert_eq!(color_text(&small, &[(255, 128, 0)]), "100 50 0");
        // A kernel that caps each part at 255: the files can't show the colors.
        fake_led(&root, "ally:rgb:joystick_rings", &[("multi_max_intensity", "255 255 255 255\n")]);
        assert!(leds(&root, &["ally:rgb:joystick_rings".into()])[0].capped);
        assert!(capped_by_kernel("16711680 65280 255 0", "255 255 255 0"));
        assert!(!capped_by_kernel("16711680 65280 255 0", "16711680 65280 255 0"));
        assert!(!capped_by_kernel("255 0 0", "255 0 0"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_preview_and_the_test_follow_the_look() {
        let l = lit("Breathe");
        let frames = preview(&l, "", 4, 32, 125);
        assert_eq!((frames.len(), frames[0].len()), (32, 4));
        assert!(frames.iter().all(|f| f.iter().all(|c| hex_rgb(c).is_some())));
        assert_ne!(frames[0], frames[16]);
    }

    #[test]
    fn packed_colors_take_every_encoding_ally_hub_knows() {
        // Printed from Ally Hub's led_intensities for (255, 128, 1) on four packed parts capped at 255.
        let mut led = Led { channels: vec!["rgb".into(); 4], channel_max: vec![255; 4], zones: 4, ..triple("ally:rgb:joystick_rings", 4, 255) };
        let c = [(255, 128, 1); 4];
        for (enc, want) in [(Encoding::Packed, "16744449 16744449 16744449 16744449"), (Encoding::PackedBgr, "98559 98559 98559 98559"), (Encoding::Hex, "0xff8001 0xff8001 0xff8001 0xff8001"), (Encoding::Clamped, "255 255 255 255")] {
            led.encoding = enc;
            assert_eq!(color_text(&led, &c), want, "{enc:?}");
            assert_eq!(Encoding::from_key(enc.key()), Some(enc));
        }
        // Hex text reads back as numbers, so a kernel that cuts it down is still seen.
        assert_eq!(numbers("0xff8001 7"), Some(vec![0xFF8001, 7]));
        assert!(capped_by_kernel("0xff0000 0x00ff00", "255 255") && !capped_by_kernel("0xff0000", "16711680"));
        assert_eq!(numbers("red"), None);
        assert_eq!(TEST_ENCODINGS.iter().map(|e| e.key()).collect::<Vec<_>>(), ["hex", "packed", "packed_bgr"]);
    }

    #[test]
    fn a_picked_way_is_used_and_only_what_it_needs_is_checked() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("turbinehub-lights-way-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        crate::lights_hid::tests::ally_x(&root);
        // A keyboard backlight with a color file of its own, and a name that only looks like a light.
        fake_led(&root, "asus::kbd_backlight", &[("multi_intensity", "0 0 0\n"), ("brightness", "0\n")]);
        std::fs::create_dir_all(root.join("sys/class/leds/phantom:rgb:logo")).unwrap();
        let names: Vec<String> = ["ally:rgb:joystick_rings", "asus::kbd_backlight", "phantom:rgb:logo", "../x"].iter().map(|s| s.to_string()).collect();
        let mode = |p: &str, m: u32| std::fs::set_permissions(root.join(p), std::fs::Permissions::from_mode(m)).unwrap();
        for f in ["multi_intensity", "brightness"] {
            mode(&format!("sys/class/leds/ally:rgb:joystick_rings/{f}"), 0o644);
            mode(&format!("sys/class/leds/asus::kbd_backlight/{f}"), 0o644);
        }
        mode("dev/hidraw3", 0o600);
        // A name without a color file is never driven.
        let auto = driven(&root, &names, "");
        assert_eq!(auto.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["ally:rgb:joystick_rings", "asus::kbd_backlight"]);
        let rings = main_light(&auto).unwrap();
        assert_eq!((rings.name.as_str(), rings.route(), rings.fixed), ("ally:rgb:joystick_rings", "chip:m6".to_string(), false));
        // Capped by its own files: only the chip routes are worth trying. Uncapped: the files come first.
        assert_eq!(test_routes(rings), ["chip:m6", "chip:m7", "chip:m8", "chip:m2", "chip:m3"]);
        let open = Led { capped: false, ..rings.clone() };
        assert_eq!(test_routes(&open), ["files:hex", "files:packed", "files:packed_bgr", "chip:m6", "chip:m7", "chip:m8", "chip:m2", "chip:m3"]);
        assert_eq!(test_routes(&auto[1]), ["files"]);
        // The chip path needs the chip's node and nothing else: the rings' files stay closed and don't count.
        assert_eq!(closed(&root, rings), ["/dev/hidraw3 (mode 600)"]);
        mode("dev/hidraw3", 0o602);
        assert!(closed(&root, rings).is_empty());
        // Ally Hub's route, or the test's: used as said, on the chip's own node.
        let m3 = driven(&root, &names, "chip:m3");
        assert_eq!((m3[0].route(), m3[0].fixed, m3[0].method.name), ("chip:m3".to_string(), true, "Full handshake"));
        // The keyboard has no chip, so a chip route leaves it on its files.
        assert_eq!((m3[1].route(), m3[1].fixed), ("files".to_string(), false));
        // The files path needs the two files of that light, and says which is closed.
        let hex = driven(&root, &names, "files:hex");
        assert_eq!((hex[0].route(), hex[0].uses_chip(), hex[0].encoding), ("files:hex".to_string(), false, Encoding::Hex));
        assert_eq!(closed(&root, &hex[0]), ["/sys/class/leds/ally:rgb:joystick_rings/multi_intensity (mode 644)", "/sys/class/leds/ally:rgb:joystick_rings/brightness (mode 644)"]);
        mode("sys/class/leds/ally:rgb:joystick_rings/multi_intensity", 0o666);
        assert_eq!(closed(&root, &hex[0]).len(), 1);
        std::fs::remove_file(root.join("sys/class/leds/ally:rgb:joystick_rings/brightness")).unwrap();
        assert_eq!(closed(&root, &hex[0]), ["/sys/class/leds/ally:rgb:joystick_rings/brightness (missing)"]);
        assert!(closed_why(&closed(&root, &hex[0])).contains("isn't there") && closed_why(&["/dev/hidraw3 (mode 600)".to_string()]).contains("closes them again"));
        // Junk is no route.
        for (r, ok) in [("", true), ("files", true), ("files:hex", true), ("chip:m8", true), ("chip:m9", false), ("files:rot13", false), ("hid", false), ("chip:", false)] {
            assert_eq!(route_ok(r), ok, "{r}");
        }
        // The report lists every file of a light with its mode, the other LEDs by name, and the way in use.
        let d = diagnostics(&root, "chip:m7");
        assert!(d[0].starts_with("ally:rgb:joystick_rings: zones 4") && d[0].contains("route m7 (Slow and steady)") && d[0].ends_with("way picked"), "{d:?}");
        assert!(d.iter().any(|l| l == "  ally:rgb:joystick_rings/multi_intensity: mode 666, holds [255 255 255 255]"), "{d:?}");
        assert!(d.iter().any(|l| l.starts_with("asus::kbd_backlight: zones 1") && l.contains("not writable: /sys/class/leds/asus::kbd_backlight/multi_intensity (mode 644)")), "{d:?}");
        assert!(d.iter().any(|l| l == "other LEDs (one color, never driven): phantom:rgb:logo"), "{d:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_light_left_in_its_own_effect_is_put_back_to_plain_colors() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("turbinehub-lights-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fake_led(&root, "x:rgb:ring", &[("multi_intensity", "0 0 0\n"), ("brightness", "0\n"), ("effect", "rainbow\n"), ("effect_index", "Static breathing rainbow\n"), ("mode_index", "wave\n")]);
        let led = leds(&root, &["x:rgb:ring".into()]);
        assert_eq!(led[0].modes, [("effect".to_string(), "Static".to_string())]);
        // Closed to this user: left alone. Open: written once, until it says so itself.
        std::fs::set_permissions(root.join("sys/class/leds/x:rgb:ring/effect"), std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(mode_writes(&root, &led).is_empty());
        std::fs::set_permissions(root.join("sys/class/leds/x:rgb:ring/effect"), std::fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(mode_writes(&root, &led), [("/sys/class/leds/x:rgb:ring/effect".to_string(), "Static".to_string())]);
        std::fs::write(root.join("sys/class/leds/x:rgb:ring/effect"), "[Static]\n").unwrap();
        assert!(mode_writes(&root, &led).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_chip_gets_frames_on_routes_that_animate_and_its_own_modes_on_the_others() {
        let m6 = lights_hid::method_of("m6").unwrap();
        let m3 = lights_hid::method_of("m3").unwrap();
        let breathe = lit("Breathe");
        assert!(matches!(chip_frame(&scene(&breathe), m6, 500), ChipLook::Zones(z) if z.len() == 4));
        // The same look on Ally Hub's default route: the chip's pulse in the corrected color, at its top step.
        let ChipLook::Own(show) = chip_frame(&scene(&breathe), m3, 500) else { panic!("frames on m3") };
        assert_eq!((show.mode, show.color, show.color2, show.level), (Mode::Pulse, gamma((0xE0, 0x56, 0x4A), 1.0), (0, 0, 0), 3));
        // It doesn't change from frame to frame, so it is sent once.
        assert_eq!(chip_frame(&scene(&breathe), m3, 9000), ChipLook::Own(show));
        // The brightness slider becomes the chip's steps; off is a plain black at step 0.
        let dim = Lighting { brightness: 30, ..lit("Static") };
        assert!(matches!(chip_frame(&scene(&dim), m3, 0), ChipLook::Own(s) if s.mode == Mode::Solid && s.level == 1));
        let off = Lighting { brightness: 0, ..lit("Static") };
        assert_eq!(chip_frame(&scene(&off), m3, 0), ChipLook::Own(Show { level: 0, ..Show::solid((0, 0, 0)) }));
        let modes = |effect: &str| match chip_frame(&scene(&lit(effect)), m3, 0) {
            ChipLook::Own(s) => s.mode,
            ChipLook::Zones(_) => panic!("{effect}"),
        };
        assert_eq!([modes("Static"), modes("Heartbeat"), modes("Color cycle"), modes("Wave"), modes("Strobe"), modes("Candle"), modes("Twinkle"), modes("Spiral")], [Mode::Solid, Mode::Pulse, Mode::Rainbow, Mode::Pulse, Mode::Pulse, Mode::Pulse, Mode::Pulse, Mode::Spiral]);
        // The chip's own spiral at full brightness, on any route; drawn frame by frame below it where frames work.
        let spiral = PRESETS.iter().find(|p| p.name == "RGB Spiral").unwrap().apply(&Lighting { brightness: 100, ..Lighting::default() });
        assert!(matches!(chip_frame(&scene(&spiral), m6, 0), ChipLook::Own(s) if s.mode == Mode::Spiral && s.level == 3));
        let lower = Lighting { brightness: 60, ..spiral };
        assert!(matches!(chip_frame(&scene(&lower), m6, 0), ChipLook::Zones(_)));
        assert!(matches!(chip_frame(&scene(&lower), m3, 0), ChipLook::Own(s) if s.mode == Mode::Spiral && s.level == 2));
    }
}

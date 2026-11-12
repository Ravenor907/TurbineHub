//! The Lighting page: the player's lighting choices (`Lighting`, kept in lighting.json), Ally Hub's effects and
//! presets, the page with its live preview, and Test lights. The background helper plays the look (`lights`).

use super::plan::{Plan, Step, dialog, refresh, toast, with_helper};
use super::{Ui, arr, b, hint, n, obj, row, s, section, with};
use crate::hardware::Machine;
use crate::helper;
use crate::json::{self, Json};
use crate::lights::{self, Rgb, hex_rgb};
use crate::modules;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const EFFECTS: &[&str] = &["Static", "Breathe", "Heartbeat", "Color cycle", "Wave", "Strobe", "Candle", "Twinkle", "Spiral"];
pub const COLORS: &[(&str, &str)] = &[("Brass", "#C99E52"), ("Red", "#E0564A"), ("Orange", "#E08A3C"), ("Green", "#6FAE8C"), ("Blue", "#6E9CC8"), ("Purple", "#A58BC6"), ("Pink", "#FF5FA2"), ("Cyan", "#05D9E8"), ("White", "#FFFFFF")];
/// Colors an effect takes when it needs more than the player picked (Ally Hub's).
const MORE: &[&str] = &["#E11D48", "#8B5CF6", "#06B6D4", "#22C55E"];
/// The report's name for the lights (in place of a game's id).
pub const REPORT: &str = "lighting";
const LAYOUTS: &[(&str, &str)] = &[("linked", "Flows across both sticks"), ("mirror", "Sticks mirror each other"), ("same", "Both sticks match")];

/// The player's lighting and dock choices. The background helper plays the look; Test lights checks the lights.
#[derive(Debug, Clone, PartialEq)]
pub struct Lighting {
    pub effect: String,
    /// The preset the look came from, or empty once the player changed it.
    pub preset: String,
    /// 0 (slow) to 100 (fast). 50 is the effect's normal pace.
    pub speed: u32,
    /// 0 to 100 percent, as the eye sees it (see `lights::dim`).
    pub brightness: u32,
    /// The main color, "#RRGGBB".
    pub color: String,
    /// More colors for the effects that blend several (wave, candle, twinkle, strobe, spiral).
    pub colors: Vec<String>,
    /// The effect's own setting, 0 to 100: breathe's lowest point, the cycle's color strength, the candle's
    /// flicker, the twinkle's sparkle, the spiral's color spread.
    pub param: u32,
    /// The color wheel instead of the picked colors.
    pub rainbow: bool,
    /// Spiral: turning counter-clockwise.
    pub ccw: bool,
    /// Spiral on two rings: "linked", "mirror" or "same".
    pub layout: String,
    /// Spiral: the controller's lighting chip draws it, where there is one (smoothest, rainbow only).
    pub chip: bool,
    pub per_game: bool,
    pub battery_level: bool,
    /// Red flashes below 15% on battery.
    pub low_flash: bool,
    pub off_when_docked: bool,
    pub tv_sound_when_docked: bool,
    /// The lights are left to HueSync, the Decky plugin: Turbine Hub doesn't touch them.
    pub huesync: bool,
    /// With `per_game`: the look each game keeps, game id ("steam:1145360") to a color ("#RRGGBB", the player's
    /// effect in that color) or a preset ("preset:Ember").
    pub game_colors: BTreeMap<String, String>,
}

impl Default for Lighting {
    fn default() -> Self {
        Lighting {
            effect: "Static".into(),
            preset: String::new(),
            speed: 50,
            brightness: 60,
            color: COLORS[0].1.into(),
            colors: Vec::new(),
            param: 50,
            rainbow: false,
            ccw: false,
            layout: "linked".into(),
            chip: true,
            per_game: false,
            battery_level: false,
            low_flash: false,
            off_when_docked: false,
            tv_sound_when_docked: false,
            huesync: false,
            game_colors: BTreeMap::new(),
        }
    }
}

fn valid_hex(c: &str) -> bool {
    hex_rgb(c).is_some()
}

/// The effect's own setting before the player changes it (Ally Hub's defaults).
pub fn default_param(effect: &str) -> u32 {
    match effect {
        "Breathe" => 25,
        "Color cycle" => 100,
        "Candle" => 55,
        "Twinkle" => 45,
        "Spiral" => 75,
        _ => 50,
    }
}

/// What the effect's own setting is called, for the effects that have one.
fn param_title(effect: &str) -> Option<&'static str> {
    match effect {
        "Breathe" => Some("Lowest brightness"),
        "Color cycle" => Some("Color strength"),
        "Candle" => Some("Flicker strength"),
        "Twinkle" => Some("Sparkle amount"),
        "Spiral" => Some("Color spread"),
        _ => None,
    }
}

/// How many colors an effect takes: at least, and at most.
fn color_count(effect: &str, rainbow: bool) -> (usize, usize) {
    match effect {
        "Color cycle" => (1, 1),
        "Wave" => (2, 4),
        "Strobe" | "Candle" => (1, 2),
        "Twinkle" => (2, 2),
        "Spiral" if !rainbow => (2, 4),
        "Spiral" => (1, 4),
        _ => (1, 1),
    }
}

/// The speed slider (0 to 100) as a pace: 0.25 times normal at 0, normal at 50, 4 times at 100.
pub fn pace(speed: u32) -> f64 {
    0.25 * 16f64.powf(speed.min(100) as f64 / 100.0)
}

/// A pace (1.0 is normal) as the speed slider's value.
pub fn speed_of(pace: f64) -> u32 {
    (100.0 * (pace / 0.25).ln() / 16f64.ln()).round().clamp(0.0, 100.0) as u32
}

/// One of Ally Hub's ready-made looks.
pub struct Preset {
    pub name: &'static str,
    effect: &'static str,
    colors: &'static [&'static str],
    speed: u32,
    param: u32,
    /// Spiral: (counter-clockwise, layout, drawn by the chip, rainbow).
    spiral: Option<(bool, &'static str, bool, bool)>,
}

pub const PRESETS: &[Preset] = &[
    Preset { name: "ROG Pulse", effect: "Breathe", colors: &["#FF0033"], speed: 50, param: 20, spiral: None },
    Preset { name: "Xbox Glow", effect: "Breathe", colors: &["#22C55E"], speed: 32, param: 30, spiral: None },
    Preset { name: "Rainbow", effect: "Color cycle", colors: &[], speed: 50, param: 100, spiral: None },
    Preset { name: "Aurora", effect: "Wave", colors: &["#22C55E", "#14B8A6", "#6366F1", "#A855F7"], speed: 28, param: 50, spiral: None },
    Preset { name: "Synthwave", effect: "Wave", colors: &["#FF2A6D", "#05D9E8"], speed: 42, param: 50, spiral: None },
    Preset { name: "Ocean", effect: "Wave", colors: &["#0057FF", "#00D5FF", "#0033AA"], speed: 12, param: 50, spiral: None },
    Preset { name: "Sakura", effect: "Wave", colors: &["#FF5FA2", "#FFD6E7"], speed: 21, param: 50, spiral: None },
    Preset { name: "Ember", effect: "Candle", colors: &["#FF5A00", "#FF1A00"], speed: 50, param: 60, spiral: None },
    Preset { name: "Heartbeat", effect: "Heartbeat", colors: &["#FF0022"], speed: 50, param: 50, spiral: None },
    Preset { name: "Starlight", effect: "Twinkle", colors: &["#2B1D8F", "#FFFFFF"], speed: 50, param: 50, spiral: None },
    // The chip's own spiral is the smoothest, and rainbow only.
    Preset { name: "RGB Spiral", effect: "Spiral", colors: &[], speed: 50, param: 75, spiral: Some((false, "linked", true, true)) },
    // Its own colors, so drawn frame by frame (the chip's spiral would show a rainbow instead).
    Preset { name: "Neon Vortex", effect: "Spiral", colors: &["#FF2A6D", "#05D9E8", "#A855F7"], speed: 62, param: 100, spiral: Some((true, "mirror", false, false)) },
];

impl Preset {
    /// The player's choices with this preset's look (brightness, the per-game, battery and dock choices stay).
    pub fn apply(&self, l: &Lighting) -> Lighting {
        let (ccw, layout, chip, rainbow) = self.spiral.unwrap_or((false, "linked", true, false));
        Lighting {
            effect: self.effect.into(),
            preset: self.name.into(),
            speed: self.speed,
            param: self.param,
            color: self.colors.first().map(|c| c.to_string()).unwrap_or_else(|| l.color.clone()),
            colors: self.colors.iter().skip(1).map(|c| c.to_string()).collect(),
            rainbow,
            ccw,
            layout: layout.into(),
            chip,
            ..l.clone()
        }
    }
    /// A few colors that show what the preset looks like.
    fn palette(&self) -> Vec<String> {
        if self.colors.is_empty() { ["#FF0000", "#FFD000", "#00FF40", "#00A0FF", "#A000FF"].iter().map(|c| c.to_string()).collect() } else { self.colors.iter().map(|c| c.to_string()).collect() }
    }
}

pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// A look ready to draw: the effect with its colors, pace and settings, checked and filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub effect: String,
    pub colors: Vec<Rgb>,
    /// 1.0 is normal.
    pub speed: f64,
    /// 0 to 1.
    pub param: f64,
    pub rainbow: bool,
    pub ccw: bool,
    pub layout: String,
    pub chip: bool,
}

impl Look {
    pub fn of(l: &Lighting) -> Look {
        let effect = if EFFECTS.contains(&l.effect.as_str()) { l.effect.clone() } else { "Static".into() };
        let (lo, hi) = color_count(&effect, l.rainbow);
        let mut colors: Vec<Rgb> = std::iter::once(&l.color).chain(&l.colors).filter_map(|c| hex_rgb(c)).take(hi).collect();
        while colors.len() < lo {
            colors.push(hex_rgb(MORE[colors.len() % MORE.len()]).unwrap_or((255, 255, 255)));
        }
        Look { effect, colors, speed: pace(l.speed), param: l.param.min(100) as f64 / 100.0, rainbow: l.rainbow, ccw: l.ccw, layout: l.layout.clone(), chip: l.chip }
    }
    /// One effect in these colors, nothing else (the charge, the low battery flash).
    pub fn effect(effect: &str, colors: &[&str], speed: u32, param: u32) -> Look {
        Look::of(&Lighting { effect: effect.into(), color: colors.first().map(|c| c.to_string()).unwrap_or_default(), colors: colors.iter().skip(1).map(|c| c.to_string()).collect(), speed, param, ..Lighting::default() })
    }
    /// A game's own look: "#RRGGBB" (the player's look in that color) or "preset:Name".
    pub fn from_ref(v: &str, l: &Lighting) -> Option<Look> {
        if valid_hex(v) {
            return Some(Look::of(&Lighting { color: v.to_string(), rainbow: false, ..l.clone() }));
        }
        Some(Look::of(&preset(v.strip_prefix("preset:")?)?.apply(l)))
    }
}

impl Lighting {
    fn file(folder: &Path) -> PathBuf {
        folder.join("lighting.json")
    }
    pub fn parse(text: &str) -> Lighting {
        let d = Lighting::default();
        let Ok(j) = json::parse(text) else { return d };
        let flag = |k: &str, def: bool| j.get(k).and_then(Json::as_bool).unwrap_or(def);
        let pct = |k: &str, def: u32| j.get(k).and_then(Json::as_f64).map(|v| (v.round() as i64).clamp(0, 100) as u32).unwrap_or(def);
        let effect = j.str_at("effect").filter(|e| EFFECTS.contains(e)).unwrap_or(&d.effect).to_string();
        let game = |v: &str| valid_hex(v) || v.strip_prefix("preset:").is_some_and(|p| preset(p).is_some());
        Lighting {
            preset: j.str_at("preset").filter(|p| preset(p).is_some()).unwrap_or("").to_string(),
            speed: pct("speed", d.speed),
            brightness: pct("brightness", d.brightness),
            color: j.str_at("color").filter(|c| valid_hex(c)).unwrap_or(&d.color).to_ascii_uppercase(),
            colors: j.get("colors").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).filter(|c| valid_hex(c)).take(3).map(|c| c.to_ascii_uppercase()).collect(),
            param: pct("param", default_param(&effect)),
            rainbow: flag("rainbow", false),
            ccw: flag("ccw", false),
            layout: j.str_at("layout").filter(|x| LAYOUTS.iter().any(|(k, _)| k == x)).unwrap_or("linked").to_string(),
            chip: flag("chip", true),
            per_game: flag("perGame", false),
            battery_level: flag("batteryLevel", false),
            low_flash: flag("lowFlash", false),
            off_when_docked: flag("offWhenDocked", false),
            tv_sound_when_docked: flag("tvSoundWhenDocked", false),
            huesync: flag("huesync", false),
            game_colors: j.get("gameColors").map(Json::entries).unwrap_or(&[]).iter().filter_map(|(id, c)| Some((id.clone(), c.as_str().filter(|c| game(c))?.to_string()))).collect(),
            effect,
        }
    }
    pub fn to_json(&self) -> String {
        obj! {
            "effect": s(&self.effect), "preset": s(&self.preset), "speed": n(self.speed as f64), "brightness": n(self.brightness as f64),
            "color": s(&self.color), "colors": arr(self.colors.iter().map(s)), "param": n(self.param as f64), "rainbow": b(self.rainbow),
            "ccw": b(self.ccw), "layout": s(&self.layout), "chip": b(self.chip), "perGame": b(self.per_game), "batteryLevel": b(self.battery_level),
            "lowFlash": b(self.low_flash), "offWhenDocked": b(self.off_when_docked), "tvSoundWhenDocked": b(self.tv_sound_when_docked), "huesync": b(self.huesync),
            "gameColors": Json::Obj(self.game_colors.iter().map(|(id, c)| (id.clone(), s(c))).collect())
        }
        .to_text()
            + "\n"
    }
    pub fn load(folder: &Path) -> Lighting {
        std::fs::read_to_string(Self::file(folder)).map(|t| Lighting::parse(&t)).unwrap_or_default()
    }
    pub fn save(&self, folder: &Path) {
        let _ = crate::write_atomic(&Self::file(folder), self.to_json().as_bytes());
    }
    /// Has the player saved lighting choices? The background helper drives the lights only then.
    pub fn saved(folder: &Path) -> bool {
        Self::file(folder).is_file()
    }
    /// The main color as red, green and blue, 0 to 255.
    pub fn rgb(&self) -> Rgb {
        hex_rgb(&self.color).unwrap_or((0, 0, 0))
    }
    /// The look's name in a few words: the preset's, or the effect's.
    pub fn label(&self) -> String {
        if self.huesync {
            "HueSync".into()
        } else if self.preset.is_empty() {
            self.effect.clone()
        } else {
            self.preset.clone()
        }
    }
}

/// The lighting files to write for the look's first moment: (path, value) per light driven through its files.
/// Lights driven through the lighting chip are left to the background helper.
pub fn light_writes(root: &Path, m: &Machine, l: &Lighting) -> Vec<(String, String)> {
    let leds = lights::leds(root, &m.rgb_leds);
    let scene = lights::Scene { lighting: l, game: "", battery: None, on_battery: false, charging: false, docked: false };
    lights::frame(&scene, &leds, 0)
}

/// What docking changes, in a few words.
pub fn dock_words(l: &Lighting) -> String {
    match (l.off_when_docked, l.tv_sound_when_docked) {
        (true, true) => "Lights off, sound to the TV",
        (true, false) => "Lights off",
        (false, true) => "Sound to the TV",
        (false, false) => "Nothing changes",
    }
    .into()
}

/// Is HueSync, the Decky plugin for handheld lights, installed?
pub fn huesync_installed(home: &Path) -> bool {
    std::fs::read_dir(home.join("homebrew/plugins")).is_ok_and(|d| {
        d.flatten().any(|e| e.file_name().to_string_lossy().to_ascii_lowercase().contains("huesync") || std::fs::read_to_string(e.path().join("plugin.json")).ok().and_then(|t| json::parse(&t).ok()).and_then(|j| j.str_at("name").map(|n| n.eq_ignore_ascii_case("huesync"))).unwrap_or(false))
    })
}

// ---------------------------------------------------------------- Test lights, through the background helper

fn test_request(folder: &Path) -> PathBuf {
    folder.join("lighting-test")
}
fn test_file(data: &Path) -> PathBuf {
    data.join("agent/lighting-test.json")
}

/// When Test lights was last asked for (seconds since 1970), for the background helper.
pub fn test_asked(folder: &Path) -> Option<u64> {
    std::fs::read_to_string(test_request(folder)).ok()?.trim().parse().ok()
}

/// The background helper's answer to Test lights: when, and what happened per light in plain words.
pub fn save_test(data: &Path, at: u64, lines: &[String]) {
    let _ = crate::write_atomic(&test_file(data), (obj! {"at": n(at as f64), "lines": arr(lines.iter().map(s))}.to_text() + "\n").as_bytes());
}

pub fn last_test(data: &Path) -> Option<(u64, Vec<String>)> {
    let j = json::parse(&std::fs::read_to_string(test_file(data)).ok()?).ok()?;
    Some((j.get("at")?.as_f64()? as u64, j.get("lines").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect()))
}

/// The lights can't be driven yet: the player saved lighting choices but the files (or the lighting chip) aren't
/// open to them. The checkup offers the fix.
pub fn locked(root: &Path, folder: &Path, m: &Machine) -> bool {
    let leds = lights::leds(root, &m.rgb_leds);
    Lighting::saved(folder) && !Lighting::load(folder).huesync && !leds.is_empty() && !leds.iter().all(|l| lights::writable(root, l))
}

// ---------------------------------------------------------------- the page

/// A slider row from 0 to 100.
fn slider(key: &str, title: &str, line: &str, v: u32, low: &str, high: &str) -> Json {
    let mut r = with(row("light", key, "slider", title, line), "value", n(v as f64));
    for (k, x) in [("min", 0.0), ("max", 100.0), ("step", 10.0)] {
        r = with(r, k, n(x));
    }
    with(with(with(r, "live", b(true)), "low", s(low)), "high", s(high))
}

fn choice(key: &str, title: &str, line: &str, options: &[&str], index: Option<usize>) -> Json {
    with(with(row("light", key, "choice", title, line), "options", arr(options.iter().map(|o| s(*o)))), "index", n(index.map(|i| i as f64).unwrap_or(-1.0)))
}

fn color_choice(key: &str, title: &str, now: &str) -> Json {
    let names: Vec<&str> = COLORS.iter().map(|(name, _)| *name).collect();
    with(choice(key, title, "", &names, COLORS.iter().position(|(_, h)| h.eq_ignore_ascii_case(now))), "swatches", arr(COLORS.iter().map(|(_, h)| s(*h))))
}

/// The two rings of a handheld's sticks, or one ring per light elsewhere.
fn rings(led: &lights::Led) -> usize {
    if led.name.contains("joystick") && led.zones >= 2 && led.zones.is_multiple_of(2) { 2 } else { 1 }
}

pub(crate) fn page(ui: &mut Ui, m: &Machine) -> Json {
    let l = Lighting::load(&ui.folder());
    let leds = lights::leds(&ui.ctx.root, &m.rgb_leds);
    let veteran = ui.veteran();
    let mut hints = Vec::new();
    let mut sections = Vec::new();
    let t = |key: &str, title: &str, line: &str, on: bool| with(row("light", key, "toggle", title, line), "on", b(on));
    let huesync = huesync_installed(&ui.ctx.home);
    let handoff = t("huesync", "Let HueSync run the lights", "HueSync, the Decky plugin, sets them from Steam's quick menu. Turbine Hub leaves them alone.", l.huesync);
    if l.huesync {
        sections.push(section("", vec![handoff]));
        return obj! {"crumb": s("My machine"), "title": s("Lighting"), "intro": s("HueSync runs the lights. Turn this off to set them here."), "sections": arr(sections), "hints": arr([])};
    }

    // With colors per game on, the look picked while a game runs is that game's own.
    let playing = ui.agent().game_now(crate::now()).cloned().filter(|_| l.per_game);
    let game_ref = playing.as_ref().and_then(|(id, _, _)| l.game_colors.get(id)).cloned();
    let game_name = playing.as_ref().map(|(_, name, _)| if name.is_empty() { "this game".to_string() } else { name.clone() });
    let shown_preset = match &game_ref {
        Some(r) => r.strip_prefix("preset:").unwrap_or("").to_string(),
        None => l.preset.clone(),
    };

    // Up front: the preview, the looks to pick from, and the brightness.
    let main = leds.iter().max_by_key(|x| x.zones);
    let zones = main.map(|x| x.zones).unwrap_or(1).clamp(1, 16);
    let frames = lights::preview(&l, playing.as_ref().map(|(id, _, _)| id.as_str()).unwrap_or(""), zones, 80, 100);
    let preview = with(with(with(with(with(row("none", "preview", "preview", "Preview", ""), "frames", arr(frames.into_iter().map(|f| arr(f.into_iter().map(s))))), "step", n(100.0)), "rings", n(main.map(rings).unwrap_or(1) as f64)), "level", n(0.35 + 0.65 * l.brightness.min(100) as f64 / 100.0)), "value", s(format!("{} \u{b7} {}%", l.label(), l.brightness)));
    let names: Vec<&str> = PRESETS.iter().map(|p| p.name).collect();
    let looks_title = match &game_name {
        Some(g) => format!("Look for {g}"),
        None => "Looks".to_string(),
    };
    let looks = with(with(choice("preset", &looks_title, "", &names, PRESETS.iter().position(|p| p.name == shown_preset)), "cards", b(true)), "palettes", arr(PRESETS.iter().map(|p| arr(p.palette().into_iter().map(s)))));
    sections.push(section("", vec![preview, slider("brightness", "Brightness", "", l.brightness, "Off", "Full"), looks]));

    // The effect and its settings.
    let mut effect_rows = vec![with(choice("effect", "Effect", "", EFFECTS, EFFECTS.iter().position(|e| *e == l.effect)), "wide", b(true))];
    if l.effect != "Static" {
        effect_rows.push(slider("speed", "Speed", "", l.speed, "Slow", "Fast"));
    }
    if let Some(title) = param_title(&l.effect) {
        effect_rows.push(slider("param", title, "", l.param, "Less", "More"));
    }
    let (_, most) = color_count(&l.effect, l.rainbow);
    let uses_color = !(l.effect == "Color cycle" || (l.effect == "Spiral" && l.rainbow));
    if uses_color {
        let (title, now) = match (&game_name, &game_ref) {
            (Some(g), Some(r)) if valid_hex(r) => (format!("Color for {g}"), r.clone()),
            (Some(g), _) => (format!("Color for {g}"), l.color.clone()),
            _ => ("Color".to_string(), l.color.clone()),
        };
        effect_rows.push(color_choice("color", &title, &now));
        if most >= 2 && game_name.is_none() {
            let second = l.colors.first().cloned().unwrap_or_else(|| Look::of(&l).colors.get(1).map(|c| lights::hex(*c)).unwrap_or_default());
            effect_rows.push(color_choice("color2", "Second color", &second));
        }
    }
    if l.effect == "Spiral" {
        effect_rows.push(choice("dir", "Direction", "", &["Clockwise", "Counter-clockwise"], Some(l.ccw as usize)));
        if main.is_some_and(|x| rings(x) == 2) {
            let opts: Vec<&str> = LAYOUTS.iter().map(|(_, w)| *w).collect();
            effect_rows.push(choice("layout", "Two rings", "", &opts, LAYOUTS.iter().position(|(k, _)| *k == l.layout)));
        }
        if leds.iter().any(|x| x.uses_chip()) {
            effect_rows.push(t("chipspin", "The controller draws the spiral", "Smoothest, in rainbow colors, at full brightness. Off: Turbine Hub draws it in your colors.", l.chip));
        }
    }
    sections.push(section("Effect", effect_rows));

    let mut color_rows = Vec::new();
    if !matches!(l.effect.as_str(), "Color cycle" | "Wave") {
        color_rows.push(t("rainbow", "Rainbow", "The color wheel instead of the colors above", l.rainbow));
    }
    color_rows.push(t("pergame", "A look per game", "Pick a look or a color while a game runs and the game keeps it", l.per_game));
    if m.battery().is_some() {
        color_rows.push(t("level", "Battery level", "Red to green with the charge, breathing while it charges", l.battery_level));
        color_rows.push(t("lowflash", "Low battery flash", "Flashes red below 15% on battery", l.low_flash));
    }
    sections.push(section("Colors", color_rows));

    // Test lights and who runs the lights.
    let mut light_rows = Vec::new();
    let (test_line, test_value) = match last_test(&ui.data) {
        Some((at, lines)) if !lines.is_empty() => (lines.join(" "), format!("{} ago", super::ago(crate::now(), at))),
        _ => ("Each part lights up in turn, then the result shows here.".to_string(), String::new()),
    };
    light_rows.push(with(with(with(row("light-test", "", "button", "Test lights", &test_line), "button", s("Test")), "value", s(test_value)), "icon", s("light")));
    if huesync {
        light_rows.push(handoff);
    }
    light_rows.push(with(with(row("light-report", "", "button", "Lights not right?", "Write a report with what Turbine Hub sees of the lights, to send with a problem."), "button", s("Write a report")), "icon", s("log")));
    if veteran {
        for led in &leds {
            let how = match &led.chip {
                Some(c) => format!("ASUS lighting chip at {}{}", c.lighting, if c.dynamic.is_some() { ", Dynamic Lighting off" } else { "" }),
                None if led.capped => "Files capped at 255, no lighting chip found".to_string(),
                None => format!("/sys/class/leds/{}", led.name),
            };
            light_rows.push(with(row("none", "", "info", &led.name, &how), "value", s(if led.zones == 1 { "1 zone".into() } else { format!("{} zones", led.zones) })));
        }
    }
    sections.push(section("Lights", light_rows));

    let ready = leds.iter().all(|led| lights::writable(&ui.ctx.root, led)) || ui.ledger().find_key(&modules::lighting_access().key()).is_some();
    if let Some(off) = super::machine::agent_off_row(ui, "Effects, a look per game and the battery level need") {
        sections.insert(0, section("", vec![off]));
    } else if !ready {
        sections.insert(0, section("", vec![with(with(row("light-test", "", "button", "The lights need a one-time setup", "Test lights asks for it once. After that the lights work by themselves, after restarts and sleep too."), "button", s("Set up")), "tone", s("warn"))]));
    }
    hints.push(hint("x", "Test lights"));
    obj! {
        "crumb": s("My machine"), "title": s("Lighting"),
        "intro": s(if game_name.is_some() { "A game is running: the look and color you pick are its own." } else { "Pick a look with A. The background helper plays it." }),
        "sections": arr(sections), "hints": arr(hints), "x": obj! {"act": s("light-test"), "arg": s("")}
    }
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, value: &str) -> Option<Json> {
    let num: Option<u64> = value.parse::<f64>().ok().map(|v| v.round().max(0.0) as u64);
    match action {
        "light" => {
            let mut l = Lighting::load(&ui.folder());
            let idx = num.unwrap_or(0) as usize;
            let game = ui.agent().game_now(crate::now()).filter(|_| l.per_game).map(|(id, _, _)| id.clone());
            let custom = |l: &mut Lighting| l.preset.clear();
            match arg {
                "preset" => {
                    let p = PRESETS.get(idx)?;
                    match game {
                        Some(g) => drop(l.game_colors.insert(g, format!("preset:{}", p.name))),
                        None => l = p.apply(&l),
                    }
                }
                "effect" => {
                    l.effect = EFFECTS.get(idx)?.to_string();
                    l.param = default_param(&l.effect);
                    custom(&mut l);
                }
                "color" => {
                    let color = COLORS.get(idx)?.1.to_string();
                    match game {
                        Some(g) => drop(l.game_colors.insert(g, color)),
                        None => {
                            l.color = color;
                            custom(&mut l);
                        }
                    }
                }
                "color2" => {
                    l.colors = vec![COLORS.get(idx)?.1.to_string()];
                    custom(&mut l);
                }
                "speed" => {
                    l.speed = (num? as u32).min(100);
                    custom(&mut l);
                }
                "param" => {
                    l.param = (num? as u32).min(100);
                    custom(&mut l);
                }
                "dir" => {
                    l.ccw = idx == 1;
                    custom(&mut l);
                }
                "layout" => {
                    l.layout = LAYOUTS.get(idx)?.0.into();
                    custom(&mut l);
                }
                "chipspin" => {
                    l.chip = !l.chip;
                    custom(&mut l);
                }
                "rainbow" => {
                    l.rainbow = !l.rainbow;
                    custom(&mut l);
                }
                "brightness" => l.brightness = (num? as u32).min(100),
                "pergame" => l.per_game = !l.per_game,
                "level" => l.battery_level = !l.battery_level,
                "lowflash" => l.low_flash = !l.low_flash,
                "huesync" => l.huesync = !l.huesync,
                "dockoff" => l.off_when_docked = !l.off_when_docked,
                "docksound" => l.tv_sound_when_docked = !l.tv_sound_when_docked,
                _ => return None,
            }
            l.save(&ui.folder());
            Some(refresh())
        }
        "light-report" => {
            let mut p = Plan::new("Write a lighting report?", "Turbine Hub writes what it sees of the lights and your lighting choices into a text file. Nothing is sent anywhere.", "Write it");
            p.steps.push(Step::Report { game: REPORT.into() });
            p.done = "Report written".into();
            Some(dialog(ui, p))
        }
        "light-test" => {
            let m = ui.machine();
            let l = Lighting::load(&ui.folder());
            if m.rgb_leds.is_empty() {
                return Some(toast("No lights found on this machine."));
            }
            let access = modules::lighting_access();
            let has_access = ui.ledger().find_key(&access.key()).is_some();
            if has_access && ui.agent_running() {
                // The background helper runs the test, so it goes the same way the effects do.
                if !Lighting::saved(&ui.folder()) {
                    l.save(&ui.folder());
                }
                let _ = crate::write_atomic(&test_request(&ui.folder()), crate::now().to_string().as_bytes());
                return Some(with(refresh(), "toast", s("Each part of the lights lights up in turn. The result shows under Test lights.")));
            }
            let writes = light_writes(&ui.ctx.root, &m, &l);
            let mut p = Plan::new("Set up the lights?", "Turbine Hub's helper lets the background helper set the lights from now on, after restarts and sleep too.", "Set up");
            if !has_access {
                p.items.push((access.name(), if ui.veteran() { access.exact() } else { "Asked once, so lighting never needs the password again.".into() }));
                p.steps.push(Step::Host(helper::apply(&access)));
            } else if writes.is_empty() {
                return Some(with(toast("Test lights needs the background helper. Settings, General turns it on."), "open", s("settings")));
            } else {
                p = Plan::new("Show the color on the lights?", "Sets the color and brightness now. Effects play once the background helper runs.", "Show");
            }
            p.steps.extend(writes.into_iter().map(|(path, value)| Step::HostWrite { path, value }));
            if !Lighting::saved(&ui.folder()) {
                l.save(&ui.folder());
            }
            p.done = if has_access { "Lights set".into() } else { "Lights set up".into() };
            Some(if has_access { dialog(ui, p) } else { with_helper(ui, p) })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_round_trip_and_junk_is_refused() {
        let mut l = PRESETS.iter().find(|p| p.name == "Neon Vortex").unwrap().apply(&Lighting { brightness: 30, per_game: true, ..Lighting::default() });
        l.game_colors.insert("steam:1".into(), "preset:Ember".into());
        l.game_colors.insert("steam:2".into(), "#6E9CC8".into());
        assert_eq!(Lighting::parse(&l.to_json()), l);
        assert_eq!((l.brightness, l.per_game, l.effect.as_str(), l.colors.len(), l.ccw, l.chip), (30, true, "Spiral", 2, true, false));
        let bad = Lighting::parse(r##"{"effect":"Lava","color":"red","colors":["#12","#ABCDEF"],"layout":"zigzag","gameColors":{"a":"preset:Nope","b":"#00FF00","c":"rm -rf"},"speed":400}"##);
        assert_eq!((bad.effect.as_str(), bad.color.as_str(), bad.colors.clone(), bad.layout.as_str(), bad.speed), ("Static", COLORS[0].1, vec!["#ABCDEF".to_string()], "linked", 100));
        assert_eq!(bad.game_colors.keys().collect::<Vec<_>>(), ["b"]);
        // Older files: effect, color and switches only.
        let old = Lighting::parse(r##"{"effect":"Breathe","speed":50,"brightness":60,"color":"#E0564A","rainbow":false,"perGame":true}"##);
        assert_eq!((old.param, old.chip, old.per_game, old.preset.as_str()), (25, true, true, ""));
    }

    #[test]
    fn every_preset_is_a_full_look() {
        assert_eq!(PRESETS.len(), 12);
        for p in PRESETS {
            let l = p.apply(&Lighting::default());
            let look = Look::of(&l);
            assert!(EFFECTS.contains(&l.effect.as_str()) && !look.colors.is_empty(), "{}", p.name);
            assert_eq!(Lighting::parse(&l.to_json()).label(), p.name);
        }
        let spiral = Look::of(&preset("RGB Spiral").unwrap().apply(&Lighting::default()));
        assert!(spiral.chip && spiral.rainbow);
        assert!((pace(50) - 1.0).abs() < 1e-9 && speed_of(1.0) == 50 && speed_of(0.5) == 25 && speed_of(4.0) == 100);
        // A wave always has two colors at least.
        assert_eq!(Look::of(&Lighting { effect: "Wave".into(), ..Lighting::default() }).colors.len(), 2);
    }
}

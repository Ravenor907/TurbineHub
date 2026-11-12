//! The Lighting page: the player's lighting choices (`Lighting`, kept in lighting.json), Ally Hub's effects and
//! presets, the page with its live preview, the guided light test, who else is setting the lights, and the lighting
//! check. The background helper plays the look (`lights`).

use super::plan::{Plan, Step, dialog, guide, refresh, toast, with_helper};
use super::{Ui, arr, b, n, obj, row, s, section, with};
use crate::hardware::Machine;
use crate::helper;
use crate::json::{self, Json};
use crate::lights::{self, Rgb, hex_rgb};
use crate::lights_hid;
use crate::modules;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const EFFECTS: &[&str] = &["Static", "Breathe", "Heartbeat", "Color cycle", "Wave", "Strobe", "Candle", "Twinkle", "Spiral"];
/// The colors to pick from. Jade, Turbine Hub's own color, comes first and is the color before the player picks
/// one; a saved choice keeps its color whatever this list says.
pub const COLORS: &[(&str, &str)] = &[("Jade", "#6FAE8C"), ("Brass", "#C99E52"), ("Red", "#E0564A"), ("Orange", "#E08A3C"), ("Blue", "#6E9CC8"), ("Purple", "#A58BC6"), ("Pink", "#FF5FA2"), ("Cyan", "#05D9E8"), ("White", "#FFFFFF")];
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
    /// The lights are left to Ally Hub on the same machine: Turbine Hub doesn't touch them.
    pub allyhub: bool,
    /// The way the lights are driven, once the guided test found one the player saw working ("files:hex",
    /// "chip:m6", see `lights::route_ok`). Empty until then: Ally Hub's saved choice is the starting point when it
    /// is on the same machine, the light's own files otherwise.
    pub route: String,
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
            allyhub: false,
            route: String::new(),
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
            allyhub: flag("allyhub", false),
            route: j.str_at("route").filter(|r| lights::route_ok(r)).unwrap_or("").to_string(),
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
            "allyhub": b(self.allyhub), "route": s(&self.route),
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
    /// The other app the player left the lights to, by name. Turbine Hub keeps its hands off while this is set.
    pub fn left_to(&self) -> Option<&'static str> {
        if self.huesync {
            Some("HueSync")
        } else if self.allyhub {
            Some("Ally Hub")
        } else {
            None
        }
    }
    /// The main color as red, green and blue, 0 to 255.
    pub fn rgb(&self) -> Rgb {
        hex_rgb(&self.color).unwrap_or((0, 0, 0))
    }
    /// The look's name in a few words: the preset's, or the effect's.
    pub fn label(&self) -> String {
        if let Some(name) = self.left_to() {
            name.into()
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
    let leds = lights::driven(root, &m.rgb_leds, &l.route);
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

// ---------------------------------------------------------------- what the host says about the lights

/// Ally Hub's saved lighting choice (its config.json, read through the host and never written).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AllyHub {
    /// "allyhub" when Ally Hub drives the rings itself, "huesync" (its default) when it leaves them alone.
    pub controller: String,
    /// The chip route its guided test saved ("m6"), or empty.
    pub hid_method: String,
    /// "hid" for the lighting chip, or how packed colors go to the files ("hex", "packed", "packed_bgr"), or empty.
    pub encoding: String,
}

/// What only the host can say about the lights: the sandbox doesn't see the player's ~/.config, the session's
/// services or /etc. Gathered with read-only commands (`host_questions`), kept in a small file so the background
/// helper starts from the same facts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostLights {
    /// The host was asked. Until then nothing below means anything.
    pub asked: bool,
    pub allyhub: Option<AllyHub>,
    /// Ally Hub's background agent is running now.
    pub allyhub_agent: bool,
    /// Decky Loader is running now (HueSync works through it).
    pub decky_running: bool,
    /// The helper's lighting rule file as the host has it, or None when it isn't there.
    pub rule: Option<String>,
    /// systemd's word for the helper's unit that opens the lights at every start and wake ("enabled"), or empty.
    pub reapply: String,
}

/// The read-only commands that answer `HostLights`, in the order `HostLights::from_host` takes their output. The
/// app runs each on the host.
pub fn host_questions(home: &Path) -> Vec<Vec<String>> {
    let line = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<String>>();
    vec![
        line(&["cat", &home.join(".config/allyhub/config.json").to_string_lossy()]),
        line(&["systemctl", "--user", "is-active", "allyhub-agent.service"]),
        line(&["systemctl", "is-active", "plugin_loader.service"]),
        line(&["cat", turbinehub_helper::policy::ACCESS_FILE]),
        line(&["systemctl", "is-enabled", "turbinehub-reapply.service"]),
    ]
}

impl HostLights {
    /// From what the commands of `host_questions` printed: `config` and `rule` only when the file could be read,
    /// the others whatever systemctl said.
    pub fn from_host(config: Option<&str>, agent: &str, decky: &str, rule: Option<&str>, reapply: &str) -> HostLights {
        let allyhub = config.and_then(|t| json::parse(t).ok()).map(|j| {
            let light = j.get("lighting");
            let word = |k: &str, def: &str| light.and_then(|l| l.str_at(k)).unwrap_or(def).chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(24).collect::<String>();
            // Ally Hub leaves the rings to HueSync unless its settings say otherwise.
            AllyHub { controller: word("controller", "huesync"), hid_method: word("hid_method", ""), encoding: word("encoding", "") }
        });
        let first = |t: &str| t.lines().next().unwrap_or("").trim().to_string();
        HostLights { asked: true, allyhub, allyhub_agent: first(agent) == "active", decky_running: first(decky) == "active", rule: rule.map(|r| r.trim().to_string()), reapply: first(reapply) }
    }
    fn file(folder: &Path) -> PathBuf {
        folder.join("lighting-host.json")
    }
    pub fn to_json(&self) -> String {
        let ally = match &self.allyhub {
            Some(a) => obj! {"controller": s(&a.controller), "hidMethod": s(&a.hid_method), "encoding": s(&a.encoding)},
            None => Json::Null,
        };
        obj! {"asked": b(self.asked), "allyhub": ally, "allyhubAgent": b(self.allyhub_agent), "deckyRunning": b(self.decky_running), "rule": self.rule.as_ref().map(s).unwrap_or(Json::Null), "reapply": s(&self.reapply)}.to_text() + "\n"
    }
    pub fn parse(text: &str) -> HostLights {
        let Ok(j) = json::parse(text) else { return HostLights::default() };
        let flag = |k: &str| j.get(k).and_then(Json::as_bool).unwrap_or(false);
        let allyhub = j.get("allyhub").filter(|a| a.str_at("controller").is_some()).map(|a| AllyHub { controller: a.str_at("controller").unwrap_or("").into(), hid_method: a.str_at("hidMethod").unwrap_or("").into(), encoding: a.str_at("encoding").unwrap_or("").into() });
        HostLights { asked: flag("asked"), allyhub, allyhub_agent: flag("allyhubAgent"), decky_running: flag("deckyRunning"), rule: j.str_at("rule").map(String::from), reapply: j.str_at("reapply").unwrap_or("").into() }
    }
    pub fn load(folder: &Path) -> HostLights {
        std::fs::read_to_string(Self::file(folder)).map(|t| HostLights::parse(&t)).unwrap_or_default()
    }
    /// Kept for the background helper, written only when something changed.
    pub fn save(&self, folder: &Path) {
        if HostLights::load(folder) != *self {
            let _ = crate::write_atomic(&Self::file(folder), self.to_json().as_bytes());
        }
    }
    /// The way Ally Hub drives the lights on this machine, as a route: what its own guided test saved. Ally Hub
    /// uses "Full handshake" when its settings say chip and name no route.
    pub fn allyhub_route(&self) -> String {
        let Some(a) = &self.allyhub else { return String::new() };
        let route = match a.encoding.as_str() {
            "hid" => format!("chip:{}", if lights_hid::method_of(&a.hid_method).is_some() { a.hid_method.as_str() } else { lights_hid::ALLYHUB_DEFAULT }),
            "" => String::new(),
            enc => format!("files:{enc}"),
        };
        if lights::route_ok(&route) { route } else { String::new() }
    }
}

/// The way the lights are driven now: the guided test's answer, else Ally Hub's saved choice on the same machine,
/// else "" (decided from the light's own files).
pub fn route_in_use(l: &Lighting, host: &HostLights) -> String {
    if l.route.is_empty() { host.allyhub_route() } else { l.route.clone() }
}

/// Where the way in use came from, in a few words.
fn route_source(l: &Lighting, host: &HostLights) -> &'static str {
    if !l.route.is_empty() {
        "your light test"
    } else if !host.allyhub_route().is_empty() {
        "Ally Hub's saved choice"
    } else {
        "picked by itself"
    }
}

/// Another app that is setting the lights now. Two at once overwrite each other many times a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rival {
    /// The switch in `Lighting` that leaves the lights to it ("allyhub", "huesync").
    pub key: &'static str,
    pub name: &'static str,
    /// Where its own lighting is turned off, step by step. Turbine Hub never changes another app's settings.
    pub off: &'static [(&'static str, &'static str)],
}

const ALLYHUB: Rival = Rival { key: "allyhub", name: "Ally Hub", off: &[("1", "Open Ally Hub and go to Lighting."), ("2", "Press \"Use HueSync instead\" and say yes. Ally Hub then stops changing the rings (HueSync doesn't need to be installed for that)."), ("3", "Come back here. This note goes away once Ally Hub has let go.")] };
const HUESYNC: Rival = Rival { key: "huesync", name: "HueSync", off: &[("1", "Open Steam's quick menu (the ... button) and pick the plug icon, then HueSync."), ("2", "Turn its light control off there. Or turn the plugin off: Turbine Hub's Store, Decky plugins."), ("3", "Come back here. This note goes away once HueSync has let go.")] };

/// The other apps setting the lights now: Ally Hub when its background agent runs and its settings say it drives
/// the rings itself, HueSync when the plugin is installed and Decky Loader runs.
pub fn rivals(host: &HostLights, home: &Path) -> Vec<Rival> {
    let mut out = Vec::new();
    if host.allyhub_agent && host.allyhub.as_ref().is_some_and(|a| a.controller == "allyhub") {
        out.push(ALLYHUB);
    }
    if host.decky_running && huesync_installed(home) {
        out.push(HUESYNC);
    }
    out
}

// ---------------------------------------------------------------- the guided light test, through the background helper

/// The guided test in progress: the ways still to try, and what the player said so far. Kept by the app while it
/// runs; the background helper lights the rings for each way it is asked for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LightTest {
    pub routes: Vec<String>,
    /// The way being shown now.
    pub at: usize,
    /// The stamp of the last request, so each one is newer than the one before.
    pub asked: u64,
    /// One line per way tried: what was sent and what the player saw.
    pub log: Vec<String>,
    /// Two rings to look at (the question's words).
    pub rings: bool,
}

fn test_request(folder: &Path) -> PathBuf {
    folder.join("lighting-test")
}
fn test_file(data: &Path) -> PathBuf {
    data.join("agent/lighting-test.json")
}
fn step_file(data: &Path) -> PathBuf {
    data.join("agent/lighting-step.json")
}
fn state_file(data: &Path) -> PathBuf {
    data.join("agent/lights.json")
}
fn stuck_file(folder: &Path) -> PathBuf {
    folder.join("lighting-stuck")
}

/// The word that ends a test: the background helper goes back to the look.
pub const TEST_END: &str = "end";

/// What the page last asked the background helper to show: its stamp, and the way to light the rings red with
/// (or `TEST_END`).
pub fn test_asked(folder: &Path) -> Option<(u64, String)> {
    let text = std::fs::read_to_string(test_request(folder)).ok()?;
    let mut words = text.split_whitespace();
    let at = words.next()?.parse().ok()?;
    let route = words.next().unwrap_or(TEST_END).to_string();
    (route == TEST_END || (lights::route_ok(&route) && !route.is_empty())).then_some((at, route))
}

fn ask_test(folder: &Path, at: u64, route: &str) {
    let _ = crate::write_atomic(&test_request(folder), format!("{at} {route}\n").as_bytes());
}

/// The background helper's answer to one way of the test: the request it answers, the way, whether it could be
/// sent at all, and what happened in a few lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Tried {
    pub at: u64,
    pub route: String,
    pub sent: bool,
    pub lines: Vec<String>,
}

pub fn save_step(data: &Path, t: &Tried) {
    let _ = crate::write_atomic(&step_file(data), (obj! {"at": n(t.at as f64), "route": s(&t.route), "sent": b(t.sent), "lines": arr(t.lines.iter().map(s))}.to_text() + "\n").as_bytes());
}

pub fn last_step(data: &Path) -> Option<Tried> {
    let j = json::parse(&std::fs::read_to_string(step_file(data)).ok()?).ok()?;
    Some(Tried { at: j.get("at")?.as_f64()? as u64, route: j.str_at("route")?.to_string(), sent: j.get("sent").and_then(Json::as_bool).unwrap_or(false), lines: j.get("lines").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect() })
}

/// How the last light test ended: when, and what happened per way in plain words.
pub fn save_test(data: &Path, at: u64, lines: &[String]) {
    let _ = crate::write_atomic(&test_file(data), (obj! {"at": n(at as f64), "lines": arr(lines.iter().map(s))}.to_text() + "\n").as_bytes());
}

pub fn last_test(data: &Path) -> Option<(u64, Vec<String>)> {
    let j = json::parse(&std::fs::read_to_string(test_file(data)).ok()?).ok()?;
    Some((j.get("at")?.as_f64()? as u64, j.get("lines").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect()))
}

/// The background helper's own notes about the lights (what it drives and how, what it wrote and read back, what
/// failed), for the lighting check.
pub fn save_state(data: &Path, at: u64, lines: &[String]) {
    let _ = crate::write_atomic(&state_file(data), (obj! {"at": n(at as f64), "lines": arr(lines.iter().map(s))}.to_text() + "\n").as_bytes());
}

pub fn last_state(data: &Path) -> Option<(u64, Vec<String>)> {
    let j = json::parse(&std::fs::read_to_string(state_file(data)).ok()?).ok()?;
    Some((j.get("at")?.as_f64()? as u64, j.get("lines").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect()))
}

// ---------------------------------------------------------------- closed lights

/// The main light can't be driven: what exactly is closed for the way in use, whether the one-time setup was done
/// already, and whether opening it again was tried and didn't hold.
#[derive(Debug, Clone, PartialEq)]
pub struct Shut {
    pub closed: Vec<String>,
    pub setup_done: bool,
    pub stuck: bool,
}

/// Is the light Turbine Hub drives closed to it? Asked of exactly what the way in use needs (`lights::closed`), for
/// the main light only: another LED that stays closed never makes the lights "need their setup".
pub(crate) fn shut_now(ui: &Ui, m: &Machine, l: &Lighting) -> Option<Shut> {
    let leds = lights::driven(&ui.ctx.root, &m.rgb_leds, &route_in_use(l, &ui.host_lights));
    let closed = lights::closed(&ui.ctx.root, lights::main_light(&leds)?);
    if closed.is_empty() {
        return None;
    }
    let stuck = std::fs::read_to_string(stuck_file(&ui.folder())).is_ok_and(|t| t.trim() == closed.join("\n"));
    Some(Shut { setup_done: ui.ledger().find_key(&modules::lighting_access().key()).is_some(), stuck, closed })
}

/// The lights can't be driven yet: the player saved lighting choices and didn't leave the lights to another app,
/// but what the way in use needs isn't open. The checkup offers the fix.
pub(crate) fn locked(ui: &Ui, m: &Machine) -> Option<Shut> {
    let l = Lighting::load(&ui.folder());
    if !Lighting::saved(&ui.folder()) || l.left_to().is_some() {
        return None;
    }
    shut_now(ui, m, &l)
}

/// The checkup's line for closed lights once the setup was done: what happened, and in Veteran mode exactly what
/// is closed.
pub(crate) fn shut_words(ui: &Ui, sh: &Shut) -> String {
    if !sh.setup_done {
        return String::new();
    }
    let what = if ui.veteran() { format!(" Closed: {}.", sh.closed.join(", ")) } else { String::new() };
    if sh.stuck { format!("{}{what}", lights::closed_why(&sh.closed)) } else { format!("The setup is done, and they are closed again. One press opens them.{what}") }
}

/// After the helper opened the lights: looks again at exactly the paths the way in use needs, and says what is
/// still closed and why instead of asking for the same setup forever. Remembers it, so the checkup offers the
/// lighting check next and not the same fix. `exact`: name the paths (Veteran mode).
pub fn check_open(root: &Path, folder: &Path, need: &[String], exact: bool) -> Result<(), String> {
    let closed = lights::closed_among(root, need);
    if closed.is_empty() {
        let _ = std::fs::remove_file(stuck_file(folder));
        return Ok(());
    }
    let _ = crate::write_atomic(&stuck_file(folder), closed.join("\n").as_bytes());
    let what = if exact { format!(": {}", closed.join(", ")) } else { String::new() };
    Err(format!("The lights are still closed{what}. {}", lights::closed_why(&closed)))
}

// ---------------------------------------------------------------- the lighting check

/// Where the lighting check writes its report: one file, replaced each time, so it is easy to find and send.
pub fn report_file(ui: &Ui) -> PathBuf {
    super::games::reports_dir(ui).join("TurbineHub-lighting-report.txt")
}

/// Everything Turbine Hub knows about the lights, for the lighting check: the way in use, who else is setting the
/// lights, the helper's part, what is closed, every LED and ASUS interface, what was written and what reads back,
/// the background helper's own notes and the last light test.
pub(crate) fn report_lines(ui: &Ui) -> Vec<String> {
    let m = ui.machine();
    let l = Lighting::load(&ui.folder());
    let host = &ui.host_lights;
    let route = route_in_use(&l, host);
    let leds = lights::driven(&ui.ctx.root, &m.rgb_leds, &route);
    let mut out = vec![
        format!("way in use: {} ({}); route setting [{}]", lights::route_words(&route), route_source(&l, host), route),
        format!("driving: choices saved {}, background helper {}, lights left to {}", if Lighting::saved(&ui.folder()) { "yes" } else { "no" }, if ui.agent_running() { "running" } else { "not running" }, l.left_to().unwrap_or("nobody")),
    ];
    out.push(if !host.asked {
        "others: the host wasn't asked yet".into()
    } else {
        let ally = match &host.allyhub {
            Some(a) => format!("Ally Hub settings found (controller {}, hid_method {}, encoding {}), its agent {}", a.controller, if a.hid_method.is_empty() { "none" } else { &a.hid_method }, if a.encoding.is_empty() { "none" } else { &a.encoding }, if host.allyhub_agent { "active" } else { "not active" }),
            None => "Ally Hub not found".into(),
        };
        let names: Vec<&str> = rivals(host, &ui.ctx.home).iter().map(|r| r.name).collect();
        format!("others: {ally}; HueSync {}, Decky Loader {}; setting the lights now: {}", if huesync_installed(&ui.ctx.home) { "installed" } else { "not installed" }, if host.decky_running { "running" } else { "not running" }, if names.is_empty() { "nobody else".to_string() } else { names.join(", ") })
    });
    let recorded = ui.ledger().find_key(&modules::lighting_access().key()).is_some();
    out.push(format!(
        "helper: {}{}, lighting rule in its record: {}, rule file on the host: {}, unit that opens the lights at every start and wake: {}",
        if helper::installed(&ui.ctx.root) { "installed" } else { "not installed" },
        if ui.helper_outdated() { " (older than this Turbine Hub)" } else { "" },
        if recorded { "yes" } else { "no" },
        match (&host.rule, host.asked) {
            (_, false) => "not asked yet".to_string(),
            (None, true) => "missing".to_string(),
            (Some(r), true) => format!("[{}]", r.replace('\n', " | ")),
        },
        if !host.asked { "not asked yet" } else if host.reapply.is_empty() { "not installed" } else { &host.reapply }
    ));
    for led in &leds {
        let shut = lights::closed(&ui.ctx.root, led);
        out.push(format!("step: {} open to you for {}: {}", led.name, led.route(), if shut.is_empty() { "yes".to_string() } else { format!("NO, closed: {}", shut.join(", ")) }));
    }
    if let Ok(t) = std::fs::read_to_string(stuck_file(&ui.folder())) {
        out.push(format!("step: opening again was tried and these stayed closed: {}", t.trim().replace('\n', ", ")));
    }
    out.extend(lights::diagnostics(&ui.ctx.root, &route));
    // What the look's first moment writes to each light's files, and what the files hold now.
    let scene = lights::Scene { lighting: &l, game: "", battery: None, on_battery: false, charging: false, docked: false };
    for (path, value) in lights::frame(&scene, &leds, 0) {
        let now = std::fs::read_to_string(ui.ctx.root.join(path.trim_start_matches('/'))).map(|t| t.trim().to_string()).unwrap_or_else(|_| "(unreadable)".into());
        let same = lights::numbers(&value).is_some() && lights::numbers(&value) == lights::numbers(&now);
        out.push(format!("write and read back: {path} written [{value}] (the look's first moment), holds [{now}]: {}", if same { "kept" } else if ui.agent_running() { "differs (the background helper may be animating)" } else { "DIFFERS" }));
    }
    match last_state(&ui.data) {
        Some((at, lines)) => {
            out.push(format!("background helper's notes ({} ago):", super::ago(crate::now(), at)));
            out.extend(lines.into_iter().map(|x| format!("  {x}")));
        }
        None => out.push("background helper's notes: none yet".into()),
    }
    match last_test(&ui.data) {
        Some((at, lines)) => {
            out.push(format!("last light test ({} ago):", super::ago(crate::now(), at)));
            out.extend(lines.into_iter().map(|x| format!("  {x}")));
        }
        None => out.push("last light test: never run".into()),
    }
    if let Some(t) = &ui.light_test {
        out.push(format!("light test running: way {} of {}", t.at + 1, t.routes.len()));
        out.extend(t.log.iter().map(|x| format!("  {x}")));
    }
    out.push(format!("choices: {}", l.to_json().trim()));
    out
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

/// The switch that leaves the lights to another app.
fn handoff_row(r: &Rival, on: bool) -> Json {
    let line = match r.key {
        "huesync" => "HueSync, the Decky plugin, sets them from Steam's quick menu. Turbine Hub leaves them alone.".to_string(),
        _ => format!("{} sets them. Turbine Hub leaves them alone.", r.name),
    };
    with(row("light", r.key, "toggle", &format!("Let {} run the lights", r.name), &line), "on", b(on))
}

/// The rows of the guided test while it runs: the question, what the background helper said about this way, and
/// the three answers.
fn test_rows(ui: &Ui, t: &LightTest) -> Vec<Json> {
    let route = t.routes.get(t.at).cloned().unwrap_or_default();
    let tried = last_step(&ui.data).filter(|x| x.at == t.asked && x.route == route);
    let question = if t.rings { "Did both rings turn red?" } else { "Did the lights turn red?" };
    let mut line = match &tried {
        None => "The lights turn red in a moment. Then answer below.".to_string(),
        Some(x) if !x.sent => format!("This way couldn't be sent, so nothing changed: {}", x.lines.join(" ")),
        Some(_) => "Look at the lights now, then answer below.".to_string(),
    };
    if ui.veteran() {
        line = format!("{line} Way: {}.{}", lights::route_words(&route), tried.as_ref().filter(|x| x.sent).map(|x| format!(" {}", x.lines.join(" "))).unwrap_or_default());
    }
    let answer = |arg: &str, title: &str, line: &str, button: &str| with(row("light-answer", arg, "button", title, line), "button", s(button));
    vec![
        with(with(with(row("none", "light-question", "info", question, &line), "value", s(format!("Way {} of {}", t.at + 1, t.routes.len()))), "tone", s("warn")), "icon", s("light")),
        answer("yes", "Yes", "Turbine Hub keeps this way and your look comes back.", "Yes"),
        answer("no", "No", "Nothing changed, or the color is wrong. The next way is tried.", "No"),
        answer("partly", "Only partly", "One ring, or only some parts turned red. The next way is tried.", "Partly"),
        answer("stop", "Stop the test", "Your look comes back and nothing is changed.", "Stop"),
    ]
}

pub(crate) fn page(ui: &mut Ui, m: &Machine) -> Json {
    let l = Lighting::load(&ui.folder());
    let route = route_in_use(&l, &ui.host_lights);
    let leds = lights::driven(&ui.ctx.root, &m.rgb_leds, &route);
    let veteran = ui.veteran();
    let mut sections = Vec::new();
    let t = |key: &str, title: &str, line: &str, on: bool| with(row("light", key, "toggle", title, line), "on", b(on));
    let huesync = huesync_installed(&ui.ctx.home);
    let others = rivals(&ui.host_lights, &ui.ctx.home);
    if let Some(name) = l.left_to() {
        let r = if l.huesync { &HUESYNC } else { &ALLYHUB };
        sections.push(section("", vec![handoff_row(r, true)]));
        return obj! {"crumb": s("My machine"), "title": s("Lighting"), "intro": s(format!("{name} runs the lights. Turn this off to set them here.")), "sections": arr(sections)};
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
    let main = lights::main_light(&leds);
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

    // The light test, the lighting check and who runs the lights.
    let mut light_rows = Vec::new();
    let (test_line, test_value) = match last_test(&ui.data) {
        Some((at, lines)) if !lines.is_empty() => (lines.last().cloned().unwrap_or_default(), format!("{} ago", super::ago(crate::now(), at))),
        _ => ("The lights turn red one way after another. You say when they do, and Turbine Hub keeps that way.".to_string(), String::new()),
    };
    light_rows.push(with(with(with(row("light-test", "", "button", "Test lights", &test_line), "button", s("Test")), "value", s(test_value)), "icon", s("light")));
    if huesync {
        light_rows.push(handoff_row(&HUESYNC, false));
    }
    if ui.host_lights.allyhub.is_some() {
        light_rows.push(handoff_row(&ALLYHUB, false));
    }
    light_rows.push(with(with(row("light-report", "", "button", "Lighting check", "Lights not right? One press writes everything Turbine Hub sees of the lights into a text file you can send."), "button", s("Write a report")), "icon", s("log")));
    if veteran {
        for led in &leds {
            let shut = lights::closed(&ui.ctx.root, led);
            let how = match &led.chip {
                Some(c) => format!("ASUS lighting chip at {}, route {}{}", c.lighting, led.method.key, if c.dynamic.is_some() { ", Dynamic Lighting off" } else { "" }),
                None if led.capped && !led.fixed => "Files capped at 255, no lighting chip found".to_string(),
                None if led.packed() => format!("/sys/class/leds/{}, colors as {}", led.name, led.encoding.key()),
                None => format!("/sys/class/leds/{}", led.name),
            };
            let how = if shut.is_empty() { how } else { format!("{how}. Closed: {}", shut.join(", ")) };
            light_rows.push(with(row("none", &format!("led-{}", led.name), "info", &led.name, &how), "value", s(if led.zones == 1 { "1 zone".into() } else { format!("{} zones", led.zones) })));
        }
        if !leds.is_empty() {
            light_rows.push(with(row("none", "light-way", "info", "Way in use", &format!("{} ({})", lights::route_words(&route), route_source(&l, &ui.host_lights))), "value", s(if route.is_empty() { "auto".to_string() } else { route.clone() })));
        }
    }
    sections.push(section("Lights", light_rows));

    // First on the page, in this order: the test in progress, another app setting the lights, closed lights.
    let mut first = Vec::new();
    if let Some(test) = ui.light_test.clone() {
        first.push(section("Light test", test_rows(ui, &test)));
    }
    if let Some(r) = others.first() {
        let who = with(with(row("none", "light-rival", "info", &format!("{} is setting the lights now", r.name), "Two apps setting the same lights overwrite each other many times a second, so one has to let go. Pick who runs them."), "tone", s("warn")), "icon", s("light"));
        let mine = with(row("light-rival", r.key, "button", "Let Turbine Hub drive them", &format!("Shows where to turn {}'s lighting off. Turbine Hub never changes another app's settings.", r.name)), "button", s("How"));
        let theirs = with(row("light", r.key, "button", &format!("Leave them to {}", r.name), "Turbine Hub keeps its hands off the lights until you turn this back."), "button", s("Leave"));
        first.push(section("", vec![who, mine, theirs]));
    }
    if let Some(off) = super::machine::agent_off_row(ui, "Effects, a look per game and the battery level need") {
        first.push(section("", vec![off]));
    } else if let Some(sh) = shut_now(ui, m, &l) {
        let list = if veteran { format!(" Closed: {}.", sh.closed.join(", ")) } else { String::new() };
        let r = if !sh.setup_done {
            with(row("light-test", "", "button", "The lights need a one-time setup", "Test lights asks for it once. After that the lights work by themselves, after restarts and sleep too."), "button", s("Set up"))
        } else if sh.stuck {
            with(row("light-report", "", "button", "The lights stay closed", &format!("{}{list}", lights::closed_why(&sh.closed))), "button", s("Write a report"))
        } else {
            with(row("light-test", "", "button", "The lights are closed again", &format!("The setup is done, and something closed them since. One press opens them.{list}")), "button", s("Open them"))
        };
        first.push(section("", vec![with(r, "tone", s("warn"))]));
    }
    for (i, sec) in first.into_iter().enumerate() {
        sections.insert(i, sec);
    }
    obj! {
        "crumb": s("My machine"), "title": s("Lighting"),
        "intro": s(if ui.light_test.is_some() { "The light test is running: answer what you see on the lights." } else if game_name.is_some() { "A game is running: the look and color you pick are its own." } else { "Pick a look with A. The background helper plays it." }),
        "sections": arr(sections)
    }
}

/// Ends the guided test: the background helper goes back to the look, and the result is kept for the page and the
/// lighting check.
fn end_test(ui: &mut Ui, result: &str) {
    if let Some(mut t) = ui.light_test.take() {
        ask_test(&ui.folder(), t.asked.max(crate::now()) + 1, TEST_END);
        t.log.push(result.to_string());
        save_test(&ui.data, crate::now(), &t.log);
    }
}

/// Asks the background helper to light the rings red the way the test is at now.
fn show_way(ui: &mut Ui) {
    let folder = ui.folder();
    if let Some(t) = ui.light_test.as_mut() {
        t.asked = t.asked.max(crate::now().saturating_sub(1)) + 1;
        if let Some(route) = t.routes.get(t.at) {
            ask_test(&folder, t.asked, route);
        }
    }
}

/// What the background helper said about the way the test is at, for the test's record.
fn tried_words(ui: &Ui, t: &LightTest) -> String {
    let route = t.routes.get(t.at).cloned().unwrap_or_default();
    match last_step(&ui.data).filter(|x| x.at == t.asked && x.route == route) {
        Some(x) => format!("{}: {}", if x.sent { "sent" } else { "NOT sent" }, x.lines.join(" ")),
        None => "no answer from the background helper yet".into(),
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
                // The lights go to one other app at most; handing them over ends a test in progress.
                "huesync" => (l.huesync, l.allyhub) = (!l.huesync, false),
                "allyhub" => (l.allyhub, l.huesync) = (!l.allyhub, false),
                "dockoff" => l.off_when_docked = !l.off_when_docked,
                "docksound" => l.tv_sound_when_docked = !l.tv_sound_when_docked,
                _ => return None,
            }
            if l.left_to().is_some() {
                end_test(ui, "Stopped: the lights were left to another app.");
            }
            l.save(&ui.folder());
            Some(refresh())
        }
        "light-rival" => {
            let r = [ALLYHUB, HUESYNC].into_iter().find(|r| r.key == arg)?;
            // Turbine Hub drives them from here on, so its own switch for leaving them alone goes off.
            let mut l = Lighting::load(&ui.folder());
            if l.left_to().is_some() {
                (l.huesync, l.allyhub) = (false, false);
                l.save(&ui.folder());
            }
            Some(guide(&format!("Turn {}'s lighting off", r.name), &format!("Turbine Hub never changes another app's settings, so this part is yours. Until {} lets go, it keeps painting over Turbine Hub's colors.", r.name), r.off))
        }
        "light-report" => {
            let file = report_file(ui);
            let mut p = Plan::new("Run the lighting check?", "Turbine Hub writes everything it sees of the lights into one text file: every light and its files, the controller's lighting interfaces, the way in use, who else is setting the lights, what its helper set up, and how each step went. Nothing is sent anywhere.", "Write it");
            // With the background helper off, the look's first moment is written now so the report can say what
            // the lights' files kept of it.
            if !ui.agent_running() {
                let m = ui.machine();
                let l = Lighting::load(&ui.folder());
                let leds: Vec<lights::Led> = lights::driven(&ui.ctx.root, &m.rgb_leds, &route_in_use(&l, &ui.host_lights)).into_iter().filter(|x| !x.uses_chip() && lights::writable(&ui.ctx.root, x)).collect();
                let scene = lights::Scene { lighting: &l, game: "", battery: None, on_battery: false, charging: false, docked: false };
                p.steps.extend(lights::frame(&scene, &leds, 0).into_iter().map(|(path, value)| Step::HostWrite { path, value }));
            }
            p.steps.push(Step::Report { game: REPORT.into() });
            p.done = format!("Lighting report written: {}", file.display());
            Some(dialog(ui, p))
        }
        "light-answer" => {
            let Some(t) = ui.light_test.clone() else { return Some(refresh()) };
            let route = t.routes.get(t.at).cloned().unwrap_or_default();
            let tried = tried_words(ui, &t);
            let note = |saw: &str| format!("Way {} of {}, {} ({route}): {tried}; you saw: {saw}.", t.at + 1, t.routes.len(), lights::route_words(&route));
            match arg {
                "yes" => {
                    let mut l = Lighting::load(&ui.folder());
                    l.route = route.clone();
                    l.save(&ui.folder());
                    if let Some(x) = ui.light_test.as_mut() {
                        x.log.push(note("red"));
                    }
                    end_test(ui, &format!("Works: way {} of {} ({}). Saved.", t.at + 1, t.routes.len(), lights::route_words(&route)));
                    super::note_activity(&ui.folder(), crate::now(), "Light test: found the way that works");
                    Some(with(refresh(), "toast", s("Saved. The lights use this way from now on, and your look comes back in a moment.")))
                }
                "no" | "partly" => {
                    let last = t.at + 1 >= t.routes.len();
                    if let Some(x) = ui.light_test.as_mut() {
                        x.log.push(note(if arg == "no" { "not red" } else { "only partly red" }));
                        x.at += 1;
                    }
                    if last {
                        end_test(ui, &format!("None of the {} ways turned the lights red. The lighting check writes a report to send.", t.routes.len()));
                        return Some(with(refresh(), "toast", s("None of the ways worked. Run the Lighting check below and send its report, so this can be fixed.")));
                    }
                    show_way(ui);
                    Some(with(refresh(), "toast", s("Trying the next way. The lights turn red in a moment.")))
                }
                "stop" => {
                    end_test(ui, "Stopped before a way was found.");
                    Some(with(refresh(), "toast", s("Light test stopped. Your look comes back in a moment.")))
                }
                _ => None,
            }
        }
        "light-test" => {
            let m = ui.machine();
            let l = Lighting::load(&ui.folder());
            let route = route_in_use(&l, &ui.host_lights);
            let leds = lights::driven(&ui.ctx.root, &m.rgb_leds, &route);
            let Some(main) = lights::main_light(&leds).cloned() else {
                return Some(toast("No lights found on this machine."));
            };
            if let Some(name) = l.left_to() {
                return Some(toast(&format!("The lights are left to {name}. Turn that off on the Lighting page to test them here.")));
            }
            if ui.light_test.is_some() {
                return Some(with(refresh(), "toast", s("The light test is running. Answer what you see, or stop it.")));
            }
            let access = modules::lighting_access();
            let has_access = ui.ledger().find_key(&access.key()).is_some();
            let shut = shut_now(ui, &m, &l);
            if has_access && shut.is_none() && ui.agent_running() {
                // The background helper lights the rings, so the test goes the same way the effects do.
                let routes = lights::test_routes(&main);
                if routes.is_empty() {
                    return Some(toast("This system cuts the lights' colors down to blue, and no lighting chip was found to send them to instead. The Lighting check writes a report to send."));
                }
                if !Lighting::saved(&ui.folder()) {
                    l.save(&ui.folder());
                }
                ui.light_test = Some(LightTest { rings: rings(&main) == 2, log: vec![format!("Light test on {}: {} ways to try.", main.name, routes.len())], routes, ..LightTest::default() });
                show_way(ui);
                return Some(with(refresh(), "toast", s("The lights turn red in a moment. Then say what you see.")));
            }
            let writes = light_writes(&ui.ctx.root, &m, &Lighting { route: route.clone(), ..l.clone() });
            let mut p = Plan::new("Set up the lights?", "Turbine Hub's helper lets the background helper set the lights from now on, after restarts and sleep too.", "Set up");
            if !has_access {
                p.items.push((access.name(), if ui.veteran() { access.exact() } else { "Set up once, so the lights never need a password.".into() }));
            } else if let Some(sh) = &shut {
                // Set up before, closed now: the helper opens them again, and Turbine Hub looks again afterwards.
                p = Plan::new("Open the lights again?", "The one-time setup is done, but the lights are closed to Turbine Hub right now. Its helper opens them again, then Turbine Hub checks that they stayed open and says so if they didn't.", "Open them");
                p.items.push(("Closed now".into(), if ui.veteran() { sh.closed.join(", ") } else { "What the background helper writes the colors to.".into() }));
            } else if writes.is_empty() {
                return Some(with(toast("Test lights needs the background helper. Settings, General turns it on."), "open", s("settings")));
            } else {
                p = Plan::new("Show the color on the lights?", "Sets the color and brightness now. Effects play once the background helper runs.", "Show");
            }
            let opening = !has_access || shut.is_some();
            if opening {
                p.steps.push(Step::Host(helper::apply(&access)));
                p.steps.push(Step::LightsCheck { need: lights::needs(&main), exact: ui.veteran() });
            }
            // The color itself, on lights that take it through their files: after the helper opened them.
            if !has_access || shut.is_none() {
                p.steps.extend(writes.into_iter().map(|(path, value)| Step::HostWrite { path, value }));
            }
            if !Lighting::saved(&ui.folder()) {
                l.save(&ui.folder());
            }
            p.done = if !has_access {
                "Lights set up".into()
            } else if shut.is_some() {
                "Lights opened again".into()
            } else {
                "Lights set".into()
            };
            Some(if opening { with_helper(ui, p) } else { dialog(ui, p) })
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

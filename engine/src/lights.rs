//! Lighting effects on the kernel's RGB lights (every `*:rgb:*` under /sys/class/leds, whoever made them). The
//! background helper asks for a frame a few times a second and writes what changed: the color file
//! (`multi_intensity`, one red-green-blue set per part of the light) and `brightness`. Nothing here knows any
//! device; a light with one part simply shows one color.

use crate::ui::machine::Lighting;
use std::collections::BTreeMap;
use std::path::Path;

/// One light as the kernel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Led {
    pub name: String,
    /// How many red-green-blue sets its color file holds.
    pub zones: usize,
    /// The brightest value its brightness file takes.
    pub max: u32,
}

/// The lights among `names` (from `Machine::rgb_leds`) with what each can do.
pub fn leds(root: &Path, names: &[String]) -> Vec<Led> {
    names
        .iter()
        .filter(|n| !n.contains('/') && !n.contains(".."))
        .map(|n| {
            let dir = root.join("sys/class/leds").join(n);
            let text = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_default();
            Led { name: n.clone(), zones: (text("multi_intensity").split_whitespace().count() / 3).max(1), max: text("max_brightness").trim().parse().ok().filter(|v| *v > 0).unwrap_or(255) }
        })
        .collect()
}

/// May anyone write this light's files? True once the helper's lighting access rule is in effect. Asked of the
/// files' own permissions, because inside the app's sandbox /sys is read only whatever they say.
pub fn writable(root: &Path, led: &Led) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let dir = root.join("sys/class/leds").join(&led.name);
    ["multi_intensity", "brightness"].iter().all(|f| std::fs::metadata(dir.join(f)).is_ok_and(|m| m.permissions().mode() & 0o002 != 0))
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
    /// A handheld with an outside screen connected.
    pub docked: bool,
}

impl Scene<'_> {
    fn dark(&self) -> bool {
        self.lighting.brightness == 0 || (self.docked && self.lighting.off_when_docked)
    }
    fn still(&self) -> bool {
        self.dark() || self.lighting.effect == "Static"
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

fn lerp(slow: f64, fast: f64, speed: u32) -> f64 {
    slow + (fast - slow) * speed.min(100) as f64 / 100.0
}

/// A color from the color wheel at full strength. `hue` in degrees.
pub fn hue_rgb(hue: f64) -> (u8, u8, u8) {
    let h = hue.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    ((r * 255.0_f64).round() as u8, (g * 255.0_f64).round() as u8, (b * 255.0_f64).round() as u8)
}

/// The charge as a color: green from half up, orange below, red under a fifth.
pub fn level_rgb(pct: u32) -> (u8, u8, u8) {
    match pct {
        50.. => (40, 220, 80),
        20..50 => (255, 150, 0),
        _ => (255, 30, 20),
    }
}

/// A number from 0 up to 1 that looks random and is the same every time for the same two inputs.
fn noise(a: u64, b: u64) -> f64 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_add(0x632B_E59B_D9B4_E019).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    (x % 10_000) as f64 / 10_000.0
}

fn hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#').filter(|h| h.len() == 6)?;
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some((p(0)?, p(2)?, p(4)?))
}

/// The color the effect starts from for one part of a light: the charge when the lights show it, else the running
/// game's own color, else the rainbow, else the chosen color.
fn base(scene: &Scene, zone: usize, zones: usize, turn: f64) -> (u8, u8, u8) {
    let l = scene.lighting;
    if let (true, Some(pct)) = (l.battery_level, scene.battery) {
        return level_rgb(pct);
    }
    if l.per_game
        && let Some(c) = l.game_colors.get(scene.game).and_then(|c| hex_rgb(c))
    {
        return c;
    }
    if l.rainbow {
        return hue_rgb(360.0 * (zone as f64 / zones as f64 + turn));
    }
    l.rgb()
}

fn scaled(c: (u8, u8, u8), f: f64) -> (u8, u8, u8) {
    let s = |v: u8| (v as f64 * f.clamp(0.0, 1.0)).round() as u8;
    (s(c.0), s(c.1), s(c.2))
}

/// One light's colors (one per part) and its brightness from 0 to 1, `t_ms` milliseconds into the effect.
pub fn look(scene: &Scene, zones: usize, t_ms: u64) -> (Vec<(u8, u8, u8)>, f64) {
    let l = scene.lighting;
    let zones = zones.max(1);
    let full = l.brightness.min(100) as f64 / 100.0;
    if scene.dark() {
        return (vec![(0, 0, 0); zones], 0.0);
    }
    let t = t_ms as f64;
    // How far through its period an effect is, 0 up to 1.
    let phase = |slow_ms: f64, fast_ms: f64| (t / lerp(slow_ms, fast_ms, l.speed)).fract();
    let plain = |turn: f64| (0..zones).map(|z| base(scene, z, zones, turn)).collect::<Vec<_>>();
    match l.effect.as_str() {
        "Breathe" => {
            let p = phase(7000.0, 1800.0);
            (plain(0.0), full * (0.08 + 0.92 * (1.0 - (p * std::f64::consts::TAU).cos()) / 2.0))
        }
        "Heartbeat" => {
            let p = phase(2200.0, 800.0);
            let beat = |start: f64| if (start..start + 0.14).contains(&p) { ((p - start) / 0.14 * std::f64::consts::PI).sin() } else { 0.0 };
            (plain(0.0), full * (0.1 + 0.9 * beat(0.0).max(beat(0.22) * 0.8)))
        }
        "Color cycle" => {
            let p = phase(40_000.0, 5000.0);
            (vec![hue_rgb(360.0 * p); zones], full)
        }
        "Wave" => {
            let p = phase(20_000.0, 3000.0);
            ((0..zones).map(|z| hue_rgb(360.0 * (p + z as f64 / zones as f64))).collect(), full)
        }
        "Spiral" => {
            let p = phase(4000.0, 900.0);
            let colors = plain(phase(60_000.0, 20_000.0));
            let lit = |z: usize| {
                if zones == 1 {
                    return 0.25 + 0.75 * (1.0 - p);
                }
                let d = (z as f64 - p * zones as f64).rem_euclid(zones as f64);
                let d = d.min(zones as f64 - d) / (zones as f64 / 2.0);
                (1.0 - d).powi(2).max(0.08)
            };
            (colors.into_iter().enumerate().map(|(z, c)| scaled(c, lit(z))).collect(), full)
        }
        "Candle" => {
            let step = t / lerp(260.0, 90.0, l.speed);
            let (a, b) = (noise(step as u64, 1), noise(step as u64 + 1, 1));
            (plain(0.0), full * (0.55 + 0.45 * (a + (b - a) * step.fract())))
        }
        "Twinkle" => {
            let step = t / lerp(900.0, 250.0, l.speed);
            let colors = plain(phase(60_000.0, 20_000.0));
            (colors.into_iter().enumerate().map(|(z, c)| scaled(c, if noise(step as u64, z as u64 + 7) < 0.3 { 1.0 - 0.7 * step.fract() } else { 0.3 })).collect(), full)
        }
        _ => (plain(0.0), full),
    }
}

/// The files to write for this moment: (path, value) for every light's color file and brightness.
pub fn frame(scene: &Scene, leds: &[Led], t_ms: u64) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for led in leds {
        let (colors, level) = look(scene, led.zones, t_ms);
        let dir = format!("/sys/class/leds/{}", led.name);
        out.push((format!("{dir}/multi_intensity"), colors.iter().map(|(r, g, b)| format!("{r} {g} {b}")).collect::<Vec<_>>().join(" ")));
        out.push((format!("{dir}/brightness"), ((level.clamp(0.0, 1.0) * led.max as f64).round() as u32).to_string()));
    }
    out
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
    pub fn forget(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::machine::EFFECTS;

    fn scene(l: &Lighting) -> Scene<'_> {
        Scene { lighting: l, game: "", battery: None, on_battery: false, docked: false }
    }

    fn lit(effect: &str) -> Lighting {
        Lighting { effect: effect.into(), brightness: 100, color: "#E0564A".into(), ..Lighting::default() }
    }

    #[test]
    fn every_effect_gives_valid_frames_and_all_but_static_move() {
        let ring = Led { name: "multicolor:rgb:ring".into(), zones: 4, max: 255 };
        let dot = Led { name: "x:rgb:power".into(), zones: 1, max: 100 };
        for effect in EFFECTS {
            let l = lit(effect);
            let sc = scene(&l);
            let mut seen = std::collections::BTreeSet::new();
            for t in (0..20_000).step_by(50) {
                let f = frame(&sc, &[ring.clone(), dot.clone()], t);
                assert_eq!(f.len(), 4);
                assert_eq!(f[0].0, "/sys/class/leds/multicolor:rgb:ring/multi_intensity");
                assert_eq!(f[0].1.split(' ').filter(|v| v.parse::<u8>().is_ok()).count(), 12, "{effect}");
                assert_eq!(f[2].1.split(' ').count(), 3);
                assert!(f[1].1.parse::<u32>().unwrap() <= 255 && f[3].1.parse::<u32>().unwrap() <= 100, "{effect}");
                seen.insert(f);
            }
            assert_eq!(seen.len() > 1, *effect != "Static", "{effect}");
            assert_eq!(frame_ms(&sc), if *effect == "Static" { 1000 } else { 50 });
            assert_eq!(frame_ms(&Scene { on_battery: true, ..sc }), if *effect == "Static" { 1000 } else { 125 });
        }
    }

    #[test]
    fn static_shows_the_color_at_the_chosen_brightness() {
        let l = Lighting { brightness: 60, ..lit("Static") };
        assert_eq!(look(&scene(&l), 2, 0), (vec![(0xE0, 0x56, 0x4A); 2], 0.6));
        let led = Led { name: "a:rgb:b".into(), zones: 1, max: 255 };
        assert_eq!(frame(&scene(&l), &[led], 999), vec![("/sys/class/leds/a:rgb:b/multi_intensity".to_string(), "224 86 74".to_string()), ("/sys/class/leds/a:rgb:b/brightness".to_string(), "153".to_string())]);
        // The rainbow spreads over the parts of a light and ignores the color.
        let l = Lighting { rainbow: true, ..lit("Static") };
        assert_eq!(look(&scene(&l), 3, 5000).0, vec![(255, 0, 0), (0, 255, 0), (0, 0, 255)]);
    }

    #[test]
    fn breathing_stays_within_the_chosen_brightness() {
        let l = Lighting { brightness: 50, speed: 100, ..lit("Breathe") };
        let levels: Vec<f64> = (0..1800).step_by(25).map(|t| look(&scene(&l), 1, t).1).collect();
        let (low, high) = (levels.iter().cloned().fold(1.0, f64::min), levels.iter().cloned().fold(0.0, f64::max));
        assert!(low < 0.06 && high > 0.49 && high < 0.501, "{low} {high}");
        // Slower at a lower speed: a quarter of a second moves it less.
        let slow = Lighting { speed: 0, ..l.clone() };
        assert!((look(&scene(&slow), 1, 250).1 - look(&scene(&slow), 1, 0).1).abs() < (look(&scene(&l), 1, 250).1 - look(&scene(&l), 1, 0).1).abs());
    }

    #[test]
    fn the_wave_runs_around_the_parts_and_the_cycle_keeps_them_together() {
        let wave = look(&scene(&lit("Wave")), 4, 0).0;
        assert_eq!(wave.iter().collect::<std::collections::BTreeSet<_>>().len(), 4);
        let cycle = look(&scene(&lit("Color cycle")), 4, 1234).0;
        assert!(cycle.iter().all(|c| *c == cycle[0]));
        // The spiral's bright spot is on one part at a time.
        let spiral = look(&scene(&lit("Spiral")), 6, 0).0;
        assert_eq!(spiral[0], (0xE0, 0x56, 0x4A));
        assert!(spiral[3].0 < 30);
    }

    #[test]
    fn charge_game_and_dock_come_before_the_chosen_color() {
        let mut l = Lighting { battery_level: true, per_game: true, ..lit("Static") };
        l.game_colors.insert("steam:1145360".into(), "#6E9CC8".into());
        let sc = Scene { lighting: &l, game: "steam:1145360", battery: Some(81), on_battery: true, docked: false };
        assert_eq!(look(&sc, 1, 0).0, vec![level_rgb(81)]);
        assert_eq!((level_rgb(50), level_rgb(49), level_rgb(19)), ((40, 220, 80), (255, 150, 0), (255, 30, 20)));
        // A desktop has no battery: the game's color shows.
        assert_eq!(look(&Scene { battery: None, ..sc }, 1, 0).0, vec![(0x6E, 0x9C, 0xC8)]);
        // Another game has no color of its own yet: the chosen one shows.
        let plain = Lighting { battery_level: false, ..l.clone() };
        assert_eq!(look(&Scene { lighting: &plain, game: "steam:7", ..sc }, 1, 0).0, vec![(0xE0, 0x56, 0x4A)]);
        // Docked with lights off while docked: dark, and nothing to animate.
        let l = Lighting { off_when_docked: true, ..lit("Wave") };
        let docked = Scene { lighting: &l, game: "", battery: Some(50), on_battery: false, docked: true };
        assert_eq!(look(&docked, 2, 500), (vec![(0, 0, 0); 2], 0.0));
        assert_eq!(frame_ms(&docked), 1000);
        assert_eq!(frame_ms(&Scene { docked: false, ..docked }), 50);
    }

    #[test]
    fn only_what_changed_is_written() {
        let l = lit("Breathe");
        let led = [Led { name: "a:rgb:b".into(), zones: 1, max: 255 }];
        let mut shown = Shown::default();
        assert_eq!(shown.changed(frame(&scene(&l), &led, 0)).len(), 2);
        // Breathing changes the brightness only; the color file is left alone.
        let next = shown.changed(frame(&scene(&l), &led, 400));
        assert_eq!(next.len(), 1);
        assert!(next[0].0.ends_with("/brightness"));
        assert!(shown.changed(frame(&scene(&l), &led, 400)).is_empty());
        shown.forget();
        assert_eq!(shown.changed(frame(&scene(&l), &led, 400)).len(), 2);
    }

    #[test]
    fn lights_are_read_from_the_kernel_and_checked_for_access() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("turbinehub-lights-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("sys/class/leds/multicolor:rgb:ring");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("multi_intensity"), "0 0 0 0 0 0\n").unwrap();
        std::fs::write(dir.join("brightness"), "0\n").unwrap();
        std::fs::write(dir.join("max_brightness"), "3\n").unwrap();
        let found = leds(&root, &["multicolor:rgb:ring".into(), "gone:rgb:x".into(), "../../etc".into()]);
        assert_eq!(found, vec![Led { name: "multicolor:rgb:ring".into(), zones: 2, max: 3 }, Led { name: "gone:rgb:x".into(), zones: 1, max: 255 }]);
        let mode = |f: &str, m: u32| std::fs::set_permissions(dir.join(f), std::fs::Permissions::from_mode(m)).unwrap();
        mode("multi_intensity", 0o644);
        mode("brightness", 0o666);
        assert!(!writable(&root, &found[0]));
        mode("multi_intensity", 0o666);
        assert!(writable(&root, &found[0]));
        assert!(!writable(&root, &found[1]));
        let _ = std::fs::remove_dir_all(&root);
    }
}

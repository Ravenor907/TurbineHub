//! The ASUS lighting chip, for the ROG Ally series' joystick rings on kernels whose LED files can't show their
//! colors (see `lights`). Used for a light whose files are capped, or when the guided test (or Ally Hub's saved
//! choice on the same machine) says the chip is the way that works; and only on the interface the chip takes its
//! commands on: picked by its HID usage (0xFF31/0x0080) among ASUS's interfaces, not by being the first node behind
//! the light (`turbinehub_helper::hid`). The commands are the ones Armoury Crate, Handheld Daemon and HueSync send.
//!
//! What Ally Hub learned on real machines, kept here:
//! - Windows Dynamic Lighting overrides custom colors and comes back after a sleep: switched off once per session
//!   (best effort: when the chip refuses that report the colors still go out).
//! - The kernel's own LED driver re-sends its capped (blue) value over the chip's: its light is switched off, once
//!   per session with a second for it to finish, or before every send, as the route says.
//! - "RGB enable" goes before anything else, or after Windows turned the rings off the chip ignores every color.
//! - No one sequence works on every firmware, so there are eight routes (`METHODS`, Ally Hub's `HID_METHODS`) and a
//!   guided test that tries them on the real rings and keeps the one the player saw working.
//! - The chip is slow to take packets: on the routes that can be animated, after the first frame of a session
//!   (which sets and commits every part) a frame is one packet when every part shows the same color, only the parts
//!   that changed otherwise, and nothing when nothing changed. These frames are never saved to the chip's memory.
//!   On the other routes the chip runs one of its own modes (solid, pulse, rainbow, spiral), sent once per change.

use crate::lights::Rgb;
use std::path::Path;
use turbinehub_helper::hid::{self, Role};

const REPORT: u8 = 0x5A;
const RGB_ENABLE: [u8; 5] = [REPORT, 0xD1, 0x09, 0x01, 0x02];
const DYNAMIC_OFF: [u8; 2] = [0x06, 0x01];
/// Zone numbers: all, then left stick left and right half, right stick left and right half.
const ALL: u8 = 0x00;

/// The chip behind one light: its commands interface and its Dynamic Lighting interface ("/dev/hidrawN").
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub lighting: String,
    pub dynamic: Option<String>,
}

struct Node {
    name: String,
    product: u16,
    role: Option<Role>,
    /// The interface's top-level HID usages, as (page, usage).
    apps: Vec<(u16, u16)>,
}

fn nodes(root: &Path) -> Vec<Node> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("sys/class/hidraw")).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| hid::node_name_ok(n)).collect()).unwrap_or_default();
    names.sort_by_key(|n| n.trim_start_matches("hidraw").parse::<u32>().unwrap_or(0));
    names
        .into_iter()
        .filter_map(|name| {
            let dir = root.join("sys/class/hidraw").join(&name).join("device");
            let uevent = std::fs::read_to_string(dir.join("uevent")).unwrap_or_default();
            let (vid, product) = hid::ids(&uevent)?;
            let desc = std::fs::read(dir.join("report_descriptor")).unwrap_or_default();
            (vid == hid::ASUS_VID).then(|| Node { role: hid::role(&uevent, &desc), apps: hid::collections(&desc), name, product })
        })
        .collect()
}

/// The USB product id of the device behind an LED, when it hangs off a HID device.
fn led_product(root: &Path, led: &str) -> Option<u16> {
    let mut p = std::fs::canonicalize(root.join("sys/class/leds").join(led).join("device")).ok()?;
    for _ in 0..4 {
        if let Some(ids) = hid::ids(&std::fs::read_to_string(p.join("uevent")).unwrap_or_default()) {
            return Some(ids.1);
        }
        p = p.parent()?.to_path_buf();
    }
    None
}

/// The lighting chip for the light `led`, if this machine has one. When the light's own device is known, only its
/// interfaces count (a second ASUS device, like a keyboard, is never picked).
pub fn find(root: &Path, led: &str) -> Option<Chip> {
    let mut all = nodes(root);
    if let Some(pid) = led_product(root, led)
        && all.iter().any(|n| n.product == pid)
    {
        all.retain(|n| n.product == pid);
    }
    let dev = |n: &Node| format!("/dev/{}", n.name);
    let lighting = all.iter().find(|n| n.role == Some(Role::Lighting)).map(dev)?;
    Some(Chip { lighting, dynamic: all.iter().find(|n| n.role == Some(Role::Dynamic)).map(dev) })
}

/// A device node's permission bits, or None when it isn't there.
pub fn node_mode(root: &Path, node: &str) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(root.join(node.trim_start_matches('/'))).ok().map(|m| m.permissions().mode() & 0o777)
}

/// May this user write to the chip? True once the helper's lighting access rule opened its commands interface.
/// Only the write bit counts: output reports are written and feature reports go through an ioctl, and both work
/// on a node opened for writing only.
pub fn writable(root: &Path, chip: &Chip) -> bool {
    node_mode(root, &chip.lighting).is_some_and(|m| m & 0o002 != 0)
}

/// Every ASUS HID interface with what it is for, its usages and its mode, for problem reports.
pub fn describe(root: &Path) -> Vec<String> {
    nodes(root)
        .iter()
        .map(|n| {
            let mode = node_mode(root, &format!("/dev/{}", n.name)).map(|m| format!("{m:o}")).unwrap_or_else(|| "missing".into());
            let role = match n.role {
                Some(Role::Lighting) => "lighting commands",
                Some(Role::Dynamic) => "Dynamic Lighting",
                None => "not lighting",
            };
            let apps = n.apps.iter().map(|(p, u)| format!("{p:04x}/{u:04x}")).collect::<Vec<_>>().join(", ");
            format!("hid {} 0b05:{:04x}: {role}, mode {mode}, usages {}", n.name, n.product, if apps.is_empty() { "?" } else { &apps })
        })
        .collect()
}

/// One thing to do towards the chip.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Send this report to this hidraw node: as an output report (a plain write, falling back to a feature report
    /// when the node refuses it, as Ally Hub does) or, with `feature`, as a feature report from the start. A report
    /// that isn't a `must` is tried and forgotten when it fails (Dynamic Lighting off).
    Send { node: String, data: Vec<u8>, feature: bool, must: bool },
    /// Write this value to a light's file (the kernel's own light off).
    File { path: String, value: String },
    /// Wait this many milliseconds.
    Pause(u64),
}

/// When a route switches the kernel's own light off (so its driver has no color of its own to send again).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelOff {
    Never,
    /// On the first send of a session, then `settle_ms` for the driver to finish its own packets.
    Once,
    /// Before every send.
    Always,
}

/// One way of talking to the chip. Ally Hub's `HID_METHODS`, each varying one suspect: the kernel's light left on
/// or off, the "ASUS Tech.Inc." handshake, the parts set one by one or all at once, output or feature reports, the
/// set and apply commands every time or only at first, the pause between packets, the whole sequence sent twice.
#[derive(Debug, PartialEq)]
pub struct Method {
    pub key: &'static str,
    pub name: &'static str,
    pub kernel_off: KernelOff,
    pub settle_ms: u64,
    pub init: bool,
    pub each: bool,
    pub feature: bool,
    /// Set and apply only on the first send or when the mode changes; later colors are bare zone commands. These
    /// routes can be animated frame by frame.
    pub commit_first: bool,
    pub gap_ms: u64,
    pub repeat: u32,
}

const fn method(key: &'static str, name: &'static str, kernel_off: KernelOff, init: bool, each: bool, feature: bool) -> Method {
    Method { key, name, kernel_off, settle_ms: 0, init, each, feature, commit_first: false, gap_ms: 4, repeat: 1 }
}

pub const METHODS: &[Method] = &[
    method("m1", "Chip only, kernel lights off", KernelOff::Always, false, false, false),
    method("m2", "Chip only, zone by zone", KernelOff::Always, false, true, false),
    method("m3", "Full handshake", KernelOff::Always, true, true, false),
    method("m4", "Feature reports", KernelOff::Always, true, true, true),
    method("m5", "Kernel lights left on", KernelOff::Never, true, false, false),
    Method { settle_ms: 1000, commit_first: true, gap_ms: 20, ..method("m6", "HueSync custom colors", KernelOff::Once, false, true, false) },
    Method { settle_ms: 1000, gap_ms: 30, repeat: 2, ..method("m7", "Slow and steady", KernelOff::Once, true, true, false) },
    Method { commit_first: true, gap_ms: 20, ..method("m8", "Zone by zone, system lights untouched", KernelOff::Never, false, true, false) },
];
/// The routes the guided test tries, in Ally Hub's order (m1, m4 and m5 fail outright on the real Ally X).
pub const TEST_METHODS: &[&str] = &["m6", "m7", "m8", "m2", "m3"];
/// Ally Hub's route when its settings say "chip" and name none.
pub const ALLYHUB_DEFAULT: &str = "m3";
/// Turbine Hub's own route until a test or Ally Hub's settings say another: the first one the test tries.
pub const AUTO: &str = "m6";

pub fn method_of(key: &str) -> Option<&'static Method> {
    METHODS.iter().find(|m| m.key == key)
}

/// The route used when nothing names one.
pub fn auto() -> &'static Method {
    &METHODS[5]
}

fn packet(data: &[u8]) -> Vec<u8> {
    let mut p = data.to_vec();
    p.resize(64, 0);
    p
}

/// The chip's own modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Solid = 0,
    Pulse = 1,
    Rainbow = 2,
    Spiral = 3,
}

/// The chip's speed for its own effects, from the effect's speed (1.0 is normal).
fn speed_byte(speed: f64) -> u8 {
    if speed < 0.7 {
        0xE1
    } else if speed < 1.5 {
        0xEB
    } else {
        0xF5
    }
}

/// The chip's four brightness steps, from the slider's 0 to 100.
pub fn level(pct: u32) -> u8 {
    match pct.min(100) * 255 / 100 {
        0 => 0,
        1..85 => 1,
        85..170 => 2,
        _ => 3,
    }
}

/// What the chip is asked to show with one of its own modes.
#[derive(Debug, Clone, PartialEq)]
pub struct Show {
    pub mode: Mode,
    pub color: Rgb,
    pub color2: Rgb,
    /// The effect's speed (1.0 is normal); not used by `Solid`.
    pub speed: f64,
    /// The chip's brightness step, 0 to 3.
    pub level: u8,
    /// Spiral: 1 turns left.
    pub direction: u8,
    /// One color per ring half instead of `color` (solid colors only).
    pub zones: Option<Vec<Rgb>>,
}

impl Show {
    pub fn solid(color: Rgb) -> Show {
        Show { mode: Mode::Solid, color, color2: (0, 0, 0), speed: 1.0, level: 3, direction: 0, zones: None }
    }
}

fn zone(z: u8, mode: Mode, c: Rgb, c2: Rgb, speed: u8, direction: u8) -> Vec<u8> {
    packet(&[REPORT, 0xB3, z, mode as u8, c.0, c.1, c.2, speed, direction, 0, c2.0, c2.1, c2.2])
}

/// The 64-byte packets that make the rings show this: the handshake when `init`, RGB enable, the brightness step,
/// one zone command (or four when `each`, or when every half has its own color), then set and apply. Ally Hub's
/// `hid_packets`, byte for byte.
pub fn packets(show: &Show, init: bool, each: bool) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if init {
        let mut hello = vec![REPORT];
        hello.extend_from_slice(b"ASUS Tech.Inc.");
        out.push(packet(&hello));
    }
    out.push(packet(&RGB_ENABLE));
    out.push(packet(&[REPORT, 0xBA, 0xC5, 0xC4, show.level.min(3)]));
    let speed = if show.mode == Mode::Solid { 0 } else { speed_byte(show.speed) };
    let one = |z: u8, c: Rgb| zone(z, show.mode, c, show.color2, speed, show.direction);
    match &show.zones {
        Some(four) => out.extend(four.iter().take(4).enumerate().map(|(i, c)| one(i as u8 + 1, *c))),
        None if each => out.extend((1..=4).map(|z| one(z, show.color))),
        None => out.push(one(ALL, show.color)),
    }
    out.push(packet(&[REPORT, 0xB5]));
    out.push(packet(&[REPORT, 0xB4]));
    out
}

/// What has been sent to the chip this session (Ally Hub's `_HID_SESSION`).
#[derive(Debug, Default)]
pub struct Session {
    kernel_off: bool,
    dynamic_off: bool,
    /// What the chip was last committed to: "static:255", or one of its own modes with its brightness step.
    mode: Option<String>,
    /// The four colors showing, while frames are streamed.
    last: Option<Vec<Rgb>>,
    /// The chip's own mode that is running, so it is sent once per change and never per frame.
    running: Option<Show>,
}

impl Session {
    /// Start over: the next send does the whole first-time sequence (after a sleep, or when sending failed).
    pub fn reset(&mut self) {
        *self = Session::default();
    }

    /// The kernel's light off and Windows Dynamic Lighting off, as often as the route asks.
    fn start(&mut self, m: &Method, chip: &Chip, led: &str, out: &mut Vec<Out>) {
        let off = match m.kernel_off {
            KernelOff::Never => false,
            KernelOff::Once => !self.kernel_off,
            KernelOff::Always => true,
        };
        if off {
            self.kernel_off = true;
            out.push(Out::File { path: format!("/sys/class/leds/{led}/brightness"), value: "0".into() });
            if m.settle_ms > 0 {
                out.push(Out::Pause(m.settle_ms));
            }
        }
        // Once per session for the gentle routes (HueSync does it once), every time for the others.
        let gentle = m.commit_first || m.kernel_off == KernelOff::Once;
        if !(gentle && self.dynamic_off) {
            self.dynamic_off = true;
            if let Some(d) = &chip.dynamic {
                // Best effort: when the chip refuses this one, the colors still go out.
                out.push(Out::Send { node: d.clone(), data: DYNAMIC_OFF.to_vec(), feature: false, must: false });
            }
        }
    }

    /// A route's commit rule: set, apply and the brightness step are dropped when the mode is committed already.
    fn shape(&mut self, m: &Method, list: Vec<Vec<u8>>, mode: &str) -> Vec<Vec<u8>> {
        if !m.commit_first {
            return list;
        }
        if self.mode.as_deref() == Some(mode) {
            return list.into_iter().filter(|p| !matches!(p[1], 0xB5 | 0xB4 | 0xBA)).collect();
        }
        self.mode = Some(mode.to_string());
        list
    }

    fn send(m: &Method, chip: &Chip, list: &[Vec<u8>], gap: u64, repeat: u32, out: &mut Vec<Out>) {
        for i in 0..repeat.max(1) {
            if i > 0 {
                out.push(Out::Pause(200));
            }
            for p in list {
                out.push(Out::Send { node: chip.lighting.clone(), data: p.clone(), feature: m.feature, must: true });
                out.push(Out::Pause(gap));
            }
        }
    }

    /// One frame of four ring halves (already at their strength), for the routes that can be animated (Ally Hub's
    /// `hid_zone_frame`). Once the colors are committed a frame is one packet when every half shows the same color,
    /// only the halves that changed otherwise, and nothing when nothing changed.
    pub fn zones(&mut self, m: &Method, chip: &Chip, led: &str, colors: &[Rgb]) -> Vec<Out> {
        let four: Vec<Rgb> = (0..4).map(|i| colors.get(i * colors.len().max(1) / 4).copied().unwrap_or((0, 0, 0))).collect();
        let mut out = Vec::new();
        self.start(m, chip, led, &mut out);
        self.running = None;
        let list = match &self.last {
            Some(last) if m.commit_first && self.mode.as_deref() == Some("static:255") => {
                if *last == four {
                    return out;
                }
                if four.iter().all(|c| *c == four[0]) {
                    vec![zone(ALL, Mode::Solid, four[0], (0, 0, 0), 0, 0)]
                } else {
                    four.iter().zip(last).enumerate().filter(|(_, (c, old))| c != old).map(|(i, (c, _))| zone(i as u8 + 1, Mode::Solid, *c, (0, 0, 0), 0, 0)).collect()
                }
            }
            _ => {
                let list = packets(&Show { zones: Some(four.clone()), ..Show::solid(four[0]) }, m.init, true);
                self.shape(m, list, "static:255")
            }
        };
        Self::send(m, chip, &list, 6, 1, &mut out);
        self.last = Some(four);
        out
    }

    /// One of the chip's own modes, sent once per change and never per frame (Ally Hub's `hid_apply_effect`): the
    /// chip then runs it by itself. Nothing while it already runs.
    pub fn show(&mut self, m: &Method, chip: &Chip, led: &str, show: &Show) -> Vec<Out> {
        if self.running.as_ref() == Some(show) {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.start(m, chip, led, &mut out);
        // The chip's spiral is one command for both rings.
        let each = m.each && show.mode != Mode::Spiral;
        let list = packets(show, m.init, each);
        let list = self.shape(m, list, &format!("{:?}:{}", show.mode, show.level));
        Self::send(m, chip, &list, m.gap_ms, m.repeat, &mut out);
        self.running = Some(show.clone());
        self.last = None;
        out
    }

    /// One plain color on every half, the way the guided test lights the rings (Ally Hub's `hid_test_color`).
    pub fn test(&mut self, m: &Method, chip: &Chip, led: &str, color: Rgb) -> Vec<Out> {
        let mut out = Vec::new();
        self.start(m, chip, led, &mut out);
        let list = packets(&Show::solid(color), m.init, m.each);
        let list = self.shape(m, list, "static:255");
        Self::send(m, chip, &list, m.gap_ms, m.repeat, &mut out);
        self.running = None;
        self.last = None;
        out
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An Ally X under `root`: the rings' LED (capped at 255) on its HID device, the config interface with ASUS
    /// lighting, the Dynamic Lighting interface, a gamepad, and an ASUS keyboard with lighting of its own.
    pub(crate) fn ally_x(root: &Path) {
        let w = |p: &str, t: &[u8]| {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        };
        let config: &[u8] = &[0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x81, 0x02, 0xC0, 0x06, 0x31, 0xFF, 0x09, 0x80, 0xA1, 0x01, 0x91, 0x00, 0xC0];
        let lamp: &[u8] = &[0x05, 0x59, 0x09, 0x01, 0xA1, 0x01, 0xC0];
        let pad: &[u8] = &[0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0xC0];
        let dev = "sys/devices/pci0000:00/usb3/3-3/3-3:1.0/0003:0B05:1B4C.0001";
        w(&format!("{dev}/uevent"), b"HID_ID=0003:00000B05:00001B4C\n");
        for (n, pid, desc) in [("hidraw0", "19B6", config), ("hidraw1", "1B4C", pad), ("hidraw2", "1B4C", lamp), ("hidraw3", "1B4C", config)] {
            w(&format!("sys/class/hidraw/{n}/device/uevent"), format!("HID_ID=0003:00000B05:0000{pid}\n").as_bytes());
            w(&format!("sys/class/hidraw/{n}/device/report_descriptor"), desc);
            w(&format!("dev/{n}"), b"");
        }
        let led = "sys/class/leds/ally:rgb:joystick_rings";
        for (f, t) in [("multi_index", "rgb rgb rgb rgb\n"), ("multi_intensity", "255 255 255 255\n"), ("multi_max_intensity", "255 255 255 255\n"), ("brightness", "255\n"), ("max_brightness", "255\n")] {
            w(&format!("{led}/{f}"), t.as_bytes());
        }
        std::os::unix::fs::symlink(root.join(dev), root.join(led).join("device")).unwrap();
    }

    #[test]
    fn the_chip_is_the_lighting_interface_of_the_rings_own_device() {
        let root = std::env::temp_dir().join(format!("turbinehub-hid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        ally_x(&root);
        // hidraw0 has the same lighting collection but is another ASUS device: never picked.
        let chip = find(&root, "ally:rgb:joystick_rings").unwrap();
        assert_eq!(chip, Chip { lighting: "/dev/hidraw3".into(), dynamic: Some("/dev/hidraw2".into()) });
        let led = &crate::lights::leds(&root, &["ally:rgb:joystick_rings".into()])[0];
        assert!(led.capped && led.uses_chip() && led.zones == 4 && led.method.key == AUTO);
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join("dev/hidraw3"), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!writable(&root, &chip) && !crate::lights::writable(&root, led));
        // What is closed is named exactly: the chip's node, not the light's files.
        let closed = crate::lights::closed(&root, led);
        assert_eq!(closed.len(), 1);
        assert!(closed[0].contains("/dev/hidraw3") && closed[0].contains("600"), "{closed:?}");
        // The write bit is all it takes; reading is never needed.
        std::fs::set_permissions(root.join("dev/hidraw3"), std::fs::Permissions::from_mode(0o602)).unwrap();
        assert!(writable(&root, &chip) && crate::lights::writable(&root, led));
        let d = describe(&root);
        assert_eq!(d.len(), 4);
        assert!(d[3].contains("hidraw3 0b05:1b4c: lighting commands, mode 602, usages 0001/0006, ff31/0080"), "{d:?}");
        assert!(d[2].contains("Dynamic Lighting") && d[2].contains("usages 0059/0001"), "{d:?}");
        assert!(crate::lights::diagnostics(&root, "").iter().any(|l| l.contains("driven by lighting chip /dev/hidraw3")));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn sends(out: &[Out]) -> Vec<Vec<u8>> {
        out.iter().filter_map(|o| if let Out::Send { data, .. } = o { Some(data.clone()) } else { None }).collect()
    }

    /// The first 14 bytes of each packet as hex, the way the expected lines below were printed from Ally Hub's
    /// own `hid_packets`.
    fn hexes(list: &[Vec<u8>]) -> String {
        assert!(list.iter().all(|p| p.len() == 64 && p[15..].iter().all(|b| *b == 0)));
        list.iter().map(|p| p[..14].iter().map(|b| format!("{b:02x}")).collect::<String>()).collect::<Vec<_>>().join(" ")
    }

    const ENABLE: &str = "5ad1090102000000000000000000 5abac5c403000000000000000000";
    const HELLO: &str = "5a4153555320546563682e496e63";
    const END: &str = "5ab5000000000000000000000000 5ab4000000000000000000000000";

    #[test]
    fn every_route_sends_ally_hubs_packets_byte_for_byte() {
        // Printed from Ally Hub's hid_packets("solid", color, init=m["init"], each=m["each"]) for each of its
        // HID_METHODS, for red and for (14, 165, 233).
        let all = |c: &str| format!("5ab30000{c}00000000000000");
        let four = |c: &str| (1..=4).map(|z| format!("5ab30{z}00{c}00000000000000")).collect::<Vec<_>>().join(" ");
        for (color, c) in [((255, 0, 0), "ff0000"), ((14, 165, 233), "0ea5e9")] {
            let want = |m: &str| match m {
                "m1" => format!("{ENABLE} {} {END}", all(c)),
                "m2" | "m6" | "m8" => format!("{ENABLE} {} {END}", four(c)),
                "m3" | "m4" | "m7" => format!("{HELLO} {ENABLE} {} {END}", four(c)),
                _ => format!("{HELLO} {ENABLE} {} {END}", all(c)),
            };
            for m in METHODS {
                assert_eq!(hexes(&packets(&Show::solid(color), m.init, m.each)), want(m.key), "{} {color:?}", m.key);
            }
        }
        // The handshake is the report id and "ASUS Tech.Inc.", whole.
        assert_eq!(&packets(&Show::solid((1, 2, 3)), true, false)[0][..16], b"\x5aASUS Tech.Inc.\0");
        // The chip's own modes, from Ally Hub's hid_effect_packets: the spiral turning left and fast, a breathe,
        // a color cycle at brightness 100 of 255, a strobe in two colors at 40, a wave, and a color at brightness 0.
        let own = |show: Show, each: bool| hexes(&packets(&show, false, each));
        let b = |mode: Mode, color: Rgb, color2: Rgb, speed: f64, pct: u32| Show { mode, color, color2, speed, level: level(pct), direction: 0, zones: None };
        assert_eq!(own(Show { direction: 1, ..b(Mode::Spiral, (0, 0, 0), (0, 0, 0), 2.0, 100) }, false), format!("{ENABLE} 5ab30003000000f5010000000000 {END}"));
        let z = |tail: &str| (1..=4).map(|z| format!("5ab30{z}{tail}")).collect::<Vec<_>>().join(" ");
        assert_eq!(own(b(Mode::Pulse, (255, 0, 7), (0, 0, 0), 1.0, 100), true), format!("{ENABLE} {} {END}", z("01ff0007eb000000000000")));
        assert_eq!(own(b(Mode::Rainbow, (255, 255, 255), (0, 0, 0), 0.5, 39), true), format!("5ad1090102000000000000000000 5abac5c402000000000000000000 {} {END}", z("02ffffffe1000000000000")));
        assert_eq!(own(b(Mode::Pulse, (255, 0, 0), (0, 0, 255), 2.0, 16), true), format!("5ad1090102000000000000000000 5abac5c401000000000000000000 {} {END}", z("01ff0000f500000000ff00")));
        assert_eq!(own(b(Mode::Pulse, (4, 197, 39), (1, 184, 147), 2.0, 100), true), format!("{ENABLE} {} {END}", z("0104c527f5000001b89300")));
        assert_eq!(own(b(Mode::Solid, (0, 109, 233), (0, 0, 0), 1.0, 0), true), format!("5ad1090102000000000000000000 5abac5c400000000000000000000 {} {END}", z("00006de900000000000000")));
        // Ally Hub's brightness steps (its 0 to 255 as the slider's 0 to 100).
        assert_eq!([level(0), level(1), level(33), level(34), level(66), level(67), level(100)], [0, 1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn the_table_is_ally_hubs() {
        let row = |k: &str| method_of(k).map(|m| (m.kernel_off, m.settle_ms, m.init, m.each, m.feature, m.commit_first, m.gap_ms, m.repeat)).unwrap();
        assert_eq!(row("m1"), (KernelOff::Always, 0, false, false, false, false, 4, 1));
        assert_eq!(row("m2"), (KernelOff::Always, 0, false, true, false, false, 4, 1));
        assert_eq!(row("m3"), (KernelOff::Always, 0, true, true, false, false, 4, 1));
        assert_eq!(row("m4"), (KernelOff::Always, 0, true, true, true, false, 4, 1));
        assert_eq!(row("m5"), (KernelOff::Never, 0, true, false, false, false, 4, 1));
        assert_eq!(row("m6"), (KernelOff::Once, 1000, false, true, false, true, 20, 1));
        assert_eq!(row("m7"), (KernelOff::Once, 1000, true, true, false, false, 30, 2));
        assert_eq!(row("m8"), (KernelOff::Never, 0, false, true, false, true, 20, 1));
        assert_eq!((METHODS.len(), TEST_METHODS, ALLYHUB_DEFAULT, auto().key), (8, &["m6", "m7", "m8", "m2", "m3"][..], "m3", AUTO));
        assert!(TEST_METHODS.iter().all(|k| method_of(k).is_some()) && method_of("m9").is_none());
    }

    #[test]
    fn a_session_commits_once_then_sends_only_what_changed() {
        let chip = Chip { lighting: "/dev/hidraw3".into(), dynamic: Some("/dev/hidraw2".into()) };
        let m6 = method_of("m6").unwrap();
        let mut s = Session::default();
        let red = vec![(255, 0, 0); 4];
        let first = s.zones(m6, &chip, "ally:rgb:joystick_rings", &red);
        // The kernel's light off, a second to settle, Dynamic Lighting off (best effort), then enable, brightness,
        // four parts, set and apply.
        assert_eq!(first[0], Out::File { path: "/sys/class/leds/ally:rgb:joystick_rings/brightness".into(), value: "0".into() });
        assert_eq!(first[1], Out::Pause(1000));
        assert_eq!(first[2], Out::Send { node: "/dev/hidraw2".into(), data: vec![0x06, 0x01], feature: false, must: false });
        let p = sends(&first);
        assert_eq!(p.len(), 9);
        assert_eq!(hexes(&p[1..]), format!("{ENABLE} 5ab30100ff000000000000000000 5ab30200ff000000000000000000 5ab30300ff000000000000000000 5ab30400ff000000000000000000 {END}"));
        assert!(first[3..].iter().all(|o| !matches!(o, Out::Send { must: false, .. } | Out::Send { feature: true, .. })));
        // The same again: nothing. One color for all: one packet, to zone "all".
        assert!(s.zones(m6, &chip, "x", &red).is_empty());
        let green = s.zones(m6, &chip, "x", &[(0, 9, 0); 4]);
        assert_eq!(sends(&green), vec![zone(0, Mode::Solid, (0, 9, 0), (0, 0, 0), 0, 0)]);
        // Two halves change: two packets.
        let two = s.zones(m6, &chip, "x", &[(0, 9, 0), (1, 1, 1), (0, 9, 0), (2, 2, 2)]);
        assert_eq!(sends(&two), vec![zone(2, Mode::Solid, (1, 1, 1), (0, 0, 0), 0, 0), zone(4, Mode::Solid, (2, 2, 2), (0, 0, 0), 0, 0)]);
        // The chip's spiral is sent once; going back to colors commits again.
        let spiral = Show { mode: Mode::Spiral, color: (0, 0, 0), color2: (0, 0, 0), speed: 1.0, level: 3, direction: 1, zones: None };
        let sp = s.show(m6, &chip, "x", &spiral);
        assert_eq!(&sends(&sp)[2][..9], &[0x5A, 0xB3, 0x00, 0x03, 0, 0, 0, 0xEB, 0x01]);
        assert!(s.show(m6, &chip, "x", &spiral).is_empty());
        assert_eq!(sends(&s.zones(m6, &chip, "x", &red)).len(), 8);
        // After a sleep: the whole sequence again, Dynamic Lighting and the kernel's light included.
        s.reset();
        assert_eq!(sends(&s.zones(m6, &chip, "x", &red)).len(), 9);
    }

    #[test]
    fn each_route_keeps_its_own_habits() {
        let chip = Chip { lighting: "/dev/hidraw3".into(), dynamic: Some("/dev/hidraw2".into()) };
        let red = (255, 0, 0);
        let offs = |o: &[Out]| o.iter().filter(|x| matches!(x, Out::File { .. })).count();
        let dyns = |o: &[Out]| o.iter().filter(|x| matches!(x, Out::Send { must: false, .. })).count();
        let gaps = |o: &[Out]| o.iter().filter_map(|x| if let Out::Pause(ms) = x { Some(*ms) } else { None }).collect::<Vec<_>>();
        // m3 (Ally Hub's default): the kernel's light and Dynamic Lighting off before every color, the handshake,
        // four parts, set and apply every time, 4 ms apart.
        let m3 = method_of("m3").unwrap();
        let mut s = Session::default();
        for _ in 0..2 {
            let o = s.test(m3, &chip, "rings", red);
            assert_eq!((offs(&o), dyns(&o), sends(&o).len()), (1, 1, 10));
            assert!(gaps(&o).iter().all(|g| *g == 4));
        }
        // m7: off once with a second to settle, then the whole sequence twice, 30 ms apart with 0.2 s between.
        let m7 = method_of("m7").unwrap();
        let mut s = Session::default();
        let o = s.test(m7, &chip, "rings", red);
        assert_eq!((offs(&o), dyns(&o), sends(&o).len()), (1, 1, 19));
        assert_eq!(gaps(&o).iter().filter(|g| **g == 200).count(), 1);
        assert_eq!(gaps(&o)[0], 1000);
        let o = s.test(m7, &chip, "rings", (0, 255, 0));
        assert_eq!((offs(&o), dyns(&o), sends(&o).len()), (0, 0, 18));
        // m8: the kernel's light is never touched; set and apply only the first time.
        let m8 = method_of("m8").unwrap();
        let mut s = Session::default();
        let o = s.test(m8, &chip, "rings", red);
        assert_eq!((offs(&o), dyns(&o), sends(&o).len()), (0, 1, 9));
        let o = s.test(m8, &chip, "rings", (0, 255, 0));
        assert_eq!((offs(&o), dyns(&o)), (0, 0));
        assert_eq!(hexes(&sends(&o)), "5ad1090102000000000000000000 5ab3010000ff0000000000000000 5ab3020000ff0000000000000000 5ab3030000ff0000000000000000 5ab3040000ff0000000000000000");
        // m4: every report is a feature report.
        let o = Session::default().test(method_of("m4").unwrap(), &chip, "rings", red);
        assert!(o.iter().all(|x| !matches!(x, Out::Send { must: true, feature: false, .. })));
        // A route that can't be animated runs the chip's own mode, once per change.
        let mut s = Session::default();
        let pulse = Show { mode: Mode::Pulse, color: red, color2: (0, 0, 0), speed: 1.0, level: 3, direction: 0, zones: None };
        assert_eq!(sends(&s.show(m3, &chip, "rings", &pulse)).len(), 10);
        assert!(s.show(m3, &chip, "rings", &pulse).is_empty());
        assert_eq!(sends(&s.show(m3, &chip, "rings", &Show { level: 2, ..pulse.clone() })).len(), 10);
        // No Dynamic Lighting interface: nothing is sent to one.
        let bare = Chip { lighting: "/dev/hidraw3".into(), dynamic: None };
        assert_eq!(dyns(&Session::default().test(m3, &bare, "rings", red)), 0);
    }
}

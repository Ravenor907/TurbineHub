//! The ASUS lighting chip, for the ROG Ally series' joystick rings on kernels whose LED files can't show their
//! colors (see `lights`). Only used for a light whose files are capped, and only on the interface the chip takes its
//! commands on: picked by its HID usage (0xFF31/0x0080) among ASUS's interfaces, not by being the first node behind
//! the light (`turbinehub_helper::hid`). The commands are the ones Armoury Crate, Handheld Daemon and HueSync send.
//!
//! What Ally Hub learned on real machines, kept here:
//! - Windows Dynamic Lighting overrides custom colors and comes back after a sleep: switched off once per session.
//! - The kernel's own LED driver re-sends its capped (blue) value over the chip's: its light is switched off once
//!   per session, with a second for it to finish.
//! - "RGB enable" goes before anything else, or after Windows turned the rings off the chip ignores every color.
//! - The chip is slow to take packets: after the first frame of a session (which sets and commits every part), a
//!   frame is one packet when every part shows the same color, only the parts that changed otherwise, and nothing
//!   when nothing changed. These frames are never saved to the chip's memory.

use crate::lights::Rgb;
use std::path::Path;
use turbinehub_helper::hid::{self, Role};

const REPORT: u8 = 0x5A;
const RGB_ENABLE: [u8; 5] = [REPORT, 0xD1, 0x09, 0x01, 0x02];
const DYNAMIC_OFF: [u8; 2] = [0x06, 0x01];
const SOLID: u8 = 0x00;
const SPIRAL: u8 = 0x03;
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
            (vid == hid::ASUS_VID).then(|| Node { role: hid::role(&uevent, &std::fs::read(dir.join("report_descriptor")).unwrap_or_default()), name, product })
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

/// May this user write to the chip? True once the helper's lighting access rule opened its interfaces.
pub fn writable(root: &Path, chip: &Chip) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(root.join(chip.lighting.trim_start_matches('/'))).is_ok_and(|m| m.permissions().mode() & 0o002 != 0)
}

/// Every ASUS HID interface with what it is for, for problem reports.
pub fn describe(root: &Path) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    nodes(root)
        .iter()
        .map(|n| {
            let mode = std::fs::metadata(root.join("dev").join(&n.name)).map(|m| format!("{:o}", m.permissions().mode() & 0o777)).unwrap_or_else(|_| "missing".into());
            let role = match n.role {
                Some(Role::Lighting) => "lighting commands",
                Some(Role::Dynamic) => "Dynamic Lighting",
                None => "not lighting",
            };
            format!("hid {} 0b05:{:04x}: {role}, mode {mode}", n.name, n.product)
        })
        .collect()
}

/// One thing to do towards the chip.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Write this report to this hidraw node.
    Send { node: String, data: Vec<u8> },
    /// Write this value to a light's file (the kernel's own light off).
    File { path: String, value: String },
    /// Wait this many milliseconds.
    Pause(u64),
}

fn packet(data: &[u8]) -> Vec<u8> {
    let mut p = data.to_vec();
    p.resize(64, 0);
    p
}

fn zone(z: u8, mode: u8, c: Rgb, speed: u8, direction: u8) -> Vec<u8> {
    packet(&[REPORT, 0xB3, z, mode, c.0, c.1, c.2, speed, direction, 0, 0, 0, 0])
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

/// What has been sent to the chip this session.
#[derive(Debug, Default)]
pub struct Session {
    /// The kernel's light was switched off and Dynamic Lighting too.
    started: bool,
    /// What the chip was last committed to: "static", or a spiral with its speed and direction.
    mode: Option<String>,
    /// The four colors showing.
    last: Option<Vec<Rgb>>,
}

impl Session {
    /// Start over: the next frame does the whole first-time sequence (after a sleep, or when sending failed).
    pub fn reset(&mut self) {
        *self = Session::default();
    }

    fn start(&mut self, chip: &Chip, led: &str, out: &mut Vec<Out>) {
        if self.started {
            return;
        }
        self.started = true;
        out.push(Out::File { path: format!("/sys/class/leds/{led}/brightness"), value: "0".into() });
        out.push(Out::Pause(1000));
        if let Some(d) = &chip.dynamic {
            out.push(Out::Send { node: d.clone(), data: DYNAMIC_OFF.to_vec() });
        }
    }

    /// The packets that set the four ring halves to these colors (already at their strength).
    pub fn zones(&mut self, chip: &Chip, led: &str, colors: &[Rgb]) -> Vec<Out> {
        let four: Vec<Rgb> = (0..4).map(|i| colors.get(i * colors.len().max(1) / 4).copied().unwrap_or((0, 0, 0))).collect();
        let mut out = Vec::new();
        self.start(chip, led, &mut out);
        let send = |data: Vec<u8>| Out::Send { node: chip.lighting.clone(), data };
        match &self.last {
            Some(last) if self.mode.as_deref() == Some("static") => {
                if *last == four {
                    return out;
                }
                if four.iter().all(|c| *c == four[0]) {
                    out.push(send(zone(ALL, SOLID, four[0], 0, 0)));
                } else {
                    for (i, (c, old)) in four.iter().zip(last).enumerate() {
                        if c != old {
                            out.push(send(zone(i as u8 + 1, SOLID, *c, 0, 0)));
                            out.push(Out::Pause(6));
                        }
                    }
                }
            }
            _ => {
                let mut list = vec![packet(&RGB_ENABLE), packet(&[REPORT, 0xBA, 0xC5, 0xC4, 3])];
                list.extend(four.iter().enumerate().map(|(i, c)| zone(i as u8 + 1, SOLID, *c, 0, 0)));
                list.extend([packet(&[REPORT, 0xB5]), packet(&[REPORT, 0xB4])]);
                for p in list {
                    out.push(send(p));
                    out.push(Out::Pause(20));
                }
                self.mode = Some("static".into());
            }
        }
        self.last = Some(four);
        out
    }

    /// The packets that start the chip's own rainbow spiral, once; nothing while it already runs.
    pub fn spiral(&mut self, chip: &Chip, led: &str, speed: f64, ccw: bool) -> Vec<Out> {
        let key = format!("spiral:{}:{ccw}", speed_byte(speed));
        if self.mode.as_deref() == Some(key.as_str()) {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.start(chip, led, &mut out);
        let list = [packet(&RGB_ENABLE), packet(&[REPORT, 0xBA, 0xC5, 0xC4, 3]), zone(ALL, SPIRAL, (0, 0, 0), speed_byte(speed), if ccw { 0x01 } else { 0x00 }), packet(&[REPORT, 0xB5]), packet(&[REPORT, 0xB4])];
        for p in list {
            out.push(Out::Send { node: chip.lighting.clone(), data: p });
            out.push(Out::Pause(20));
        }
        self.mode = Some(key);
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
        assert!(led.capped && led.uses_chip() && led.zones == 4);
        assert!(!writable(&root, &chip) && !crate::lights::writable(&root, led));
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join("dev/hidraw3"), std::fs::Permissions::from_mode(0o602)).unwrap();
        assert!(writable(&root, &chip) && crate::lights::writable(&root, led));
        let d = describe(&root);
        assert_eq!(d.len(), 4);
        assert!(d[3].contains("hidraw3 0b05:1b4c: lighting commands, mode 602"), "{d:?}");
        assert!(crate::lights::diagnostics(&root)[0].contains("driven by lighting chip /dev/hidraw3"));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn sends(out: &[Out]) -> Vec<Vec<u8>> {
        out.iter().filter_map(|o| if let Out::Send { data, .. } = o { Some(data.clone()) } else { None }).collect()
    }

    #[test]
    fn a_session_commits_once_then_sends_only_what_changed() {
        let chip = Chip { lighting: "/dev/hidraw3".into(), dynamic: Some("/dev/hidraw2".into()) };
        let mut s = Session::default();
        let red = vec![(255, 0, 0); 4];
        let first = s.zones(&chip, "ally:rgb:joystick_rings", &red);
        // The kernel's light off, a second to settle, Dynamic Lighting off, then enable, brightness, four parts,
        // set and apply.
        assert_eq!(first[0], Out::File { path: "/sys/class/leds/ally:rgb:joystick_rings/brightness".into(), value: "0".into() });
        assert_eq!(first[1], Out::Pause(1000));
        assert_eq!(first[2], Out::Send { node: "/dev/hidraw2".into(), data: vec![0x06, 0x01] });
        let p = sends(&first);
        assert_eq!(p.len(), 9);
        assert!(p[1..].iter().all(|x| x.len() == 64));
        assert_eq!(&p[1][..5], &[0x5A, 0xD1, 0x09, 0x01, 0x02]);
        assert_eq!(&p[3][..7], &[0x5A, 0xB3, 0x01, 0x00, 255, 0, 0]);
        assert_eq!((&p[7][..2], &p[8][..2]), (&[0x5A, 0xB5][..], &[0x5A, 0xB4][..]));
        // The same again: nothing. One color for all: one packet, to zone "all".
        assert!(s.zones(&chip, "x", &red).is_empty());
        let green = s.zones(&chip, "x", &[(0, 9, 0); 4]);
        assert_eq!(sends(&green), vec![zone(0, 0, (0, 9, 0), 0, 0)]);
        // Two halves change: two packets.
        let two = s.zones(&chip, "x", &[(0, 9, 0), (1, 1, 1), (0, 9, 0), (2, 2, 2)]);
        assert_eq!(sends(&two), vec![zone(2, 0, (1, 1, 1), 0, 0), zone(4, 0, (2, 2, 2), 0, 0)]);
        // The chip's spiral is sent once; going back to colors commits again.
        let sp = s.spiral(&chip, "x", 1.0, true);
        assert_eq!(&sends(&sp)[2][..9], &[0x5A, 0xB3, 0x00, 0x03, 0, 0, 0, 0xEB, 0x01]);
        assert!(s.spiral(&chip, "x", 1.1, true).is_empty());
        assert_eq!(sends(&s.zones(&chip, "x", &red)).len(), 8);
        // After a sleep: the whole sequence again, Dynamic Lighting and the kernel's light included.
        s.reset();
        assert_eq!(sends(&s.zones(&chip, "x", &red)).len(), 9);
    }
}

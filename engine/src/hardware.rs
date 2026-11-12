//! What this machine is and has, read once from /sys, /proc and udev's records (all readable without a password).
//! Every module asks `Machine` what applies, so only what the hardware has is shown. All paths are read under a root
//! folder: "/" on the real machine, a fake tree in tests and screenshot runs.

use std::fs;
use std::path::{Path, PathBuf};

/// What kind of machine, from the hardware alone. A TV PC is a desktop whose main screen is a TV (`Machine::on_tv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Handheld,
    Laptop,
    Desktop,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Handheld => "handheld",
            Kind::Laptop => "laptop",
            Kind::Desktop => "desktop",
        }
    }
}

/// The built-in screen, from its own EDID.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    /// Diagonal in inches (0 when the screen doesn't say).
    pub inches: f64,
    /// The refresh range the screen reports, when it reports one.
    pub range_hz: Option<(u32, u32)>,
    pub mode: (u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Amd,
    Intel,
    Nvidia,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gpu {
    /// "card1".
    pub card: String,
    pub vendor: GpuVendor,
    pub pci_id: String,
    pub driver: String,
    /// The driver's power cap file and its limits in microwatts (AMD), when there is one.
    pub power_cap: Option<(String, u64, u64, u64)>,
    /// The card's own power cap (power1_cap_default), when the driver says it.
    pub power_cap_default: Option<u64>,
    /// Drives a connected screen.
    pub drives_screen: bool,
    /// The PCI slot ("0000:03:00.0") and the board maker's ids ("1eae:8811").
    pub slot: String,
    pub subsystem: String,
    /// The version of the card's power firmware (SMU), from the driver's list of the chip's parts. It names the card
    /// family: 13.0.x is RX 7000, 14.0.x RX 9000.
    pub mp1: Option<(u32, u32, u32)>,
    /// The voltage offset and its range from pp_od_clk_voltage, which only exists while overdrive is on.
    pub od: Option<turbinehub_helper::gpu::Od>,
}

impl Gpu {
    /// The highest power cap the driver takes now: above the card's own default it refuses unless overdrive is on.
    pub fn power_max(&self, overdrive: bool) -> Option<u64> {
        let (_, _, max, _) = self.power_cap.as_ref()?;
        Some(match self.power_cap_default {
            Some(d) if !overdrive && d <= *max => d,
            _ => *max,
        })
    }

    /// A Radeon RX 7000 or RX 9000 card on amdgpu: the cards undervolting is offered on.
    pub fn undervolt_family(&self) -> bool {
        self.vendor == GpuVendor::Amd && self.driver == "amdgpu" && turbinehub_helper::gpu::family_ok(self.mp1)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Edid {
    pub maker: String,
    pub name: String,
    /// Width and height in centimetres (0 when the screen doesn't say).
    pub size_cm: (u32, u32),
    /// The refresh range the screen reports (VRR range), when it reports one.
    pub range_hz: Option<(u32, u32)>,
    /// The highest refresh among the timings it lists.
    pub max_hz: u32,
    pub best_mode: (u32, u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    /// "card1-DP-4".
    pub connector: String,
    pub connected: bool,
    /// eDP, LVDS or DSI: the built-in screen.
    pub internal: bool,
    pub hdmi: bool,
    pub edid: Option<Edid>,
}

impl Display {
    /// About 40 inches across or more, or an HDMI screen with a TV-sized panel.
    pub fn looks_like_tv(&self) -> bool {
        let Some(e) = &self.edid else { return false };
        let (w, h) = e.size_cm;
        let diag_in = (((w * w + h * h) as f64).sqrt() / 2.54) as u32;
        // TVs often report 0 x 0 or a token size; 1600 x 900 cm style values also appear. Trust sane sizes only.
        (40..=120).contains(&diag_in) || (self.hdmi && w == 0 && h == 0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Battery {
    pub name: String,
    pub capacity: Option<u32>,
    pub charge_limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveRole {
    /// SteamOS runs from it.
    System,
    /// No partitions or file system: ready to be set up.
    Empty,
    /// Has a Windows (NTFS) partition. Read only to Turbine Hub.
    Windows,
    /// A SteamOS installer stick.
    Installer,
    /// Anything else.
    Data,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Partition {
    pub name: String,
    pub fs: String,
    pub label: String,
    pub part_name: String,
    pub mount: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drive {
    pub name: String,
    pub model: String,
    pub bytes: u64,
    pub removable: bool,
    pub partitions: Vec<Partition>,
    pub role: DriveRole,
}

/// A firmware power limit (TDP), e.g. ppt_pl1_spl.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerLimit {
    pub name: String,
    pub path: String,
    pub min: u64,
    pub max: u64,
    pub current: u64,
}

/// A USB device that may wake the machine.
#[derive(Debug, Clone, PartialEq)]
pub struct WakeDevice {
    pub path: String,
    pub vendor: String,
    pub product: String,
    pub name: String,
    pub enabled: bool,
    /// Wired inside the case (a built-in controller or keyboard): the kernel calls its port fixed.
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Machine {
    pub kind: Kind,
    /// The built-in screen, when there is one.
    pub panel: Option<Panel>,
    /// The maker's own name for it, from the firmware (empty when the firmware has only a code or a placeholder).
    pub name: String,
    pub vendor: String,
    pub product: String,
    pub cpu: String,
    pub memory_gb: u32,
    pub gpus: Vec<Gpu>,
    pub displays: Vec<Display>,
    pub batteries: Vec<Battery>,
    pub drives: Vec<Drive>,
    pub power_limits: Vec<PowerLimit>,
    /// Energy preferences the CPU offers (empty when it has none).
    pub cpu_epp: Vec<String>,
    pub rgb_leds: Vec<String>,
    pub wake_devices: Vec<WakeDevice>,
    /// efibootmgr's number for Windows Boot Manager, when Windows is installed.
    pub windows_boot: Option<String>,
    pub steamos: String,
    /// The amdgpu driver's feature mask, when it is loaded.
    pub amdgpu_mask: Option<u32>,
    /// The running kernel's release.
    pub kernel: String,
}

struct Tree<'a>(&'a Path);

impl Tree<'_> {
    fn p(&self, abs: &str) -> PathBuf {
        self.0.join(abs.trim_start_matches('/'))
    }
    fn text(&self, abs: &str) -> String {
        fs::read_to_string(self.p(abs)).map(|s| s.trim().to_string()).unwrap_or_default()
    }
    fn num(&self, abs: &str) -> Option<u64> {
        self.text(abs).parse().ok()
    }
    fn names(&self, abs: &str) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.p(abs)).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        v.sort();
        v
    }
    fn link_name(&self, abs: &str) -> String {
        fs::read_link(self.p(abs)).ok().and_then(|l| l.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default()
    }
}

/// Parses an EDID block (and its CTA extensions). Returns None for anything that isn't one.
pub fn parse_edid(b: &[u8]) -> Option<Edid> {
    if b.len() < 128 || b[..8] != [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0] {
        return None;
    }
    let id = u16::from_be_bytes([b[8], b[9]]);
    let letter = |n: u16| (b'A' - 1 + (n & 0x1f) as u8) as char;
    let mut e = Edid { maker: [letter(id >> 10), letter(id >> 5), letter(id)].iter().collect(), size_cm: (b[21] as u32, b[22] as u32), ..Edid::default() };
    let mut timing = |d: &[u8]| {
        let clock = u16::from_le_bytes([d[0], d[1]]) as u64 * 10_000;
        let ha = d[2] as u64 | ((d[4] as u64 >> 4) << 8);
        let hb = d[3] as u64 | ((d[4] as u64 & 0xf) << 8);
        let va = d[5] as u64 | ((d[7] as u64 >> 4) << 8);
        let vb = d[6] as u64 | ((d[7] as u64 & 0xf) << 8);
        let total = (ha + hb) * (va + vb);
        if total > 0 {
            e.max_hz = e.max_hz.max(((clock as f64 / total as f64) + 0.5) as u32);
            if (ha * va) as u32 > e.best_mode.0 * e.best_mode.1 {
                e.best_mode = (ha as u32, va as u32);
            }
        }
    };
    for off in [54, 72, 90, 108] {
        let d = &b[off..off + 18];
        if d[0] != 0 || d[1] != 0 {
            timing(d);
        } else if d[3] == 0xfc {
            e.name = String::from_utf8_lossy(&d[5..18]).trim_end_matches(['\n', ' ']).trim().to_string();
        } else if d[3] == 0xfd {
            let min = d[5] as u32 + if d[4] & 1 != 0 { 255 } else { 0 };
            let max = d[6] as u32 + if d[4] & 2 != 0 { 255 } else { 0 };
            e.range_hz = Some((min, max));
        }
    }
    for ext in b[128..].chunks_exact(128).take(b[126] as usize) {
        if ext[0] == 0x02 && ext[2] >= 4 {
            let mut off = ext[2] as usize;
            while off + 18 <= 127 && (ext[off] != 0 || ext[off + 1] != 0) {
                timing(&ext[off..off + 18]);
                off += 18;
            }
        }
    }
    if let Some((_, max)) = e.range_hz {
        e.max_hz = e.max_hz.max(max);
    }
    Some(e)
}

/// A readable name from the firmware's product fields: the version field when it holds a name (some makers put the
/// name there and a code in the product field), else the product field. Part codes ("RC73XA_RC73XA", "8APU1") and
/// placeholders ("Default string", "1.0") are dropped; a one-word name gets the maker in front.
pub fn firmware_name(vendor: &str, product: &str, version: &str) -> String {
    let placeholder = |s: &str| {
        let l = s.to_ascii_lowercase();
        l.is_empty() || l.contains("default") || l.contains("to be filled") || l.contains("not specified") || l.contains("system product") || l.contains("system version") || s.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    };
    let code = |w: &str| w.contains('_') || (w.len() >= 4 && w.bytes().any(|b| b.is_ascii_digit()) && w.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-'));
    let clean = |s: &str| s.split_whitespace().filter(|w| !code(w)).collect::<Vec<_>>().join(" ");
    let pick = [version, product].into_iter().map(clean).find(|c| !placeholder(c) && c.chars().any(|ch| ch.is_alphabetic())).unwrap_or_default();
    if pick.is_empty() || pick.contains(' ') {
        return pick;
    }
    let maker = vendor.split_whitespace().next().unwrap_or("").trim_end_matches([',', '.']);
    if maker.is_empty() || placeholder(maker) || pick.starts_with(maker) { pick } else { format!("{maker} {pick}") }
}

/// The Windows Boot Manager entry in efibootmgr's list, only when the partition it starts from is in this machine
/// (`partitions`: the GPT partition uuids udev knows). Firmware keeps the entry after Windows is wiped; offering it
/// then would restart the machine into nothing.
pub fn windows_boot_entry(efibootmgr: &str, partitions: &[String]) -> Option<String> {
    turbinehub_helper::ops::windows_entries(efibootmgr).into_iter().find(|(_, u)| partitions.iter().any(|p| p == u)).map(|(n, _)| n)
}

/// The GPT partition uuids udev knows, in lower case.
fn partition_uuids(t: &Tree) -> Vec<String> {
    t.names("/run/udev/data")
        .iter()
        .filter(|n| n.starts_with('b'))
        .filter_map(|n| udev_props(t, &n[1..]).into_iter().find(|(k, _)| k == "ID_PART_ENTRY_UUID").map(|(_, v)| v.to_ascii_lowercase()))
        .collect()
}

fn udev_props(t: &Tree, dev: &str) -> Vec<(String, String)> {
    t.text(&format!("/run/udev/data/b{dev}"))
        .lines()
        .filter_map(|l| l.strip_prefix("E:")?.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

fn prop<'a>(p: &'a [(String, String)], k: &str) -> &'a str {
    p.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).unwrap_or("")
}

fn read_drives(t: &Tree, home_dev: &str) -> Vec<Drive> {
    let mounts: Vec<(String, String)> = t.text("/proc/mounts").lines().filter_map(|l| {
        let mut f = l.split_whitespace();
        Some((f.next()?.to_string(), f.next()?.replace("\\040", " ")))
    }).collect();
    let mut drives = Vec::new();
    for name in t.names("/sys/block") {
        if ["loop", "zram", "ram", "dm-", "sr", "md"].iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let base = format!("/sys/block/{name}");
        let bytes = t.num(&format!("{base}/size")).unwrap_or(0) * 512;
        if bytes == 0 {
            continue;
        }
        let mut partitions = Vec::new();
        for part in t.names(&base).into_iter().filter(|p| p.starts_with(&name)) {
            let props = udev_props(t, &t.text(&format!("{base}/{part}/dev")));
            partitions.push(Partition {
                mount: mounts.iter().find(|(d, _)| *d == format!("/dev/{part}")).map(|(_, m)| m.clone()),
                fs: prop(&props, "ID_FS_TYPE").into(),
                label: prop(&props, "ID_FS_LABEL").into(),
                part_name: prop(&props, "ID_PART_ENTRY_NAME").into(),
                name: part,
            });
        }
        let whole = udev_props(t, &t.text(&format!("{base}/dev")));
        let removable = t.text(&format!("{base}/removable")) == "1";
        let mounted_at = |m: &str| partitions.iter().any(|p| p.mount.as_deref() == Some(m));
        let holds_home = !home_dev.is_empty() && t.names(&base).iter().any(|p| t.text(&format!("{base}/{p}/dev")) == home_dev);
        let role = if holds_home || mounted_at("/") || mounted_at("/home") {
            DriveRole::System
        } else if partitions.iter().any(|p| p.fs == "ntfs" || p.fs == "ntfs3" || p.fs == "BitLocker") {
            DriveRole::Windows
        } else if partitions.is_empty() && prop(&whole, "ID_FS_TYPE").is_empty() {
            DriveRole::Empty
        } else if partitions.iter().any(|p| p.part_name.starts_with("rootfs-")) {
            DriveRole::Installer
        } else {
            DriveRole::Data
        };
        drives.push(Drive { model: t.text(&format!("{base}/device/model")), name, bytes, removable, partitions, role });
    }
    drives
}

impl Machine {
    /// Reads the machine under `root`. `efibootmgr` is that program's output (it needs no password to list), and
    /// `home_dev` the "major:minor" device of the home folder, which finds the system drive even inside a sandbox whose
    /// own mount list doesn't show the host's.
    pub fn read(root: &Path, efibootmgr: &str, home_dev: &str) -> Machine {
        let t = Tree(root);
        let dmi = |f: &str| t.text(&format!("/sys/class/dmi/id/{f}"));
        let (vendor, product, version) = (dmi("sys_vendor"), dmi("product_name"), dmi("product_version"));

        let mut gpus = Vec::new();
        let mut displays = Vec::new();
        for name in t.names("/sys/class/drm") {
            if let Some(card) = name.strip_prefix("card").filter(|c| c.bytes().all(|b| b.is_ascii_digit())) {
                let dev = format!("/sys/class/drm/card{card}/device");
                let vendor_id = t.text(&format!("{dev}/vendor"));
                let vendor = match vendor_id.as_str() {
                    "0x1002" => GpuVendor::Amd,
                    "0x8086" => GpuVendor::Intel,
                    "0x10de" => GpuVendor::Nvidia,
                    _ => GpuVendor::Other,
                };
                let power_cap = t.names(&format!("{dev}/hwmon")).into_iter().find_map(|h| {
                    let hw = format!("{dev}/hwmon/{h}");
                    Some((format!("/sys/class/drm/card{card}/device/hwmon/{h}/power1_cap"), t.num(&format!("{hw}/power1_cap_min"))?, t.num(&format!("{hw}/power1_cap_max"))?, t.num(&format!("{hw}/power1_cap"))?))
                });
                let power_cap_default = t.names(&format!("{dev}/hwmon")).into_iter().find_map(|h| t.num(&format!("{dev}/hwmon/{h}/power1_cap_default")));
                let hex = |f: &str| t.text(&format!("{dev}/{f}")).trim_start_matches("0x").to_ascii_lowercase();
                let ip = |f: &str| t.num(&format!("{dev}/ip_discovery/die/0/MP1/0/{f}")).map(|v| v as u32);
                let mp1 = ip("major").zip(ip("minor")).zip(ip("revision")).map(|((a, b), c)| (a, b, c));
                let od = fs::read_to_string(t.p(&format!("{dev}/pp_od_clk_voltage"))).ok().map(|text| turbinehub_helper::gpu::parse_od(&text));
                gpus.push(Gpu {
                    card: format!("card{card}"),
                    vendor,
                    pci_id: format!("{}:{}", vendor_id.trim_start_matches("0x"), t.text(&format!("{dev}/device")).trim_start_matches("0x")),
                    driver: t.link_name(&format!("{dev}/driver")),
                    power_cap,
                    power_cap_default,
                    drives_screen: false,
                    slot: fs::canonicalize(t.p(&dev)).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).filter(|s| turbinehub_helper::gpu::slot_ok(s)).unwrap_or_default(),
                    subsystem: format!("{}:{}", hex("subsystem_vendor"), hex("subsystem_device")),
                    mp1,
                    od,
                });
            } else if name.starts_with("card") && name.contains('-') && !name.contains("Writeback") {
                let base = format!("/sys/class/drm/{name}");
                let kind = name.split_once('-').map(|(_, k)| k).unwrap_or("");
                let edid = fs::read(t.p(&format!("{base}/edid"))).ok().and_then(|b| parse_edid(&b));
                displays.push(Display {
                    connected: t.text(&format!("{base}/status")) == "connected",
                    internal: ["eDP", "LVDS", "DSI"].iter().any(|k| kind.starts_with(k)),
                    hdmi: kind.starts_with("HDMI"),
                    edid,
                    connector: name,
                });
            }
        }
        for g in &mut gpus {
            g.drives_screen = displays.iter().any(|d| d.connected && d.connector.starts_with(&format!("{}-", g.card)));
        }

        let batteries: Vec<Battery> = t
            .names("/sys/class/power_supply")
            .into_iter()
            .filter(|n| t.text(&format!("/sys/class/power_supply/{n}/type")) == "Battery" && t.text(&format!("/sys/class/power_supply/{n}/scope")) != "Device")
            .map(|n| Battery {
                capacity: t.num(&format!("/sys/class/power_supply/{n}/capacity")).map(|v| v as u32),
                charge_limit: t.num(&format!("/sys/class/power_supply/{n}/charge_control_end_threshold")).map(|v| v as u32),
                name: n,
            })
            .collect();

        let mut power_limits = Vec::new();
        for fw in t.names("/sys/class/firmware-attributes") {
            for a in ["ppt_pl1_spl", "ppt_pl2_sppt", "ppt_pl3_fppt"] {
                let d = format!("/sys/class/firmware-attributes/{fw}/attributes/{a}");
                if let (Some(min), Some(max), Some(current)) = (t.num(&format!("{d}/min_value")), t.num(&format!("{d}/max_value")), t.num(&format!("{d}/current_value"))) {
                    power_limits.push(PowerLimit { name: a.into(), path: format!("{d}/current_value"), min, max, current });
                }
            }
        }

        let has_internal = displays.iter().any(|d| d.internal);
        let panel = displays.iter().find(|d| d.internal).and_then(|d| d.edid.as_ref()).map(|e| Panel {
            inches: (((e.size_cm.0 * e.size_cm.0 + e.size_cm.1 * e.size_cm.1) as f64).sqrt() / 2.54 * 10.0).round() / 10.0,
            range_hz: e.range_hz,
            mode: e.best_mode,
        });
        let chassis: u32 = dmi("chassis_type").parse().unwrap_or(0);
        // A handheld, whoever makes it: a battery and a built-in screen, in a tablet-style case (DMI chassis 30 to
        // 32) or with a screen of 10 inches or less.
        let small_screen = panel.is_some_and(|p| p.inches > 0.0 && p.inches <= 10.0);
        let kind = if !batteries.is_empty() && has_internal && (matches!(chassis, 30..=32) || small_screen) {
            Kind::Handheld
        } else if !batteries.is_empty() && has_internal {
            Kind::Laptop
        } else {
            Kind::Desktop
        };

        let wake_devices = t
            .names("/sys/bus/usb/devices")
            .into_iter()
            // Interfaces, root hubs and hubs (class 09) aren't devices a player would block.
            .filter(|n| !n.contains(':') && !n.starts_with("usb") && t.text(&format!("/sys/bus/usb/devices/{n}/bDeviceClass")) != "09")
            .filter_map(|n| {
                let d = format!("/sys/bus/usb/devices/{n}");
                let state = t.text(&format!("{d}/power/wakeup"));
                (!state.is_empty()).then(|| WakeDevice {
                    path: format!("{d}/power/wakeup"),
                    vendor: t.text(&format!("{d}/idVendor")),
                    product: t.text(&format!("{d}/idProduct")),
                    name: t.text(&format!("{d}/product")),
                    enabled: state == "enabled",
                    built_in: t.text(&format!("{d}/removable")) == "fixed",
                })
            })
            .collect();

        let cpu = t.text("/proc/cpuinfo").lines().find_map(|l| l.strip_prefix("model name").map(|r| r.trim_start_matches([' ', '\t', ':']).to_string())).unwrap_or_default();
        let memory_gb = t.text("/proc/meminfo").lines().find_map(|l| l.strip_prefix("MemTotal:")?.split_whitespace().next()?.parse::<u64>().ok()).map(|kb| ((kb as f64) / 1_048_576.0).round() as u32).unwrap_or(0);
        // Inside the Flatpak (and a dev container) /etc is the sandbox's; the host's copy is under /run/host.
        let os = ["/run/host/os-release", "/run/host/etc/os-release", "/etc/os-release"].iter().map(|p| t.text(p)).find(|s| !s.is_empty()).unwrap_or_default();
        let steamos = os.lines().find_map(|l| l.strip_prefix("VERSION_ID=")).unwrap_or("").trim_matches('"').to_string();

        Machine {
            kind,
            panel,
            name: firmware_name(&vendor, &product, &version),
            vendor,
            product,
            cpu,
            memory_gb,
            gpus,
            displays,
            batteries,
            drives: read_drives(&t, home_dev),
            power_limits,
            cpu_epp: t.text("/sys/devices/system/cpu/cpufreq/policy0/energy_performance_available_preferences").split_whitespace().map(String::from).collect(),
            // Every multicolor light: an LED with a color file, whatever it is called (not only "*:rgb:*").
            rgb_leds: t.names("/sys/class/leds").into_iter().filter(|n| n.contains(":rgb:") || t.p(&format!("/sys/class/leds/{n}/multi_intensity")).exists()).collect(),
            wake_devices,
            windows_boot: windows_boot_entry(efibootmgr, &partition_uuids(&t)),
            steamos,
            amdgpu_mask: turbinehub_helper::gpu::parse_mask(&t.text(turbinehub_helper::gpu::MASK_FILE)),
            kernel: t.text(turbinehub_helper::gpu::OSRELEASE),
        }
    }

    /// The driver's overdrive is on now (the restart after turning it on happened).
    pub fn overdrive_on(&self) -> bool {
        self.amdgpu_mask.is_some_and(|m| m & turbinehub_helper::gpu::OVERDRIVE_BIT != 0)
    }

    /// The name to show: the firmware's name for handhelds and laptops (desktop boards only carry codes), else a
    /// plain description.
    pub fn name(&self) -> String {
        match self.kind {
            Kind::Handheld | Kind::Laptop if !self.name.is_empty() => self.name.clone(),
            Kind::Handheld => "Handheld".into(),
            Kind::Laptop => "Laptop".into(),
            Kind::Desktop if self.on_tv() => "Living room PC".into(),
            Kind::Desktop => "Desktop PC".into(),
        }
    }

    pub fn connected_displays(&self) -> impl Iterator<Item = &Display> {
        self.displays.iter().filter(|d| d.connected)
    }

    /// A desktop whose only connected screens look like TVs.
    pub fn on_tv(&self) -> bool {
        self.kind == Kind::Desktop && self.connected_displays().count() > 0 && self.connected_displays().all(Display::looks_like_tv)
    }

    /// The graphics card that drives the screens (the discrete one when there are two).
    pub fn main_gpu(&self) -> Option<&Gpu> {
        self.gpus.iter().find(|g| g.drives_screen).or_else(|| self.gpus.iter().find(|g| g.vendor != GpuVendor::Intel)).or(self.gpus.first())
    }

    pub fn has_nvidia(&self) -> bool {
        self.gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia)
    }

    pub fn battery(&self) -> Option<&Battery> {
        self.batteries.first()
    }
}

/// The "major:minor" device holding `path`, as the kernel numbers it.
pub fn device_of(path: &Path) -> String {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).map(|m| {
        let d = m.dev();
        format!("{}:{}", ((d >> 32) & 0xffff_f000) | ((d >> 8) & 0xfff), (d & 0xff) | ((d >> 12) & 0xffff_ff00))
    }).unwrap_or_default()
}

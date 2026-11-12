//! What the helper may change, and nothing else. Every change is one typed `Setting`, checked against fixed lists
//! here before anything is touched. Config files under /etc are never taken from the caller: the helper writes them
//! itself from these settings (see `render`), because a hand-written sysctl, udev, modprobe or systemd file can run
//! any command as root.

use crate::json::{Json, quote};

/// One change the helper can make.
#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    /// A kernel setting from `SYSCTLS`, kept in /etc/sysctl.d and applied right away.
    Sysctl { key: String, value: u64 },
    /// A runtime value under /sys matching `SYSFS`. /sys forgets at reboot, so the boot unit puts it back.
    Sysfs { path: String, value: String },
    /// A udev rule that stops (or allows) one USB device waking the machine.
    Wake { vendor: String, product: String, wake: bool },
    /// A fixed udev rule that lets the signed-in player use a device without a password.
    Access { what: Access },
    /// zram swap size and compression, through zram-generator. Takes effect at the next boot.
    Zram { percent: u32, algorithm: String },
    /// A kernel module loaded at every boot.
    Module { name: String },
    /// A system service from `SERVICES` turned on or off.
    Service { unit: String, enabled: bool },
}

/// The fixed device-access rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// The device's RGB lights (the kernel's LED files), so the background agent can animate them many times a
    /// second.
    Lighting,
    /// /dev/ntsync for Proton's NTSync.
    Ntsync,
}

impl Access {
    pub fn key(self) -> &'static str {
        match self {
            Access::Lighting => "lighting",
            Access::Ntsync => "ntsync",
        }
    }

    fn from_key(k: &str) -> Option<Access> {
        match k {
            "lighting" => Some(Access::Lighting),
            "ntsync" => Some(Access::Ntsync),
            _ => None,
        }
    }

    /// The rule text. Fixed: nothing in it comes from the caller.
    pub fn rule(self) -> &'static str {
        match self {
            Access::Lighting => "SUBSYSTEM==\"leds\", KERNEL==\"*:rgb:*\", RUN+=\"/usr/bin/chmod 0666 /sys%p/brightness /sys%p/multi_intensity\"\n",
            Access::Ntsync => "KERNEL==\"ntsync\", MODE=\"0666\"\n",
        }
    }
}

/// Kernel settings the tune-up may change, with the values allowed.
pub const SYSCTLS: &[(&str, u64, u64)] = &[
    ("vm.swappiness", 0, 200),
    ("vm.page-cluster", 0, 3),
    ("vm.vfs_cache_pressure", 1, 500),
    ("vm.dirty_bytes", 0, 4 << 30),
    ("vm.dirty_background_bytes", 0, 4 << 30),
    ("vm.max_map_count", 65_530, 2_147_483_642),
    ("vm.watermark_boost_factor", 0, 15_000),
    ("vm.compaction_proactiveness", 0, 100),
    ("kernel.split_lock_mitigate", 0, 1),
    ("kernel.nmi_watchdog", 0, 1),
];

/// How a /sys value is checked.
#[derive(Debug, Clone, Copy)]
pub enum Values {
    /// A whole number in this range.
    Int(u64, u64),
    /// One of these words.
    Words(&'static [&'static str]),
    /// A whole number between the values in two files next to it (the kernel's own limits).
    IntBetweenSiblings(&'static str, &'static str),
}

/// /sys values the helper may write. A `*` matches within one path part, never across a `/`.
pub const SYSFS: &[(&str, Values)] = &[
    ("/sys/class/power_supply/*/charge_control_end_threshold", Values::Int(20, 100)),
    ("/sys/class/power_supply/*/charge_control_start_threshold", Values::Int(0, 99)),
    ("/sys/devices/system/cpu/cpufreq/policy*/energy_performance_preference", Values::Words(&["default", "performance", "balance_performance", "balance_power", "power"])),
    ("/sys/devices/system/cpu/cpufreq/policy*/scaling_governor", Values::Words(&["performance", "powersave", "schedutil", "ondemand"])),
    ("/sys/devices/system/cpu/cpufreq/boost", Values::Int(0, 1)),
    ("/sys/kernel/mm/transparent_hugepage/enabled", Values::Words(&["always", "madvise", "never"])),
    ("/sys/kernel/mm/transparent_hugepage/defrag", Values::Words(&["always", "defer", "defer+madvise", "madvise", "never"])),
    ("/sys/kernel/mm/transparent_hugepage/khugepaged/defrag", Values::Int(0, 1)),
    ("/sys/bus/usb/devices/*/power/wakeup", Values::Words(&["enabled", "disabled"])),
    ("/sys/bus/pci/devices/*/power/wakeup", Values::Words(&["enabled", "disabled"])),
    ("/sys/class/drm/card*/device/power_dpm_force_performance_level", Values::Words(&["auto", "low", "high"])),
    ("/sys/class/drm/card*/device/hwmon/hwmon*/power1_cap", Values::IntBetweenSiblings("power1_cap_min", "power1_cap_max")),
    ("/sys/class/firmware-attributes/*/attributes/ppt_pl1_spl/current_value", Values::IntBetweenSiblings("min_value", "max_value")),
    ("/sys/class/firmware-attributes/*/attributes/ppt_pl2_sppt/current_value", Values::IntBetweenSiblings("min_value", "max_value")),
    ("/sys/class/firmware-attributes/*/attributes/ppt_pl3_fppt/current_value", Values::IntBetweenSiblings("min_value", "max_value")),
];

/// Services that may be turned on or off.
pub const SERVICES: &[&str] = &["sshd.service", "plugin_loader.service", "tailscaled.service"];

/// zram compression the helper may pick.
pub const ZRAM_ALGORITHMS: &[&str] = &["zstd", "lz4", "lzo-rle"];

/// Kernel modules that may be loaded at boot.
pub const MODULES: &[&str] = &["ntsync"];

/// Does `path` match `pattern` part by part (`*` within a part)?
pub fn path_matches(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    p.len() == s.len() && p.iter().zip(&s).all(|(p, s)| part_matches(p, s))
}

fn part_matches(pattern: &str, part: &str) -> bool {
    if part.is_empty() && !pattern.is_empty() || part == "." || part == ".." {
        return false;
    }
    match pattern.split_once('*') {
        None => pattern == part,
        Some((pre, post)) => part.len() >= pre.len() + post.len() && part.starts_with(pre) && part.ends_with(post),
    }
}

/// The rule for this /sys path, if it is one the helper may write.
pub fn sysfs_rule(path: &str) -> Option<Values> {
    if !path.starts_with("/sys/") || path.contains("//") || path.ends_with('/') {
        return None;
    }
    SYSFS.iter().find(|(p, _)| path_matches(p, path)).map(|(_, v)| *v)
}

fn hex4(s: &str) -> bool {
    s.len() == 4 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn small_word(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(&b))
}

impl Setting {
    /// What it changes, as one string. A new setting with the same key replaces the old one (keeping the first
    /// "before"), so changing a value twice is still one undo.
    pub fn key(&self) -> String {
        match self {
            Setting::Sysctl { key, .. } => format!("sysctl:{key}"),
            Setting::Sysfs { path, .. } => format!("sysfs:{path}"),
            Setting::Wake { vendor, product, .. } => format!("wake:{vendor}:{product}"),
            Setting::Access { what } => format!("access:{}", what.key()),
            Setting::Zram { .. } => "zram".into(),
            Setting::Module { name } => format!("module:{name}"),
            Setting::Service { unit, .. } => format!("service:{unit}"),
        }
    }

    /// Checks everything that can be checked without the system: names on the lists, values in range. /sys values
    /// whose limits live next to them are checked again by `ops` against those files.
    pub fn check(&self) -> Result<(), String> {
        match self {
            Setting::Sysctl { key, value } => match SYSCTLS.iter().find(|(k, ..)| k == key) {
                None => Err(format!("{key} isn't a setting Turbine Hub changes")),
                Some((_, lo, hi)) if value < lo || value > hi => Err(format!("{key} must be between {lo} and {hi}")),
                Some(_) => Ok(()),
            },
            Setting::Sysfs { path, value } => match sysfs_rule(path) {
                None => Err(format!("{path} isn't a value Turbine Hub changes")),
                Some(Values::Int(lo, hi)) => match value.parse::<u64>() {
                    Ok(v) if v >= lo && v <= hi => Ok(()),
                    _ => Err(format!("{path} takes a number from {lo} to {hi}")),
                },
                Some(Values::Words(w)) if w.contains(&value.as_str()) => Ok(()),
                Some(Values::Words(w)) => Err(format!("{path} takes one of: {}", w.join(", "))),
                Some(Values::IntBetweenSiblings(..)) if value.parse::<u64>().is_ok() => Ok(()),
                Some(Values::IntBetweenSiblings(..)) => Err(format!("{path} takes a whole number")),
            },
            Setting::Wake { vendor, product, .. } if hex4(vendor) && hex4(product) => Ok(()),
            Setting::Wake { .. } => Err("A USB id is four lower-case hex digits".into()),
            Setting::Access { .. } => Ok(()),
            Setting::Zram { percent, algorithm } => {
                if !(25..=200).contains(percent) {
                    Err("zram size is 25 to 200% of memory".into())
                } else if !ZRAM_ALGORITHMS.contains(&algorithm.as_str()) {
                    Err(format!("zram compression is one of: {}", ZRAM_ALGORITHMS.join(", ")))
                } else {
                    Ok(())
                }
            }
            Setting::Module { name } if MODULES.contains(&name.as_str()) => Ok(()),
            Setting::Module { name } => Err(format!("{name} isn't a module Turbine Hub loads")),
            Setting::Service { unit, .. } if SERVICES.contains(&unit.as_str()) => Ok(()),
            Setting::Service { unit, .. } => Err(format!("{unit} isn't a service Turbine Hub changes")),
        }
    }

    /// Reads a setting from the caller's JSON and checks it.
    pub fn from_json(j: &Json) -> Result<Setting, String> {
        let s = |k: &str| j.str_at(k).map(str::to_string).ok_or_else(|| format!("{k} is missing"));
        let n = |k: &str| j.get(k).and_then(Json::as_f64).filter(|v| v.fract() == 0.0 && *v >= 0.0 && *v < 1.8e19).map(|v| v as u64).ok_or_else(|| format!("{k} must be a whole number"));
        let b = |k: &str| j.get(k).and_then(Json::as_bool).ok_or_else(|| format!("{k} must be true or false"));
        let setting = match j.str_at("kind").unwrap_or("") {
            "sysctl" => Setting::Sysctl { key: s("key")?, value: n("value")? },
            "sysfs" => Setting::Sysfs { path: s("path")?, value: s("value")? },
            "wake" => Setting::Wake { vendor: s("vendor")?, product: s("product")?, wake: b("wake")? },
            "access" => Setting::Access { what: Access::from_key(&s("what")?).ok_or("Unknown access rule")? },
            "zram" => Setting::Zram { percent: u32::try_from(n("percent")?).map_err(|_| "percent is too big")?, algorithm: s("algorithm")? },
            "module" => Setting::Module { name: s("name")? },
            "service" => Setting::Service { unit: s("unit")?, enabled: b("enabled")? },
            other => return Err(format!("Unknown setting kind {:?}", other.chars().take(32).collect::<String>())),
        };
        if let Setting::Sysfs { value, .. } | Setting::Zram { algorithm: value, .. } = &setting
            && !small_word(value)
        {
            return Err("That value isn't allowed".into());
        }
        setting.check()?;
        Ok(setting)
    }

    pub fn to_json(&self) -> String {
        match self {
            Setting::Sysctl { key, value } => format!("{{\"kind\":\"sysctl\",\"key\":{},\"value\":{value}}}", quote(key)),
            Setting::Sysfs { path, value } => format!("{{\"kind\":\"sysfs\",\"path\":{},\"value\":{}}}", quote(path), quote(value)),
            Setting::Wake { vendor, product, wake } => format!("{{\"kind\":\"wake\",\"vendor\":{},\"product\":{},\"wake\":{wake}}}", quote(vendor), quote(product)),
            Setting::Access { what } => format!("{{\"kind\":\"access\",\"what\":{}}}", quote(what.key())),
            Setting::Zram { percent, algorithm } => format!("{{\"kind\":\"zram\",\"percent\":{percent},\"algorithm\":{}}}", quote(algorithm)),
            Setting::Module { name } => format!("{{\"kind\":\"module\",\"name\":{}}}", quote(name)),
            Setting::Service { unit, enabled } => format!("{{\"kind\":\"service\",\"unit\":{},\"enabled\":{enabled}}}", quote(unit)),
        }
    }

    /// The file under /etc this setting lives in, if any (/sys values and services have none).
    pub fn file(&self) -> Option<&'static str> {
        match self {
            Setting::Sysctl { .. } => Some(SYSCTL_FILE),
            Setting::Wake { .. } => Some(WAKE_FILE),
            Setting::Access { .. } => Some(ACCESS_FILE),
            Setting::Zram { .. } => Some(ZRAM_FILE),
            Setting::Module { .. } => Some(MODULES_FILE),
            Setting::Sysfs { .. } | Setting::Service { .. } => None,
        }
    }

    /// What it changes, in a player's words ("Battery charge limit"), never a file or key name.
    pub fn name(&self) -> String {
        match self {
            Setting::Sysctl { key, .. } => match key.as_str() {
                "vm.swappiness" => "Compressed memory use",
                "vm.page-cluster" => "Compressed memory read size",
                "vm.vfs_cache_pressure" => "File cache drop rate",
                "vm.dirty_bytes" => "Largest write burst",
                "vm.dirty_background_bytes" => "Background write start",
                "vm.max_map_count" => "Memory maps per game",
                "vm.watermark_boost_factor" => "Memory reserve boost",
                "vm.compaction_proactiveness" => "Background memory tidying",
                "kernel.split_lock_mitigate" => "Split-lock slowdowns",
                "kernel.nmi_watchdog" => "Lockup watchdog",
                _ => "A kernel setting",
            }
            .into(),
            Setting::Sysfs { path, .. } => {
                let file = path.rsplit('/').next().unwrap_or("");
                let policy = path.split('/').find_map(|p| p.strip_prefix("policy")).map(|n| format!(" (core group {n})")).unwrap_or_default();
                match file {
                    "charge_control_end_threshold" => "Battery charge limit".into(),
                    "charge_control_start_threshold" => "Battery charge start".into(),
                    "energy_performance_preference" => format!("Processor speed preference{policy}"),
                    "scaling_governor" => format!("Processor speed rule{policy}"),
                    "boost" => "Processor boost".into(),
                    "enabled" if path.contains("transparent_hugepage") => "Huge pages".into(),
                    "defrag" if path.contains("khugepaged") => "Background huge page tidying".into(),
                    "defrag" => "Huge page tidying".into(),
                    "wakeup" => "Device wake".into(),
                    "power_dpm_force_performance_level" => "Graphics speed level".into(),
                    "power1_cap" => "Graphics card power limit".into(),
                    _ if path.contains("/ppt_pl1_spl/") => "Power limit".into(),
                    _ if path.contains("/ppt_pl2_sppt/") => "Short boost power limit".into(),
                    _ if path.contains("/ppt_pl3_fppt/") => "Peak boost power limit".into(),
                    _ => "A hardware setting".into(),
                }
            }
            Setting::Wake { vendor, product, .. } => format!("USB device {vendor}:{product} wakes the machine"),
            Setting::Access { what: Access::Lighting } => "Lighting without a password".into(),
            Setting::Access { what: Access::Ntsync } => "Games may use NTSync".into(),
            Setting::Zram { .. } => "Compressed memory size".into(),
            Setting::Module { name } if name == "ntsync" => "NTSync loads at every start".into(),
            Setting::Module { name } => format!("Kernel module {name} loads at every start"),
            Setting::Service { unit, .. } => match unit.as_str() {
                "sshd.service" => "SSH (sign in from another computer)",
                "plugin_loader.service" => "Decky Loader",
                "tailscaled.service" => "Tailscale",
                _ => "A system service",
            }
            .into(),
        }
    }

    /// A value of this setting in a player's words, with its unit: `raw` as the kernel or the record holds it.
    pub fn shown(&self, raw: &str) -> String {
        let raw = raw.trim();
        let num = raw.parse::<u64>().ok();
        let on_off = |n: u64| if n == 0 { "off" } else { "on" }.to_string();
        match (self, num) {
            (Setting::Sysctl { key, .. }, Some(n)) => match key.as_str() {
                "vm.dirty_bytes" | "vm.dirty_background_bytes" if n == 0 => "set by share of memory".into(),
                "vm.dirty_bytes" | "vm.dirty_background_bytes" => format!("{} MB", n / (1 << 20)),
                "kernel.split_lock_mitigate" | "kernel.nmi_watchdog" => on_off(n),
                "vm.max_map_count" if n >= 1_000_000_000 => "as many as it asks for".into(),
                _ => grouped(n),
            },
            (Setting::Sysfs { path, .. }, Some(n)) => {
                let file = path.rsplit('/').next().unwrap_or("");
                match file {
                    "charge_control_end_threshold" | "charge_control_start_threshold" => format!("{n}%"),
                    "power1_cap" => format!("{} W", (n + 500_000) / 1_000_000),
                    "current_value" => format!("{n} W"),
                    "boost" | "defrag" => on_off(n),
                    _ => n.to_string(),
                }
            }
            (Setting::Sysfs { .. }, None) => match raw {
                "enabled" => "yes".into(),
                "disabled" => "no".into(),
                "balance_performance" => "balanced".into(),
                "balance_power" => "power saving".into(),
                "defer+madvise" => "in the background".into(),
                "madvise" => "when a program asks".into(),
                w => w.replace('_', " "),
            },
            (Setting::Service { .. }, _) => if raw == "true" { "on" } else { "off" }.into(),
            _ => raw.to_string(),
        }
    }

    /// The value this setting sets, in a player's words.
    pub fn value_shown(&self) -> String {
        match self {
            Setting::Sysctl { value, .. } => self.shown(&value.to_string()),
            Setting::Sysfs { value, .. } => self.shown(value),
            Setting::Wake { wake, .. } => if *wake { "yes" } else { "no" }.into(),
            Setting::Access { .. } | Setting::Module { .. } => "on".into(),
            Setting::Zram { percent, .. } => format!("{percent}% of memory"),
            Setting::Service { enabled, .. } => if *enabled { "on" } else { "off" }.into(),
        }
    }

    /// Exactly what is written where, for Veteran mode.
    pub fn exact(&self) -> String {
        match self {
            Setting::Sysctl { key, value } => format!("{key} = {value} ({SYSCTL_FILE})"),
            Setting::Sysfs { path, value } => format!("{path} = {value}"),
            Setting::Wake { vendor, product, wake } => format!("{WAKE_FILE}: {vendor}:{product} power/wakeup={}", if *wake { "enabled" } else { "disabled" }),
            Setting::Access { what } => format!("{ACCESS_FILE}: {} rule", what.key()),
            Setting::Zram { percent, algorithm } => format!("{ZRAM_FILE}: zram-size = ram * {percent} / 100, compression-algorithm = {algorithm}"),
            Setting::Module { name } => format!("{MODULES_FILE}: {name}"),
            Setting::Service { unit, enabled } => format!("systemctl {} --now {unit}", if *enabled { "enable" } else { "disable" }),
        }
    }

    /// Plain words for Review changes and the record.
    pub fn words(&self) -> String {
        match self {
            Setting::Access { .. } | Setting::Module { .. } => self.name(),
            Setting::Wake { vendor, product, wake } => format!("USB device {vendor}:{product} {} wake the machine", if *wake { "can" } else { "can't" }),
            Setting::Service { .. } => format!("{} turned {}", self.name(), self.value_shown()),
            _ => format!("{} set to {}", self.name(), self.value_shown()),
        }
    }
}

/// A whole number with commas between thousands.
fn grouped(n: u64) -> String {
    let t = n.to_string();
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if i > 0 && (t.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub const SYSCTL_FILE: &str = "/etc/sysctl.d/99-zz-turbinehub.conf";
pub const WAKE_FILE: &str = "/etc/udev/rules.d/71-turbinehub-wake.rules";
pub const ACCESS_FILE: &str = "/etc/udev/rules.d/70-turbinehub-access.rules";
pub const ZRAM_FILE: &str = "/etc/systemd/zram-generator.conf.d/turbinehub.conf";
pub const MODULES_FILE: &str = "/etc/modules-load.d/turbinehub.conf";
pub const FILES: &[&str] = &[SYSCTL_FILE, WAKE_FILE, ACCESS_FILE, ZRAM_FILE, MODULES_FILE];

const HEADER: &str = "# Written by Turbine Hub. Undo from Turbine Hub: Settings, Review changes.\n";

/// The text of one of `FILES` for these settings, or None when no setting lives in it (the file is removed).
pub fn render(file: &str, settings: &[&Setting]) -> Option<String> {
    let mut body = String::new();
    for s in settings.iter().filter(|s| s.file() == Some(file)) {
        match s {
            Setting::Sysctl { key, value } => body.push_str(&format!("{key} = {value}\n")),
            Setting::Wake { vendor, product, wake } => body.push_str(&format!(
                "ACTION==\"add\", SUBSYSTEM==\"usb\", ATTRS{{idVendor}}==\"{vendor}\", ATTRS{{idProduct}}==\"{product}\", ATTR{{power/wakeup}}=\"{}\"\n",
                if *wake { "enabled" } else { "disabled" }
            )),
            Setting::Access { what } => body.push_str(what.rule()),
            Setting::Zram { percent, algorithm } => body.push_str(&format!("[zram0]\nzram-size = ram * {percent} / 100\ncompression-algorithm = {algorithm}\n")),
            Setting::Module { name } => body.push_str(&format!("{name}\n")),
            Setting::Sysfs { .. } | Setting::Service { .. } => {}
        }
    }
    (!body.is_empty()).then(|| format!("{HEADER}{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    fn from(text: &str) -> Result<Setting, String> {
        Setting::from_json(&parse(text).unwrap())
    }

    #[test]
    fn sysfs_patterns_stay_within_one_part() {
        assert!(sysfs_rule("/sys/class/power_supply/BAT0/charge_control_end_threshold").is_some());
        assert!(sysfs_rule("/sys/devices/system/cpu/cpufreq/policy12/energy_performance_preference").is_some());
        assert!(sysfs_rule("/sys/class/drm/card1/device/hwmon/hwmon3/power1_cap").is_some());
        for bad in [
            "/sys/class/power_supply/../../../etc/shadow/charge_control_end_threshold",
            "/sys/class/power_supply/BAT0/x/charge_control_end_threshold",
            "/sys/class/power_supply//charge_control_end_threshold",
            "/sys/class/power_supply/./charge_control_end_threshold",
            "/sys/class/drm/foo/device/power_dpm_force_performance_level",
            "/etc/passwd",
            "/sys/kernel/mm/transparent_hugepage/enabled/",
            "sys/kernel/mm/transparent_hugepage/enabled",
        ] {
            assert!(sysfs_rule(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn values_are_checked() {
        assert!(from(r#"{"kind":"sysctl","key":"vm.swappiness","value":100}"#).is_ok());
        assert!(from(r#"{"kind":"sysctl","key":"vm.swappiness","value":201}"#).is_err());
        assert!(from(r#"{"kind":"sysctl","key":"kernel.core_pattern","value":1}"#).is_err());
        assert!(from(r#"{"kind":"sysctl","key":"vm.swappiness","value":1.5}"#).is_err());
        assert!(from(r#"{"kind":"sysctl","key":"vm.swappiness","value":-1}"#).is_err());
        assert!(from(r#"{"kind":"sysfs","path":"/sys/class/power_supply/BAT0/charge_control_end_threshold","value":"80"}"#).is_ok());
        assert!(from(r#"{"kind":"sysfs","path":"/sys/class/power_supply/BAT0/charge_control_end_threshold","value":"5"}"#).is_err());
        assert!(from(r#"{"kind":"sysfs","path":"/sys/kernel/mm/transparent_hugepage/enabled","value":"always"}"#).is_ok());
        assert!(from(r#"{"kind":"sysfs","path":"/sys/kernel/mm/transparent_hugepage/enabled","value":"always\nnever"}"#).is_err());
        assert!(from(r#"{"kind":"wake","vendor":"046d","product":"c548","wake":false}"#).is_ok());
        assert!(from(r#"{"kind":"wake","vendor":"046d\"","product":"c548","wake":false}"#).is_err());
        assert!(from(r#"{"kind":"wake","vendor":"046D","product":"c548","wake":false}"#).is_err());
        assert!(from(r#"{"kind":"zram","percent":100,"algorithm":"zstd"}"#).is_ok());
        assert!(from(r#"{"kind":"zram","percent":100,"algorithm":"zstd\n[x]"}"#).is_err());
        assert!(from(r#"{"kind":"zram","percent":10,"algorithm":"zstd"}"#).is_err());
        assert!(from(r#"{"kind":"service","unit":"sshd.service","enabled":true}"#).is_ok());
        assert!(from(r#"{"kind":"service","unit":"getty@tty1.service","enabled":true}"#).is_err());
        assert!(from(r#"{"kind":"module","name":"ntsync"}"#).is_ok());
        assert!(from(r#"{"kind":"module","name":"evil"}"#).is_err());
        assert!(from(r#"{"kind":"access","what":"lighting"}"#).is_ok());
        assert!(from(r#"{"kind":"access","what":"everything"}"#).is_err());
        assert!(from(r#"{"kind":"file","path":"/etc/sudoers"}"#).is_err());
    }

    #[test]
    fn settings_round_trip_through_json() {
        for text in [
            r#"{"kind":"sysctl","key":"vm.swappiness","value":100}"#,
            r#"{"kind":"sysfs","path":"/sys/kernel/mm/transparent_hugepage/defrag","value":"defer+madvise"}"#,
            r#"{"kind":"wake","vendor":"046d","product":"c548","wake":false}"#,
            r#"{"kind":"access","what":"ntsync"}"#,
            r#"{"kind":"zram","percent":100,"algorithm":"zstd"}"#,
            r#"{"kind":"module","name":"ntsync"}"#,
            r#"{"kind":"service","unit":"sshd.service","enabled":true}"#,
        ] {
            assert_eq!(from(text).unwrap().to_json(), text);
        }
    }

    #[test]
    fn words_are_plain_with_units() {
        let sysfs = |p: &str, v: &str| Setting::Sysfs { path: p.into(), value: v.into() };
        assert_eq!(sysfs("/sys/class/power_supply/BAT0/charge_control_end_threshold", "80").words(), "Battery charge limit set to 80%");
        assert_eq!(sysfs("/sys/class/firmware-attributes/asus/attributes/ppt_pl1_spl/current_value", "35").words(), "Power limit set to 35 W");
        assert_eq!(sysfs("/sys/class/firmware-attributes/asus/attributes/ppt_pl3_fppt/current_value", "35").words(), "Peak boost power limit set to 35 W");
        assert_eq!(sysfs("/sys/class/drm/card1/device/hwmon/hwmon3/power1_cap", "279000000").words(), "Graphics card power limit set to 279 W");
        assert_eq!(Setting::Sysctl { key: "vm.max_map_count".into(), value: 2_147_483_642 }.words(), "Memory maps per game set to as many as it asks for");
        assert_eq!(Setting::Sysctl { key: "vm.dirty_bytes".into(), value: 268_435_456 }.words(), "Largest write burst set to 256 MB");
        assert_eq!(Setting::Sysctl { key: "vm.swappiness".into(), value: 60 }.shown("60"), "60");
        assert_eq!(Setting::Sysctl { key: "vm.max_map_count".into(), value: 1 }.shown("65530"), "65,530");
        assert_eq!(Setting::Service { unit: "plugin_loader.service".into(), enabled: false }.words(), "Decky Loader turned off");
        assert_eq!(Setting::Zram { percent: 100, algorithm: "zstd".into() }.words(), "Compressed memory size set to 100% of memory");
        // Veteran's words name the exact key or file.
        assert_eq!(Setting::Sysctl { key: "vm.swappiness".into(), value: 100 }.exact(), format!("vm.swappiness = 100 ({SYSCTL_FILE})"));
        assert!(Setting::Zram { percent: 100, algorithm: "zstd".into() }.exact().starts_with(ZRAM_FILE));
    }

    #[test]
    fn files_are_written_from_settings_only() {
        let a = from(r#"{"kind":"sysctl","key":"vm.swappiness","value":100}"#).unwrap();
        let b = from(r#"{"kind":"sysctl","key":"vm.page-cluster","value":0}"#).unwrap();
        let w = from(r#"{"kind":"wake","vendor":"046d","product":"c548","wake":false}"#).unwrap();
        let t = render(SYSCTL_FILE, &[&a, &w, &b]).unwrap();
        assert!(t.starts_with("# Written by Turbine Hub"));
        assert!(t.ends_with("vm.swappiness = 100\nvm.page-cluster = 0\n"));
        assert!(render(WAKE_FILE, &[&a]).is_none());
        assert!(render(WAKE_FILE, &[&w]).unwrap().contains("ATTRS{idVendor}==\"046d\", ATTRS{idProduct}==\"c548\", ATTR{power/wakeup}=\"disabled\""));
    }
}

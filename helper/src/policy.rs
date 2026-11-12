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
    /// The joystick ring lights: the kernel's LED files and ASUS's lighting chip, so the background agent can
    /// animate them many times a second.
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
            Access::Lighting => concat!(
                "SUBSYSTEM==\"leds\", KERNEL==\"*:rgb:*\", RUN+=\"/usr/bin/chmod 0666 /sys%p/brightness /sys%p/multi_intensity\"\n",
                "SUBSYSTEM==\"hidraw\", ATTRS{idVendor}==\"0b05\", TAG+=\"uaccess\"\n",
            ),
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

    /// Plain words for Review changes.
    pub fn words(&self) -> String {
        match self {
            Setting::Sysctl { key, value } => format!("Kernel setting {key} set to {value}"),
            Setting::Sysfs { path, value } => format!("{} set to {value}", path.rsplit('/').next().unwrap_or(path)),
            Setting::Wake { vendor, product, wake: false } => format!("USB device {vendor}:{product} can't wake the machine"),
            Setting::Wake { vendor, product, wake: true } => format!("USB device {vendor}:{product} can wake the machine"),
            Setting::Access { what: Access::Lighting } => "Lighting works without a password".into(),
            Setting::Access { what: Access::Ntsync } => "Games can use NTSync".into(),
            Setting::Zram { percent, algorithm } => format!("zram swap at {percent}% of memory ({algorithm})"),
            Setting::Module { name } => format!("Kernel module {name} loads at startup"),
            Setting::Service { unit, enabled } => format!("{} turned {}", unit.trim_end_matches(".service"), if *enabled { "on" } else { "off" }),
        }
    }
}

/// Every root-owned file AllyHub (the app Turbine Hub replaces) may have written. The cleanup job removes only these,
/// backing each one up first.
pub const ALLYHUB_FILES: &[&str] = &[
    "/etc/udev/rules.d/99-allyhub-leds.rules",
    "/etc/udev/rules.d/70-allyhub-hid.rules",
    "/etc/systemd/system/allyhub-lighting-perms.service",
    "/etc/allyhub/lighting-perms.sh",
    "/etc/udev/rules.d/99-allyhub-charge.rules",
    "/etc/udev/rules.d/71-allyhub-charge-perm.rules",
    "/etc/udev/rules.d/71-allyhub-nowake.rules",
    "/etc/udev/rules.d/70-allyhub-ntsync.rules",
    "/etc/modules-load.d/allyhub-ntsync.conf",
    "/etc/sysctl.d/99-zz-allyhub.conf",
    "/etc/sysctl.d/99-zz-allyhub-zram.conf",
    "/etc/tmpfiles.d/allyhub-tuneup.conf",
    "/etc/systemd/system/allyhub-zram.service",
    "/etc/allyhub/zram.sh",
    "/etc/tmpfiles.d/allyhub-boost.conf",
    "/etc/polkit-1/rules.d/49-allyhub-decky.rules",
    "/etc/allyhub/decky-restart-allowed",
    "/etc/systemd/system/allyhub-edid.service",
    "/etc/allyhub/edid-override.sh",
    "/etc/allyhub/edid-144.bin",
];

/// AllyHub's system services, turned off before their files go.
pub const ALLYHUB_UNITS: &[&str] = &["allyhub-lighting-perms.service", "allyhub-zram.service", "allyhub-edid.service"];

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

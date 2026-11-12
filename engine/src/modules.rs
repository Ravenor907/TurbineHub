//! My machine: one card per thing this machine has, in a fixed order per kind of machine, and the helper settings
//! each control turns into. Nothing here touches the system; the app sends the settings to the helper.

use crate::hardware::{DriveRole, GpuVendor, Kind, Machine};
use crate::helper::{Access, Ledger, Setting};
use crate::tuneup::{self, State};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    /// The current state in a few words.
    pub value: String,
    pub line: &'static str,
    pub experimental: bool,
}

fn card(id: &'static str, title: &'static str, icon: &'static str, value: String, line: &'static str) -> Card {
    Card { id, title, icon, value, line, experimental: false }
}

fn watts(microwatts: u64) -> u64 {
    (microwatts + 500_000) / 1_000_000
}

/// The cards for this machine. `boost_on` is the player's Game Boost setting.
pub fn cards(root: &Path, m: &Machine, ledger: &Ledger, boost_on: bool) -> Vec<Card> {
    let mut out = Vec::new();
    let tune = tuneup::plan(root, ledger);
    let tune_on = tune.iter().filter(|i| i.state != State::Off).count();
    let tune_value = format!("{tune_on} of {} on", tune.len());
    let blocked = ledger.settings().iter().filter(|s| matches!(s, Setting::Wake { wake: false, .. })).count();
    let can_wake = m.wake_devices.iter().filter(|d| d.enabled).count();
    let wake_value = if blocked > 0 { format!("{blocked} blocked") } else { if can_wake == 1 { "1 device can wake it".to_string() } else { format!("{can_wake} devices can wake it") } };

    if matches!(m.kind, Kind::Handheld | Kind::Laptop) {
        if let Some(pl) = m.power_limits.first() {
            out.push(card("power", "Power", "gauge", format!("{} W", pl.current), "Power limit (TDP) and Game Boost while a game runs."));
        } else if !m.cpu_epp.is_empty() {
            out.push(card("power", "Power", "gauge", if boost_on { "Game Boost on" } else { "Game Boost off" }.into(), "Full CPU speed while a game runs, quiet otherwise."));
        }
        if let Some(p) = m.panel {
            let v = match p.range_hz {
                Some((lo, hi)) if hi > lo => format!("{lo} to {hi} Hz"),
                _ => format!("{} x {}", p.mode.0, p.mode.1),
            };
            out.push(card("display", "Display", "display", v, "Refresh range for the built-in screen."));
        }
        if !m.rgb_leds.is_empty() {
            let n = m.rgb_leds.len();
            out.push(card("lighting", "Lighting", "light", if n == 1 { "1 light".into() } else { format!("{n} lights") }, "Effects, colors per game and battery level on the device's lights."));
        }
        if let Some(b) = m.battery() {
            let v = match b.charge_limit {
                Some(l) if l < 100 => format!("Stops at {l}%"),
                Some(_) => "Charges to 100%".into(),
                None => b.capacity.map(|c| format!("{c}%")).unwrap_or_default(),
            };
            out.push(card("battery", "Battery", "battery", v, "Charge limit and battery health history."));
        }
        out.push(card("sleep", "Sleep", "moon", wake_value.clone(), "What woke the machine and how much each sleep used."));
        out.push(card("tuneup", "Tune-up", "shield", tune_value, "Memory and kernel settings from Bazzite and CachyOS."));
        if m.kind == Kind::Handheld {
            out.push(card("dock", "Dock", "dock", "Off".into(), "Lighting and HDMI audio when docked to a TV."));
        }
    } else {
        if let Some(g) = m.main_gpu() {
            let (name, line) = match g.vendor {
                GpuVendor::Nvidia => ("Graphics card", "Nvidia on SteamOS is experimental. What works and what doesn't, in plain words."),
                _ => ("Graphics card", "Power limit, with a safe reset."),
            };
            let v = g.power_cap.as_ref().map(|(_, _, _, now)| format!("{} W limit", watts(*now))).unwrap_or_else(|| "Detected".into());
            out.push(card("gpu", name, "gauge", v, line));
        }
        if !m.cpu_epp.is_empty() {
            out.push(card("cpu", "Processor", "bolt", if boost_on { "Game Boost on" } else { "Game Boost off" }.into(), "Full speed while a game runs, quiet otherwise."));
        }
        let screens: Vec<_> = m.connected_displays().collect();
        if !screens.is_empty() {
            let max = screens.iter().filter_map(|d| d.edid.as_ref().map(|e| e.max_hz)).max().unwrap_or(0);
            let v = if screens.len() == 1 { format!("1 screen, up to {max} Hz") } else { format!("{} screens", screens.len()) };
            out.push(card("monitors", if m.on_tv() { "TV" } else { "Monitors" }, "display", v, "Refresh rate, VRR, HDR and which screen Game Mode uses."));
        }
        let unused = m.drives.iter().filter(|d| d.role == DriveRole::Empty).count();
        out.push(card("drives", "Drives", "drive", if unused > 0 { format!("{unused} not set up") } else { format!("{} drives", m.drives.len()) }, "Add drives as game libraries. Windows drives are read only."));
        out.push(card("sound", "Sound", "speaker", "Output".into(), "Pick the output: speakers, headset or the TV."));
        out.push(card("sleep", "Sleep and wake", "moon", wake_value, "Wake from the controller, the network or the keyboard."));
        out.push(card("tuneup", "Tune-up", "shield", tune_value, "Memory and kernel settings from Bazzite and CachyOS."));
    }
    if m.windows_boot.is_some() {
        out.push(card("windows", "Restart to Windows", "reboot", "Windows found".into(), "Boot into Windows once, for games that only run there."));
    }
    out
}

/// The charge limit, for every battery that has one.
pub fn charge_limit(m: &Machine, percent: u32) -> Vec<Setting> {
    m.batteries.iter().filter(|b| b.charge_limit.is_some()).map(|b| Setting::Sysfs { path: format!("/sys/class/power_supply/{}/charge_control_end_threshold", b.name), value: percent.to_string() }).collect()
}

/// A power limit (TDP) in watts: all three firmware limits set to it, each kept within its own range.
pub fn tdp(m: &Machine, watts: u64) -> Vec<Setting> {
    m.power_limits.iter().map(|p| Setting::Sysfs { path: p.path.clone(), value: watts.clamp(p.min, p.max).to_string() }).collect()
}

/// The main graphics card's power limit in watts, within the driver's range. None without one.
pub fn gpu_power(m: &Machine, watts: u64) -> Option<Setting> {
    let (path, min, max, _) = m.main_gpu()?.power_cap.clone()?;
    Some(Setting::Sysfs { path, value: (watts * 1_000_000).clamp(min, max).to_string() })
}

/// Lets (or stops) one USB device wake the machine, from the next time it's plugged in or the machine starts.
pub fn usb_wake(vendor: &str, product: &str, wake: bool) -> Setting {
    Setting::Wake { vendor: vendor.into(), product: product.into(), wake }
}

pub fn lighting_access() -> Setting {
    Setting::Access { what: Access::Lighting }
}

/// Game Boost: the /sys values to set while a game runs (`on`) and after it ends. Not recorded: the agent sets
/// them through the helper's set-now job, and a reboot always clears them.
pub fn game_boost(root: &Path, on: bool) -> Vec<(String, String)> {
    let base = "/sys/devices/system/cpu/cpufreq";
    let mut policies: Vec<String> = std::fs::read_dir(root.join(base.trim_start_matches('/'))).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("policy")).collect()).unwrap_or_default();
    policies.sort_by_key(|p| p.trim_start_matches("policy").parse::<u32>().unwrap_or(0));
    policies.into_iter().filter(|p| root.join(format!("{}/{p}/energy_performance_preference", base.trim_start_matches('/'))).exists()).map(|p| (format!("{base}/{p}/energy_performance_preference"), if on { "performance" } else { "balance_performance" }.to_string())).collect()
}

/// The three power modes of Home: how eager the processor is and, where the machine has a power limit, how much it
/// may draw. Each is a set of ordinary settings, sent through the helper and recorded like any other change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    Quiet,
    Balanced,
    Boost,
}

impl PowerMode {
    pub const ALL: [PowerMode; 3] = [PowerMode::Quiet, PowerMode::Balanced, PowerMode::Boost];
    pub fn title(self) -> &'static str {
        match self {
            PowerMode::Quiet => "Quiet",
            PowerMode::Balanced => "Balanced",
            PowerMode::Boost => "Boost",
        }
    }
    /// What the mode is for, in a few words.
    pub fn line(self) -> &'static str {
        match self {
            PowerMode::Quiet => "Cool and quiet, the battery lasts longest.",
            PowerMode::Balanced => "A middle way for most games.",
            PowerMode::Boost => "Everything the machine has, louder and warmer.",
        }
    }
    /// The kernel's word for how eager the processor is in this mode, among the ones this processor offers.
    pub fn preference(self, m: &Machine) -> Option<&'static str> {
        let wanted: &[&'static str] = match self {
            PowerMode::Quiet => &["power", "balance_power"],
            PowerMode::Balanced => &["balance_performance", "default"],
            PowerMode::Boost => &["performance"],
        };
        wanted.iter().copied().find(|w| m.cpu_epp.iter().any(|have| have == w))
    }
    /// The power limit in watts for this mode: a quarter, half and all of the way from the lowest to the highest
    /// the machine allows. None without a power limit.
    pub fn watts(self, m: &Machine) -> Option<u64> {
        let p = m.power_limits.first()?;
        let span = p.max.saturating_sub(p.min);
        Some(match self {
            PowerMode::Quiet => p.min + span / 4,
            PowerMode::Balanced => p.min + span / 2,
            PowerMode::Boost => p.max,
        })
    }
}

/// The files of the processor's speed policies that take a preference, in order.
fn preference_files(root: &Path) -> Vec<String> {
    game_boost(root, true).into_iter().map(|(path, _)| path).collect()
}

/// Does this machine have power modes? It needs a processor that takes a preference; the power limit is extra.
pub fn has_power_modes(root: &Path, m: &Machine) -> bool {
    !preference_files(root).is_empty() && PowerMode::ALL.iter().all(|x| x.preference(m).is_some())
}

/// The settings one power mode stands for: the preference for every processor policy, then the power limits.
pub fn power_mode(root: &Path, m: &Machine, mode: PowerMode) -> Vec<Setting> {
    let Some(word) = mode.preference(m) else { return Vec::new() };
    let mut out: Vec<Setting> = preference_files(root).into_iter().map(|path| Setting::Sysfs { path, value: word.to_string() }).collect();
    if out.is_empty() {
        return out;
    }
    if let Some(w) = mode.watts(m) {
        out.extend(tdp(m, w));
    }
    out
}

/// The mode the machine is in now: the one whose every setting is what the kernel holds. None when the values are
/// somebody else's (Steam's slider, a plugin, a limit set by hand).
pub fn power_mode_now(root: &Path, m: &Machine) -> Option<PowerMode> {
    let live = |path: &str| std::fs::read_to_string(root.join(path.trim_start_matches('/'))).map(|t| t.trim().to_string()).unwrap_or_default();
    PowerMode::ALL.into_iter().find(|mode| {
        let settings = power_mode(root, m, *mode);
        !settings.is_empty() && settings.iter().all(|x| matches!(x, Setting::Sysfs { path, value } if live(path) == *value))
    })
}

#[cfg(test)]
mod power_mode_tests {
    use super::*;
    use crate::fake::{self, Preset};

    fn made(name: &str, preset: Preset) -> (std::path::PathBuf, Machine) {
        let dir = std::env::temp_dir().join(format!("turbinehub-modes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        fake::machine(&dir, preset);
        let m = Machine::read(&dir, fake::EFI_WITH_WINDOWS, fake::HOME_DEV);
        (dir, m)
    }

    #[test]
    fn a_handheld_gets_three_modes_with_a_limit_each() {
        let (root, m) = made("handheld", Preset::Handheld);
        assert!(has_power_modes(&root, &m));
        // The made-up handheld allows 7 to 35 W.
        assert_eq!(PowerMode::ALL.map(|x| x.watts(&m)), [Some(14), Some(21), Some(35)]);
        assert_eq!(PowerMode::ALL.map(|x| x.preference(&m)), [Some("power"), Some("balance_performance"), Some("performance")]);
        let quiet = power_mode(&root, &m, PowerMode::Quiet);
        // Two processor policies, then the three firmware limits, each kept inside its own range.
        assert_eq!(quiet.len(), 5);
        assert!(matches!(&quiet[0], Setting::Sysfs { path, value } if path.ends_with("policy0/energy_performance_preference") && value == "power"));
        assert!(quiet[2..].iter().all(|x| matches!(x, Setting::Sysfs { value, .. } if value == "14")));
        // Every one of them is a setting the helper takes.
        for mode in PowerMode::ALL {
            for x in power_mode(&root, &m, mode) {
                assert!(x.check().is_ok(), "{x:?}");
            }
        }
        // Nothing is in a mode until its values are the kernel's.
        assert_eq!(power_mode_now(&root, &m), None);
        for x in &quiet {
            if let Setting::Sysfs { path, value } = x {
                std::fs::write(root.join(path.trim_start_matches('/')), format!("{value}\n")).unwrap();
            }
        }
        let m = Machine::read(&root, fake::EFI_WITH_WINDOWS, fake::HOME_DEV);
        assert_eq!(power_mode_now(&root, &m), Some(PowerMode::Quiet));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_desktop_gets_the_preference_alone_and_a_processor_without_one_gets_nothing() {
        let (root, m) = made("desktop", Preset::Desktop);
        assert!(has_power_modes(&root, &m));
        assert_eq!(PowerMode::Boost.watts(&m), None);
        let boost = power_mode(&root, &m, PowerMode::Boost);
        assert!(!boost.is_empty() && boost.iter().all(|x| matches!(x, Setting::Sysfs { path, value } if path.ends_with("energy_performance_preference") && value == "performance")));
        // The made-up machines start on balance_performance, which is Balanced where there is no limit to match.
        assert_eq!(power_mode_now(&root, &m), Some(PowerMode::Balanced));
        let mut bare = m.clone();
        bare.cpu_epp.clear();
        assert!(!has_power_modes(&root, &bare) && power_mode(&root, &bare, PowerMode::Quiet).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

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
    let wake_value = if blocked > 0 { format!("{blocked} blocked") } else { format!("{can_wake} devices can wake it") };

    if matches!(m.kind, Kind::Handheld | Kind::Laptop) {
        if let Some(pl) = m.power_limits.first() {
            out.push(card("power", "Power", "gauge", format!("{} W", pl.current), "Power limit (TDP) and Game Boost while a game runs."));
        } else if !m.cpu_epp.is_empty() {
            out.push(card("power", "Power", "gauge", if boost_on { "Game Boost on" } else { "Game Boost off" }.into(), "Full CPU speed while a game runs, quiet otherwise."));
        }
        if let Some(model) = m.model {
            out.push(Card { experimental: model.hz_experimental.is_some(), ..card("display", "Display", "display", format!("{} to {} Hz", model.hz.0, model.hz.1), "Refresh range, plus a guided test for a higher one.") });
        }
        if m.model.is_some_and(|x| x.ally_lighting) || !m.rgb_leds.is_empty() {
            out.push(card("lighting", "Lighting", "light", "Rings".into(), "Joystick ring effects, colors per game, battery rings."));
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

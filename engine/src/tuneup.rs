//! The system tune-up: memory and kernel settings from Bazzite, CachyOS and CryoUtilities, each checked against what
//! the running SteamOS already does, so only what's missing is offered.

use crate::helper::{Access, Ledger, Setting};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Turbine Hub turned it on.
    On,
    /// Not on yet.
    Off,
    /// SteamOS already does this: nothing to add.
    SteamOsDoes,
}

impl State {
    pub fn key(self) -> &'static str {
        match self {
            State::On => "on",
            State::Off => "off",
            State::SteamOsDoes => "steamos",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: &'static str,
    pub title: &'static str,
    pub why: &'static str,
    pub settings: Vec<Setting>,
    pub state: State,
    /// Takes effect at the next restart.
    pub restart: bool,
}

fn sysctl(key: &str, value: u64) -> Setting {
    Setting::Sysctl { key: key.into(), value }
}

fn sysfs(path: &str, value: &str) -> Setting {
    Setting::Sysfs { path: path.into(), value: value.into() }
}

fn read(root: &Path, abs: &str) -> String {
    std::fs::read_to_string(root.join(abs.trim_start_matches('/'))).map(|s| turbinehub_helper::ops::live_word(&s)).unwrap_or_default()
}

/// zram's size as a share of memory, from /proc/swaps and /proc/meminfo (percent, 0 when there is no zram).
fn zram_percent(root: &Path) -> u64 {
    let swaps = read(root, "/proc/swaps");
    let zram_kb: u64 = swaps.lines().filter(|l| l.starts_with("/dev/zram")).filter_map(|l| l.split_whitespace().nth(2)?.parse::<u64>().ok()).sum();
    let mem_kb: u64 = read(root, "/proc/meminfo").lines().find_map(|l| l.strip_prefix("MemTotal:")?.split_whitespace().next()?.parse().ok()).unwrap_or(0);
    (zram_kb * 100).checked_div(mem_kb).unwrap_or(0)
}

/// Is this setting already true on the machine?
fn in_effect(root: &Path, s: &Setting) -> bool {
    match s {
        Setting::Sysctl { key, value } => read(root, &format!("/proc/sys/{}", key.replace('.', "/"))) == value.to_string(),
        Setting::Sysfs { path, value } => read(root, path) == *value,
        Setting::Module { name } => root.join(format!("dev/{name}")).exists(),
        Setting::Access { what: Access::Ntsync } => {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(root.join("dev/ntsync")).map(|m| m.permissions().mode() & 0o006 == 0o006).unwrap_or(false)
        }
        _ => false,
    }
}

/// The tune-up's items for this machine, with each one's state.
pub fn plan(root: &Path, ledger: &Ledger) -> Vec<Item> {
    let thp = "/sys/kernel/mm/transparent_hugepage";
    let raw: Vec<(&'static str, &'static str, &'static str, Vec<Setting>, bool)> = vec![
        ("zram", "Bigger zram swap", "More room for big games before they stutter: compressed memory up to the size of your RAM.", vec![Setting::Zram { percent: 100, algorithm: "zstd".into() }], true),
        ("swap", "Swap tuning for zram", "Uses compressed memory sooner and in smaller pieces, which suits zram.", vec![sysctl("vm.swappiness", 100), sysctl("vm.page-cluster", 0)], false),
        ("memory", "Memory tuning", "Fewer stutters while games load: smaller write bursts, file caches kept longer, no compaction pauses.", vec![sysctl("vm.vfs_cache_pressure", 50), sysctl("vm.dirty_bytes", 268_435_456), sysctl("vm.dirty_background_bytes", 67_108_864), sysctl("vm.watermark_boost_factor", 0), sysctl("vm.compaction_proactiveness", 0)], false),
        ("hugepages", "Huge pages", "Faster memory access for games, without the pauses of always defragmenting.", vec![sysfs(&format!("{thp}/enabled"), "always"), sysfs(&format!("{thp}/defrag"), "defer+madvise")], false),
        ("splitlock", "Split-lock slowdowns off", "Stops the kernel slowing down some older games on purpose.", vec![sysctl("kernel.split_lock_mitigate", 0)], false),
        ("mapcount", "Memory maps for big games", "Some large games crash without it.", vec![sysctl("vm.max_map_count", 2_147_483_642)], false),
        ("ntsync", "NTSync", "Faster Windows-style syncing for games on Proton versions that use it.", vec![Setting::Module { name: "ntsync".into() }, Setting::Access { what: Access::Ntsync }], false),
    ];
    raw.into_iter()
        .map(|(id, title, why, settings, restart)| {
            let ours = settings.iter().all(|s| ledger.find_key(&s.key()).map(|e| e.change == turbinehub_helper::ledger::Change::Set(s.clone())).unwrap_or(false));
            let effect = if id == "zram" { zram_percent(root) >= 90 } else { settings.iter().all(|s| in_effect(root, s)) };
            let state = if ours {
                State::On
            } else if effect {
                State::SteamOsDoes
            } else {
                State::Off
            };
            Item { id, title, why, settings, state, restart }
        })
        .collect()
}

//! My machine's cards, the tune-up plan and the helper settings controls turn into.

mod support;

use std::path::Path;
use turbinehub_engine::fake::{self, Preset};
use turbinehub_engine::hardware::Machine;
use turbinehub_engine::helper::{Change, Ledger, Setting};
use turbinehub_engine::{modules, tuneup};

fn machine(preset: Preset, tag: &str) -> (std::path::PathBuf, Machine) {
    let root = support::scratch(&format!("mod-{tag}"));
    fake::machine(&root, preset);
    let m = Machine::read(&root, fake::EFI_WITH_WINDOWS, fake::HOME_DEV);
    (root, m)
}

fn ids(root: &Path, m: &Machine) -> Vec<&'static str> {
    modules::cards(root, m, &Ledger::default(), false).iter().map(|c| c.id).collect()
}

/// Every setting a control produces is one the helper itself accepts.
fn helper_accepts(s: &Setting) {
    assert_eq!(&Setting::from_json(&turbinehub_helper::json::parse(&s.to_json()).unwrap()).unwrap(), s);
}

#[test]
fn a_handheld_and_a_desktop_get_their_own_cards() {
    let (root, ally) = machine(Preset::AllyX, "ally");
    assert_eq!(ids(&root, &ally), ["power", "display", "lighting", "battery", "sleep", "tuneup", "dock", "windows"]);
    let cards = modules::cards(&root, &ally, &Ledger::default(), false);
    assert_eq!(cards[0].value, "17 W");
    assert!(cards[1].experimental);
    assert_eq!(cards[3].value, "Charges to 100%");
    let (root2, pc) = machine(Preset::Desktop, "pc");
    assert_eq!(ids(&root2, &pc), ["gpu", "cpu", "monitors", "drives", "sound", "sleep", "tuneup", "windows"]);
    let cards = modules::cards(&root2, &pc, &Ledger::default(), true);
    assert_eq!(cards[0].value, "304 W limit");
    assert_eq!(cards[1].value, "Game Boost on");
    assert_eq!(cards[2].value, "1 screen, up to 240 Hz");
    assert_eq!(cards[3].value, "1 not set up");
    let (root3, tv) = machine(Preset::TvPc, "tv");
    assert_eq!(modules::cards(&root3, &tv, &Ledger::default(), false)[2].title, "TV");
    for r in [root, root2, root3] {
        let _ = std::fs::remove_dir_all(r);
    }
}

#[test]
fn controls_turn_into_checked_settings() {
    let (root, ally) = machine(Preset::AllyX, "ctl");
    let c = modules::charge_limit(&ally, 80);
    assert_eq!(c, vec![Setting::Sysfs { path: "/sys/class/power_supply/BAT0/charge_control_end_threshold".into(), value: "80".into() }]);
    let t = modules::tdp(&ally, 50);
    assert_eq!(t.iter().map(|s| match s { Setting::Sysfs { value, .. } => value.as_str(), _ => "" }).collect::<Vec<_>>(), ["35", "37", "40"]);
    let (_, pc) = machine(Preset::Desktop, "ctl2");
    assert_eq!(modules::gpu_power(&pc, 100), Some(Setting::Sysfs { path: "/sys/class/drm/card1/device/hwmon/hwmon6/power1_cap".into(), value: "150000000".into() }));
    assert!(modules::gpu_power(&ally, 20).is_none());
    for s in c.iter().chain(&t).chain(modules::gpu_power(&pc, 250).iter()).chain([modules::usb_wake("3554", "fa09", false), modules::lighting_access()].iter()) {
        helper_accepts(s);
    }
    let boost = modules::game_boost(&root, true);
    assert_eq!(boost.len(), 2);
    assert_eq!(boost[1], ("/sys/devices/system/cpu/cpufreq/policy1/energy_performance_preference".into(), "performance".into()));
    assert_eq!(modules::game_boost(&root, false)[0].1, "balance_performance");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_tune_up_offers_only_what_steamos_lacks() {
    let (root, _) = machine(Preset::Desktop, "tune");
    let plan = tuneup::plan(&root, &Ledger::default());
    let state = |id: &str| plan.iter().find(|i| i.id == id).unwrap().state;
    assert_eq!(state("swap"), tuneup::State::SteamOsDoes);
    assert_eq!(state("mapcount"), tuneup::State::SteamOsDoes);
    assert_eq!(state("splitlock"), tuneup::State::Off);
    assert_eq!(state("hugepages"), tuneup::State::Off);
    assert_eq!(state("zram"), tuneup::State::Off);
    assert!(plan.iter().find(|i| i.id == "zram").unwrap().restart);
    std::fs::write(root.join("proc/swaps"), "Filename Type Size Used Priority\n/dev/zram0 partition 65000000 0 100\n").unwrap();
    assert_eq!(tuneup::plan(&root, &Ledger::default()).iter().find(|i| i.id == "zram").unwrap().state, tuneup::State::SteamOsDoes);
    let mut l = Ledger::default();
    for s in &plan.iter().find(|i| i.id == "splitlock").unwrap().settings {
        helper_accepts(s);
        l.record(Change::Set(s.clone()), vec![], 1, "3.9.5");
    }
    assert_eq!(tuneup::plan(&root, &l).iter().find(|i| i.id == "splitlock").unwrap().state, tuneup::State::On);
    for i in &plan {
        i.settings.iter().for_each(helper_accepts);
    }
    let _ = std::fs::remove_dir_all(&root);
}

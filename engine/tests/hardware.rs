//! Machine detection on fake machines built from real ones.

mod support;

use turbinehub_engine::fake::{self, Preset};
use turbinehub_engine::hardware::{self, DriveRole, GpuVendor, Kind, Machine};

fn read(preset: Preset, efi: &str) -> Machine {
    let root = support::scratch(&format!("hw-{preset:?}"));
    fake::machine(&root, preset);
    let m = Machine::read(&root, efi, fake::HOME_DEV);
    let _ = std::fs::remove_dir_all(&root);
    m
}

#[test]
fn reads_a_real_monitors_edid() {
    let e = hardware::parse_edid(fake::MONITOR_EDID).unwrap();
    assert_eq!(e.maker, "DEL");
    assert_eq!(e.name, "AW2726DM");
    assert_eq!(e.best_mode, (2560, 1440));
    assert_eq!(e.range_hz, Some((48, 240)));
    assert_eq!(e.max_hz, 240);
    assert!(hardware::parse_edid(&[0u8; 128]).is_none());
    assert!(hardware::parse_edid(&fake::MONITOR_EDID[..100]).is_none());
    let tv = hardware::parse_edid(&fake::tv_edid("TV 55", 121, 68)).unwrap();
    assert_eq!((tv.maker.as_str(), tv.name.as_str(), tv.best_mode, tv.max_hz), ("SAM", "TV 55", (3840, 2160), 120));
}

#[test]
fn knows_a_handheld_by_what_it_has() {
    let m = read(Preset::Handheld, "");
    assert_eq!(m.kind, Kind::Handheld);
    assert_eq!(m.name(), "Handheld One");
    let p = m.panel.unwrap();
    assert_eq!((p.inches, p.range_hz, p.mode), (6.9, Some((48, 120)), (1920, 1080)));
    assert_eq!(m.battery().unwrap().charge_limit, Some(100));
    assert_eq!(m.power_limits.len(), 3);
    assert_eq!((m.power_limits[0].name.as_str(), m.power_limits[0].min, m.power_limits[0].max), ("ppt_pl1_spl", 7, 35));
    assert_eq!(m.rgb_leds, vec!["ally:rgb:joystick_rings"]);
    assert_eq!(m.memory_gb, 23);
    assert!(!m.on_tv());
    assert_eq!(m.steamos, "3.9.5");
    assert!(m.windows_boot.is_none());
}

#[test]
fn knows_a_desktop_its_screens_and_drives() {
    let m = read(Preset::Desktop, fake::EFI_WITH_WINDOWS);
    assert_eq!(m.kind, Kind::Desktop);
    assert_eq!(m.name(), "Desktop PC");
    assert_eq!(m.cpu, "13th Gen Intel(R) Core(TM) i9-13900K");
    assert_eq!(m.memory_gb, 62);
    let g = m.main_gpu().unwrap();
    assert_eq!((g.vendor, g.pci_id.as_str(), g.driver.as_str(), g.drives_screen), (GpuVendor::Amd, "1002:7550", "amdgpu", true));
    assert_eq!(g.power_cap.as_ref().unwrap().2, 340_000_000);
    // Above the card's own 304 W the driver refuses while overdrive is off.
    assert_eq!((g.power_cap_default, g.power_max(false), g.power_max(true)), (Some(304_000_000), Some(304_000_000), Some(340_000_000)));
    assert_eq!((g.slot.as_str(), g.subsystem.as_str(), g.mp1), ("0000:03:00.0", "1eae:8811", Some((14, 0, 3))));
    assert!(g.undervolt_family() && g.od.is_none() && !m.overdrive_on() && m.kernel.starts_with("7.2.7"));
    assert!(g.power_cap.as_ref().unwrap().0.ends_with("/hwmon/hwmon6/power1_cap"));
    let screens: Vec<_> = m.connected_displays().collect();
    assert_eq!(screens.len(), 1);
    assert_eq!(screens[0].edid.as_ref().unwrap().max_hz, 240);
    assert!(!m.on_tv());
    let role = |n: &str| m.drives.iter().find(|d| d.name == n).unwrap().role;
    assert_eq!((role("nvme1n1"), role("nvme0n1"), role("sda")), (DriveRole::System, DriveRole::Empty, DriveRole::Windows));
    assert_eq!(m.windows_boot.as_deref(), Some("0003"));
    assert_eq!(m.wake_devices.len(), 2);
    assert!(m.wake_devices.iter().all(|d| d.enabled));
    assert!(m.batteries.is_empty() && m.power_limits.is_empty());
}

#[test]
fn a_desktop_on_a_tv_is_a_tv_pc() {
    let m = read(Preset::TvPc, "");
    assert_eq!(m.kind, Kind::Desktop);
    assert!(m.on_tv());
}

#[test]
fn only_windows_boot_manager_counts() {
    let u = "0a1b2c3d-0000-4000-8000-00000000000a".to_string();
    let efi = format!("Boot0000* SteamOS\nBoot000A* Windows Boot Manager\tHD(1,GPT,{},0x800,0x32000)", u.to_uppercase());
    assert_eq!(hardware::windows_boot_entry(&efi, std::slice::from_ref(&u)), Some("000A".into()));
    // Windows wiped: the firmware still lists it, the partition is gone.
    assert_eq!(hardware::windows_boot_entry(&efi, &[]), None);
    assert_eq!(hardware::windows_boot_entry("Boot000A* Windows Boot Manager\tHD()", std::slice::from_ref(&u)), None);
    assert_eq!(hardware::windows_boot_entry("Boot0000* SteamOS\nBoot0001* UEFI OS\n", &[u]), None);
}

#[test]
fn names_come_from_the_firmware_without_part_codes() {
    use hardware::firmware_name as n;
    assert_eq!(n("ASUSTeK COMPUTER INC.", "ROG Xbox Ally X RC73XA_RC73XA", "1.0"), "ROG Xbox Ally X");
    assert_eq!(n("LENOVO", "83L3", "Legion Go S 8APU1"), "Legion Go S");
    assert_eq!(n("Valve", "Jupiter", "1"), "Valve Jupiter");
    assert_eq!(n("Micro-Star International Co., Ltd.", "MS-7E07", "4.0"), "");
    assert_eq!(n("System manufacturer", "System Product Name", "System Version"), "");
    assert_eq!(n("To Be Filled By O.E.M.", "To Be Filled By O.E.M.", "Default string"), "");
}

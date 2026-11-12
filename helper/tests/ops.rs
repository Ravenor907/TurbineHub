//! The helper's jobs run against a scratch folder standing in for "/", with a fake runner that records the programs
//! it would have started.

use std::fs;
use std::path::PathBuf;
use turbinehub_helper::json::parse;
use turbinehub_helper::ops::{self, Owner, Run, Sys};
use turbinehub_helper::policy::{self, Setting};
use turbinehub_helper::sha256;

#[derive(Default)]
struct Fake {
    calls: Vec<String>,
    enabled: Vec<String>,
    decky_bytes: Vec<u8>,
    decky_digest: String,
    efi: String,
    fail_modprobe: bool,
}

impl Run for Fake {
    fn run(&mut self, prog: &str, args: &[&str]) -> Result<String, String> {
        self.calls.push(format!("{} {}", prog.rsplit('/').next().unwrap(), args.join(" ")).trim().to_string());
        match (prog, args) {
            (ops::SYSTEMCTL, ["is-enabled", u]) => Ok(if self.enabled.iter().any(|e| e == u) { "enabled\n" } else { "disabled\n" }.into()),
            (ops::SYSTEMCTL, ["enable", "--now", u]) => {
                self.enabled.push(u.to_string());
                Ok(String::new())
            }
            (ops::SYSTEMCTL, ["disable", "--now", u]) => {
                self.enabled.retain(|e| e != u);
                Ok(String::new())
            }
            (ops::MODPROBE, _) if self.fail_modprobe => Err("modprobe failed: not found".into()),
            (ops::CURL, a) if a.contains(&"-o") => {
                let i = a.iter().position(|x| *x == "-o").unwrap();
                fs::write(a[i + 1], &self.decky_bytes).unwrap();
                Ok(String::new())
            }
            (ops::CURL, a) if a.last().unwrap().contains("/releases/tags/") => {
                let tag = a.last().unwrap().rsplit('/').next().unwrap();
                Ok(format!(
                    r#"{{"assets":[{{"name":"PluginLoader","digest":"sha256:{}","browser_download_url":"https://github.com/SteamDeckHomebrew/decky-loader/releases/download/{tag}/PluginLoader"}}]}}"#,
                    self.decky_digest
                ))
            }
            (ops::EFIBOOTMGR, []) => Ok(self.efi.clone()),
            _ => Ok(String::new()),
        }
    }
}

fn machine(tag: &str) -> Sys<Fake> {
    let root = std::env::temp_dir().join(format!("turbinehub-helper-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let w = |p: &str, t: &str| {
        let f = root.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, t).unwrap();
    };
    w("etc/os-release", "NAME=\"SteamOS\"\nVERSION_ID=3.9.5\n");
    w("etc/passwd", "root:x:0:0::/root:/bin/bash\ndeck:x:1000:1000::/home/deck:/bin/bash\n");
    w("proc/sys/vm/swappiness", "60\n");
    w("proc/sys/vm/page-cluster", "3\n");
    w("sys/devices/platform/bat/BAT0/charge_control_end_threshold", "100\n");
    w("sys/kernel/mm/transparent_hugepage/enabled", "always [madvise] never\n");
    w("sys/devices/card1/hwmon/hwmon3/power1_cap", "220000000\n");
    w("sys/devices/card1/hwmon/hwmon3/power1_cap_min", "150000000\n");
    w("sys/devices/card1/hwmon/hwmon3/power1_cap_max", "304000000\n");
    w("secret", "do not touch\n");
    fs::create_dir_all(root.join("sys/class/power_supply")).unwrap();
    std::os::unix::fs::symlink("../../devices/platform/bat/BAT0", root.join("sys/class/power_supply/BAT0")).unwrap();
    fs::create_dir_all(root.join("sys/class/drm/card1")).unwrap();
    std::os::unix::fs::symlink("../../../devices/card1", root.join("sys/class/drm/card1/device")).unwrap();
    // A link that leads out of /sys must be refused even though its name matches.
    fs::create_dir_all(root.join("sys/class/power_supply/EVIL")).unwrap();
    std::os::unix::fs::symlink("../../../../secret", root.join("sys/class/power_supply/EVIL/charge_control_end_threshold")).unwrap();
    fs::create_dir_all(root.join("var/lib/turbinehub")).unwrap();
    fs::write(root.join("var/lib/turbinehub/owner"), "1000\n").unwrap();
    Sys { root, run: Fake::default(), now: 1000 }
}

fn set(text: &str) -> Setting {
    Setting::from_json(&parse(text).unwrap()).unwrap()
}

fn read(sys: &Sys<Fake>, p: &str) -> String {
    fs::read_to_string(sys.p(p)).unwrap_or_default()
}

const CHARGE: &str = "/sys/class/power_supply/BAT0/charge_control_end_threshold";

#[test]
fn apply_writes_records_and_undo_puts_it_back() {
    let mut sys = machine("apply");
    let a = sys.apply(set(r#"{"kind":"sysctl","key":"vm.swappiness","value":100}"#)).unwrap();
    assert_eq!(read(&sys, "/proc/sys/vm/swappiness"), "100\n");
    assert!(read(&sys, policy::SYSCTL_FILE).ends_with("vm.swappiness = 100\n"));
    // Changing it again is the same change, and Undo still goes back to 60.
    let a2 = sys.apply(set(r#"{"kind":"sysctl","key":"vm.swappiness","value":150}"#)).unwrap();
    assert_eq!(a, a2);
    let b = sys.apply(set(&format!(r#"{{"kind":"sysfs","path":"{CHARGE}","value":"80"}}"#))).unwrap();
    assert_eq!(read(&sys, CHARGE), "80");
    let l = sys.ledger().unwrap();
    assert_eq!(l.entries.len(), 2);
    assert_eq!(l.entries[0].before("value"), Some("60"));
    assert_eq!(l.entries[1].before("value"), Some("100"));
    assert_eq!(l.entries[0].steamos, "3.9.5");

    sys.undo(a).unwrap();
    assert_eq!(read(&sys, "/proc/sys/vm/swappiness"), "60\n");
    assert!(!sys.p(policy::SYSCTL_FILE).exists());
    sys.undo(b).unwrap();
    assert_eq!(read(&sys, CHARGE), "100");
    assert!(sys.ledger().unwrap().entries.is_empty());
    assert!(sys.undo(b).is_err());
}

#[test]
fn reboot_and_update_drift_is_found_and_fixed() {
    let mut sys = machine("reapply");
    sys.apply(set(&format!(r#"{{"kind":"sysfs","path":"{CHARGE}","value":"80"}}"#))).unwrap();
    sys.apply(set(r#"{"kind":"sysfs","path":"/sys/kernel/mm/transparent_hugepage/enabled","value":"always"}"#)).unwrap();
    sys.apply(set(r#"{"kind":"wake","vendor":"046d","product":"c548","wake":false}"#)).unwrap();
    assert!(sys.verify().unwrap().is_empty());
    // A reboot forgets /sys values; a SteamOS update resets a rule file.
    fs::write(sys.p(CHARGE), "100\n").unwrap();
    fs::write(sys.p("/sys/kernel/mm/transparent_hugepage/enabled"), "always [madvise] never\n").unwrap();
    fs::remove_file(sys.p(policy::WAKE_FILE)).unwrap();
    let drift = sys.verify().unwrap();
    assert_eq!(drift.len(), 3, "{drift:?}");
    sys.reapply().unwrap();
    assert_eq!(read(&sys, CHARGE), "80");
    assert!(read(&sys, policy::WAKE_FILE).contains("046d"));
    assert!(sys.run.calls.iter().any(|c| c == "udevadm control --reload"));
}

#[test]
fn refuses_what_is_not_on_the_lists() {
    let mut sys = machine("refuse");
    // Matches the pattern but leads out of /sys.
    let evil = Setting::Sysfs { path: "/sys/class/power_supply/EVIL/charge_control_end_threshold".into(), value: "80".into() };
    assert!(sys.apply(evil).is_err());
    assert_eq!(read(&sys, "/secret"), "do not touch\n");
    // Not on this machine: nothing is created under /sys.
    assert!(sys.apply(set(r#"{"kind":"sysfs","path":"/sys/class/power_supply/BAT9/charge_control_end_threshold","value":"80"}"#)).is_err());
    assert!(!sys.p("/sys/class/power_supply/BAT9").exists());
    // Outside the kernel's own limits.
    let cap = "/sys/class/drm/card1/device/hwmon/hwmon3/power1_cap";
    assert!(sys.apply(set(&format!(r#"{{"kind":"sysfs","path":"{cap}","value":"400000000"}}"#))).is_err());
    assert!(sys.apply(set(&format!(r#"{{"kind":"sysfs","path":"{cap}","value":"200000000"}}"#))).is_ok());
    assert!(sys.set_now("/etc/shadow", "x").is_err());
    assert!(sys.boot_next("00;1").is_err());
}

#[test]
fn a_failed_change_leaves_nothing_behind() {
    let mut sys = machine("fail");
    sys.run.fail_modprobe = true;
    assert!(sys.apply(set(r#"{"kind":"module","name":"ntsync"}"#)).is_err());
    assert!(!sys.p(policy::MODULES_FILE).exists());
    assert!(sys.ledger().unwrap().entries.is_empty());
}

#[test]
fn memory_limits_go_back_through_their_share_of_memory() {
    let mut sys = machine("dirty");
    // The kernel's defaults: the limits are shares of memory, the byte limits read 0 (and refuse 0 as a value).
    for (k, v) in [("dirty_bytes", "0"), ("dirty_background_bytes", "0"), ("dirty_ratio", "20"), ("dirty_background_ratio", "10")] {
        fs::write(sys.p(&format!("/proc/sys/vm/{k}")), format!("{v}\n")).unwrap();
    }
    let a = sys.apply(set(r#"{"kind":"sysctl","key":"vm.dirty_bytes","value":268435456}"#)).unwrap();
    let b = sys.apply(set(r#"{"kind":"sysctl","key":"vm.dirty_background_bytes","value":67108864}"#)).unwrap();
    // Setting a byte limit makes the kernel zero its share.
    fs::write(sys.p("/proc/sys/vm/dirty_ratio"), "0\n").unwrap();
    fs::write(sys.p("/proc/sys/vm/dirty_background_ratio"), "0\n").unwrap();
    let l = sys.ledger().unwrap();
    assert_eq!((l.entries[0].before("value"), l.entries[0].before("ratio")), (Some("0"), Some("20")));
    assert_eq!((l.entries[1].before("value"), l.entries[1].before("ratio")), (Some("0"), Some("10")));
    sys.undo(b).unwrap();
    sys.undo(a).unwrap();
    assert_eq!(read(&sys, "/proc/sys/vm/dirty_ratio"), "20\n");
    assert_eq!(read(&sys, "/proc/sys/vm/dirty_background_ratio"), "10\n");
    // 0 is never written to a byte limit: the kernel would refuse it.
    assert_eq!(read(&sys, "/proc/sys/vm/dirty_bytes"), "268435456\n");
    assert!(!sys.p(policy::SYSCTL_FILE).exists());

    // A record from before the share was kept gets the kernel's default share.
    let mut sys = machine("dirty-old");
    fs::write(sys.p("/proc/sys/vm/dirty_bytes"), "0\n").unwrap();
    let id = sys.apply(set(r#"{"kind":"sysctl","key":"vm.dirty_bytes","value":268435456}"#)).unwrap();
    let text = read(&sys, ops::LEDGER).replace(",\"ratio\":\"20\"", "");
    fs::write(sys.p(ops::LEDGER), text).unwrap();
    fs::write(sys.p("/proc/sys/vm/dirty_ratio"), "0\n").unwrap();
    sys.undo(id).unwrap();
    assert_eq!(read(&sys, "/proc/sys/vm/dirty_ratio"), "20\n");
}

#[test]
fn services_go_back_to_how_they_were() {
    let mut sys = machine("svc");
    let id = sys.apply(set(r#"{"kind":"service","unit":"sshd.service","enabled":true}"#)).unwrap();
    assert!(sys.run.enabled.contains(&"sshd.service".to_string()));
    sys.undo(id).unwrap();
    assert!(!sys.run.enabled.contains(&"sshd.service".to_string()));
}

fn owner(_sys: &Sys<Fake>) -> Owner {
    Owner { uid: 1000, name: "deck".into(), home: PathBuf::from("/home/deck") }
}

#[test]
fn decky_installs_only_a_build_matching_githubs_checksum() {
    let mut sys = machine("decky");
    sys.run.decky_bytes = b"official loader".to_vec();
    sys.run.decky_digest = sha256::digest_hex(b"official loader");
    let o = owner(&sys);
    assert!(sys.decky_install("v3.1.0; rm -rf /", &o).is_err());
    let id = sys.decky_install("v3.1.0", &o).unwrap();
    assert_eq!(fs::read(sys.p("/home/deck/homebrew/services/PluginLoader")).unwrap(), b"official loader");
    assert!(read(&sys, ops::DECKY_UNIT).contains("ExecStart=/home/deck/homebrew/services/PluginLoader"));
    assert!(sys.run.enabled.contains(&"plugin_loader.service".to_string()));
    sys.undo(id).unwrap();
    assert!(!sys.p("/home/deck/homebrew/services/PluginLoader").exists());
    assert!(!sys.p(ops::DECKY_UNIT).exists());

    sys.run.decky_bytes = b"something else".to_vec();
    let e = sys.decky_install("v3.1.0", &o).unwrap_err();
    assert!(e.contains("SHA-256"), "{e}");
    assert!(!sys.p("/home/deck/homebrew/services/PluginLoader").exists());
}

#[test]
fn restart_to_windows_only_picks_windows() {
    let mut sys = machine("efi");
    let u = "0a1b2c3d-0000-4000-8000-00000000000a";
    sys.run.efi = format!("BootCurrent: 0000\nBoot0000* SteamOS\tHD(1,GPT,{u},0x800,0x80000)\nBoot0003* Windows Boot Manager\tHD(1,GPT,{u},0x800,0x80000)/\\EFI\\Microsoft\\Boot\\bootmgfw.efi\nBoot0004  Windows Boot Manager\tHD(1,GPT,{u},0x800,0x80000)\nBoot0005* Windows Boot Manager\n");
    assert_eq!(ops::windows_entries(&sys.run.efi), vec![("0003".to_string(), u.to_string())]);
    assert!(sys.boot_next("0000").is_err());
    // A Windows that was wiped: its entry stays in the firmware, its partition is gone.
    assert!(sys.boot_next("0003").is_err());
    assert!(!sys.run.calls.iter().any(|c| c.starts_with("efibootmgr -n")));
    fs::create_dir_all(sys.p("/dev/disk/by-partuuid")).unwrap();
    fs::write(sys.p(&format!("/dev/disk/by-partuuid/{u}")), b"").unwrap();
    sys.boot_next("0003").unwrap();
    assert!(sys.run.calls.iter().any(|c| c == "efibootmgr -n 0003"));
}

#[test]
fn install_and_uninstall() {
    let mut sys = machine("install");
    let from = sys.root.join("helper-from-flatpak");
    fs::write(&from, b"helper").unwrap();
    let o = sys.user(1000).unwrap();
    sys.install(&from, &o).unwrap();
    assert_eq!(fs::read(sys.p(ops::BIN)).unwrap(), b"helper");
    let rule = read(&sys, ops::POLKIT_RULE);
    assert!(rule.contains("subject.user == \"deck\"") && rule.contains(ops::BIN));
    assert!(sys.run.calls.iter().any(|c| c == "systemctl enable turbinehub-reapply.service"));
    sys.apply(set(r#"{"kind":"sysctl","key":"vm.page-cluster","value":0}"#)).unwrap();
    sys.uninstall().unwrap();
    assert_eq!(read(&sys, "/proc/sys/vm/page-cluster"), "3\n");
    assert!(!sys.p(ops::STATE_DIR).exists() && !sys.p(ops::POLKIT_RULE).exists());
}

#[test]
fn only_asus_lighting_interfaces_are_opened_and_only_for_writing() {
    use std::os::unix::fs::PermissionsExt;
    let mut sys = machine("hid");
    // The Ally's config interface (a keyboard and ASUS lighting), its Dynamic Lighting interface, its gamepad, and
    // a receiver from another maker that has the same lighting collection.
    let config: &[u8] = &[0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x81, 0x02, 0xC0, 0x06, 0x31, 0xFF, 0x09, 0x80, 0xA1, 0x01, 0x91, 0x00, 0xC0];
    let lamp: &[u8] = &[0x05, 0x59, 0x09, 0x01, 0xA1, 0x01, 0xC0];
    let pad: &[u8] = &[0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0xC0];
    let node = |sys: &Sys<Fake>, n: &str, id: &str, desc: &[u8]| {
        let d = sys.p(&format!("/sys/class/hidraw/{n}/device"));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("uevent"), format!("HID_ID=0003:{id}\n")).unwrap();
        fs::write(d.join("report_descriptor"), desc).unwrap();
    };
    fs::create_dir_all(sys.p("/dev")).unwrap();
    for (n, id, desc) in [("hidraw0", "00000B05:00001B4C", config), ("hidraw1", "00000B05:00001B4C", lamp), ("hidraw2", "00000B05:00001B4C", pad), ("hidraw3", "0000046D:0000C548", config)] {
        node(&sys, n, id, desc);
        fs::write(sys.p(&format!("/dev/{n}")), "").unwrap();
        fs::set_permissions(sys.p(&format!("/dev/{n}")), fs::Permissions::from_mode(0o600)).unwrap();
    }
    let mode = |sys: &Sys<Fake>, n: &str| fs::metadata(sys.p(&format!("/dev/{n}"))).unwrap().permissions().mode() & 0o777;
    // The rings' own files, closed as the kernel leaves them, and a light whose file leads out of /sys.
    for (led, f) in [("ally:rgb:joystick_rings", "multi_intensity"), ("ally:rgb:joystick_rings", "brightness")] {
        let path = sys.p(&format!("/sys/class/leds/{led}/{f}"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "0\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    fs::create_dir_all(sys.p("/sys/class/leds/evil")).unwrap();
    std::os::unix::fs::symlink("../../../../secret", sys.p("/sys/class/leds/evil/multi_intensity")).unwrap();
    let led_mode = |sys: &Sys<Fake>, f: &str| fs::metadata(sys.p(&format!("/sys/class/leds/ally:rgb:joystick_rings/{f}"))).unwrap().permissions().mode() & 0o777;
    // Not before the player asked for the lighting rule.
    assert_eq!(sys.hid_access("hidraw0"), Ok(false));
    assert_eq!(mode(&sys, "hidraw0"), 0o600);
    sys.open_lights();
    assert_eq!(led_mode(&sys, "multi_intensity"), 0o644);
    sys.apply(set(r#"{"kind":"access","what":"lighting"}"#)).unwrap();
    assert!(read(&sys, policy::ACCESS_FILE).contains("hid-access %k"));
    assert!(sys.run.calls.iter().any(|c| c == "udevadm trigger --action=add --subsystem-match=hidraw"));
    // The helper never waits for udev to finish: the rule starts the helper again, and that would wait on itself.
    assert!(!sys.run.calls.iter().any(|c| c.contains("--settle")));
    // Open at once, without waiting for udev; nothing outside /sys changes.
    assert_eq!([led_mode(&sys, "multi_intensity"), led_mode(&sys, "brightness")], [0o666, 0o666]);
    // A light's own effect file is opened with them (so it can be set back to plain colors), other files never.
    for (f, want) in [("effect", 0o666), ("trigger", 0o644)] {
        let path = sys.p(&format!("/sys/class/leds/ally:rgb:joystick_rings/{f}"));
        fs::write(&path, "x\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        sys.open_lights();
        assert_eq!(led_mode(&sys, f), want, "{f}");
    }
    assert_eq!(fs::metadata(sys.p("/secret")).unwrap().permissions().mode() & 0o777 & 0o002, 0);
    // Closed again (a sleep, the controller service): the boot and wake run opens them again.
    fs::set_permissions(sys.p("/sys/class/leds/ally:rgb:joystick_rings/multi_intensity"), fs::Permissions::from_mode(0o644)).unwrap();
    let _ = sys.reapply();
    assert_eq!(led_mode(&sys, "multi_intensity"), 0o666);
    assert_eq!(sys.hid_access("hidraw0"), Ok(true));
    assert_eq!(sys.hid_access("hidraw1"), Ok(true));
    assert_eq!(sys.hid_access("hidraw2"), Ok(false));
    assert_eq!(sys.hid_access("hidraw3"), Ok(false));
    // Written by anyone, read by root only.
    assert_eq!([mode(&sys, "hidraw0"), mode(&sys, "hidraw1"), mode(&sys, "hidraw2"), mode(&sys, "hidraw3")], [0o602, 0o602, 0o600, 0o600]);
    // An interface another lighting app opened for reading and writing (Ally Hub's rule sets 0666) keeps that.
    fs::set_permissions(sys.p("/dev/hidraw1"), fs::Permissions::from_mode(0o664)).unwrap();
    assert_eq!(sys.hid_access("hidraw1"), Ok(true));
    assert_eq!(mode(&sys, "hidraw1"), 0o666);
    for bad in ["../hidraw0", "hidraw0/../../etc", "event0", "sda"] {
        assert!(sys.hid_access(bad).is_err(), "{bad}");
    }
    // A link in /dev is never followed.
    node(&sys, "hidraw4", "00000B05:00001B4C", config);
    std::os::unix::fs::symlink("../secret", sys.p("/dev/hidraw4")).unwrap();
    assert!(sys.hid_access("hidraw4").is_err());
    // At boot, reapply opens them again (a fresh /dev).
    fs::set_permissions(sys.p("/dev/hidraw0"), fs::Permissions::from_mode(0o600)).unwrap();
    let _ = sys.reapply();
    assert_eq!(mode(&sys, "hidraw0"), 0o602);
    assert_eq!(read(&sys, "/secret"), "do not touch\n");
    let _ = fs::remove_dir_all(&sys.root);
}


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
    sys.run.efi = "BootCurrent: 0000\nBoot0000* SteamOS\tHD(...)\nBoot0003* Windows Boot Manager\tHD(...)\n".into();
    assert_eq!(ops::windows_entries(&sys.run.efi), vec!["0003"]);
    assert!(sys.boot_next("0000").is_err());
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


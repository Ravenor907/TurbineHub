//! The helper's jobs run against a scratch folder standing in for "/", with a fake runner that records the programs
//! it would have started.

use std::fs;
use std::path::PathBuf;
use turbinehub_helper::gpu;
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
    /// The graphics driver standing in: the lowest offset it takes, and the one waiting for "c".
    od_min: i32,
    od_pending: Option<i32>,
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

    /// The driver: "vo N" is checked against its range (EINVAL past it) and waits; "c" commits it to the file. Each
    /// write is recorded with the power limit the card had at that moment.
    fn od(&mut self, file: &std::path::Path, command: &str) -> std::io::Result<()> {
        let cap = fs::read_to_string(file.parent().unwrap().join("hwmon/hwmon3/power1_cap")).unwrap_or_default();
        self.calls.push(format!("od {command} (cap {})", cap.trim()));
        let einval = || std::io::Error::from_raw_os_error(22);
        match command.split_whitespace().collect::<Vec<_>>()[..] {
            ["vo", n] => {
                let v: i32 = n.parse().map_err(|_| einval())?;
                if v < self.od_min || v > 0 {
                    return Err(einval());
                }
                self.od_pending = Some(v);
                Ok(())
            }
            ["c"] => {
                if let Some(v) = self.od_pending.take() {
                    fs::write(file, gpu::od_rx9000(v))?;
                }
                Ok(())
            }
            _ => Err(einval()),
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
    // SteamOS keeps only the files a keep list names when it updates: every file of Turbine Hub's is on it.
    let keep = read(&sys, ops::KEEP_LIST);
    for f in policy::FILES.iter().copied().chain([ops::POLKIT_RULE, ops::REAPPLY_UNIT, gpu::GUARD_UNIT, gpu::SLEEP_UNIT]) {
        assert!(keep.lines().any(|l| l == f), "{f} isn't kept");
    }
    assert!(keep.lines().any(|l| l == policy::AMDGPU_FILE));
    // The graphics card voltage guard: it notices a stop while armed at boot, and clears its mark at a clean
    // shutdown and before a sleep.
    let guard = read(&sys, gpu::GUARD_UNIT);
    assert!(guard.contains("ExecStart=/var/lib/turbinehub/bin/turbinehub-helper gpu-guard boot") && guard.contains("ExecStop=/var/lib/turbinehub/bin/turbinehub-helper gpu-guard stop") && guard.contains("RemainAfterExit=yes"));
    assert!(read(&sys, gpu::SLEEP_UNIT).contains("Before=sleep.target") && read(&sys, gpu::SLEEP_UNIT).contains("WantedBy=sleep.target"));
    assert!(sys.run.calls.iter().any(|c| c == "systemctl enable --now turbinehub-gpu-guard.service") && sys.run.calls.iter().any(|c| c == "systemctl enable turbinehub-gpu-sleep.service"));
    assert!(keep.lines().all(|l| l.starts_with("##") || l.starts_with("/etc/")));
    sys.apply(set(r#"{"kind":"sysctl","key":"vm.page-cluster","value":0}"#)).unwrap();
    // A SteamOS update from before the keep list: /etc is wiped but for the unit. The next start puts the rule,
    // the keep list and the settings files back, so the owner is never asked for the password again.
    for f in [ops::POLKIT_RULE, ops::KEEP_LIST, policy::SYSCTL_FILE] {
        fs::remove_file(sys.p(f)).unwrap();
    }
    let fixed = sys.reapply().unwrap();
    assert!(fixed.iter().any(|f| f == ops::POLKIT_RULE) && fixed.iter().any(|f| f == ops::KEEP_LIST));
    assert_eq!(read(&sys, ops::POLKIT_RULE), rule);
    assert!(read(&sys, policy::SYSCTL_FILE).contains("vm.page-cluster"));
    // Nothing to put back the second time.
    assert!(!sys.reapply().unwrap().iter().any(|f| f == ops::POLKIT_RULE || f == ops::KEEP_LIST));
    sys.uninstall().unwrap();
    assert_eq!(read(&sys, "/proc/sys/vm/page-cluster"), "3\n");
    assert!(!sys.p(ops::STATE_DIR).exists() && !sys.p(ops::POLKIT_RULE).exists() && !sys.p(ops::KEEP_LIST).exists());
    assert!(!sys.p(gpu::GUARD_UNIT).exists() && !sys.p(gpu::SLEEP_UNIT).exists());
}

// ---------------------------------------------------------------- graphics card voltage

const SLOT: &str = "0000:03:00.0";
const OD: &str = "/sys/bus/pci/devices/0000:03:00.0/pp_od_clk_voltage";
const CAP: &str = "/sys/class/drm/card1/device/hwmon/hwmon3/power1_cap";
const BOOT: &str = "bc4d8459-eade-421c-9f99-4fa13ff81b4e";

/// The made-up machine with an RX 9070 XT in slot 0000:03:00.0 (the same card as card1), overdrive off, the guard's
/// units set up.
fn rx9070(tag: &str) -> Sys<Fake> {
    let mut sys = machine(tag);
    let w = |sys: &Sys<Fake>, p: &str, t: &str| {
        let f = sys.p(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, t).unwrap();
    };
    let dev = "/sys/devices/card1";
    for (f, v) in [("vendor", "0x1002"), ("device", "0x7550"), ("subsystem_vendor", "0x1eae"), ("subsystem_device", "0x8811"), ("ip_discovery/die/0/MP1/0/major", "14"), ("ip_discovery/die/0/MP1/0/minor", "0"), ("ip_discovery/die/0/MP1/0/revision", "3")] {
        w(&sys, &format!("{dev}/{f}"), &format!("{v}\n"));
    }
    std::os::unix::fs::symlink("../../../bus/pci/drivers/amdgpu", sys.p(&format!("{dev}/driver"))).unwrap();
    fs::create_dir_all(sys.p("/sys/bus/pci/devices")).unwrap();
    std::os::unix::fs::symlink("../../../devices/card1", sys.p("/sys/bus/pci/devices/0000:03:00.0")).unwrap();
    w(&sys, gpu::MASK_FILE, "0xfff7bfff\n");
    w(&sys, gpu::OSRELEASE, "7.2.7-valve2-2-neptune-72\n");
    w(&sys, gpu::BOOT_ID, &format!("{BOOT}\n"));
    w(&sys, gpu::GUARD_UNIT, "unit");
    w(&sys, gpu::SLEEP_UNIT, "unit");
    sys.run.od_min = -200;
    sys
}

/// The same machine after the restart that turned overdrive on: the voltage file is there, at `offset`.
fn overdrive_on(sys: &Sys<Fake>, offset: i32) {
    fs::write(sys.p(gpu::MASK_FILE), "0xfff7ffff\n").unwrap();
    fs::write(sys.p(OD), gpu::od_rx9000(offset)).unwrap();
}

fn voltage(offset: i32) -> Setting {
    set(&format!(r#"{{"kind":"gpu-voltage","slot":"{SLOT}","device":"1002:7550","subsystem":"1eae:8811","offset":{offset}}}"#))
}

fn job(sys: &mut Sys<Fake>, what: &str, offset: Option<i32>) -> Result<i32, String> {
    let extra = offset.map(|o| format!(r#","offset":{o}"#)).unwrap_or_default();
    sys.gpu_job(&parse(&format!(r#"{{"do":"{what}","slot":"{SLOT}","device":"1002:7550","subsystem":"1eae:8811"{extra}}}"#)).unwrap())
}

fn od_calls(sys: &Sys<Fake>) -> Vec<String> {
    sys.run.calls.iter().filter(|c| c.starts_with("od ")).cloned().collect()
}

#[test]
fn overdrive_is_written_from_the_live_mask_on_rx_7000_and_9000_only() {
    let mut sys = rx9070("overdrive");
    let od = |mask: &str, kernel: &str| set(&format!(r#"{{"kind":"gpu-overdrive","mask":"{mask}","kernel":"{kernel}"}}"#));
    // Another mask than the driver's own plus the bit, or another kernel, is refused.
    assert!(sys.apply(od("0xfffeffff", "7.2.7-valve2-2-neptune-72")).unwrap_err().contains("0xfff7bfff"));
    assert!(sys.apply(od("0xfff7ffff", "6.11.11-valve1")).is_err());
    let id = sys.apply(od("0xfff7ffff", "7.2.7-valve2-2-neptune-72")).unwrap();
    let file = read(&sys, policy::AMDGPU_FILE);
    assert!(file.ends_with("options amdgpu ppfeaturemask=0xfff7ffff\n"), "{file}");
    // Nothing changes before the restart, and nothing is run for it.
    assert!(!sys.run.calls.iter().any(|c| c.starts_with("modprobe")));
    assert!(sys.verify().unwrap().is_empty());
    // A SteamOS update drops /etc but its keep list: the reapply unit writes it again.
    fs::remove_file(sys.p(policy::AMDGPU_FILE)).unwrap();
    sys.reapply().unwrap();
    assert!(sys.p(policy::AMDGPU_FILE).exists());
    sys.undo(id).unwrap();
    assert!(!sys.p(policy::AMDGPU_FILE).exists());
    // An RX 6000 card (SMU 11.0.7) or a handheld's chip is never offered it.
    fs::write(sys.p("/sys/devices/card1/ip_discovery/die/0/MP1/0/major"), "11\n").unwrap();
    fs::write(sys.p("/sys/devices/card1/ip_discovery/die/0/MP1/0/revision"), "7\n").unwrap();
    assert!(sys.apply(od("0xfff7ffff", "7.2.7-valve2-2-neptune-72")).unwrap_err().contains("RX 7000 and RX 9000"));
}

#[test]
fn an_offset_needs_tuning_on_a_range_and_an_up_to_date_helper() {
    let mut sys = rx9070("needs");
    assert!(job(&mut sys, "try", Some(-30)).unwrap_err().contains("restart"));
    // RX 6000: an offset without a range.
    fs::write(sys.p(OD), gpu::OD_RX6000).unwrap();
    assert_eq!(job(&mut sys, "try", Some(-30)).unwrap_err(), gpu::NO_RANGE);
    overdrive_on(&sys, 0);
    // Out of Turbine Hub's own range, off the 5 mV steps, another card in the slot.
    assert!(job(&mut sys, "try", Some(-155)).is_err() && job(&mut sys, "try", Some(-33)).is_err() && job(&mut sys, "try", Some(5)).is_err());
    let other = sys.gpu_job(&parse(&format!(r#"{{"do":"try","slot":"{SLOT}","device":"1002:744c","subsystem":"1eae:8811","offset":-30}}"#)).unwrap());
    assert!(other.unwrap_err().contains("isn't in this machine"));
    fs::remove_file(sys.p(gpu::GUARD_UNIT)).unwrap();
    assert!(job(&mut sys, "try", Some(-30)).unwrap_err().contains("needs an update"));
    assert!(od_calls(&sys).is_empty());
}

#[test]
fn a_try_arms_the_guard_writes_vo_then_c_and_records_nothing() {
    let mut sys = rx9070("try");
    overdrive_on(&sys, 0);
    assert_eq!(job(&mut sys, "try", Some(-30)), Ok(-30));
    assert_eq!(od_calls(&sys), vec!["od vo -30 (cap 220000000)", "od c (cap 220000000)"]);
    assert_eq!(gpu::parse_od(&read(&sys, OD)).offset, Some(-30));
    assert_eq!(gpu::parse_armed(&read(&sys, gpu::ARMED)), Some((BOOT.to_string(), 1000, -30)));
    assert!(sys.ledger().unwrap().entries.is_empty());
    // Nobody kept it: back to the card's own voltage. The mark says 0 now, which trips nothing.
    assert_eq!(job(&mut sys, "back", None), Ok(0));
    assert_eq!(gpu::parse_od(&read(&sys, OD)).offset, Some(0));
    assert_eq!(gpu::parse_armed(&read(&sys, gpu::ARMED)).map(|a| a.2), Some(0));
    fs::write(sys.p(gpu::BOOT_ID), "another-boot\n").unwrap();
    assert_eq!(sys.gpu_guard("boot"), Ok(false));
    assert!(!sys.p(gpu::TRIPPED).exists());
    fs::write(sys.p(gpu::BOOT_ID), format!("{BOOT}\n")).unwrap();
    // The driver refuses what its own range doesn't take (EINVAL), and says so plainly.
    sys.run.od_min = -50;
    let e = job(&mut sys, "try", Some(-60)).unwrap_err();
    assert!(e.starts_with("The graphics card refused -60 mV"), "{e}");
}

#[test]
fn an_offset_is_never_recorded_the_power_limit_goes_first_and_every_limit_change_commits_it_again() {
    let mut sys = rx9070("keep");
    overdrive_on(&sys, 0);
    let cap = |w: u64| set(&format!(r#"{{"kind":"sysfs","path":"{CAP}","value":"{}"}}"#, w * 1_000_000));
    // An offset is set for the session only: the record never takes one, so nothing could put it back.
    assert!(sys.apply(voltage(-30)).unwrap_err().contains("isn't recorded"));
    assert!(sys.ledger().unwrap().entries.is_empty() && od_calls(&sys).is_empty());
    sys.apply(cap(250)).unwrap();
    // Something else set the limit since (Steam, a reset): the recorded one goes in before the offset.
    fs::write(sys.p(CAP), "304000000\n").unwrap();
    assert_eq!(job(&mut sys, "try", Some(-30)), Ok(-30));
    assert_eq!(od_calls(&sys), vec!["od vo -30 (cap 250000000)", "od c (cap 250000000)"]);
    // Kernel 7.2 drops the offset when the limit changes after it: the slider's next limit commits it again.
    sys.run.calls.clear();
    sys.apply(cap(280)).unwrap();
    assert_eq!(od_calls(&sys), vec!["od vo -30 (cap 280000000)", "od c (cap 280000000)"]);
    // So does a power limit put back by Undo, and one set for the session.
    sys.run.calls.clear();
    let cap_id = sys.ledger().unwrap().find_key(&format!("sysfs:{CAP}")).unwrap().id;
    sys.undo(cap_id).unwrap();
    assert_eq!(od_calls(&sys).len(), 2);
    sys.run.calls.clear();
    sys.set_now(CAP, "260000000").unwrap();
    assert_eq!(od_calls(&sys).len(), 2);
    // Back at 0: a limit change writes no offset.
    assert_eq!(job(&mut sys, "back", None), Ok(0));
    sys.run.calls.clear();
    sys.set_now(CAP, "250000000").unwrap();
    assert!(od_calls(&sys).is_empty());
}

#[test]
fn reapply_never_writes_an_offset_at_boot_or_after_a_wake() {
    let mut sys = rx9070("boot");
    overdrive_on(&sys, 0);
    sys.apply(set(&format!(r#"{{"kind":"sysfs","path":"{CAP}","value":"250000000"}}"#))).unwrap();
    job(&mut sys, "try", Some(-30)).unwrap();
    // After a wake the card keeps its offset and its limit: nothing is written, not even the limit (on kernel 7.2
    // that would drop the offset).
    sys.run.calls.clear();
    sys.reapply().unwrap();
    assert!(od_calls(&sys).is_empty());
    assert!(!sys.run.calls.iter().any(|c| c.contains("power1_cap")));
    assert_eq!(gpu::parse_od(&read(&sys, OD)).offset, Some(-30));
    // A wake that lost the limit: it goes in again, which drops the offset, so the card is set to 0 to say so
    // truthfully. The offset itself is never written back.
    fs::write(sys.p(CAP), "304000000\n").unwrap();
    sys.reapply().unwrap();
    assert_eq!(read(&sys, CAP).trim(), "250000000");
    assert_eq!(od_calls(&sys), vec!["od vo 0 (cap 250000000)", "od c (cap 250000000)"]);
    // A restart: the card starts at 0, the boot unit puts the power limit back, never an offset.
    fs::write(sys.p(OD), gpu::od_rx9000(0)).unwrap();
    fs::write(sys.p(CAP), "304000000\n").unwrap();
    sys.run.calls.clear();
    sys.reapply().unwrap();
    assert!(od_calls(&sys).is_empty());
    assert!(!sys.verify().unwrap().iter().any(|d| d.contains("voltage")));
    // The background helper's old jobs are gone: nothing can put a value back.
    for gone in ["start", "disarm"] {
        assert!(job(&mut sys, gone, None).unwrap_err().contains("Unknown"), "{gone}");
    }
    assert!(od_calls(&sys).is_empty());
}

#[test]
fn a_stop_while_armed_trips_the_offset_and_nothing_puts_it_back() {
    let mut sys = rx9070("guard");
    overdrive_on(&sys, 0);
    job(&mut sys, "try", Some(-60)).unwrap();
    assert!(sys.p(gpu::ARMED).exists());
    // The guard's start in the same boot (the unit started again) changes nothing.
    assert_eq!(sys.gpu_guard("boot"), Ok(false));
    assert!(sys.p(gpu::ARMED).exists());
    // The machine went down: the next start has another boot id.
    fs::write(sys.p(gpu::BOOT_ID), "another-boot\n").unwrap();
    fs::write(sys.p(OD), gpu::od_rx9000(0)).unwrap();
    assert_eq!(sys.gpu_guard("boot"), Ok(true));
    assert_eq!(gpu::parse_tripped(&read(&sys, gpu::TRIPPED)).map(|t| t.0), Some(-60));
    assert!(!sys.p(gpu::ARMED).exists());
    // Nothing put it back. A new try (the player chose again) clears the trip; going back goes to what is asked.
    assert_eq!(gpu::parse_od(&read(&sys, OD)).offset, Some(0));
    assert_eq!(job(&mut sys, "try", Some(-40)), Ok(-40));
    assert!(!sys.p(gpu::TRIPPED).exists());
    assert_eq!(job(&mut sys, "back", Some(-20)), Ok(-20));
    assert_eq!(job(&mut sys, "back", None), Ok(0));
    // A clean shutdown and a sleep clear the mark: the next start trips nothing.
    job(&mut sys, "try", Some(-40)).unwrap();
    sys.gpu_guard("stop").unwrap();
    assert!(!sys.p(gpu::ARMED).exists());
    job(&mut sys, "try", Some(-45)).unwrap();
    sys.gpu_guard("sleep").unwrap();
    fs::write(sys.p(gpu::BOOT_ID), "third-boot\n").unwrap();
    assert_eq!(sys.gpu_guard("boot"), Ok(false));
    assert!(!sys.p(gpu::TRIPPED).exists());
    // Safe reset: 0, the mark and any trip gone.
    assert_eq!(job(&mut sys, "reset", None), Ok(0));
    assert!(sys.ledger().unwrap().entries.is_empty() && !sys.p(gpu::ARMED).exists());
}

#[test]
fn a_reset_card_trips_only_while_the_mark_still_names_an_offset() {
    let mut sys = rx9070("reset");
    overdrive_on(&sys, 0);
    job(&mut sys, "try", Some(-30)).unwrap();
    // While it reads -30 there is nothing to trip.
    job(&mut sys, "trip", None).unwrap();
    assert!(!sys.p(gpu::TRIPPED).exists());
    // Going back to 0 on purpose is no reset either: the mark says 0.
    job(&mut sys, "back", None).unwrap();
    job(&mut sys, "trip", None).unwrap();
    assert!(!sys.p(gpu::TRIPPED).exists());
    // The driver reset the card (a ring timeout): it reads 0 while the mark still names -30.
    job(&mut sys, "try", Some(-30)).unwrap();
    fs::write(sys.p(OD), gpu::od_rx9000(0)).unwrap();
    job(&mut sys, "trip", Some(-30)).unwrap();
    assert_eq!(gpu::parse_tripped(&read(&sys, gpu::TRIPPED)).map(|t| t.0), Some(-30));
    assert!(!sys.p(gpu::ARMED).exists());
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


#[test]
fn a_wake_rule_shows_at_once_and_undo_gives_the_wake_back() {
    let mut sys = machine("wake");
    let dev = |sys: &Sys<Fake>, port: &str, vendor: &str, product: &str| {
        let d = sys.p(&format!("/sys/bus/usb/devices/{port}"));
        fs::create_dir_all(d.join("power")).unwrap();
        fs::write(d.join("idVendor"), format!("{vendor}\n")).unwrap();
        fs::write(d.join("idProduct"), format!("{product}\n")).unwrap();
        fs::write(d.join("power/wakeup"), "enabled\n").unwrap();
    };
    dev(&sys, "1-2", "046d", "c548");
    dev(&sys, "1-3", "046d", "c548");
    dev(&sys, "1-4", "3554", "fa09");
    let wake = |sys: &Sys<Fake>, port: &str| read(sys, &format!("/sys/bus/usb/devices/{port}/power/wakeup")).trim().to_string();
    let id = sys.apply(set(r#"{"kind":"wake","vendor":"046d","product":"c548","wake":false}"#)).unwrap();
    // Both receivers of that kind stop waking the machine now, not at the next replug; the other device is left.
    assert_eq!([wake(&sys, "1-2"), wake(&sys, "1-3"), wake(&sys, "1-4")], ["disabled", "disabled", "enabled"]);
    assert_eq!(sys.ledger().unwrap().entries[0].before("value"), Some("enabled"));
    assert!(sys.verify().unwrap().is_empty());
    // Something else switched one back on: the check says the change isn't in effect.
    fs::write(sys.p("/sys/bus/usb/devices/1-3/power/wakeup"), "enabled\n").unwrap();
    assert_eq!(sys.verify().unwrap(), vec!["USB device 046d:c548 can't wake the machine isn't in effect"]);
    // Changing the rule keeps the first "before"; Undo gives the wake back and removes the rule.
    assert_eq!(sys.apply(set(r#"{"kind":"wake","vendor":"046d","product":"c548","wake":false}"#)).unwrap(), id);
    assert_eq!(wake(&sys, "1-3"), "disabled");
    sys.undo(id).unwrap();
    assert_eq!([wake(&sys, "1-2"), wake(&sys, "1-3"), wake(&sys, "1-4")], ["enabled", "enabled", "enabled"]);
    assert!(!sys.p(policy::WAKE_FILE).exists() && sys.ledger().unwrap().entries.is_empty());
    // A device that isn't plugged in: the rule is written, nothing live is touched, and Undo is just the rule.
    let id = sys.apply(set(r#"{"kind":"wake","vendor":"1234","product":"abcd","wake":false}"#)).unwrap();
    assert_eq!(sys.ledger().unwrap().entries[0].before("value"), None);
    sys.undo(id).unwrap();
    assert_eq!([wake(&sys, "1-2"), wake(&sys, "1-4")], ["enabled", "enabled"]);
}

#[test]
fn a_value_whose_folder_got_a_new_number_is_still_found_and_a_gone_one_can_be_undone() {
    let mut sys = machine("moved");
    const CAP: &str = "/sys/class/drm/card1/device/hwmon/hwmon3/power1_cap";
    let id = sys.apply(set(&format!(r#"{{"kind":"sysfs","path":"{CAP}","value":"200000000"}}"#))).unwrap();
    // The next start: the kernel calls the card's folder hwmon5, and the cap is back at its own default.
    fs::rename(sys.p("/sys/devices/card1/hwmon/hwmon3"), sys.p("/sys/devices/card1/hwmon/hwmon5")).unwrap();
    let moved = "/sys/class/drm/card1/device/hwmon/hwmon5/power1_cap";
    fs::write(sys.p(moved), "220000000\n").unwrap();
    assert_eq!(sys.verify().unwrap(), vec!["Graphics card power limit set to 200 W isn't in effect"]);
    sys.reapply().unwrap();
    assert_eq!(read(&sys, moved), "200000000");
    assert!(sys.verify().unwrap().is_empty());
    sys.undo(id).unwrap();
    assert_eq!(read(&sys, moved), "220000000");
    assert!(sys.ledger().unwrap().entries.is_empty());
    // Two folders with that file: the helper doesn't guess.
    let id = sys.apply(set(&format!(r#"{{"kind":"sysfs","path":"{moved}","value":"200000000"}}"#))).unwrap();
    fs::rename(sys.p("/sys/devices/card1/hwmon/hwmon5"), sys.p("/sys/devices/card1/hwmon/hwmon7")).unwrap();
    fs::create_dir_all(sys.p("/sys/devices/card1/hwmon/hwmon8")).unwrap();
    for f in ["power1_cap", "power1_cap_min", "power1_cap_max"] {
        fs::write(sys.p(&format!("/sys/devices/card1/hwmon/hwmon8/{f}")), "1\n").unwrap();
    }
    assert!(sys.reapply().is_err());
    assert_eq!(read(&sys, "/sys/devices/card1/hwmon/hwmon8/power1_cap"), "1\n");
    // Nothing to put back (the value's place is gone), and the change still leaves the record.
    sys.undo(id).unwrap();
    assert_eq!(read(&sys, "/sys/devices/card1/hwmon/hwmon7/power1_cap"), "200000000");
    // A battery taken out: its charge limit can still be undone.
    let id = sys.apply(set(&format!(r#"{{"kind":"sysfs","path":"{CHARGE}","value":"80"}}"#))).unwrap();
    fs::remove_dir_all(sys.p("/sys/devices/platform/bat/BAT0")).unwrap();
    sys.undo(id).unwrap();
    assert!(sys.ledger().unwrap().entries.is_empty());
    // A link out of /sys is still refused, not skipped.
    let evil = "/sys/class/power_supply/EVIL/charge_control_end_threshold";
    assert!(sys.set_now(evil, "80").is_err());
    assert_eq!(read(&sys, "/secret"), "do not touch\n");
}

#[test]
fn a_change_number_is_a_whole_number() {
    let id = |t: &str| ops::change_id(&parse(t).unwrap());
    assert_eq!(id(r#"{"id":7}"#), Ok(7));
    for bad in [r#"{"id":1.9}"#, r#"{"id":0}"#, r#"{"id":-1}"#, r#"{"id":0.5}"#, r#"{"id":1e300}"#] {
        assert_eq!(id(bad), Err("id must be a whole number".to_string()), "{bad}");
    }
    for bad in [r#"{"id":"1"}"#, r#"{}"#, r#"{"id":null}"#] {
        assert_eq!(id(bad), Err("id is missing".to_string()), "{bad}");
    }
}

#[test]
fn the_log_keeps_each_job_on_one_line() {
    let sys = machine("log");
    sys.log("1000 nope ok\n1 0 uninstall ok\r\u{1b}[2J refused: Unknown job");
    sys.log(&"x".repeat(5000));
    let text = read(&sys, ops::LOG);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert_eq!(lines[0], "1000 1000 nope ok 1 0 uninstall ok  [2J refused: Unknown job");
    assert_eq!(lines[1].len(), "1000 ".len() + 400);
    assert_eq!(ops::log_text("plain words, 100%"), "plain words, 100%");
}

#[test]
fn one_job_at_a_time_and_none_waits_for_ever() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let sys = machine("lock");
    let dir = sys.p(ops::STATE_DIR);
    // A lock file an older helper left readable by everyone (any program could hold it and stop every job).
    fs::write(dir.join("lock"), "").unwrap();
    fs::set_permissions(dir.join("lock"), fs::Permissions::from_mode(0o644)).unwrap();
    let first = ops::take_lock(&dir, Duration::ZERO).unwrap().expect("the lock");
    assert_eq!(fs::metadata(dir.join("lock")).unwrap().permissions().mode() & 0o777, 0o600);
    // A second job waits its time, then says so instead of hanging.
    let start = Instant::now();
    let busy = ops::take_lock(&dir, Duration::from_millis(150)).unwrap_err();
    assert!(busy.contains("busy"), "{busy}");
    assert!(start.elapsed() >= Duration::from_millis(150) && start.elapsed() < Duration::from_secs(5));
    // It gets the lock as soon as the first job ends.
    let waiter = std::thread::spawn({
        let dir = dir.clone();
        move || ops::take_lock(&dir, Duration::from_secs(10)).map(|l| l.is_some())
    });
    std::thread::sleep(Duration::from_millis(100));
    drop(first);
    assert_eq!(waiter.join().unwrap(), Ok(true));
    assert!(start.elapsed() < Duration::from_secs(5));
    // No folder yet (before the first install): no lock, and the job goes on.
    assert!(ops::take_lock(&sys.p("/var/lib/nowhere"), Duration::ZERO).unwrap().is_none());
    assert!(ops::LOCK_WAIT < Duration::from_secs(120));
}

#[test]
fn a_program_that_never_ends_is_stopped() {
    use std::time::{Duration, Instant};
    let start = Instant::now();
    let e = ops::run_limited("/usr/bin/sleep", &["30"], Duration::from_millis(200)).unwrap_err();
    assert_eq!(e, "sleep took too long, so it was stopped");
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(ops::run_limited("/usr/bin/printf", &["%s", "hello"], Duration::from_secs(5)), Ok("hello".to_string()));
    assert_eq!(ops::run_limited("/usr/bin/false", &[], Duration::from_secs(5)), Err("false failed: no message".to_string()));
    assert!(ops::run_limited("/usr/bin/no-such-program", &[], Duration::from_secs(5)).is_err());
    // udev and the like answer in moments; a service gets its start time; a download gets its own limit and a bit.
    assert_eq!(ops::time_limit(ops::UDEVADM, &["trigger", "--action=add"]), Duration::from_secs(20));
    assert_eq!(ops::time_limit(ops::SYSTEMCTL, &["enable", "--now", "sshd.service"]), Duration::from_secs(100));
    assert_eq!(ops::time_limit(ops::CURL, &["-fsSL", "--max-time", "600", "-o", "x", "https://github.com"]), Duration::from_secs(630));
    // Each is well inside what the app allows a job (two minutes, half an hour for a download).
    assert!(ops::time_limit(ops::EFIBOOTMGR, &[]) < Duration::from_secs(120));
}

#[test]
fn status_says_which_turbine_hub_the_helper_came_with() {
    let sys = machine("status");
    let j = parse(&sys.status().unwrap()).unwrap();
    assert_eq!(j.str_at("version"), Some(ops::VERSION));
    // Never the crate's own number, which stays the same from release to release.
    assert_ne!(ops::VERSION, env!("CARGO_PKG_VERSION"));
}

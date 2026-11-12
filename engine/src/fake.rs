//! Fake machines: a folder that looks like "/" on a known machine, for tests and the app's screenshot mode, so both
//! show real screens without real hardware. Values come from real machines (ids and serials removed).

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// A handheld: AMD APU, 7-inch 1080p built-in screen (48 to 120 Hz), battery with a charge limit, firmware
    /// power limits and RGB lights. No particular maker's device.
    Handheld,
    /// A desktop: Intel CPU, AMD RX 9070 XT, a 1440p monitor, an empty second drive, a Windows drive.
    Desktop,
    /// A living-room PC on a 55-inch TV over HDMI.
    TvPc,
}

/// The real monitor's EDID (serial blanked).
pub const MONITOR_EDID: &[u8] = include_bytes!("../tests/data/monitor-2560x1440.edid");

/// The home partition's device in every preset (the third partition of the system drive).
pub const HOME_DEV: &str = "259:3";

pub const EFI_WITH_WINDOWS: &str = "BootCurrent: 0000\nBootOrder: 0000,0003\nBoot0000* SteamOS\tHD(1,GPT,5e0c1d2a-0000-4000-8000-000000000001,0x800,0x80000)/\\EFI\\steamos\\steamcl.efi\nBoot0003* Windows Boot Manager\tHD(1,GPT,0a1b2c3d-0000-4000-8000-00000000000a,0x800,0x32000)/\\EFI\\Microsoft\\Boot\\bootmgfw.efi\n";
/// The partition the made-up Windows Boot Manager entry starts from.
pub const WINDOWS_ESP: &str = "0a1b2c3d-0000-4000-8000-00000000000a";

fn w(root: &Path, p: &str, text: &str) {
    let f = root.join(p.trim_start_matches('/'));
    if let Some(d) = f.parent() {
        let _ = fs::create_dir_all(d);
    }
    let _ = fs::write(f, text);
}

fn link(root: &Path, target: &str, at: &str) {
    let f = root.join(at.trim_start_matches('/'));
    if let Some(d) = f.parent() {
        let _ = fs::create_dir_all(d);
    }
    let _ = std::os::unix::fs::symlink(target, f);
}

/// A 128-byte EDID for a built-in handheld screen: 1920x1080, 7 inches, 48 to 120 Hz.
pub fn panel_edid() -> Vec<u8> {
    let mut b = tv_edid("Panel", 15, 9);
    // 1920x1080 at 120 Hz: 297 MHz, 2200 x 1125 total.
    let dtd = [0x04, 0x74, 0x80, 0x18, 0x71, 0x38, 0x2d, 0x40, 0x58, 0x2c, 0x45, 0x00, 0x96, 0x58, 0x00, 0x00, 0x00, 0x1e];
    b[54..72].copy_from_slice(&dtd);
    b[95] = 48;
    b[96] = 120;
    b[127] = (0u8).wrapping_sub(b[..127].iter().fold(0u8, |a, x| a.wrapping_add(*x)));
    b
}

/// A 128-byte EDID for a TV: given maker, name, size and one 3840x2160 60 Hz timing.
pub fn tv_edid(name: &str, w_cm: u8, h_cm: u8) -> Vec<u8> {
    let mut b = vec![0u8; 128];
    b[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    b[8..10].copy_from_slice(&0x4c2du16.to_be_bytes()); // "SAM"
    b[18] = 1;
    b[19] = 3;
    b[21] = w_cm;
    b[22] = h_cm;
    // 3840x2160 at 60 Hz (CTA timing: 594 MHz, 4400 x 2250 total).
    let dtd = [0x08, 0xe8, 0x00, 0x30, 0xf2, 0x70, 0x5a, 0x80, 0xb0, 0x58, 0x8a, 0x00, 0x50, 0x1d, 0x74, 0x00, 0x00, 0x1e];
    b[54..72].copy_from_slice(&dtd);
    b[72..77].copy_from_slice(&[0, 0, 0, 0xfc, 0]);
    let mut n = name.as_bytes().to_vec();
    n.push(b'\n');
    n.resize(13, b' ');
    b[77..90].copy_from_slice(&n);
    b[90..95].copy_from_slice(&[0, 0, 0, 0xfd, 0]);
    b[95] = 24;
    b[96] = 120;
    b[108..113].copy_from_slice(&[0, 0, 0, 0x10, 0]);
    b[127] = (0u8).wrapping_sub(b[..127].iter().fold(0u8, |a, x| a.wrapping_add(*x)));
    b
}

fn common(root: &Path) {
    w(root, "/run/udev/data/b259:90", &format!("E:ID_PART_ENTRY_UUID={WINDOWS_ESP}\n"));
    for p in ["policy0", "policy1"] {
        w(root, &format!("/sys/devices/system/cpu/cpufreq/{p}/energy_performance_preference"), "balance_performance\n");
    }
    w(root, "/etc/os-release", "NAME=\"SteamOS\"\nID=steamos\nVERSION_ID=3.9.5\n");
    w(root, "/sys/kernel/mm/transparent_hugepage/enabled", "always [madvise] never\n");
    w(root, "/sys/kernel/mm/transparent_hugepage/defrag", "always defer defer+madvise [madvise] never\n");
    w(root, "/proc/sys/vm/swappiness", "100\n");
    w(root, "/proc/sys/vm/page-cluster", "0\n");
    w(root, "/proc/sys/vm/max_map_count", "2147483642\n");
    w(root, "/proc/sys/kernel/split_lock_mitigate", "1\n");
}

fn drive(root: &Path, name: &str, model: &str, bytes: u64, removable: bool, dev: &str, parts: &[(&str, &str, &str, &str, &str)]) {
    let b = format!("/sys/block/{name}");
    w(root, &format!("{b}/size"), &format!("{}\n", bytes / 512));
    w(root, &format!("{b}/removable"), if removable { "1\n" } else { "0\n" });
    w(root, &format!("{b}/device/model"), &format!("{model}\n"));
    w(root, &format!("{b}/dev"), &format!("{dev}:0\n"));
    for (i, (part, fs_type, label, part_name, _)) in parts.iter().enumerate() {
        let d = format!("{dev}:{}", i + 1);
        w(root, &format!("{b}/{part}/dev"), &format!("{d}\n"));
        w(root, &format!("/run/udev/data/b{d}"), &format!("E:ID_FS_TYPE={fs_type}\nE:ID_FS_LABEL={label}\nE:ID_PART_ENTRY_NAME={part_name}\n"));
    }
    if parts.is_empty() {
        w(root, &format!("/run/udev/data/b{dev}:0"), "E:DEVTYPE=disk\nE:ID_PART_TABLE_TYPE=gpt\n");
    }
}

fn mounts(root: &Path, list: &[(&str, &str)]) {
    let text: String = list.iter().map(|(d, m)| format!("/dev/{d} {m} ext4 rw,relatime 0 0\n")).collect();
    w(root, "/proc/mounts", &text);
}

fn amd_gpu(root: &Path, card: &str, device: &str, cap: Option<(u64, u64, u64)>) {
    let dev = format!("/sys/devices/pci0000:00/{card}");
    w(root, &format!("{dev}/vendor"), "0x1002\n");
    w(root, &format!("{dev}/device"), &format!("0x{device}\n"));
    w(root, "/sys/bus/pci/drivers/amdgpu/.keep", "");
    link(root, "../../../bus/pci/drivers/amdgpu", &format!("{dev}/driver"));
    if let Some((min, max, now)) = cap {
        w(root, &format!("{dev}/hwmon/hwmon6/power1_cap_min"), &format!("{min}\n"));
        w(root, &format!("{dev}/hwmon/hwmon6/power1_cap_max"), &format!("{max}\n"));
        w(root, &format!("{dev}/hwmon/hwmon6/power1_cap"), &format!("{now}\n"));
    }
    fs::create_dir_all(root.join(format!("sys/class/drm/{card}"))).ok();
    link(root, &format!("../../../devices/pci0000:00/{card}"), &format!("/sys/class/drm/{card}/device"));
}

fn connector(root: &Path, name: &str, connected: bool, edid: Option<&[u8]>) {
    let b = format!("/sys/class/drm/{name}");
    w(root, &format!("{b}/status"), if connected { "connected\n" } else { "disconnected\n" });
    if let Some(e) = edid {
        let f = root.join(format!("sys/class/drm/{name}/edid"));
        let _ = fs::write(f, e);
    }
}

fn usb(root: &Path, port: &str, vendor: &str, product: &str, name: &str) {
    let d = format!("/sys/bus/usb/devices/{port}");
    w(root, &format!("{d}/idVendor"), &format!("{vendor}\n"));
    w(root, &format!("{d}/idProduct"), &format!("{product}\n"));
    w(root, &format!("{d}/product"), &format!("{name}\n"));
    w(root, &format!("{d}/power/wakeup"), "enabled\n");
    w(root, &format!("{d}/removable"), "removable\n");
}

/// Writes the preset's machine under `root`.
pub fn machine(root: &Path, preset: Preset) {
    let _ = fs::remove_dir_all(root);
    common(root);
    match preset {
        Preset::Handheld => {
            w(root, "/sys/class/dmi/id/sys_vendor", "Example Devices\n");
            w(root, "/sys/class/dmi/id/product_name", "Handheld One HH1A_HH1A\n");
            w(root, "/sys/class/dmi/id/chassis_type", "31\n");
            w(root, "/proc/cpuinfo", "model name\t: AMD Ryzen Z2 Extreme\n");
            w(root, "/proc/meminfo", "MemTotal:       24300000 kB\n");
            amd_gpu(root, "card1", "150e", None);
            connector(root, "card1-eDP-1", true, Some(&panel_edid()));
            connector(root, "card1-DP-1", false, None);
            let bat = "/sys/class/power_supply/BAT0";
            w(root, &format!("{bat}/type"), "Battery\n");
            w(root, &format!("{bat}/capacity"), "78\n");
            w(root, &format!("{bat}/charge_control_end_threshold"), "100\n");
            let fw = "/sys/class/firmware-attributes/platform-attributes/attributes";
            for (a, min, max, now) in [("ppt_pl1_spl", 7, 35, 17), ("ppt_pl2_sppt", 7, 37, 25), ("ppt_pl3_fppt", 7, 40, 30)] {
                w(root, &format!("{fw}/{a}/min_value"), &format!("{min}\n"));
                w(root, &format!("{fw}/{a}/max_value"), &format!("{max}\n"));
                w(root, &format!("{fw}/{a}/current_value"), &format!("{now}\n"));
            }
            w(root, "/sys/devices/system/cpu/cpufreq/policy0/energy_performance_available_preferences", "default performance balance_performance balance_power power\n");
            // Joystick rings like the ROG Ally's: four parts (two per stick), red, green and blue each.
            let rings = "/sys/class/leds/ally:rgb:joystick_rings";
            w(root, &format!("{rings}/brightness"), "128\n");
            w(root, &format!("{rings}/max_brightness"), "255\n");
            w(root, &format!("{rings}/multi_index"), "red green blue red green blue red green blue red green blue\n");
            w(root, &format!("{rings}/multi_intensity"), "255 0 0 255 0 0 255 0 0 255 0 0\n");
            drive(root, "nvme0n1", "NVMe SSD 1TB", 1_000_204_886_016, false, "259", &[("nvme0n1p1", "vfat", "esp", "esp", ""), ("nvme0n1p4", "btrfs", "rootfs", "rootfs-A", ""), ("nvme0n1p8", "ext4", "home", "home", "")]);
            drive(root, "mmcblk0", "SD 512GB", 512_000_000_000, true, "179", &[("mmcblk0p1", "ext4", "", "", "")]);
            mounts(root, &[("mmcblk0p1", "/run/media/deck/sdcard")]);
            usb(root, "3-1", "045e", "028e", "Controller");
            // Wired inside the case: the kernel calls its port fixed.
            w(root, "/sys/bus/usb/devices/3-1/removable", "fixed\n");
        }
        Preset::Desktop | Preset::TvPc => {
            w(root, "/sys/class/dmi/id/sys_vendor", "Micro-Star International Co., Ltd.\n");
            w(root, "/sys/class/dmi/id/product_name", "MS-7E07\n");
            w(root, "/sys/class/dmi/id/chassis_type", "3\n");
            w(root, "/proc/cpuinfo", "model name\t: 13th Gen Intel(R) Core(TM) i9-13900K\n");
            w(root, "/proc/meminfo", "MemTotal:       65500792 kB\n");
            w(root, "/sys/devices/system/cpu/cpufreq/policy0/energy_performance_available_preferences", "default performance balance_performance balance_power power\n");
            amd_gpu(root, "card1", "7550", Some((150_000_000, 304_000_000, 304_000_000)));
            if preset == Preset::Desktop {
                connector(root, "card1-DP-4", true, Some(MONITOR_EDID));
                connector(root, "card1-HDMI-A-5", false, None);
            } else {
                connector(root, "card1-DP-4", false, None);
                connector(root, "card1-HDMI-A-5", true, Some(&tv_edid("TV 55", 121, 68)));
            }
            drive(root, "nvme1n1", "SPCC M.2 PCIe SSD", 4_096_805_658_624, false, "259", &[("nvme1n1p1", "vfat", "esp", "esp", ""), ("nvme1n1p4", "btrfs", "rootfs", "rootfs-A", ""), ("nvme1n1p8", "ext4", "home", "home", "")]);
            drive(root, "nvme0n1", "Samsung SSD 990 PRO 2TB", 2_000_398_934_016, false, "260", &[]);
            drive(root, "sda", "Samsung SSD 870", 1_000_204_886_016, false, "8", &[("sda1", "vfat", "", "EFI system partition", ""), ("sda2", "", "", "Microsoft reserved partition", ""), ("sda3", "ntfs", "Windows", "Basic data partition", "")]);
            mounts(root, &[]);
            usb(root, "1-6.4.1", "3554", "fa09", "2.4G Wireless Receiver");
            usb(root, "1-6.4.3", "089d", "062f", "Wireless-Receiver");
        }
    }
}

/// A mounted Windows drive with two Steam libraries, a half-downloaded game, some apps and a user folder.
pub fn windows_drive(mount: &Path) {
    let _ = fs::remove_dir_all(mount);
    let acf = |lib: &str, id: &str, name: &str, dir: &str, flags: &str, size: &str| {
        w(mount, &format!("{lib}/appmanifest_{id}.acf"), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"{dir}\"\n\t\"SizeOnDisk\"\t\t\"{size}\"\n}}\n"));
        w(mount, &format!("{lib}/common/{dir}/game.exe"), "MZ");
    };
    acf("Program Files (x86)/Steam/steamapps", "1086940", "Baldur's Gate 3", "Baldurs Gate 3", "4", "120000000000");
    acf("Program Files (x86)/Steam/steamapps", "1145360", "Hades II", "Hades II", "4", "11000000000");
    acf("SteamLibrary/steamapps", "632470", "Disco Elysium", "Disco Elysium", "4", "21000000000");
    acf("SteamLibrary/steamapps", "1172470", "Apex Legends", "Apex Legends", "1026", "68000000000");
    acf("SteamLibrary/steamapps", "999", "Sneaky", "../../Windows", "4", "1");
    w(mount, "Users/Public/Desktop/x.lnk", "");
    w(mount, "Users/Sam/AppData/Local/Discord/Update.exe", "MZ");
    w(mount, "Users/Sam/AppData/Roaming/Spotify/Spotify.exe", "MZ");
    w(mount, "Program Files (x86)/MSI Afterburner/MSIAfterburner.exe", "MZ");
    w(mount, "Program Files (x86)/Battle.net/Battle.net.exe", "MZ");
    w(mount, "Windows/System32/ntoskrnl.exe", "MZ");
}

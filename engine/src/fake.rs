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
    w(root, turbinehub_helper::gpu::OSRELEASE, "7.2.7-valve2-2-neptune-72\n");
    w(root, turbinehub_helper::gpu::BOOT_ID, "5e0c1d2a-0000-4000-8000-0000000000b1\n");
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

/// An AMD graphics card in PCI slot `slot` on amdgpu: its ids, the version of its power firmware (which names the
/// family), the power limit (min, max, now, the card's own default) and the driver's feature mask with overdrive
/// off, as this RX 9070 XT's real machine has them.
fn amd_gpu(root: &Path, card: &str, slot: &str, ids: (&str, &str), mp1: (u32, u32, u32), cap: Option<(u64, u64, u64, u64)>) {
    let dev = format!("/sys/devices/pci0000:00/{slot}");
    w(root, &format!("{dev}/vendor"), "0x1002\n");
    w(root, &format!("{dev}/device"), &format!("0x{}\n", ids.0));
    let (sv, sd) = ids.1.split_once(':').unwrap_or(("", ""));
    w(root, &format!("{dev}/subsystem_vendor"), &format!("0x{sv}\n"));
    w(root, &format!("{dev}/subsystem_device"), &format!("0x{sd}\n"));
    for (f, v) in [("major", mp1.0), ("minor", mp1.1), ("revision", mp1.2)] {
        w(root, &format!("{dev}/ip_discovery/die/0/MP1/0/{f}"), &format!("{v}\n"));
    }
    w(root, "/sys/bus/pci/drivers/amdgpu/.keep", "");
    link(root, "../../../bus/pci/drivers/amdgpu", &format!("{dev}/driver"));
    link(root, &format!("../../../devices/pci0000:00/{slot}"), &format!("/sys/bus/pci/devices/{slot}"));
    if let Some((min, max, now, default)) = cap {
        w(root, &format!("{dev}/hwmon/hwmon6/power1_cap_min"), &format!("{min}\n"));
        w(root, &format!("{dev}/hwmon/hwmon6/power1_cap_max"), &format!("{max}\n"));
        w(root, &format!("{dev}/hwmon/hwmon6/power1_cap"), &format!("{now}\n"));
        w(root, &format!("{dev}/hwmon/hwmon6/power1_cap_default"), &format!("{default}\n"));
    }
    fs::create_dir_all(root.join(format!("sys/class/drm/{card}"))).ok();
    link(root, &format!("../../../devices/pci0000:00/{slot}"), &format!("/sys/class/drm/{card}/device"));
    w(root, turbinehub_helper::gpu::MASK_FILE, "0xfff7bfff\n");
}

/// The made-up desktop's graphics card slot.
pub const GPU_SLOT: &str = "0000:03:00.0";

/// The made-up desktop's RX 9070 XT in one of undervolting's states (see `ui::sample`): "on" (overdrive on after the
/// restart, the card at 0 mV), "kept" (on, at `offset`), "tripped" (on, back at 0 after a stop). Anything else
/// leaves overdrive off.
pub fn undervolt(root: &Path, state: &str, offset: i32) {
    let live = match state {
        "on" | "tripped" => 0,
        "kept" => offset,
        _ => return,
    };
    w(root, turbinehub_helper::gpu::MASK_FILE, "0xfff7ffff\n");
    w(root, &format!("/sys/devices/pci0000:00/{GPU_SLOT}/pp_od_clk_voltage"), &turbinehub_helper::gpu::od_rx9000(live));
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
            // A handheld chip (APU): its power firmware is none of the cards undervolting is offered on.
            amd_gpu(root, "card1", "0000:c4:00.0", ("150e", "1043:1f63"), (14, 0, 0), None);
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
            amd_gpu(root, "card1", GPU_SLOT, ("7550", "1eae:8811"), (14, 0, 3), Some((150_000_000, 340_000_000, 304_000_000, 304_000_000)));
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

/// A PNG of one flat color (not compressed), so the made-up pictures really are pictures.
pub fn png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    // Each row: a zero (no filter), then its pixels.
    let row: Vec<u8> = std::iter::once(0).chain(std::iter::repeat_n(rgb, width as usize).flatten()).collect();
    let raw = row.repeat(height as usize);
    // zlib with stored blocks of at most 65535 bytes, then the Adler-32 of the data.
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend((b.len() as u16).to_le_bytes());
        z.extend((!(b.len() as u16)).to_le_bytes());
        z.extend(*b);
    }
    let (mut a, mut s) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65_521;
        s = (s + a) % 65_521;
    }
    z.extend(((s << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crate::zip::crc32(&body).to_be_bytes());
    };
    let mut head = Vec::new();
    head.extend(width.to_be_bytes());
    head.extend(height.to_be_bytes());
    head.extend([8, 2, 0, 0, 0]);
    chunk(b"IHDR", &head);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

fn bytes(base: &Path, rel: &str, data: &[u8]) {
    let f = base.join(rel.trim_start_matches('/'));
    if let Some(d) = f.parent() {
        let _ = fs::create_dir_all(d);
    }
    let _ = fs::write(f, data);
}

/// The made-up SD card (or second drive), as the machine's own path.
pub const FILES_DRIVE: &str = "/run/media/deck/sdcard";
/// The made-up home folder, as the machine's own path.
pub const FILES_HOME: &str = "/home/deck";
/// The Flatpak apps of the made-up machine: (id, name, installed for everyone).
pub const FILES_APPS: &[(&str, &str, bool)] = &[
    ("com.discordapp.Discord", "Discord", true),
    ("com.heroicgameslauncher.hgl", "Heroic Games Launcher", true),
    ("org.mozilla.firefox", "Firefox", true),
    ("com.vysp3r.ProtonPlus", "ProtonPlus", false),
];

/// What the file explorer needs to show a believable machine, the same on the handheld, the desktop and the TV PC:
/// the player's folders with a few files in them, two games with their pretend Windows drives, a game that runs
/// without Windows with its saves, a few Flatpak apps, a card with a Steam library on it and things in both Recycle
/// Bins. `root` is the folder `machine` wrote (it stands for "/") and `home` the folder that stands for /home/deck.
/// The Steam library list is written new, with the card's library in it.
pub fn files(root: &Path, home: &Path) {
    use std::os::unix::fs::MetadataExt;
    let _ = fs::create_dir_all(home);
    let uid = fs::metadata(home).map(|m| m.uid()).unwrap_or(1000);
    w(home, ".config/user-dirs.dirs", "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\nXDG_DOCUMENTS_DIR=\"$HOME/Documents\"\nXDG_MUSIC_DIR=\"$HOME/Music\"\nXDG_PICTURES_DIR=\"$HOME/Pictures\"\nXDG_VIDEOS_DIR=\"$HOME/Videos\"\n");
    w(home, "Desktop/Return to Gaming Mode.desktop", "[Desktop Entry]\nType=Application\nName=Return to Gaming Mode\nExec=steamos-session-select gamescope\n");
    w(home, "Desktop/Turbine Hub.desktop", "[Desktop Entry]\nType=Application\nName=Turbine Hub\nExec=flatpak run io.github.ravenor907.TurbineHub\n");
    w(home, "Documents/My setup.json", "{\"mode\":\"guided\",\"picks\":[\"heroic\",\"discord\"]}\n");
    w(home, "Documents/Controller layout notes.txt", "Back paddles: L4 jump, R4 dodge.\nGyro only while aiming.\n");
    bytes(home, "Downloads/TurbineHub.flatpak", &[b"flatpak\0".as_slice(), &[7u8; 3000]].concat());
    w(home, "Downloads/NonSteamLaunchers-install.log", &"Installing Battle.net... done\n".repeat(40));
    bytes(home, "Downloads/bg3-mod-fixer.zip", &[b"PK\x03\x04".as_slice(), &[0u8; 5000]].concat());
    bytes(home, "Downloads/wallpaper-jade.png", &png(96, 60, [0x1f, 0x9d, 0x7a]));
    bytes(home, "Pictures/wallpaper-jade.png", &png(96, 60, [0x1f, 0x9d, 0x7a]));
    bytes(home, "Pictures/Screenshots/Baldurs Gate 3 camp.png", &png(64, 40, [0x4a, 0x35, 0x2a]));
    bytes(home, "Pictures/Screenshots/Night City.png", &png(64, 40, [0xe8, 0xd2, 0x1d]));
    let _ = fs::create_dir_all(home.join("Music"));
    let _ = fs::create_dir_all(home.join("Videos/Steam recordings"));

    // Steam: two games that run through Proton on the internal drive, each with its pretend Windows.
    let steam = home.join(".local/share/Steam");
    let card = root.join(FILES_DRIVE.trim_start_matches('/'));
    let acf = |lib: &Path, id: &str, name: &str, dir: &str| w(lib, &format!("steamapps/appmanifest_{id}.acf"), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{dir}\"\n\t\"buildid\"\t\t\"1\"\n}}\n"));
    w(&steam, "steamapps/libraryfolders.vdf", &format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n\"1\"\n{{\n\"path\" \"{}\"\n}}\n}}\n", steam.display(), card.display()));
    let _ = fs::create_dir_all(steam.join("userdata/1001/config"));
    for (id, name, dir, exe, save) in [
        ("1086940", "Baldur's Gate 3", "Baldurs Gate 3", "bin/bg3.exe", "users/steamuser/AppData/Local/Larian Studios/Baldur's Gate 3/PlayerProfiles/Public/Savegames/Story/Tav-1/Tav-1.lsv"),
        ("1091500", "Cyberpunk 2077", "Cyberpunk 2077", "bin/x64/Cyberpunk2077.exe", "users/steamuser/Saved Games/CD Projekt Red/Cyberpunk 2077/ManualSave-3/sav.dat"),
    ] {
        acf(&steam, id, name, dir);
        w(&steam, &format!("steamapps/common/{dir}/{exe}"), "MZ");
        w(&steam, &format!("steamapps/common/{dir}/game.exe"), "MZ");
        let c = format!("steamapps/compatdata/{id}/pfx/drive_c");
        w(&steam, &format!("{c}/{save}"), "save");
        w(&steam, &format!("{c}/windows/system32/kernel32.dll"), "MZ");
        let _ = fs::create_dir_all(steam.join(format!("{c}/Program Files")));
    }
    // The card: a Steam library with a game that runs without Windows, and the player's own folders.
    acf(&card, "892970", "Valheim", "Valheim");
    w(&card, "steamapps/common/Valheim/valheim.x86_64", "\x7fELF");
    w(&card, "steamapps/common/Valheim/valheim_Data/boot.config", "gfx-enable-native-gfx-jobs=\n");
    w(home, ".config/unity3d/IronGate/Valheim/worlds_local/Midgard.fwl", "world");
    w(&card, "Movies/Holiday 2025.mp4", "\0\0\0\x18ftypmp42");
    w(&card, "ROMs/readme.txt", "Put your own game backups here.\n");

    // Flatpak apps, laid out the way Flatpak lays them out.
    for (id, name, system) in FILES_APPS {
        let base = if *system { root.join("var/lib/flatpak/app") } else { home.join(".local/share/flatpak/app") };
        let build = "x86_64/stable/5c1e0a7d9b3f4e6a8c2d0f1b3a5c7e9d1f2a4b6c8d0e2f4a6b8c0d2e4f6a8b0c";
        let short = id.rsplit('.').next().unwrap_or(id).to_lowercase();
        w(&base, &format!("{id}/{build}/files/bin/{short}"), "\x7fELF");
        w(&base, &format!("{id}/{build}/export/share/applications/{id}.desktop"), &format!("[Desktop Entry]\nType=Application\nName={name}\nExec={short}\n"));
        w(&base, &format!("{id}/{build}/metadata"), &format!("[Application]\nname={id}\n"));
        link(&base, "x86_64/stable", &format!("{id}/current"));
        link(&base, build.rsplit('/').next().unwrap_or(""), &format!("{id}/x86_64/stable/active"));
        w(home, &format!(".var/app/{id}/config/settings.json"), "{}\n");
    }

    // Both Recycle Bins, written the way the Trash specification says.
    let bin = home.join(".local/share/Trash");
    bytes(&bin, "files/old-screenshot.png", &png(64, 40, [0x30, 0x30, 0x38]));
    w(&bin, "info/old-screenshot.png.trashinfo", &format!("[Trash Info]\nPath={FILES_HOME}/Pictures/Screenshots/old-screenshot.png\nDeletionDate=2026-11-12T08:15:00\n"));
    w(&bin, "files/notes (copy).txt", "An older copy.\n");
    w(&bin, "info/notes (copy).txt.trashinfo", &format!("[Trash Info]\nPath={FILES_HOME}/Documents/notes%20%28copy%29.txt\nDeletionDate=2026-11-09T19:02:41\n"));
    let card_bin = card.join(format!(".Trash-{uid}"));
    w(&card_bin, "files/old-rom.zip", "PK\x03\x04");
    w(&card_bin, "info/old-rom.zip.trashinfo", "[Trash Info]\nPath=ROMs/old-rom.zip\nDeletionDate=2026-11-01T12:00:00\n");
}

/// The moment the storage saver's made-up leftovers are dated from (the same Thursday the explorer's tests use).
pub const STORAGE_NOW: u64 = 1_794_475_865;

/// Leftovers for the storage saver, on top of `files`: shader caches, Windows files and downloads of games that
/// are gone (Hades II, app 1145360, and an app nothing remembers, 570), a shader cache and an unfinished update
/// of Baldur's Gate 3, a reset backup of Cyberpunk 2077, four Protons (Steam's choice for all games, one a game
/// is set to, and one of Steam's and one added by hand that nothing uses), Steam's old logs and dumps, what a
/// removed app left in ~/.var/app, and a big old file in Downloads.
pub fn storage(root: &Path, home: &Path) {
    let old = |base: &Path, rel: &str, data: &[u8], days: u64| {
        bytes(base, rel, data);
        if let Ok(f) = fs::File::options().write(true).open(base.join(rel.trim_start_matches('/'))) {
            let _ = f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(STORAGE_NOW - days * 86_400));
        }
    };
    let steam = home.join(".local/share/Steam");
    let card = root.join(FILES_DRIVE.trim_start_matches('/'));
    let apps = "steamapps";
    // Games that are gone.
    bytes(&steam, &format!("{apps}/shadercache/1145360/fozpipelinesv6/steam_pipeline_cache.foz"), &[1u8; 6000]);
    bytes(&steam, &format!("{apps}/shadercache/570/fozmediav1/video.foz"), &[1u8; 2500]);
    bytes(&steam, &format!("{apps}/compatdata/1145360/pfx/drive_c/users/steamuser/Saved Games/Hades II/Profile1.sav"), &[2u8; 1200]);
    bytes(&steam, &format!("{apps}/compatdata/1145360/version"), b"11.0-100\n");
    bytes(&steam, &format!("{apps}/downloading/1145360/Content/Game.pkg"), &[3u8; 9000]);
    bytes(&card, &format!("{apps}/shadercache/632470/fozpipelinesv6/steam_pipeline_cache.foz"), &[1u8; 4000]);
    // Games that are here.
    bytes(&steam, &format!("{apps}/shadercache/1086940/fozpipelinesv6/steam_pipeline_cache.foz"), &[1u8; 7000]);
    bytes(&steam, &format!("{apps}/downloading/1086940/patch.pkg"), &[3u8; 5000]);
    bytes(&steam, &format!("{apps}/temp/1086940/chunk.tmp"), &[3u8; 300]);
    bytes(&steam, &format!("{apps}/compatdata/1091500/pfx.turbinehub-backup-1794000000/drive_c/users/steamuser/Saved Games/CD Projekt Red/Cyberpunk 2077/ManualSave-1/sav.dat"), &[2u8; 800]);
    // Protons: Steam's own with their manifests, and two added by hand.
    let proton = |base: &Path, dir: &str| {
        w(base, &format!("{dir}/proton"), "#!/usr/bin/env python3\n");
        w(base, &format!("{dir}/toolmanifest.vdf"), "\"manifest\"\n{\n\t\"version\"\t\t\"2\"\n\t\"commandline\"\t\t\"/proton %verb%\"\n}\n");
        bytes(base, &format!("{dir}/files/bin/wine64"), &[4u8; 3000]);
    };
    for (id, name) in [("3658110", "Proton 11.0"), ("2805730", "Proton 9.0")] {
        proton(&steam, &format!("{apps}/common/{name}"));
        w(&steam, &format!("{apps}/appmanifest_{id}.acf"), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{name}\"\n}}\n"));
    }
    for name in ["GE-Proton10-4", "GE-Proton10-12"] {
        proton(&steam, &format!("compatibilitytools.d/{name}"));
        w(&steam, &format!("compatibilitytools.d/{name}/compatibilitytool.vdf"), &format!("\"compatibilitytools\"\n{{\n\t\"compat_tools\"\n\t{{\n\t\t\"{name}\"\n\t\t{{\n\t\t\t\"install_path\"\t\t\".\"\n\t\t\t\"display_name\"\t\t\"{name}\"\n\t\t}}\n\t}}\n}}\n"));
    }
    w(&steam, "config/config.vdf", "\"InstallConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"CompatToolMapping\"\n\t\t\t\t{\n\t\t\t\t\t\"0\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"name\"\t\t\"proton_11\"\n\t\t\t\t\t}\n\t\t\t\t\t\"1091500\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"name\"\t\t\"GE-Proton10-12\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n");
    // Steam's own notes.
    old(&steam, "logs/console_log.previous.txt", &[5u8; 1500], 3);
    old(&steam, "logs/shader_log.txt", &[5u8; 700], 40);
    old(&steam, "logs/console_log.txt", &[5u8; 900], 0);
    old(&steam, "dumps/crash_20260901.dmp", &[6u8; 2200], 70);
    old(&steam, "dumps/crash_now.dmp", &[6u8; 100], 0);
    // An app that is gone, and a big old download.
    w(home, ".var/app/com.example.OldLauncher/config/settings.json", "{\"library\":\"/home/deck/Games\"}\n");
    bytes(home, ".var/app/com.example.OldLauncher/data/games/save.dat", &[2u8; 1700]);
    old(home, "Downloads/Old game installer.iso", &[7u8; 20_000], 200);
    old(home, "Downloads/Fresh installer.iso", &[7u8; 20_000], 2);
}

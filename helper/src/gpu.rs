//! Graphics card undervolting: a voltage offset on Radeon RX 7000 and RX 9000 cards. What the amdgpu driver prints
//! in pp_od_clk_voltage, which cards Turbine Hub offers it on, and the crash guard's two small files. Shared by the
//! helper (which checks and writes) and the engine (which reads and shows).
//!
//! The offset only exists while the driver's overdrive is on (bit 0x4000 of its ppfeaturemask, set from a modprobe
//! options line and a restart). The file is read by section header, never by position: kernel 7.0 moved OD_RANGE in
//! front of OD_VDDGFX_OFFSET. RX 6000 cards print an offset but no range, and their driver checks none: they are
//! refused.

/// The driver's overdrive bit in ppfeaturemask.
pub const OVERDRIVE_BIT: u32 = 0x4000;
/// Turbine Hub's own lowest offset, whatever range the card names.
pub const FLOOR_MV: i32 = -150;
/// The slider's step, and the only offsets taken.
pub const STEP_MV: i32 = 5;
/// Below this the slider warns once more, and below `DEEP_MV` again.
pub const WARN_MV: i32 = -50;
pub const DEEP_MV: i32 = -100;

/// Written and flushed to disk before every write of an offset: "boot_id time offset". Found at the next start with
/// another boot's id, it means the machine went down while a new offset was on.
pub const ARMED: &str = "/var/lib/turbinehub/gpu-armed";
/// The offset that was turned off because the machine stopped (or the driver reset the card): "offset time". The
/// app says so, and doesn't offer that value again.
pub const TRIPPED: &str = "/var/lib/turbinehub/gpu-tripped";
pub const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";
pub const OSRELEASE: &str = "/proc/sys/kernel/osrelease";
pub const MASK_FILE: &str = "/sys/module/amdgpu/parameters/ppfeaturemask";
/// The guard's units: one notices a stop while armed (and clears the mark on a clean shutdown), the other clears it
/// before a sleep.
pub const GUARD_UNIT: &str = "/etc/systemd/system/turbinehub-gpu-guard.service";
pub const SLEEP_UNIT: &str = "/etc/systemd/system/turbinehub-gpu-sleep.service";

// Nothing ever puts an offset back by itself, at a start, a login or a wake: the owner turned that off until real
// testing is done. A value is tried and checked by the app for the session only (engine `uvcheck`), and the crash
// mark stays until a clean shutdown, a sleep, or the value going back to 0.

/// The power firmware (SMU) versions of the cards offered: RX 7000 (13.0.0, 13.0.7 and 13.0.10) and RX 9000
/// (14.0.2 and 14.0.3). Their driver code prints the offset's range and checks every value against it. Handheld and
/// laptop chips (APUs) have other versions and are never offered: their limits are the maker's.
pub const FAMILIES: &[(u32, u32, u32)] = &[(13, 0, 0), (13, 0, 7), (13, 0, 10), (14, 0, 2), (14, 0, 3)];

pub fn family_ok(mp1: Option<(u32, u32, u32)>) -> bool {
    mp1.is_some_and(|v| FAMILIES.contains(&v))
}

/// The card family's name for the player.
pub fn family_name(mp1: Option<(u32, u32, u32)>) -> &'static str {
    match mp1 {
        Some((13, ..)) => "RX 7000",
        Some((14, ..)) => "RX 9000",
        _ => "",
    }
}

/// What pp_od_clk_voltage says about the voltage offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Od {
    /// The offset in effect as the driver holds it (mV).
    pub offset: Option<i32>,
    /// The card's own lowest and highest offset (mV), from OD_RANGE. RX 6000 cards print none.
    pub range: Option<(i32, i32)>,
}

impl Od {
    /// The offsets Turbine Hub takes on this card: the card's range, never below Turbine Hub's floor or above 0.
    pub fn allowed(&self) -> Option<(i32, i32)> {
        let (min, max) = self.range?;
        let lo = round_up(min.max(FLOOR_MV));
        let hi = max.min(0);
        (lo <= hi && self.offset.is_some()).then_some((lo, hi))
    }

    /// Is this offset one Turbine Hub writes on this card? Plain words when it isn't.
    pub fn check(&self, offset: i32) -> Result<(), String> {
        let Some((lo, hi)) = self.allowed() else { return Err(NO_RANGE.into()) };
        if offset % STEP_MV != 0 {
            return Err(format!("The offset goes in steps of {STEP_MV} mV"));
        }
        if offset < lo || offset > hi {
            return Err(format!("This card takes {lo} to {hi} mV"));
        }
        Ok(())
    }
}

/// Why a card without a range is refused.
pub const NO_RANGE: &str = "This card doesn't say how far its voltage may go, so Turbine Hub won't change it. Undervolting is for RX 7000 and RX 9000 cards only.";

fn round_up(v: i32) -> i32 {
    // The first step at or above v (offsets are negative).
    let r = v.rem_euclid(STEP_MV);
    if r == 0 { v } else { v + (STEP_MV - r) }
}

/// "-50mV", "-200mv", "0 mV" as a number.
fn mv(s: &str) -> Option<i32> {
    let l = s.trim().to_ascii_lowercase();
    l.strip_suffix("mv")?.trim().parse().ok()
}

/// Reads the offset and its range from pp_od_clk_voltage, by section header.
pub fn parse_od(text: &str) -> Od {
    let mut od = Od::default();
    let mut section = "";
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("OD_") && t.ends_with(':') {
            section = t.trim_end_matches(':');
            continue;
        }
        match section {
            "OD_VDDGFX_OFFSET" if od.offset.is_none() => od.offset = mv(t),
            "OD_RANGE" => {
                if let Some(rest) = t.strip_prefix("VDDGFX_OFFSET:") {
                    let v: Vec<i32> = rest.split_whitespace().filter_map(mv).collect();
                    if let [lo, hi] = v[..] {
                        od.range = Some((lo.min(hi), lo.max(hi)));
                    }
                }
            }
            _ => {}
        }
    }
    od
}

/// The driver's feature mask: "0xfff7bfff" (or a plain number).
pub fn parse_mask(text: &str) -> Option<u32> {
    let t = text.trim();
    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => t.parse().ok(),
    }
}

pub fn mask_text(mask: u32) -> String {
    format!("0x{mask:08x}")
}

/// A PCI slot as the kernel names it: "0000:03:00.0".
pub fn slot_ok(s: &str) -> bool {
    let b = s.as_bytes();
    let hex = |r: std::ops::Range<usize>| b[r].iter().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
    b.len() == 12 && b[4] == b':' && b[7] == b':' && b[10] == b'.' && hex(0..4) && hex(5..7) && hex(8..10) && (b'0'..=b'7').contains(&b[11])
}

/// Two four-digit lower-case hex ids: "1002:7550".
pub fn ids_ok(s: &str) -> bool {
    s.len() == 9 && s.split_once(':').is_some_and(|(a, b)| [a, b].iter().all(|x| x.len() == 4 && x.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())))
}

/// A kernel release: "7.2.7-valve2-2-neptune-72-g31504582a073".
pub fn release_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"._+-".contains(&c))
}

pub fn armed_text(boot: &str, time: u64, offset: i32) -> String {
    format!("{} {time} {offset}\n", boot.trim())
}

/// (boot id, time, offset) from the crash mark.
pub fn parse_armed(text: &str) -> Option<(String, u64, i32)> {
    let mut f = text.split_whitespace();
    Some((f.next()?.to_string(), f.next()?.parse().ok()?, f.next()?.parse().ok()?))
}

pub fn tripped_text(offset: i32, time: u64) -> String {
    format!("{offset} {time}\n")
}

/// (offset, time) from the tripped file.
pub fn parse_tripped(text: &str) -> Option<(i32, u64)> {
    let mut f = text.split_whitespace();
    Some((f.next()?.parse().ok()?, f.next()?.parse().ok()?))
}

pub fn guard_unit_text(bin: &str) -> String {
    format!(
        "[Unit]\nDescription=Turbine Hub: notices when the machine stopped while a new graphics card voltage was on\nAfter=local-fs.target\n\n\
         [Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart={bin} gpu-guard boot\nExecStop={bin} gpu-guard stop\n\n\
         [Install]\nWantedBy=multi-user.target\n"
    )
}

pub fn sleep_unit_text(bin: &str) -> String {
    format!("[Unit]\nDescription=Turbine Hub: a sleep is not a crash for the graphics card voltage guard\nBefore=sleep.target\n\n[Service]\nType=oneshot\nExecStart={bin} gpu-guard sleep\n\n[Install]\nWantedBy=sleep.target\n")
}

/// Real-format pp_od_clk_voltage text (from kernel v7.2 and earlier sources), for the tests and the made-up machine:
/// an RX 9070 XT on kernel 7.2 (SMU 14.0.2 family): OD_RANGE comes before OD_VDDGFX_OFFSET.
pub fn od_rx9000(offset: i32) -> String {
    format!("OD_SCLK_OFFSET:\n0Mhz\nOD_MCLK:\n0: 97Mhz\n1: 1258MHz\nOD_RANGE:\nSCLK_OFFSET:    -500Mhz       1000Mhz\nMCLK:      97Mhz       1500Mhz\nVDDGFX_OFFSET:    -200mv          0mv\nOD_VDDGFX_OFFSET:\n{offset}mV\nOD_FAN_CURVE:\n0: 0C 0%\n")
}
/// The same card before kernel 7.0: OD_VDDGFX_OFFSET before OD_RANGE.
pub const OD_RX9000_V612: &str = "OD_SCLK_OFFSET:\n0Mhz\nOD_MCLK:\n0: 97Mhz\n1: 1258MHz\nOD_VDDGFX_OFFSET:\n0mV\nOD_RANGE:\nSCLK_OFFSET:    -500Mhz       1000Mhz\nMCLK:      97Mhz       1500Mhz\nVDDGFX_OFFSET:    -200mv          0mv\n";
/// RX 7900 XTX (SMU 13.0.0) on kernel 6.8.
pub const OD_RX7000: &str = "OD_SCLK:\n0: 500Mhz\n1: 2500Mhz\nOD_MCLK:\n0: 97Mhz\n1: 1250MHz\nOD_VDDGFX_OFFSET:\n-50mV\nOD_FAN_CURVE:\n0: 0C 0%\n1: 0C 0%\nOD_RANGE:\nSCLK:     500Mhz       3150Mhz\nMCLK:      97Mhz       1500Mhz\nVDDGFX_OFFSET:    -450mv          0mv\nFAN_CURVE(hotspot temp):     25C    100C\n";
/// RX 6800 (SMU 11): an offset, no range.
pub const OD_RX6000: &str = "OD_SCLK:\n0: 500Mhz\n1: 2475Mhz\nOD_MCLK:\n0: 97Mhz\n1: 1000MHz\nOD_VDDGFX_OFFSET:\n0mV\nOD_RANGE:\nSCLK:     500Mhz       5000Mhz\nMCLK:     674Mhz       1075Mhz\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_offset_and_its_range_by_section_in_either_order() {
        assert_eq!(parse_od(&od_rx9000(-30)), Od { offset: Some(-30), range: Some((-200, 0)) });
        assert_eq!(parse_od(OD_RX9000_V612), Od { offset: Some(0), range: Some((-200, 0)) });
        assert_eq!(parse_od(OD_RX7000), Od { offset: Some(-50), range: Some((-450, 0)) });
        // Turbine Hub's own floor is lower than any card's top.
        assert_eq!(parse_od(OD_RX7000).allowed(), Some((-150, 0)));
        assert_eq!(parse_od(&od_rx9000(-30)).allowed(), Some((-150, 0)));
        let narrow = Od { offset: Some(0), range: Some((-97, 0)) };
        assert_eq!(narrow.allowed(), Some((-95, 0)));
    }

    #[test]
    fn rx_6000_and_an_absent_file_are_refused() {
        let od = parse_od(OD_RX6000);
        assert_eq!(od, Od { offset: Some(0), range: None });
        assert!(od.allowed().is_none());
        assert_eq!(od.check(-30), Err(NO_RANGE.to_string()));
        assert_eq!(parse_od(""), Od::default());
        assert!(Od::default().check(0).is_err());
    }

    #[test]
    fn offsets_are_checked_against_the_range_the_floor_and_the_step() {
        let od = parse_od(&od_rx9000(-30));
        for ok in [0, -5, -30, -150] {
            assert!(od.check(ok).is_ok(), "{ok}");
        }
        for bad in [5, -3, -155, -200, 1] {
            assert!(od.check(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn families_masks_and_names() {
        assert!(family_ok(Some((14, 0, 3))) && family_ok(Some((13, 0, 0))) && family_ok(Some((13, 0, 7))));
        // The Steam Deck, Phoenix and Strix APUs, RX 6000.
        for no in [(11, 5, 0), (13, 0, 4), (14, 0, 0), (13, 0, 5), (11, 0, 7)] {
            assert!(!family_ok(Some(no)), "{no:?}");
        }
        assert!(!family_ok(None));
        assert_eq!(parse_mask("0xfff7bfff\n"), Some(0xfff7_bfff));
        assert_eq!(mask_text(0xfff7_bfff | OVERDRIVE_BIT), "0xfff7ffff");
        assert!(slot_ok("0000:03:00.0") && !slot_ok("0000:03:00.8") && !slot_ok("../../../etc") && !slot_ok("0000:03:00.0/x"));
        assert!(ids_ok("1002:7550") && !ids_ok("1002:755") && !ids_ok("1002:75X0"));
        assert!(release_ok("7.2.7-valve2-2-neptune-72-g31504582a073") && !release_ok("7.2\nx") && !release_ok(""));
        assert_eq!(parse_armed(&armed_text("abc\n", 10, -30)), Some(("abc".into(), 10, -30)));
        assert_eq!(parse_tripped(&tripped_text(-60, 99)), Some((-60, 99)));
    }
}

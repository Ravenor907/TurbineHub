//! Telling an ASUS lighting interface apart from every other HID interface, from what the kernel says about it
//! (its USB ids and its report descriptor). Shared by the helper, which opens exactly these interfaces for writing
//! and nothing else, and by the app, which sends the lighting commands to them.
//!
//! On the ROG Ally series the joystick rings are driven by the controller's lighting chip. Some kernels cap each
//! zone of the rings' LED files at 255, which leaves the files able to show blue only; then the colors go to the
//! chip directly, on the one interface that takes ASUS lighting commands (HID usage page 0xFF31, usage 0x0080). The
//! Windows Dynamic Lighting interface (0x0059/0x0001) is needed too, to switch it off: left on, it overrides custom
//! colors.

/// ASUS's USB vendor id. The lighting protocol is ASUS's own, so nothing from another maker is ever matched.
pub const ASUS_VID: u16 = 0x0B05;
/// The interface that takes ASUS lighting commands (reports starting 0x5A).
pub const LIGHTING: (u16, u16) = (0xFF31, 0x0080);
/// Windows Dynamic Lighting (LampArray).
pub const DYNAMIC: (u16, u16) = (0x0059, 0x0001);

/// The top-level application collections of a HID report descriptor, as (usage page, usage).
pub fn collections(desc: &[u8]) -> Vec<(u16, u16)> {
    let (mut out, mut page, mut usage, mut depth, mut i) = (Vec::new(), 0u32, 0u32, 0u32, 0usize);
    while i < desc.len() {
        let b = desc[i];
        if b == 0xFE {
            // A long item: skipped whole.
            i += 3 + desc.get(i + 1).copied().unwrap_or(0) as usize;
            continue;
        }
        let size = [0, 1, 2, 4][(b & 3) as usize];
        let data = desc.get(i + 1..(i + 1 + size).min(desc.len())).unwrap_or(&[]).iter().rev().fold(0u32, |a, x| (a << 8) | *x as u32);
        let tag = b & 0xFC;
        match tag {
            0x04 => page = data,
            0x08 => usage = data,
            0xA0 => {
                if depth == 0 && data == 0x01 {
                    let (p, u) = if size == 4 || usage > 0xFFFF { (usage >> 16, usage & 0xFFFF) } else { (page, usage) };
                    out.push((p as u16, u as u16));
                }
                depth += 1;
            }
            0xC0 => depth = depth.saturating_sub(1),
            _ => {}
        }
        if matches!(tag, 0x80 | 0x90 | 0xA0 | 0xB0 | 0xC0) {
            usage = 0;
        }
        i += 1 + size;
    }
    out
}

/// The vendor and product id in a HID device's uevent (`HID_ID=0003:00000B05:00001B4C`).
pub fn ids(uevent: &str) -> Option<(u16, u16)> {
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':');
    let _bus = parts.next()?;
    let hex = |p: Option<&str>| p.and_then(|p| u32::from_str_radix(p.trim(), 16).ok()).filter(|v| *v <= 0xFFFF).map(|v| v as u16);
    Some((hex(parts.next())?, hex(parts.next())?))
}

/// What a hidraw interface is for lighting: the commands interface, Dynamic Lighting, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Lighting,
    Dynamic,
}

/// The interface's lighting role, when it is ASUS's and has a lighting collection. Anything else is None.
pub fn role(uevent: &str, desc: &[u8]) -> Option<Role> {
    let (vid, _) = ids(uevent)?;
    if vid != ASUS_VID {
        return None;
    }
    let apps = collections(desc);
    if apps.contains(&LIGHTING) {
        Some(Role::Lighting)
    } else if apps.contains(&DYNAMIC) {
        Some(Role::Dynamic)
    } else {
        None
    }
}

/// A hidraw node's name as the kernel gives it: "hidraw" and a number, nothing else.
pub fn node_name_ok(name: &str) -> bool {
    name.strip_prefix("hidraw").is_some_and(|n| !n.is_empty() && n.len() <= 4 && n.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A report descriptor with a keyboard and the ASUS lighting collection, as on the Ally's config interface.
    pub const ALLY_CONFIG: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0xC0, // keyboard
        0x06, 0x31, 0xFF, 0x09, 0x80, 0xA1, 0x01, 0x85, 0x5A, 0x19, 0x01, 0x29, 0x3F, 0x75, 0x08, 0x95, 0x3F, 0x91, 0x00, 0xC0, // ASUS lighting
    ];
    /// Windows Dynamic Lighting (LampArray) only.
    pub const LAMP_ARRAY: &[u8] = &[0x05, 0x59, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x01, 0x09, 0x02, 0xA1, 0x02, 0xC0, 0xC0];
    /// A plain gamepad.
    pub const GAMEPAD: &[u8] = &[0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0x09, 0x30, 0x81, 0x02, 0xC0];

    #[test]
    fn collections_are_read_at_the_top_level_only() {
        assert_eq!(collections(ALLY_CONFIG), vec![(0x0001, 0x0006), LIGHTING]);
        assert_eq!(collections(LAMP_ARRAY), vec![DYNAMIC]);
        assert_eq!(collections(GAMEPAD), vec![(0x0001, 0x0005)]);
        // A four-byte usage carries its own page.
        assert_eq!(collections(&[0x0B, 0x80, 0x00, 0x31, 0xFF, 0xA1, 0x01, 0xC0]), vec![LIGHTING]);
        // Cut short or junk: no panic.
        assert!(collections(&[0x06, 0x31]).is_empty());
        assert!(collections(&[0xFE, 0xFF]).is_empty());
    }

    #[test]
    fn only_asus_lighting_interfaces_have_a_role() {
        let asus = "DRIVER=asus\nHID_ID=0003:00000B05:00001B4C\nHID_NAME=ASUSTeK Computer Inc. N-KEY Device\n";
        assert_eq!(ids(asus), Some((0x0B05, 0x1B4C)));
        assert_eq!(role(asus, ALLY_CONFIG), Some(Role::Lighting));
        assert_eq!(role(asus, LAMP_ARRAY), Some(Role::Dynamic));
        assert_eq!(role(asus, GAMEPAD), None);
        // The same descriptor from another maker is never matched.
        assert_eq!(role("HID_ID=0003:0000046D:0000C548\n", ALLY_CONFIG), None);
        assert_eq!(role("HID_ID=junk\n", ALLY_CONFIG), None);
        assert_eq!(role("", ALLY_CONFIG), None);
    }

    #[test]
    fn node_names_are_checked() {
        assert!(node_name_ok("hidraw0") && node_name_ok("hidraw12"));
        for bad in ["hidraw", "hidraw1/..", "../hidraw1", "hidrawx", "event3", "hidraw12345", "hidraw1 "] {
            assert!(!node_name_ok(bad), "{bad}");
        }
    }
}

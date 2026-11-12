//! The checkup on Home: only what needs a look, each with its fix, then tips. Every rule is here, decided from the
//! machine and a few facts the app gathers, so the screens only show the list.

use crate::hardware::{DriveRole, Kind, Machine};

/// Facts the app gathers that aren't in `Machine`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    /// The main screen's refresh rate now, when the app could read it.
    pub current_hz: Option<u32>,
    /// Updates ready for things Turbine Hub installed.
    pub updates: Vec<String>,
    /// Decky plugins with a newer version in the plugin store.
    pub plugin_updates: Vec<String>,
    /// A newer Turbine Hub.
    pub app_update: Option<String>,
    /// Battery used per hour during the last sleep, and the devices that woke the machine.
    pub last_sleep: Option<(f64, Vec<String>)>,
    /// What the helper's drift check found (after a SteamOS update).
    pub drift: Vec<String>,
    /// The machine has a password (sudo works).
    pub has_password: Option<bool>,
    /// A Windows drive was found and not brought over yet.
    pub windows_not_brought: bool,
    /// Turbine Hub isn't among the Steam library's non-Steam games, so it doesn't open from Game Mode.
    pub not_in_library: bool,
    /// The player blocked a USB device from waking the machine already, or allowed controller wake.
    pub controller_wake_on: bool,
    /// What Desktop Mode's display settings say each screen is set to now (empty in Game Mode, where they can't
    /// be asked).
    pub screens: Vec<ScreenNow>,
}

/// One screen as Desktop Mode has it set now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScreenNow {
    /// The output's name ("DP-1", "HDMI-A-1", "eDP-1").
    pub name: String,
    pub hz: Option<u32>,
    /// Variable refresh: "never", "always" or "automatic"; "" when the screen can't or it wasn't said.
    pub vrr: String,
    /// HDR on or off; None when the screen can't or it wasn't said.
    pub hdr: Option<bool>,
}

/// Reads what `kscreen-doctor -o` printed: per output its name, the refresh of the mode in use (marked with a star)
/// and its VRR and HDR settings. Color codes in the text are ignored.
pub fn screens_from_kscreen(text: &str) -> Vec<ScreenNow> {
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // An escape sequence: ESC [ ... letter.
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() && d != '[' {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    let mut out = Vec::new();
    for block in plain.split("Output:").skip(1) {
        let mut words = block.split_whitespace();
        let _id = words.next();
        let Some(name) = words.next() else { continue };
        let mut s = ScreenNow { name: name.to_string(), ..ScreenNow::default() };
        let tokens: Vec<&str> = block.split_whitespace().collect();
        for (i, t) in tokens.iter().enumerate() {
            if t.contains('*')
                && let Some((_, rate)) = t.split_once('@')
            {
                let num: String = rate.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                s.hz = num.parse::<f64>().ok().map(|v| v.round() as u32);
            }
            let next = tokens.get(i + 1).map(|x| x.to_ascii_lowercase()).unwrap_or_default();
            match *t {
                "Vrr:" if ["never", "always", "automatic"].contains(&next.as_str()) => s.vrr = next,
                "HDR:" if next == "enabled" => s.hdr = Some(true),
                "HDR:" if next == "disabled" => s.hdr = Some(false),
                _ => {}
            }
        }
        out.push(s);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Something to fix.
    Look,
    /// Good to know, nothing wrong.
    Tip,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: &'static str,
    pub level: Level,
    pub icon: &'static str,
    pub title: String,
    pub line: String,
    /// The button's words and what it opens or does.
    pub action: (&'static str, &'static str),
}

fn item(id: &'static str, level: Level, icon: &'static str, title: String, line: impl Into<String>, action: (&'static str, &'static str)) -> Item {
    Item { id, level, icon, title, line: line.into(), action }
}

fn gb(bytes: u64) -> String {
    let tb = bytes as f64 / 1e12;
    if tb >= 1.0 { format!("{:.0} TB", tb) } else { format!("{:.0} GB", bytes as f64 / 1e9) }
}

/// The checkup list, things to fix first.
pub fn items(m: &Machine, f: &Facts) -> Vec<Item> {
    let mut out = Vec::new();
    if f.has_password == Some(false) {
        out.push(item("password", Level::Look, "lock", "No password set yet".into(), "System changes need one, like the admin password on Windows.", ("Set password", "password")));
    }
    if !f.drift.is_empty() {
        out.push(item("drift", Level::Look, "shield", "Some settings need putting back".into(), "A SteamOS update reset them. One tap puts them back.", ("Put back", "reapply")));
    }
    if let Some((per_hour, wakers)) = &f.last_sleep
        && *per_hour >= 3.0
    {
        let line = match wakers.first() {
            Some(w) => format!("{w} woke the machine. Stop it from waking it?"),
            None => "More than it should. See what's using it.".into(),
        };
        out.push(item("sleep", Level::Look, "moon", format!("Sleep used {:.0}% battery an hour", per_hour), line, ("Fix", "sleep")));
    }
    if let (Some(now), Some(d)) = (f.current_hz, m.connected_displays().find(|d| !d.internal))
        && let Some(e) = &d.edid
        && e.max_hz >= now + 20
    {
        out.push(item("refresh", Level::Look, "display", format!("Your screen runs at {now} Hz"), format!("It supports {} Hz. Games feel smoother at {} Hz.", e.max_hz, e.max_hz), ("Switch", "refresh-rate")));
    }
    for d in m.drives.iter().filter(|d| d.role == DriveRole::Empty && !d.removable) {
        out.push(item("drive", Level::Look, "drive", "A drive isn't used for games".into(), format!("{}, empty. Add it to Steam as a game library?", if d.model.is_empty() { gb(d.bytes) } else { d.model.clone() }), ("Set up", "drives")));
    }
    if !f.updates.is_empty() {
        let n = f.updates.len();
        out.push(item("updates", Level::Look, "download", if n == 1 { "1 update ready".into() } else { format!("{n} updates ready") }, f.updates.join(", "), ("Update all", "update-all")));
    }
    if !f.plugin_updates.is_empty() {
        let n = f.plugin_updates.len();
        out.push(item("plugins", Level::Look, "download", if n == 1 { "1 plugin update ready".into() } else { format!("{n} plugin updates ready") }, f.plugin_updates.join(", "), ("How", "help:plugins")));
    }
    if let Some(v) = &f.app_update {
        out.push(item("app", Level::Tip, "download", format!("Turbine Hub {v} is ready"), "It installs when you say so, and starts the next time Turbine Hub opens.", ("Update", "update-app")));
    }
    if f.windows_not_brought {
        out.push(item("windows", Level::Tip, "bring", "Games on your Windows drive can be copied".into(), "Copy them over instead of downloading them again.", ("Bring it over", "bring")));
    }
    if f.not_in_library {
        out.push(item("library", Level::Tip, "games", "Turbine Hub isn't in your Game Mode library".into(), "Add it and it opens from Game Mode like a game.", ("Add", "self-tile")));
    }
    if m.has_nvidia() {
        out.push(item("nvidia", Level::Tip, "gauge", "Nvidia graphics on SteamOS".into(), "Nvidia support on SteamOS is still experimental. Turbine Hub shows what works.", ("How", "help:nvidia")));
    }
    if m.on_tv() {
        if let Some(d) = m.connected_displays().find(|d| d.hdmi)
            && d.edid.as_ref().is_some_and(|e| e.best_mode.0 >= 3840 && e.max_hz >= 100)
        {
            out.push(item("hdmi", Level::Tip, "tv", "4K at 120 Hz isn't available over HDMI".into(), "A limit of Linux graphics drivers. A DisplayPort to HDMI adapter gets you 4K at 120 Hz.", ("How", "help:hdmi")));
        }
        if !f.controller_wake_on {
            out.push(item("wake", Level::Tip, "games", "Wake with the controller".into(), "Turn the PC on from the couch with your controller.", ("Turn on", "sleep")));
        }
    }
    if m.connected_displays().count() > 1 && m.kind == Kind::Desktop {
        out.push(item("screens", Level::Tip, "display", "Game Mode shows one screen".into(), "Steam's Settings, Display picks which one. Desktop Mode uses all of them.", ("How", "help:screens")));
    }
    if m.kind == Kind::Handheld && m.battery().and_then(|b| b.charge_limit) == Some(100) {
        out.push(item("charge", Level::Tip, "battery", "Battery always charges to 100%".into(), "Stopping at 80% keeps the battery healthy when it's mostly plugged in.", ("Set a limit", "battery")));
    }
    out.sort_by_key(|i| i.level);
    out
}

/// The one-line summary above the list (no greeting). It counts what needs a look; ideas shown under it are named
/// in the second line, so the count always matches the cards.
pub fn headline(items: &[Item]) -> (String, String) {
    let n = items.iter().filter(|i| i.level == Level::Look).count();
    let tips = items.len() - n;
    let ideas = match tips {
        0 => String::new(),
        1 => " One idea below too.".into(),
        t => format!(" {t} ideas below too."),
    };
    let head = match n {
        0 => return ("Everything is running fine".into(), ideas.trim().to_string()),
        1 => "One thing could use a look".to_string(),
        2 => "Two things could use a look".into(),
        3 => "Three things could use a look".into(),
        n => format!("{n} things could use a look"),
    };
    (head, format!("Everything else is running fine.{ideas}"))
}

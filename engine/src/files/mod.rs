//! The file explorer's engine: the folders people know from Windows, on the folders that are really there.
//!
//! Two views of one disk. The Windows view names and groups places (This PC, SteamOS (C:), Users, Games, Program
//! Files, Recycle Bin); the Linux view shows every real name. Every place carries its real path, and both kinds of
//! path can be typed. Nothing here ever asks for a password: every file is read and changed as the player.
//!
//! - `fs`: the one door to the disk (`Fs`) and where the app finds each real path (`Map`).
//! - `places`: the places and the two-way mapping between Windows-style and real paths.
//! - `list`: what a place holds, sorted, grouped and searched.
//! - `guard`: what may never be changed, with the reason in plain words.
//! - `trash`: the Recycle Bin, kept the way the freedesktop Trash specification says.
//! - `ops`: new, rename, copy, move, delete, restore and empty, each as a plan first and with its undo.
//! - `preview`: what can be shown inside the app, and the properties of one item.
//! - `quick`: the pinned folders and the files opened lately.

pub mod fs;
pub mod guard;
pub mod list;
pub mod ops;
pub mod places;
pub mod preview;
pub mod quick;
pub mod trash;

pub use fs::{Disk, Fs, Map, Meta, NodeKind, Space, clean};
pub use list::{Entry, Found, Group, Kind, Limits, Listing, Options, Sort, list, search, search_with};
pub use ops::{Answers, Choice, Done, History, Op, OpKind, Plan, Progress, Undo, plan, run, undo};
pub use places::{AppAt, DriveAt, GameAt, NavItem, Places, UserDir, Who};
pub use preview::{Preview, Properties, open_line, preview, properties};
pub use quick::Quick;
pub use trash::Trashed;

use std::path::PathBuf;

/// How the places are named and grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// The folders people know from Windows.
    Windows,
    /// Every real name and path.
    Linux,
}

impl View {
    pub fn key(self) -> &'static str {
        match self {
            View::Windows => "windows",
            View::Linux => "linux",
        }
    }
    pub fn from_key(key: &str) -> View {
        if key == "linux" { View::Linux } else { View::Windows }
    }
}

/// Somewhere the explorer can be. Most places are a real folder; the rest are made up of several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// The Windows view's start page: the pinned folders and the files opened lately.
    Home,
    /// The player's folders and every drive.
    ThisPc,
    /// A real folder, by its real path.
    Folder(PathBuf),
    /// C:\Users\<user>\AppData: the three folders apps keep their settings in.
    AppData,
    /// C:\Games: every installed game.
    Games,
    /// One game (its id, like "steam:1086940"): its files, its Windows C: drive, its saves.
    Game(String),
    /// C:\Program Files: the installed apps.
    Programs,
    /// C:\Windows: SteamOS itself, locked.
    Windows,
    /// The Recycle Bin.
    Trash,
}

impl Place {
    /// A short text for the interface to hand back: "home", "pc", "games", "game:steam:1", or the real path.
    pub fn key(&self) -> String {
        match self {
            Place::Home => "home".into(),
            Place::ThisPc => "pc".into(),
            Place::Folder(p) => p.to_string_lossy().into_owned(),
            Place::AppData => "appdata".into(),
            Place::Games => "games".into(),
            Place::Game(id) => format!("game:{id}"),
            Place::Programs => "programs".into(),
            Place::Windows => "windows".into(),
            Place::Trash => "trash".into(),
        }
    }
    pub fn from_key(key: &str) -> Option<Place> {
        Some(match key {
            "home" => Place::Home,
            "pc" => Place::ThisPc,
            "appdata" => Place::AppData,
            "games" => Place::Games,
            "programs" => Place::Programs,
            "windows" => Place::Windows,
            "trash" => Place::Trash,
            k if k.starts_with("game:") => Place::Game(k[5..].to_string()),
            k if k.starts_with('/') => Place::Folder(clean(std::path::Path::new(k))),
            _ => return None,
        })
    }
}

/// The time on the player's clock: seconds since 1970 and how far the clock is from UTC, in seconds. std can't read
/// the time zone, so the app hands it over (`When::from_date_z` reads what `date +%z` prints).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct When {
    pub secs: u64,
    pub offset: i32,
}

impl When {
    pub fn utc(secs: u64) -> When {
        When { secs, offset: 0 }
    }
    /// `zone` as `date +%z` prints it: "+0100", "-0730".
    pub fn from_date_z(secs: u64, zone: &str) -> When {
        let z = zone.trim();
        let digits = z.trim_start_matches(['+', '-']);
        let offset = match (digits.get(..2).and_then(|h| h.parse::<i32>().ok()), digits.get(2..4).and_then(|m| m.parse::<i32>().ok())) {
            (Some(h), Some(m)) if digits.len() == 4 => (h * 3600 + m * 60) * if z.starts_with('-') { -1 } else { 1 },
            _ => 0,
        };
        When { secs, offset }
    }
    /// Days since 1970 on the player's calendar for a moment.
    pub fn day_of(&self, secs: u64) -> i64 {
        (secs as i64 + self.offset as i64).div_euclid(86_400)
    }
    /// Year, month and day of a day number.
    pub fn date(day: i64) -> (i64, u32, u32) {
        let z = day + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        (yoe + era * 400 + i64::from(m <= 2), m, d)
    }
    /// The day number of a date.
    pub fn day_number(y: i64, m: u32, d: u32) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
        let doy = (153 * mp + 2) / 5 + d as i64 - 1;
        era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
    }
    /// A moment as the Trash writes it: 2026-11-12T09:31:05, on the player's clock.
    pub fn stamp(&self, secs: u64) -> String {
        let local = secs as i64 + self.offset as i64;
        let (y, m, d) = When::date(local.div_euclid(86_400));
        let t = local.rem_euclid(86_400);
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", t / 3600, t % 3600 / 60, t % 60)
    }
}

/// A size the way Windows writes it: "1 KB", "3.2 MB", "214 GB".
pub fn size_words(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 KB".into();
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} KB", value.ceil().max(1.0) as u64)
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests_list;
#[cfg(test)]
mod tests_ops;
#[cfg(test)]
mod tests_places;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_go_to_text_and_back() {
        for p in [Place::Home, Place::ThisPc, Place::AppData, Place::Games, Place::Game("steam:1086940".into()), Place::Programs, Place::Windows, Place::Trash, Place::Folder("/home/deck/My Files/ü".into())] {
            assert_eq!(Place::from_key(&p.key()), Some(p));
        }
        assert_eq!(Place::from_key("/home/deck/../deck/"), Some(Place::Folder("/home/deck".into())));
        assert_eq!(Place::from_key("nonsense"), None);
    }

    #[test]
    fn the_clock_gives_dates_on_the_players_calendar() {
        // 12 Nov 2026 09:31:05 UTC.
        let secs = 1_794_475_865;
        assert_eq!(When::utc(secs).stamp(secs), "2026-11-12T09:31:05");
        assert_eq!(When::from_date_z(secs, "+0100\n").stamp(secs), "2026-11-12T10:31:05");
        assert_eq!(When::from_date_z(secs, "-1000").stamp(secs), "2026-11-11T23:31:05");
        assert_eq!(When::from_date_z(secs, "+0530").offset, 19_800);
        assert_eq!(When::from_date_z(secs, "what").offset, 0);
        assert_eq!(When::date(When::day_number(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(When::day_number(1970, 1, 1), 0);
        assert_eq!(When::date(0), (1970, 1, 1));
    }

    #[test]
    fn sizes_read_like_windows() {
        assert_eq!(size_words(0), "0 KB");
        assert_eq!(size_words(1), "1 KB");
        assert_eq!(size_words(1024), "1 KB");
        assert_eq!(size_words(14_000), "14 KB");
        assert_eq!(size_words(3_355_443), "3.2 MB");
        assert_eq!(size_words(229_780_000_000), "214 GB");
    }
}

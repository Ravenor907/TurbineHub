//! Online data with a safety net. Nothing online may block a screen, so every source has, in order: a fresh saved
//! copy, the live addresses (a second host where one exists), any older saved copy, and a snapshot built into the
//! app. Each answer says where it came from and how old it is, so screens can say "Checked 3 days ago".
//!
//! `refresh` is the one place the app goes online for the anti-cheat list, the fixes file and Valve's rating of each
//! game. It runs on the app's own thread for such things and only ever fills the cache folder; the screens read
//! that folder, so nothing online can hold one up.

use std::fs;
use std::path::Path;

pub struct Source {
    /// Saved-copy file name.
    pub name: &'static str,
    /// Addresses to try in order.
    pub urls: &'static [&'static str],
    /// A saved copy younger than this is used without going online.
    pub fresh_s: u64,
    /// The copy built into the app, used when nothing else works.
    pub bundled: Option<&'static str>,
    /// Rejects an answer that isn't what this source serves (an error page, a cut-off file).
    pub valid: fn(&str) -> bool,
}

fn json_array(t: &str) -> bool {
    matches!(crate::json::parse(t), Ok(crate::json::Json::Arr(a)) if !a.is_empty())
}

fn json_object(t: &str) -> bool {
    matches!(crate::json::parse(t), Ok(crate::json::Json::Obj(_)))
}

const DAY: u64 = 24 * 3600;

/// The community anti-cheat list: GitHub's raw file, then the same file through jsDelivr's CDN.
pub const ANTICHEAT: Source = Source {
    name: "anticheat.json",
    urls: &["https://raw.githubusercontent.com/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json", "https://cdn.jsdelivr.net/gh/AreWeAntiCheatYet/AreWeAntiCheatYet@master/games.json"],
    fresh_s: DAY,
    bundled: Some(include_str!("../data/anticheat-snapshot.json")),
    valid: json_array,
};

/// Turbine Hub's own fixes file, from its repository and the same file through jsDelivr.
pub const FIXES: Source = Source {
    name: "fixes.json",
    urls: &["https://raw.githubusercontent.com/Ravenor907/TurbineHub/main/engine/data/fixes.json", "https://cdn.jsdelivr.net/gh/Ravenor907/TurbineHub@main/engine/data/fixes.json"],
    fresh_s: DAY,
    bundled: Some(include_str!("../data/fixes.json")),
    valid: json_object,
};

/// Decky Loader's newest release, from GitHub's API (the one place that lists it).
pub const DECKY_RELEASE: Source = Source { name: "decky-release.json", urls: &["https://api.github.com/repos/SteamDeckHomebrew/decky-loader/releases/latest"], fresh_s: 6 * 3600, bundled: None, valid: has_tag };

/// The Decky plugin store's list, for telling which installed plugins have a newer version.
pub const DECKY_PLUGINS: Source = Source { name: "decky-plugins.json", urls: &["https://plugins.deckbrew.xyz/plugins"], fresh_s: DAY, bundled: None, valid: json_array };

fn has_tag(t: &str) -> bool {
    crate::json::parse(t).is_ok_and(|j| j.str_at("tag_name").is_some_and(|tag| !tag.is_empty()))
}

/// The release tag in `DECKY_RELEASE`'s answer.
pub fn release_tag(text: &str) -> Option<String> {
    crate::json::parse(text).ok()?.str_at("tag_name").filter(|t| !t.is_empty()).map(String::from)
}

/// How long a saved rating of one game stays fresh. Ratings change rarely.
pub const VALVE_FRESH_S: u64 = 7 * DAY;

/// Where Valve's rating of one Steam game is kept in the cache folder.
pub fn valve_file(cache_dir: &Path, app_id: &str) -> std::path::PathBuf {
    cache_dir.join("valve").join(format!("{app_id}.json"))
}

/// Valve's rating for one game. Ratings change rarely, so a saved one stays fresh for a week.
pub fn valve_url(app_id: &str) -> Option<String> {
    (!app_id.is_empty() && app_id.bytes().all(|b| b.is_ascii_digit())).then(|| format!("https://store.steampowered.com/saleaction/ajaxgetdeckappcompatibilityreport?nAppID={app_id}"))
}

/// Valve's answer for a game: its rating, or for a game Valve hasn't rated an empty list (kept too, so the game
/// isn't asked about again at every start).
pub fn valve_valid(t: &str) -> bool {
    let Ok(j) = crate::json::parse(t) else { return false };
    j.get("success").and_then(crate::json::Json::as_f64) == Some(1.0) && j.get("results").is_some_and(|r| r.get("resolved_category").is_some() || matches!(r, crate::json::Json::Arr(a) if a.is_empty()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Live,
    /// A saved copy (fresh, or older because every address failed).
    Saved,
    Bundled,
    /// Nothing at all.
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Got {
    pub text: String,
    pub origin: Origin,
    /// Seconds since this data was fetched (0 for live; unknown for the built-in copy).
    pub age_s: u64,
    /// True when going online failed this time.
    pub offline: bool,
}

impl Got {
    /// Plain words for the screen, or empty when the data is current.
    pub fn words(&self) -> String {
        let age = match self.age_s {
            a if a < 3600 => "less than an hour ago".to_string(),
            a if a < 2 * 3600 => "an hour ago".to_string(),
            a if a < DAY => format!("{} hours ago", a / 3600),
            a if a < 2 * DAY => "yesterday".to_string(),
            a => format!("{} days ago", a / DAY),
        };
        match (self.origin, self.offline) {
            (Origin::Live, _) | (Origin::Saved, false) => String::new(),
            (Origin::Saved, true) => format!("Checked {age}, offline now"),
            (Origin::Bundled, _) => "Offline: using the list that came with Turbine Hub".into(),
            (Origin::Missing, _) => "Offline: not checked yet".into(),
        }
    }
}

/// A saved copy dated before this (1970, in effect) is the built-in copy put there by `seed`, not a fetched one.
const SEEDED_BEFORE: u64 = 86_400;

/// A saved copy: its text, its age, and whether it is only the built-in copy.
fn saved(path: &Path, now: u64) -> Option<(String, u64, bool)> {
    let text = fs::read_to_string(path).ok()?;
    let modified = fs::metadata(path).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((text, now.saturating_sub(modified), modified < SEEDED_BEFORE))
}

/// Puts the built-in copy where the saved copy goes when there is no usable saved copy, so everything that reads
/// the cache folder has a list from the very first start, online or not. It is dated 1970: never fresh, and replaced
/// by the first fetch that works.
pub fn seed(src: &Source, cache_dir: &Path) {
    let path = cache_dir.join(src.name);
    let Some(text) = src.bundled else { return };
    if fs::read_to_string(&path).is_ok_and(|t| (src.valid)(&t)) {
        return;
    }
    if crate::write_atomic(&path, text.as_bytes()).is_ok()
        && let Ok(f) = fs::File::options().write(true).open(&path)
    {
        let _ = f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1));
    }
}

/// The saved copy when there is a usable one, else the built-in copy: what to read when going online isn't wanted.
pub fn local(src: &Source, cache_dir: &Path) -> String {
    fs::read_to_string(cache_dir.join(src.name)).ok().filter(|t| (src.valid)(t)).or_else(|| src.bundled.map(String::from)).unwrap_or_default()
}

/// Gets one source. `fetch` downloads one address (net::Client in the app, a stand-in in tests).
pub fn get(src: &Source, cache_dir: &Path, now: u64, fetch: &mut dyn FnMut(&str) -> Result<String, String>) -> Got {
    let urls: Vec<String> = src.urls.iter().map(|u| u.to_string()).collect();
    get_urls(&urls, &cache_dir.join(src.name), src.fresh_s, src.bundled, src.valid, now, fetch)
}

/// The same for any address list and saved-copy file (Valve's per-game ratings use this).
#[allow(clippy::too_many_arguments)]
pub fn get_urls(urls: &[String], cache: &Path, fresh_s: u64, bundled: Option<&str>, valid: fn(&str) -> bool, now: u64, fetch: &mut dyn FnMut(&str) -> Result<String, String>) -> Got {
    let old = saved(cache, now).filter(|(t, _, _)| valid(t));
    if let Some((text, age, false)) = &old
        && *age < fresh_s
    {
        return Got { text: text.clone(), origin: Origin::Saved, age_s: *age, offline: false };
    }
    for u in urls.iter().filter(|u| crate::net::allowed_url(u)) {
        if let Ok(text) = fetch(u)
            && valid(&text)
        {
            let _ = crate::write_atomic(cache, text.as_bytes());
            return Got { text, origin: Origin::Live, age_s: 0, offline: false };
        }
    }
    match (old, bundled) {
        (Some((text, _, true)), _) => Got { text, origin: Origin::Bundled, age_s: 0, offline: true },
        (Some((text, age, false)), _) => Got { text, origin: Origin::Saved, age_s: age, offline: true },
        (None, Some(b)) => Got { text: b.to_string(), origin: Origin::Bundled, age_s: 0, offline: true },
        (None, None) => Got { text: String::new(), origin: Origin::Missing, age_s: 0, offline: true },
    }
}

/// What one `refresh` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refreshed {
    /// Some(true) when something was fetched, Some(false) when going online was tried and nothing answered, None
    /// when every saved copy was fresh and nothing was asked.
    pub online: Option<bool>,
    /// The age of the older of the two lists in use, in seconds. None while a list is only the built-in copy.
    pub lists_age: Option<u64>,
}

/// Brings the cache folder up to date: the anti-cheat list, the fixes file, and Valve's rating for each of
/// `steam_ids`, each through its safety net. At most `budget` ratings are fetched in one go (the rest keep their
/// saved copy until next time). `anticheat_copy` is a second place the anti-cheat list is kept, for the quick
/// panel's questions, which run without the network. The stamp the screens show ("Checked today") is set to the
/// age of what is really in use.
pub fn refresh(cache_dir: &Path, steam_ids: &[String], anticheat_copy: Option<&Path>, now: u64, budget: usize, fetch: &mut dyn FnMut(&str) -> Result<String, String>) -> Refreshed {
    let (mut worked, mut failed) = (false, false);
    let mut lists_age = Some(0);
    for src in [&ANTICHEAT, &FIXES] {
        let got = get(src, cache_dir, now, fetch);
        worked |= got.origin == Origin::Live;
        failed |= got.offline;
        match got.origin {
            Origin::Live | Origin::Saved => lists_age = lists_age.map(|a: u64| a.max(got.age_s)),
            Origin::Bundled | Origin::Missing => {
                seed(src, cache_dir);
                lists_age = None;
            }
        }
        if let (true, Some(copy)) = (src.name == ANTICHEAT.name, anticheat_copy)
            && fs::read_to_string(copy).ok().as_deref() != Some(got.text.as_str())
        {
            let _ = crate::write_atomic(copy, got.text.as_bytes());
        }
    }
    let mut left = budget;
    for id in steam_ids {
        let Some(url) = valve_url(id) else { continue };
        let mut skipped = false;
        let got = get_urls(&[url], &valve_file(cache_dir, id), VALVE_FRESH_S, None, valve_valid, now, &mut |u| {
            if left == 0 {
                skipped = true;
                return Err("later".into());
            }
            left -= 1;
            fetch(u)
        });
        worked |= got.origin == Origin::Live;
        failed |= got.offline && !skipped;
    }
    if let Some(age) = lists_age {
        crate::ui::set_checked(cache_dir, now.saturating_sub(age));
    }
    Refreshed { online: if worked { Some(true) } else if failed { Some(false) } else { None }, lists_age }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-sources-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn falls_back_live_then_second_host_then_saved_then_bundled() {
        let d = dir("chain");
        let now = crate::now();
        let mut calls = Vec::new();
        // First host down, second works: saved for next time.
        let got = get(&ANTICHEAT, &d, now, &mut |u| {
            calls.push(u.to_string());
            if u.contains("jsdelivr") { Ok(r#"[{"storeIds":{"steam":"1"},"status":"Denied"}]"#.into()) } else { Err("down".into()) }
        });
        assert_eq!((got.origin, calls.len()), (Origin::Live, 2));
        assert_eq!(got.words(), "");
        // Fresh saved copy: nothing goes online.
        let got = get(&ANTICHEAT, &d, now, &mut |_| panic!("should not fetch"));
        assert_eq!(got.origin, Origin::Saved);
        // Stale and everything down: the old copy, marked offline.
        let got = get(&ANTICHEAT, &d, now + 3 * DAY, &mut |_| Err("down".into()));
        assert_eq!((got.origin, got.offline), (Origin::Saved, true));
        assert_eq!(got.words(), "Checked 3 days ago, offline now");
        // An error page is never saved or used.
        let _ = fs::remove_dir_all(&d);
        let got = get(&ANTICHEAT, &d, now, &mut |_| Ok("<html>rate limited</html>".into()));
        assert_eq!(got.origin, Origin::Bundled);
        assert!(!crate::library::parse_anticheat(&got.text).is_empty());
        assert!(!d.join("anticheat.json").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bundled_copies_are_valid_and_every_address_is_allowed() {
        for s in [&ANTICHEAT, &FIXES] {
            assert!((s.valid)(s.bundled.unwrap()), "{}", s.name);
            assert!(s.urls.len() >= 2, "{} needs a second host", s.name);
            for u in s.urls {
                assert!(crate::net::allowed_url(u), "{u}");
            }
        }
        assert!(crate::net::allowed_url(&valve_url("1086940").unwrap()));
        assert!(valve_url("1; rm").is_none());
        assert!(valve_valid(r#"{"success":1,"results":{"resolved_category":3}}"#));
        assert!(!valve_valid(r#"{"success":2}"#));
        // A game Valve hasn't rated answers with an empty list: kept, so it isn't asked about at every start.
        assert!(valve_valid(r#"{"success":1,"results":[]}"#));
        assert!(!valve_valid(r#"{"success":1}"#) && !valve_valid("<html>"));
        let got = get_urls(&[], &dir("none").join("x.json"), DAY, None, valve_valid, 0, &mut |_| Err("x".into()));
        assert_eq!((got.origin, got.words().as_str()), (Origin::Missing, "Offline: not checked yet"));
        assert_eq!(release_tag(r#"{"tag_name":"v3.1.10","name":"x"}"#).as_deref(), Some("v3.1.10"));
        assert!((DECKY_RELEASE.valid)(r#"{"tag_name":"v3.1.10"}"#) && !(DECKY_RELEASE.valid)(r#"{"message":"API rate limit exceeded"}"#));
        for s in [&DECKY_RELEASE, &DECKY_PLUGINS] {
            assert!(s.urls.iter().all(|u| crate::net::allowed_url(u)), "{}", s.name);
        }
    }

    const LIST: &str = r#"[{"storeIds":{"steam":"1245620"},"status":"Running"}]"#;
    const RATED: &str = r#"{"success":1,"results":{"appid":1,"resolved_category":3}}"#;

    /// A stand-in for the internet: answers the lists and ratings, and counts what was asked.
    fn internet(asked: &mut Vec<String>, u: &str) -> Result<String, String> {
        asked.push(u.to_string());
        if u.contains("AreWeAntiCheatYet") {
            Ok(LIST.into())
        } else if u.contains("fixes.json") {
            Ok("{}".into())
        } else {
            Ok(RATED.into())
        }
    }

    #[test]
    fn a_first_start_offline_still_has_the_built_in_lists() {
        let d = dir("seed");
        let copy = d.join("panel/anticheat.json");
        let now = crate::now();
        let r = refresh(&d, &["1245620".into()], Some(&copy), now, 40, &mut |_| Err("no network".into()));
        assert_eq!(r, Refreshed { online: Some(false), lists_age: None });
        // What the screens and the quick panel read is there, and it is the real list, not an empty one.
        let seeded = fs::read_to_string(d.join("anticheat.json")).unwrap();
        assert!(crate::library::parse_anticheat(&seeded).len() > 10);
        assert_eq!(fs::read_to_string(&copy).unwrap(), seeded);
        assert_eq!(local(&ANTICHEAT, &d), seeded);
        assert_eq!(crate::ui::checked_at(&d), None);
        // The built-in copy never counts as fetched: the next start goes online again, and says where the list is
        // from while that fails.
        let got = get(&ANTICHEAT, &d, now, &mut |_| Err("no network".into()));
        assert_eq!((got.origin, got.words().as_str()), (Origin::Bundled, "Offline: using the list that came with Turbine Hub"));
        let mut asked = Vec::new();
        let r = refresh(&d, &["1245620".into()], Some(&copy), now, 40, &mut |u| internet(&mut asked, u));
        assert_eq!((r, asked.len()), (Refreshed { online: Some(true), lists_age: Some(0) }, 3));
        assert_eq!(fs::read_to_string(d.join("anticheat.json")).unwrap(), LIST);
        assert_eq!(fs::read_to_string(&copy).unwrap(), LIST);
        assert_eq!(crate::ui::checked_at(&d), Some(now));
        // Seeding never replaces a fetched list, and an empty folder reads the built-in one.
        seed(&ANTICHEAT, &d);
        assert_eq!(local(&ANTICHEAT, &d), LIST);
        assert!(local(&ANTICHEAT, &dir("nothing")).len() > 1000);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn refresh_asks_only_for_what_is_stale_and_says_how_old_the_rest_is() {
        let d = dir("refresh");
        let now = crate::now();
        let ids: Vec<String> = (1..=5).map(|i| i.to_string()).collect();
        // Five games and room for three: three are asked about now, the other two next time.
        let mut asked = Vec::new();
        let r = refresh(&d, &ids, None, now, 3, &mut |u| internet(&mut asked, u));
        assert_eq!((r.online, asked.len()), (Some(true), 5));
        assert_eq!(ids.iter().filter(|id| valve_file(&d, id).is_file()).count(), 3);
        let mut asked = Vec::new();
        let r = refresh(&d, &ids, None, now, 3, &mut |u| internet(&mut asked, u));
        assert_eq!((r.online, asked.len()), (Some(true), 2));
        // Everything fresh: nothing is asked at all, and that isn't "offline".
        let r = refresh(&d, &ids, None, now + 60, 40, &mut |_| panic!("should not fetch"));
        assert_eq!(r, Refreshed { online: None, lists_age: Some(60) });
        // Three days on with no network: every screen keeps its data, and the stamp says how old it is.
        let later = now + 3 * DAY;
        let r = refresh(&d, &ids, None, later, 40, &mut |_| Err("no network".into()));
        assert_eq!(r, Refreshed { online: Some(false), lists_age: Some(3 * DAY) });
        assert_eq!(crate::ui::checked_words(later, crate::ui::checked_at(&d), true), "Checked 3 days ago, offline now");
        assert_eq!(fs::read_to_string(d.join("anticheat.json")).unwrap(), LIST);
        assert!(valve_valid(&fs::read_to_string(valve_file(&d, "1")).unwrap()));
        // An error page instead of a rating is never kept; an id that isn't one is never asked about.
        let r = refresh(&d, &["77".into(), "1; rm".into()], None, now, 40, &mut |u| if u.contains("nAppID") { Ok("<html>busy</html>".into()) } else { Err("x".into()) });
        assert_eq!(r.online, Some(false));
        assert!(!valve_file(&d, "77").exists());
        let _ = fs::remove_dir_all(&d);
    }
}

//! Online data with a safety net. Nothing online may block a screen, so every source has, in order: a fresh saved
//! copy, the live addresses (a second host where one exists), any older saved copy, and a snapshot built into the
//! app. Each answer says where it came from and how old it is, so screens can say "Checked 3 days ago".

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

/// Valve's rating for one game. Ratings change rarely, so a saved one stays fresh for a week.
pub fn valve_url(app_id: &str) -> Option<String> {
    (!app_id.is_empty() && app_id.bytes().all(|b| b.is_ascii_digit())).then(|| format!("https://store.steampowered.com/saleaction/ajaxgetdeckappcompatibilityreport?nAppID={app_id}"))
}

pub fn valve_valid(t: &str) -> bool {
    crate::json::parse(t).ok().and_then(|j| j.get("results").map(|r| r.get("resolved_category").is_some())).unwrap_or(false)
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

fn saved(path: &Path, now: u64) -> Option<(String, u64)> {
    let text = fs::read_to_string(path).ok()?;
    let modified = fs::metadata(path).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((text, now.saturating_sub(modified)))
}

/// Gets one source. `fetch` downloads one address (net::Client in the app, a stand-in in tests).
pub fn get(src: &Source, cache_dir: &Path, now: u64, fetch: &mut dyn FnMut(&str) -> Result<String, String>) -> Got {
    let urls: Vec<String> = src.urls.iter().map(|u| u.to_string()).collect();
    get_urls(&urls, &cache_dir.join(src.name), src.fresh_s, src.bundled, src.valid, now, fetch)
}

/// The same for any address list and saved-copy file (Valve's per-game ratings use this).
#[allow(clippy::too_many_arguments)]
pub fn get_urls(urls: &[String], cache: &Path, fresh_s: u64, bundled: Option<&str>, valid: fn(&str) -> bool, now: u64, fetch: &mut dyn FnMut(&str) -> Result<String, String>) -> Got {
    let old = saved(cache, now).filter(|(t, _)| valid(t));
    if let Some((text, age)) = &old
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
        (Some((text, age)), _) => Got { text, origin: Origin::Saved, age_s: age, offline: true },
        (None, Some(b)) => Got { text: b.to_string(), origin: Origin::Bundled, age_s: 0, offline: true },
        (None, None) => Got { text: String::new(), origin: Origin::Missing, age_s: 0, offline: true },
    }
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
        let got = get_urls(&[], &dir("none").join("x.json"), DAY, None, valve_valid, 0, &mut |_| Err("x".into()));
        assert_eq!((got.origin, got.words().as_str()), (Origin::Missing, "Offline: not checked yet"));
    }
}

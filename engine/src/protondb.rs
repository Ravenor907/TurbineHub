//! What players say about a game on ProtonDB (protondb.com), where people who played a Windows game on Linux
//! report how it went. Its summary for a game is one word: Platinum, Gold, Silver, Bronze or Borked.
//!
//! It is shown beside Valve's rating as "Players say: Gold" and is only ever shown: Turbine Hub's own label for a
//! game, its locks and its fixes come from Valve's rating, the anti-cheat list and the fixes file, never from this.
//! Fetched through `sources` like Valve's ratings (a saved copy for a week, the old copy kept when ProtonDB doesn't
//! answer), a few games at a time.

use crate::json;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Platinum,
    Gold,
    Silver,
    Bronze,
    Borked,
}

impl Tier {
    pub fn words(self) -> &'static str {
        match self {
            Tier::Platinum => "Platinum",
            Tier::Gold => "Gold",
            Tier::Silver => "Silver",
            Tier::Bronze => "Bronze",
            Tier::Borked => "Borked",
        }
    }

    /// The chip on a game's header.
    pub fn chip(self) -> String {
        format!("Players say: {}", self.words())
    }

    /// The chip's tone. Only a color: nothing is decided by it.
    pub fn tone(self) -> &'static str {
        match self {
            Tier::Platinum | Tier::Gold => "ok",
            Tier::Silver | Tier::Bronze => "warn",
            Tier::Borked => "stop",
        }
    }
}

/// A saved summary stays fresh this long. They move slowly.
pub const FRESH_S: u64 = 7 * 24 * 3600;

/// ProtonDB's summary for one Steam game.
pub fn url(app_id: &str) -> Option<String> {
    (!app_id.is_empty() && app_id.len() <= 20 && app_id.bytes().all(|b| b.is_ascii_digit())).then(|| format!("https://www.protondb.com/api/v1/reports/summaries/{app_id}.json"))
}

/// Where the summary of one game is kept in the cache folder.
pub fn file(cache: &Path, app_id: &str) -> PathBuf {
    cache.join("protondb").join(format!("{app_id}.json"))
}

/// ProtonDB's answer for a game: an object with a tier in it (an error page isn't one).
pub fn valid(text: &str) -> bool {
    json::parse(text).is_ok_and(|j| j.str_at("tier").is_some())
}

/// The tier in a summary. None for a game with too few reports ("pending"), a Linux version ("native") and
/// anything unknown.
pub fn parse(text: &str) -> Option<Tier> {
    match json::parse(text).ok()?.str_at("tier")? {
        "platinum" => Some(Tier::Platinum),
        "gold" => Some(Tier::Gold),
        "silver" => Some(Tier::Silver),
        "bronze" => Some(Tier::Bronze),
        "borked" => Some(Tier::Borked),
        _ => None,
    }
}

/// What players say about a game, from the saved summary. Never goes online.
pub fn tier(cache: &Path, app_id: &str) -> Option<Tier> {
    parse(&std::fs::read_to_string(file(cache, app_id)).ok()?)
}

/// What is kept for a game ProtonDB has no reports for (it answers 404 then), so the game isn't asked about at
/// every start.
const NO_REPORTS: &str = "{\"tier\":\"pending\",\"total\":0}\n";

/// Brings the saved summaries for `steam_ids` up to date, at most `budget` fetched in one go (the rest keep what
/// they have until next time). `fetch` downloads one address and says why when it can't. Answers how many were
/// fetched.
pub fn refresh(cache: &Path, steam_ids: &[String], now: u64, budget: usize, fetch: &mut dyn FnMut(&str) -> Result<String, String>) -> usize {
    let (mut left, mut fetched) = (budget, 0);
    for id in steam_ids {
        let Some(u) = url(id) else { continue };
        let mut none = false;
        let got = crate::sources::get_urls(&[u], &file(cache, id), FRESH_S, None, valid, now, &mut |u| {
            if left == 0 {
                return Err("later".into());
            }
            left -= 1;
            fetch(u).inspect_err(|e| none = e.contains("(404)"))
        });
        if got.origin == crate::sources::Origin::Live {
            fetched += 1;
        } else if none {
            let _ = crate::write_atomic(&file(cache, id), NO_REPORTS.as_bytes());
        }
    }
    fetched
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLD: &str = r#"{"bestReportedTier":"platinum","confidence":"strong","score":0.79,"tier":"gold","total":2107,"trendingTier":"platinum"}"#;

    #[test]
    fn the_tier_is_read_and_only_five_words_count() {
        assert_eq!(parse(GOLD), Some(Tier::Gold));
        for (word, t) in [("platinum", Tier::Platinum), ("silver", Tier::Silver), ("bronze", Tier::Bronze), ("borked", Tier::Borked)] {
            assert_eq!(parse(&format!("{{\"tier\":\"{word}\"}}")), Some(t));
        }
        assert_eq!(Tier::Gold.chip(), "Players say: Gold");
        for nothing in [r#"{"tier":"pending"}"#, r#"{"tier":"native"}"#, r#"{"total":3}"#, "<html>", ""] {
            assert_eq!(parse(nothing), None, "{nothing}");
        }
        assert!(valid(GOLD) && valid(NO_REPORTS) && !valid("<!doctype html><title>Just a moment</title>") && !valid("[]"));
        assert!(crate::net::allowed_url(&url("1245620").unwrap()));
        assert!(url("12; rm").is_none() && url("").is_none());
    }

    #[test]
    fn summaries_are_kept_for_a_week_and_survive_being_offline() {
        let d = std::env::temp_dir().join(format!("turbinehub-protondb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let now = crate::now();
        let ids: Vec<String> = ["1245620", "1086940", "70", "not an id"].iter().map(|x| x.to_string()).collect();
        let missing = crate::net::NetError::NotFound.to_string();
        let mut asked = Vec::new();
        let mut internet = |u: &str| {
            asked.push(u.to_string());
            if u.contains("/1245620.json") { Ok(GOLD.to_string()) } else if u.contains("/70.json") { Err(missing.clone()) } else { Ok("<html>busy</html>".to_string()) }
        };
        // Room for two: the third waits for next time.
        assert_eq!(refresh(&d, &ids, now, 2, &mut internet), 1);
        assert_eq!(refresh(&d, &ids, now, 2, &mut internet), 0);
        assert_eq!(asked.len(), 4);
        assert_eq!((tier(&d, "1245620"), tier(&d, "1086940"), tier(&d, "70")), (Some(Tier::Gold), None, None));
        // An error page is never kept; a game without reports is remembered as that.
        assert!(!file(&d, "1086940").exists() && file(&d, "70").exists());
        // Within the week nothing is asked for what is saved; after it, with ProtonDB down, the old answer stays.
        let mut asked = Vec::new();
        refresh(&d, &ids[..1], now + 3600, 9, &mut |u| {
            asked.push(u.to_string());
            Err("down".into())
        });
        assert!(asked.is_empty());
        assert_eq!(refresh(&d, &ids[..1], now + 8 * 24 * 3600, 9, &mut |_| Err("down".into())), 0);
        assert_eq!(tier(&d, "1245620"), Some(Tier::Gold));
        let _ = std::fs::remove_dir_all(&d);
    }
}

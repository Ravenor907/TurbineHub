//! Will my games run? Each owned game gets Works, Needs a fix (with the fix), or Won't run here (with the best
//! alternative). The data is fetched fresh, never built into the app: Valve's SteamOS rating for each game, the
//! community anti-cheat list (AreWeAntiCheatYet) and Turbine Hub's own fixes file. These parsers are pure, so they
//! are tested offline.

use crate::json::{self, Json};

/// Valve's rating, from Steam's compatibility report for the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValveRating {
    Unknown,
    Unsupported,
    Playable,
    Verified,
}

/// What the community anti-cheat list says about running on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntiCheat {
    /// Works on Linux, anti-cheat included.
    Supported,
    /// Runs, but the developer hasn't made it official.
    Running,
    /// Blocked on Linux (the developer turned it off, or it's broken).
    Blocked,
    /// On the list as planned but not working yet.
    Planned,
}

/// A fix Turbine Hub knows for a game.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    /// "proton" (use this Proton), "switch" (turn on one of the game switches) or "note" (a plain tip).
    pub kind: String,
    pub value: String,
    pub words: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Works,
    NeedsFix,
    WontRun,
    /// Nobody has rated it yet. Shown with Works, marked "Not rated yet".
    NotRated,
}

impl Label {
    pub fn key(self) -> &'static str {
        match self {
            Label::Works => "works",
            Label::NeedsFix => "fix",
            Label::WontRun => "no",
            Label::NotRated => "unrated",
        }
    }
    pub fn words(self) -> &'static str {
        match self {
            Label::Works => "Works",
            Label::NeedsFix => "Needs a fix",
            Label::WontRun => "Won't run here",
            Label::NotRated => "Not rated yet",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub label: Label,
    /// One plain sentence: what's going on, or the fix, or the alternative.
    pub note: String,
    pub fix: Option<Fix>,
    /// Its anti-cheat works online: Turbine Hub never changes how it starts.
    pub online_anticheat: bool,
}

/// What else the player has, for the "Won't run here" alternative.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub windows_installed: bool,
}

/// Reads Steam's compatibility report for one game (store.steampowered.com/saleaction/ajaxgetdeckappcompatibilityreport).
/// `resolved_category`: 0 unknown, 1 unsupported, 2 playable, 3 verified. SteamOS's own rating, when the report has
/// one, wins over the Steam Deck one.
pub fn parse_valve_report(text: &str) -> ValveRating {
    let Ok(j) = json::parse(text) else { return ValveRating::Unknown };
    let r = j.get("results").unwrap_or(&j);
    let n = r.get("steamos_resolved_category").or_else(|| r.get("resolved_category")).and_then(Json::as_f64).unwrap_or(0.0);
    match n as u32 {
        1 => ValveRating::Unsupported,
        2 => ValveRating::Playable,
        3 => ValveRating::Verified,
        _ => ValveRating::Unknown,
    }
}

/// Reads AreWeAntiCheatYet's games.json: Steam app id to status.
pub fn parse_anticheat(text: &str) -> Vec<(String, AntiCheat)> {
    let Ok(j) = json::parse(text) else { return Vec::new() };
    j.as_array()
        .iter()
        .filter_map(|g| {
            let id = g.get("storeIds").and_then(|s| s.get("steam")).and_then(|v| v.as_str().map(String::from).or_else(|| v.as_f64().map(|n| (n as u64).to_string())))?;
            let status = match g.str_at("status")? {
                "Supported" => AntiCheat::Supported,
                "Running" => AntiCheat::Running,
                "Broken" | "Denied" => AntiCheat::Blocked,
                "Planned" => AntiCheat::Planned,
                _ => return None,
            };
            Some((id, status))
        })
        .collect()
}

/// Reads Turbine Hub's fixes file: {"<steam app id>": {"kind": "...", "value": "...", "words": "..."}}.
pub fn parse_fixes(text: &str) -> Vec<(String, Fix)> {
    let Ok(j) = json::parse(text) else { return Vec::new() };
    j.entries()
        .iter()
        .filter(|(id, _)| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|(id, f)| {
            let kind = f.str_at("kind")?;
            ["proton", "switch", "note"].contains(&kind).then(|| (id.clone(), Fix { kind: kind.into(), value: f.str_at("value").unwrap_or("").into(), words: f.str_at("words").unwrap_or("").into() }))
        })
        .collect()
}

fn blocked_alternative(o: Options) -> &'static str {
    if o.windows_installed {
        "Its anti-cheat blocks SteamOS. Restart to Windows to play, or try GeForce NOW."
    } else {
        "Its anti-cheat blocks SteamOS. Play it on GeForce NOW or Xbox Cloud if it's there."
    }
}

/// The verdict for one game.
pub fn verdict(valve: ValveRating, anticheat: Option<AntiCheat>, fix: Option<&Fix>, o: Options) -> Verdict {
    let online = matches!(anticheat, Some(AntiCheat::Supported | AntiCheat::Running));
    let v = |label, note: &str, fix: Option<Fix>| Verdict { label, note: note.into(), fix, online_anticheat: online };
    match (anticheat, fix) {
        (Some(AntiCheat::Blocked | AntiCheat::Planned), _) => v(Label::WontRun, blocked_alternative(o), None),
        (_, Some(f)) => v(Label::NeedsFix, if f.words.is_empty() { "One tap fixes it." } else { &f.words }, Some(f.clone())),
        _ => match valve {
            ValveRating::Verified | ValveRating::Playable if online => v(Label::Works, "Online play works.", None),
            ValveRating::Verified => v(Label::Works, "Runs as-is.", None),
            ValveRating::Playable => v(Label::Works, "Runs, with small rough edges (like small text or a launcher).", None),
            ValveRating::Unsupported if o.windows_installed => v(Label::WontRun, "Valve marks it as not working on SteamOS. Restart to Windows to play.", None),
            ValveRating::Unsupported => v(Label::WontRun, "Valve marks it as not working on SteamOS. Try GeForce NOW or Xbox Cloud.", None),
            ValveRating::Unknown if online => v(Label::Works, "Online play works.", None),
            ValveRating::Unknown => v(Label::NotRated, "Nobody has rated it yet. Most games run.", None),
        },
    }
}

/// Counts for the summary row: (works, need a fix, won't run, not rated).
pub fn counts(verdicts: &[Verdict]) -> (usize, usize, usize, usize) {
    let c = |l: Label| verdicts.iter().filter(|v| v.label == l).count();
    (c(Label::Works), c(Label::NeedsFix), c(Label::WontRun), c(Label::NotRated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_valves_report() {
        assert_eq!(parse_valve_report(r#"{"success":1,"results":{"appid":1086940,"resolved_category":3,"resolved_items":[]}}"#), ValveRating::Verified);
        assert_eq!(parse_valve_report(r#"{"results":{"resolved_category":3,"steamos_resolved_category":1}}"#), ValveRating::Unsupported);
        assert_eq!(parse_valve_report(r#"{"results":{"resolved_category":2}}"#), ValveRating::Playable);
        assert_eq!(parse_valve_report("not json"), ValveRating::Unknown);
    }

    #[test]
    fn reads_the_anticheat_list() {
        let t = r#"[{"name":"Apex Legends","status":"Denied","storeIds":{"steam":"1172470"}},
                    {"name":"Elden Ring","status":"Supported","storeIds":{"steam":"1245620"}},
                    {"name":"No Steam","status":"Broken","storeIds":{"epic":"x"}},
                    {"name":"Odd","status":"Wat","storeIds":{"steam":"1"}}]"#;
        assert_eq!(parse_anticheat(t), vec![("1172470".into(), AntiCheat::Blocked), ("1245620".into(), AntiCheat::Supported)]);
    }

    #[test]
    fn decides_the_label() {
        let o = Options::default();
        assert_eq!(verdict(ValveRating::Verified, None, None, o).label, Label::Works);
        let apex = verdict(ValveRating::Unsupported, Some(AntiCheat::Blocked), None, Options { windows_installed: true });
        assert_eq!(apex.label, Label::WontRun);
        assert!(apex.note.contains("Restart to Windows"));
        let elden = verdict(ValveRating::Verified, Some(AntiCheat::Supported), None, o);
        assert_eq!((elden.label, elden.online_anticheat, elden.note.as_str()), (Label::Works, true, "Online play works."));
        let fixes = parse_fixes(r#"{"1091500":{"kind":"proton","value":"GE-Proton","words":"One tap: use the recommended Proton."},"x":{"kind":"proton"},"5":{"kind":"rm -rf"}}"#);
        assert_eq!(fixes.len(), 1);
        let cp = verdict(ValveRating::Playable, None, Some(&fixes[0].1), o);
        assert_eq!((cp.label, cp.note.as_str()), (Label::NeedsFix, "One tap: use the recommended Proton."));
        assert_eq!(verdict(ValveRating::Unknown, None, None, o).label, Label::NotRated);
        let all = [elden, apex, cp, verdict(ValveRating::Unknown, None, None, o)];
        assert_eq!(counts(&all), (1, 1, 1, 1));
    }
}

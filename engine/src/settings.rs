//! The player's Turbine Hub settings: one JSON file in the app's own config folder. Unknown keys are dropped, bad
//! values fall back to the default, numbers are clamped, so a hand-edited or damaged file never breaks the app.

use crate::json::{self, Json, quote};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Guided,
    Veteran,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// None until the player picks on the first screen.
    pub mode: Option<Mode>,
    /// Interface size, 50 to 150 percent.
    pub ui_scale: u32,
    /// The player picked the interface size (else it follows the machine: larger on a TV). Only a picked size is
    /// written to the file.
    pub scale_chosen: bool,
    pub animations: bool,
    /// "dark", "black", "light" or "contrast".
    pub theme: String,
    /// "brass", "iron", "copper" or "jade".
    pub accent: String,
    pub game_boost: bool,
    /// The save time machine: the background helper snapshots a game's saves each time it starts.
    pub save_snapshots: bool,
    /// Pre-releases too.
    pub test_builds: bool,
    /// Kept so older settings files still read; Turbine Hub sends no reports anywhere.
    pub reports: bool,
    /// First-run steps finished or skipped ("machine", "password", "games", "bring", "picks").
    pub setup_done: Vec<String>,
    /// Bring it over steps finished ("stores", "gamepass", "windows", "apps", "tv").
    pub brought: Vec<String>,
}

pub const THEMES: &[&str] = &["dark", "black", "light", "contrast"];
pub const ACCENTS: &[&str] = &["brass", "iron", "copper", "jade"];
pub const SETUP_STEPS: &[&str] = &["machine", "password", "games", "bring", "picks"];
pub const BRING_STEPS: &[&str] = &["stores", "gamepass", "windows", "apps", "tv"];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: None,
            ui_scale: 100,
            scale_chosen: false,
            animations: true,
            theme: "black".into(),
            accent: "jade".into(),
            game_boost: false,
            save_snapshots: false,
            test_builds: false,
            reports: false,
            setup_done: Vec::new(),
            brought: Vec::new(),
        }
    }
}

fn word(j: &Json, k: &str, allowed: &[&str], default: &str) -> String {
    j.str_at(k).filter(|v| allowed.contains(v)).unwrap_or(default).to_string()
}

fn list(j: &Json, k: &str, allowed: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = j.get(k).map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).filter(|s| allowed.contains(s)).map(String::from).collect();
    v.dedup();
    v
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let d = Settings::default();
        let Ok(j) = json::parse(text) else { return d };
        let b = |k: &str, def: bool| j.get(k).and_then(Json::as_bool).unwrap_or(def);
        Settings {
            mode: match j.str_at("mode") {
                Some("guided") => Some(Mode::Guided),
                Some("veteran") => Some(Mode::Veteran),
                _ => None,
            },
            ui_scale: j.get("uiScale").and_then(Json::as_f64).map(|v| (v.round() as i64).clamp(50, 150) as u32).unwrap_or(d.ui_scale),
            scale_chosen: j.get("uiScale").and_then(Json::as_f64).is_some(),
            animations: b("animations", d.animations),
            theme: word(&j, "theme", THEMES, &d.theme),
            accent: word(&j, "accent", ACCENTS, &d.accent),
            game_boost: b("gameBoost", d.game_boost),
            save_snapshots: b("saveSnapshots", d.save_snapshots),
            test_builds: b("testBuilds", d.test_builds),
            reports: b("reports", d.reports),
            setup_done: list(&j, "setupDone", SETUP_STEPS),
            brought: list(&j, "brought", BRING_STEPS),
        }
    }

    pub fn to_json(&self) -> String {
        let l = |v: &[String]| format!("[{}]", v.iter().map(|s| quote(s)).collect::<Vec<_>>().join(","));
        let mode = match self.mode {
            Some(Mode::Guided) => "\"guided\"",
            Some(Mode::Veteran) => "\"veteran\"",
            None => "null",
        };
        let scale = if self.scale_chosen { format!("\n  \"uiScale\": {},", self.ui_scale) } else { String::new() };
        format!(
            "{{\n  \"mode\": {mode},{scale}\n  \"animations\": {},\n  \"theme\": {},\n  \"accent\": {},\n  \"gameBoost\": {},\n  \"saveSnapshots\": {},\n  \"testBuilds\": {},\n  \"reports\": {},\n  \"setupDone\": {},\n  \"brought\": {}\n}}\n",
            self.animations,
            quote(&self.theme),
            quote(&self.accent),
            self.game_boost,
            self.save_snapshots,
            self.test_builds,
            self.reports,
            l(&self.setup_done),
            l(&self.brought)
        )
    }

    /// The interface size in use: the player's pick, or 125% on a machine used on a TV (couch distance), else 100%.
    pub fn scale_for(&self, tv: bool) -> u32 {
        if self.scale_chosen {
            self.ui_scale
        } else if tv {
            125
        } else {
            100
        }
    }

    /// Setup is finished when every step is done or skipped.
    pub fn setup_finished(&self) -> bool {
        SETUP_STEPS.iter().all(|s| self.setup_done.iter().any(|d| d == s))
    }

    pub fn file(config_home: &Path) -> PathBuf {
        config_home.join("turbinehub/settings.json")
    }

    pub fn load(config_home: &Path) -> Settings {
        std::fs::read_to_string(Self::file(config_home)).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self, config_home: &Path) -> std::io::Result<()> {
        crate::write_atomic(&Self::file(config_home), self.to_json().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_repairs() {
        let mut s = Settings { mode: Some(Mode::Veteran), ui_scale: 125, scale_chosen: true, game_boost: true, save_snapshots: true, ..Settings::default() };
        s.setup_done = vec!["machine".into(), "password".into()];
        assert_eq!(Settings::parse(&s.to_json()), s);
        let bad = Settings::parse(r#"{"mode":"hacker","uiScale":900,"theme":"pink","accent":7,"setupDone":["machine","x","machine"],"extra":1}"#);
        assert_eq!((bad.mode, bad.ui_scale, bad.theme.as_str(), bad.accent.as_str()), (None, 150, "black", "jade"));
        assert_eq!(bad.setup_done, vec!["machine"]);
        // A size the player never picked follows the machine, and stays out of the file.
        let fresh = Settings::default();
        assert_eq!((fresh.scale_for(true), fresh.scale_for(false), s.scale_for(true)), (125, 100, 125));
        assert!(!fresh.to_json().contains("uiScale"));
        assert_eq!(Settings::parse(&fresh.to_json()), fresh);
        assert_eq!(Settings::parse("garbage"), Settings::default());
        assert!(!s.setup_finished());
        s.setup_done = SETUP_STEPS.iter().map(|x| x.to_string()).collect();
        assert!(s.setup_finished());
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("turbinehub-settings-{}", std::process::id()));
        let s = Settings { mode: Some(Mode::Guided), ..Settings::default() };
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Settings::load(&dir), Settings::default());
    }
}

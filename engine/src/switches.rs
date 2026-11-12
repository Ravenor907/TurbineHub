//! Per-game switches: the short list of things a player can turn on for one game, and the file that remembers them.
//! The compatibility tool reads the same file when the game starts and sets what each switch stands for.

use crate::json::{self, Json, quote};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Switch {
    Fsr4,
    FrameGen,
    DeckMode,
    ProtonLog,
}

impl Switch {
    pub const ALL: &[Switch] = &[Switch::Fsr4, Switch::FrameGen, Switch::DeckMode, Switch::ProtonLog];

    pub fn key(self) -> &'static str {
        match self {
            Switch::Fsr4 => "fsr4",
            Switch::FrameGen => "framegen",
            Switch::DeckMode => "deck",
            Switch::ProtonLog => "log",
        }
    }

    pub fn from_key(k: &str) -> Option<Switch> {
        Switch::ALL.iter().copied().find(|s| s.key() == k)
    }

    pub fn title(self) -> &'static str {
        match self {
            Switch::Fsr4 => "FSR 4 upgrade",
            Switch::FrameGen => "Frame generation",
            Switch::DeckMode => "Steam Deck mode",
            Switch::ProtonLog => "Proton log",
        }
    }

    pub fn line(self) -> &'static str {
        match self {
            Switch::Fsr4 => "Games with FSR 3.1 use the sharper FSR 4. Costs a few frames.",
            Switch::FrameGen => "Lossless Scaling frame generation for this game.",
            Switch::DeckMode => "The game sees a handheld and uses its handheld settings.",
            Switch::ProtonLog => "Proton writes a log of the next launch, for a report.",
        }
    }

    /// The environment variable the switch sets for the game. Frame generation wraps the game's command instead.
    pub fn env(self) -> Option<(&'static str, &'static str)> {
        match self {
            Switch::Fsr4 => Some(("PROTON_FSR4_UPGRADE", "1")),
            Switch::DeckMode => Some(("SteamDeck", "1")),
            Switch::ProtonLog => Some(("PROTON_LOG", "1")),
            Switch::FrameGen => None,
        }
    }
}

/// Which switches are on, per game id ("steam:1245620"). Games with nothing on aren't kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameSwitches {
    games: Vec<(String, Vec<Switch>)>,
}

impl GameSwitches {
    pub fn parse(text: &str) -> GameSwitches {
        let mut out = GameSwitches::default();
        let Ok(j) = json::parse(text) else { return out };
        for (id, g) in j.get("games").map(Json::entries).unwrap_or(&[]) {
            for s in g.get("switches").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).filter_map(Switch::from_key) {
                out.set(id, s, true);
            }
        }
        out
    }

    pub fn to_json(&self) -> String {
        let games: Vec<String> = self.games.iter().map(|(id, on)| format!("    {}: {{\"switches\": [{}]}}", quote(id), on.iter().map(|s| quote(s.key())).collect::<Vec<_>>().join(", "))).collect();
        if games.is_empty() { "{\n  \"games\": {}\n}\n".into() } else { format!("{{\n  \"games\": {{\n{}\n  }}\n}}\n", games.join(",\n")) }
    }

    /// The switches that are on for a game, in the fixed order.
    pub fn of(&self, game_id: &str) -> &[Switch] {
        self.games.iter().find(|(id, _)| id == game_id).map(|(_, s)| s.as_slice()).unwrap_or(&[])
    }

    pub fn is_on(&self, game_id: &str, s: Switch) -> bool {
        self.of(game_id).contains(&s)
    }

    pub fn set(&mut self, game_id: &str, s: Switch, on: bool) {
        if game_id.is_empty() || game_id.len() > 200 {
            return;
        }
        let at = match self.games.iter().position(|(id, _)| id == game_id) {
            Some(i) => i,
            None if on => {
                self.games.push((game_id.to_string(), Vec::new()));
                self.games.sort_by(|a, b| a.0.cmp(&b.0));
                self.games.iter().position(|(id, _)| id == game_id).unwrap_or(0)
            }
            None => return,
        };
        let list = &mut self.games[at].1;
        list.retain(|x| *x != s);
        if on {
            list.push(s);
            list.sort();
        }
        if list.is_empty() {
            self.games.remove(at);
        }
    }

    pub fn games(&self) -> impl Iterator<Item = &str> {
        self.games.iter().map(|(id, _)| id.as_str())
    }

    pub fn file(config_home: &Path) -> PathBuf {
        config_home.join("turbinehub/games.json")
    }

    pub fn load(config_home: &Path) -> GameSwitches {
        std::fs::read_to_string(Self::file(config_home)).map(|t| GameSwitches::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self, config_home: &Path) -> std::io::Result<()> {
        crate::write_atomic(&Self::file(config_home), self.to_json().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_round_trip_and_stay_tidy() {
        let mut g = GameSwitches::default();
        assert_eq!(GameSwitches::parse(&g.to_json()), g);
        g.set("steam:1245620", Switch::ProtonLog, true);
        g.set("steam:1245620", Switch::Fsr4, true);
        g.set("steam:1245620", Switch::Fsr4, true);
        g.set("epic:Fortnite", Switch::DeckMode, true);
        g.set("steam:1", Switch::Fsr4, false);
        assert_eq!(g.of("steam:1245620"), &[Switch::Fsr4, Switch::ProtonLog]);
        assert_eq!(g.games().collect::<Vec<_>>(), vec!["epic:Fortnite", "steam:1245620"]);
        assert_eq!(GameSwitches::parse(&g.to_json()), g);
        g.set("epic:Fortnite", Switch::DeckMode, false);
        assert_eq!(g.games().count(), 1);
        assert!(g.is_on("steam:1245620", Switch::ProtonLog) && !g.is_on("steam:1245620", Switch::DeckMode));
    }

    #[test]
    fn a_damaged_file_gives_what_can_be_read() {
        assert_eq!(GameSwitches::parse("nope"), GameSwitches::default());
        let g = GameSwitches::parse(r#"{"games":{"steam:7":{"switches":["deck","turbo",3,"deck"]},"steam:8":{"switches":[]},"steam:9":7}}"#);
        assert_eq!(g.of("steam:7"), &[Switch::DeckMode]);
        assert_eq!(g.games().count(), 1);
    }

    #[test]
    fn every_switch_has_a_key_and_words() {
        for s in Switch::ALL {
            assert_eq!(Switch::from_key(s.key()), Some(*s));
            assert!(!s.title().is_empty() && s.line().ends_with('.'));
        }
        assert_eq!(Switch::FrameGen.env(), None);
        assert_eq!(Switch::DeckMode.env(), Some(("SteamDeck", "1")));
    }
}

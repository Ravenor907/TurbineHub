//! Per-game switches: the short list of things a player can turn on for one game, and the file that remembers them
//! (games.json in Turbine Hub's folder). The launch tool reads the same file when a game starts and sets what each
//! switch stands for, so the file keeps one game per line: the tool's script finds a game's line without a JSON
//! reader.

use crate::json::{self, Json, quote};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Switch {
    Fsr4,
    FrameGen,
    Handheld,
    ProtonLog,
}

impl Switch {
    pub const ALL: &[Switch] = &[Switch::Fsr4, Switch::FrameGen, Switch::Handheld, Switch::ProtonLog];

    pub fn key(self) -> &'static str {
        match self {
            Switch::Fsr4 => "fsr4",
            Switch::FrameGen => "framegen",
            Switch::Handheld => "handheld",
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
            Switch::Handheld => "Handheld mode",
            Switch::ProtonLog => "Proton log",
        }
    }

    pub fn line(self) -> &'static str {
        match self {
            Switch::Fsr4 => "Games with FSR 3.1 use the sharper FSR 4. Costs a few frames.",
            Switch::FrameGen => "Lossless Scaling frame generation for this game.",
            Switch::Handheld => "The game sees a handheld and uses its handheld settings.",
            Switch::ProtonLog => "Proton writes a log of the next launch, for a report.",
        }
    }

    /// The environment variable the switch sets for the game. Frame generation wraps the game's command instead.
    pub fn env(self) -> Option<(&'static str, &'static str)> {
        match self {
            Switch::Fsr4 => Some(("PROTON_FSR4_UPGRADE", "1")),
            Switch::Handheld => Some(("SteamDeck", "1")),
            Switch::ProtonLog => Some(("PROTON_LOG", "1")),
            Switch::FrameGen => None,
        }
    }
}

/// Which switches are on, per game id ("steam:1245620"), and the one switch for every game. Games with nothing on
/// aren't kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameSwitches {
    /// FSR 4 for every Proton game that isn't locked.
    pub fsr4_all: bool,
    pub games: BTreeMap<String, BTreeSet<Switch>>,
}

impl GameSwitches {
    pub fn parse(text: &str) -> GameSwitches {
        let mut out = GameSwitches::default();
        let Ok(j) = json::parse(text) else { return out };
        out.fsr4_all = j.get("fsr4All").and_then(Json::as_bool).unwrap_or(false);
        for (id, g) in j.get("games").map(Json::entries).unwrap_or(&[]) {
            for s in g.get("switches").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).filter_map(Switch::from_key) {
                out.set(id, s, true);
            }
        }
        out
    }

    /// One game per line, each line starting with the game's id in quotes: the launch tool's script looks for
    /// exactly that.
    pub fn to_json(&self) -> String {
        let games: Vec<String> = self.games.iter().map(|(id, on)| format!("    {}: {{\"switches\": [{}]}}", quote(id), on.iter().map(|s| quote(s.key())).collect::<Vec<_>>().join(", "))).collect();
        let body = if games.is_empty() { "{}".to_string() } else { format!("{{\n{}\n  }}", games.join(",\n")) };
        format!("{{\n  \"fsr4All\": {},\n  \"games\": {body}\n}}\n", self.fsr4_all)
    }

    /// The switches that are on for a game, in the fixed order.
    pub fn of(&self, game_id: &str) -> Vec<Switch> {
        self.games.get(game_id).map(|s| s.iter().copied().collect()).unwrap_or_default()
    }

    pub fn is_on(&self, game_id: &str, s: Switch) -> bool {
        self.games.get(game_id).is_some_and(|x| x.contains(&s))
    }

    /// `is_on` by the switch's key; false for a word that is no switch.
    pub fn on(&self, game_id: &str, key: &str) -> bool {
        Switch::from_key(key).is_some_and(|s| self.is_on(game_id, s))
    }

    /// What the launch tool applies for a game: its own switches, and FSR 4 when that is on for every game.
    pub fn applied(&self, game_id: &str) -> Vec<Switch> {
        let mut on: BTreeSet<Switch> = self.games.get(game_id).cloned().unwrap_or_default();
        if self.fsr4_all {
            on.insert(Switch::Fsr4);
        }
        on.into_iter().collect()
    }

    pub fn set(&mut self, game_id: &str, s: Switch, on: bool) {
        if game_id.is_empty() || game_id.len() > 200 || game_id.contains(['"', '\\', '\n']) {
            return;
        }
        if on {
            self.games.entry(game_id.to_string()).or_default().insert(s);
        } else if let Some(set) = self.games.get_mut(game_id) {
            set.remove(&s);
            if set.is_empty() {
                self.games.remove(game_id);
            }
        }
    }

    /// Flips one switch by its key and answers its new state. A locked game is refused and nothing changes.
    pub fn toggle(&mut self, game_id: &str, key: &str, locked: bool) -> Result<bool, &'static str> {
        if locked {
            return Err("Its anti-cheat runs online, so Turbine Hub leaves this game alone.");
        }
        let Some(s) = Switch::from_key(key) else { return Err("Unknown switch.") };
        let now = !self.is_on(game_id, s);
        self.set(game_id, s, now);
        Ok(now)
    }

    /// The file in Turbine Hub's own folder.
    pub fn file(folder: &Path) -> PathBuf {
        folder.join("games.json")
    }

    pub fn load(folder: &Path) -> GameSwitches {
        std::fs::read_to_string(Self::file(folder)).map(|t| GameSwitches::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self, folder: &Path) {
        let _ = crate::write_atomic(&Self::file(folder), self.to_json().as_bytes());
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
        g.set("epic:Fortnite", Switch::Handheld, true);
        g.set("steam:1", Switch::Fsr4, false);
        assert_eq!(g.of("steam:1245620"), vec![Switch::Fsr4, Switch::ProtonLog]);
        assert_eq!(g.games.keys().collect::<Vec<_>>(), vec!["epic:Fortnite", "steam:1245620"]);
        assert_eq!(GameSwitches::parse(&g.to_json()), g);
        g.set("epic:Fortnite", Switch::Handheld, false);
        assert_eq!(g.games.len(), 1);
        assert!(g.is_on("steam:1245620", Switch::ProtonLog) && !g.is_on("steam:1245620", Switch::Handheld));
        // An id that could break the one-line-per-game file is never kept.
        g.set("steam:\"1\n", Switch::Fsr4, true);
        assert_eq!(g.games.len(), 1);
    }

    #[test]
    fn the_file_keeps_one_game_per_line_for_the_launch_tool() {
        let mut g = GameSwitches { fsr4_all: true, ..GameSwitches::default() };
        g.set("steam:7", Switch::FrameGen, true);
        g.set("steam:70", Switch::Handheld, true);
        assert_eq!(g.to_json(), "{\n  \"fsr4All\": true,\n  \"games\": {\n    \"steam:7\": {\"switches\": [\"framegen\"]},\n    \"steam:70\": {\"switches\": [\"handheld\"]}\n  }\n}\n");
        assert_eq!(g.applied("steam:7"), vec![Switch::Fsr4, Switch::FrameGen]);
        assert_eq!(g.applied("steam:8"), vec![Switch::Fsr4]);
        assert_eq!(GameSwitches::default().to_json(), "{\n  \"fsr4All\": false,\n  \"games\": {}\n}\n");
    }

    #[test]
    fn a_damaged_file_gives_what_can_be_read() {
        assert_eq!(GameSwitches::parse("nope"), GameSwitches::default());
        let g = GameSwitches::parse(r#"{"games":{"steam:7":{"switches":["handheld","turbo",3,"handheld"]},"steam:8":{"switches":[]},"steam:9":7}}"#);
        assert_eq!(g.of("steam:7"), vec![Switch::Handheld]);
        assert_eq!(g.games.len(), 1);
    }

    #[test]
    fn toggling_is_refused_for_locked_games_and_unknown_words() {
        let mut g = GameSwitches::default();
        assert_eq!(g.toggle("steam:1", "fsr4", false), Ok(true));
        assert_eq!(g.toggle("steam:1", "framegen", false), Ok(true));
        assert!(g.toggle("steam:1", "rm", false).is_err());
        assert!(g.toggle("steam:2", "fsr4", true).is_err());
        assert!(g.on("steam:1", "fsr4") && !g.on("steam:1", "rm") && !g.on("steam:2", "fsr4"));
        assert_eq!(g.toggle("steam:1", "fsr4", false), Ok(false));
        assert_eq!(g.toggle("steam:1", "framegen", false), Ok(false));
        assert!(g.games.is_empty());
    }

    #[test]
    fn every_switch_has_a_key_and_words() {
        for s in Switch::ALL {
            assert_eq!(Switch::from_key(s.key()), Some(*s));
            assert!(!s.title().is_empty() && s.line().ends_with('.'));
        }
        assert_eq!(Switch::FrameGen.env(), None);
        assert_eq!(Switch::Handheld.env(), Some(("SteamDeck", "1")));
    }
}

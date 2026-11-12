//! One glossary for every player-facing word: the term Turbine Hub uses, what it means in plain words, and what it's
//! called on Windows. Screens and Help read it from here, and `jargon` keeps technical words out of what players read.

pub struct Term {
    pub term: &'static str,
    pub means: &'static str,
    /// The closest thing on Windows, when there is one.
    pub windows: &'static str,
}

pub const TERMS: &[Term] = &[
    Term { term: "Game Mode", means: "The full-screen console view SteamOS starts in.", windows: "Steam Big Picture, but it's the whole system" },
    Term { term: "Desktop Mode", means: "A regular desktop with windows, a mouse and a file manager.", windows: "The Windows desktop" },
    Term { term: "Proton", means: "What lets Windows games run on SteamOS. Steam picks one per game.", windows: "" },
    Term { term: "Windows files", means: "The folder of Windows bits each game gets, where many games keep their saves and settings.", windows: "C:\\Users and C:\\Windows, one per game" },
    Term { term: "Game switches", means: "Per-game options like FSR 4 and frame generation.", windows: "AMD Adrenalin or NVIDIA app game profiles" },
    Term { term: "Power limit", means: "How much power the processor may use. Lower is quieter and lasts longer.", windows: "Armoury Crate operating modes" },
    Term { term: "Game Boost", means: "Full processor speed while a game runs, quiet the rest of the time.", windows: "The High performance power plan" },
    Term { term: "Tune-up", means: "Memory and kernel settings that make games smoother.", windows: "" },
    Term { term: "Decky Loader", means: "Adds plugins to Steam's quick menu.", windows: "" },
    Term { term: "Quick panel", means: "Turbine Hub's page in Steam's quick menu while you play.", windows: "Xbox Game Bar" },
    Term { term: "Game library", means: "A drive or folder Steam installs games to.", windows: "A Steam library folder" },
    Term { term: "Password", means: "The SteamOS password for system changes.", windows: "The admin password" },
    Term { term: "Review changes", means: "Every system change Turbine Hub made, each with Undo.", windows: "System Restore, one change at a time" },
];

/// Technical words that shouldn't reach a player in Guided mode, and the word to use instead.
pub const JARGON: &[(&str, &str)] = &[
    ("prefix", "Windows files"),
    ("compatdata", "Windows files"),
    ("sysctl", "kernel setting"),
    ("flatpak", "app"),
    ("TDP", "power limit"),
    ("EPP", "Game Boost"),
    ("polkit", "permission"),
    ("sudo", "password"),
    ("udev", "device rule"),
];

/// The jargon words in a player-facing text, with their plain replacement.
pub fn jargon(text: &str) -> Vec<(&'static str, &'static str)> {
    let words: Vec<String> = text.split(|c: char| !c.is_ascii_alphanumeric()).map(str::to_ascii_lowercase).collect();
    JARGON.iter().filter(|(j, _)| words.iter().any(|w| w == &j.to_ascii_lowercase())).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_jargon_as_whole_words() {
        assert_eq!(jargon("Reset the game's prefix"), vec![("prefix", "Windows files")]);
        assert!(jargon("Prefixes aside, nothing").is_empty());
        assert!(jargon("Set a power limit").is_empty());
    }

    #[test]
    fn glossary_words_are_plain() {
        for t in TERMS {
            assert!(jargon(t.means).is_empty(), "{}: {:?}", t.term, jargon(t.means));
            assert!(!t.means.contains(['\u{2013}', '\u{2014}']));
        }
    }
}

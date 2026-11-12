//! One glossary for every player-facing word: the term Turbine Hub uses, what it means in plain words, and what it's
//! called on Windows. Screens and Help read it from here, and `jargon` keeps technical words out of what players read.

pub struct Term {
    pub term: &'static str,
    pub means: &'static str,
    /// The closest thing on Windows, when there is one.
    pub windows: &'static str,
}

pub const TERMS: &[Term] = &[
    Term { term: "Options", means: "A menu of everything the highlighted thing and its page can do. X on a controller, R on a keyboard, a right-click with a mouse.", windows: "The right-click menu" },
    Term { term: "Find", means: "The search box of a page with a list. Y on a controller, F on a keyboard.", windows: "Ctrl+F" },
    Term { term: "Pointer", means: "The arrow the right stick moves. R3 clicks with it, and R3 held opens Options.", windows: "The mouse pointer" },
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
    Term { term: "Review changes", means: "Every change Turbine Hub made, each with Undo or the place to undo it.", windows: "System Restore, one change at a time" },
    Term { term: "Kernel", means: "The core of the system that talks to the hardware. Its settings decide how memory, the processor and devices behave.", windows: "The Windows core and its registry settings" },
    Term { term: "zram", means: "Memory that squeezes what's in it, so more fits before games run short. SteamOS uses it as its swap.", windows: "Memory compression" },
    Term { term: "Swap", means: "Room that takes what doesn't fit in memory any more, so a game slows down a little instead of crashing.", windows: "The page file" },
    Term { term: "Huge pages", means: "Memory handed out in bigger pieces, which games can reach a little faster.", windows: "Large pages" },
    Term { term: "Split lock", means: "A rare way some older games use the processor. The kernel slows them down on purpose when they do it; the tune-up turns that off.", windows: "" },
    Term { term: "NTSync", means: "A faster way for Windows games to keep their parts in step, the way Windows itself does it. Newer Proton versions use it.", windows: "How Windows syncs a game's threads" },
    Term { term: "FSR 4", means: "AMD's sharpest upscaling: the game draws fewer pixels and FSR 4 fills in the rest, for more frames that still look sharp. It needs a recent Radeon graphics card.", windows: "FSR 4 in AMD Adrenalin" },
    Term { term: "Frame generation", means: "Extra frames drawn between the game's own, so motion looks smoother. It adds a little delay to your presses.", windows: "AMD Fluid Motion Frames, Lossless Scaling" },
    Term { term: "Lossless Scaling", means: "A Decky plugin that adds frame generation to any game.", windows: "Lossless Scaling on Steam" },
    Term { term: "Compatibility tool", means: "What Steam starts a game with: a Proton version, or a launch tool like Turbine Hub's that adds the game's switches. Set per game in its Properties, Compatibility.", windows: "" },
    Term { term: "Launch tool", means: "Turbine Hub's compatibility tool. Picked for a game in Steam, it carries the game's switches and Proton pick into the game.", windows: "" },
    Term { term: "GE-Proton", means: "A Proton version made by the community with extra fixes. ProtonPlus installs it.", windows: "" },
    Term { term: "Flatpak", means: "How apps from the Store are packed: each comes with what it needs and runs fenced off from the system, so it can't break SteamOS.", windows: "Store apps from the Microsoft Store" },
    Term { term: "Flathub", means: "The big free app store SteamOS apps come from. The Store tab and Discover install from it.", windows: "The Microsoft Store" },
    Term { term: "Flatseal", means: "An app that shows and changes which folders and devices each installed app may reach.", windows: "Settings, Privacy and security, App permissions" },
    Term { term: "Ludusavi", means: "An app that knows where thousands of games keep their saves and backs them up. Turbine Hub uses it for save snapshots.", windows: "" },
    Term { term: "ProtonPlus", means: "An app that installs other Proton versions, such as GE-Proton, for games Steam's own Proton has trouble with.", windows: "" },
    Term { term: "Decky", means: "Short for Decky Loader, which adds plugins to Steam's quick menu.", windows: "" },
    Term { term: "Developer mode (Decky)", means: "A switch in Decky Loader's own settings that adds a page for installing a plugin from an address. It doesn't change SteamOS and can go off again.", windows: "" },
    Term { term: "MangoHud", means: "The overlay that shows frame rate, temperatures and load over a game. MangoJuice sets it up.", windows: "MSI Afterburner's overlay" },
    Term { term: "Steam Input", means: "Steam's controller settings per game: button layouts, gyro, and controllers that act as a mouse and keyboard.", windows: "DS4Windows" },
    Term { term: "VRR", means: "Variable refresh: the screen waits for each new frame, so motion stays smooth when the frame rate drops.", windows: "FreeSync or G-Sync" },
    Term { term: "HDR", means: "Brighter highlights and deeper colors, in games made for it, on screens that have it.", windows: "Use HDR in Display settings" },
    Term { term: "TDP", means: "How much power the processor may use, in watts. Turbine Hub calls it the power limit.", windows: "Armoury Crate operating modes" },
    Term { term: "Refresh rate", means: "How many times a second the screen shows a new picture, in Hz. Higher looks smoother.", windows: "Display settings, Refresh rate" },
    Term { term: "Interface size", means: "How big Turbine Hub draws everything, 50 to 150%. Words stay readable and buttons stay big enough for a finger at every size.", windows: "Scale in Display settings" },
    Term { term: "Color vision", means: "Which colors say fine, needs a look and won't work. Red-weak, Green-weak and Blue-weak pick colors those eyes tell apart; No color uses none. Each state keeps its own mark and word. In Settings, Look.", windows: "Color filters in Accessibility settings" },
    Term { term: "Animations", means: "How screens change. Full fades and glides, Reduced only fades (nothing slides or grows), Off changes everything at once. In Settings, Look.", windows: "Animation effects in Accessibility settings" },
    Term { term: "Screen reader", means: "A program that reads the screen out loud, such as Orca in Desktop Mode. Turbine Hub tells it the name, the kind and the state of whatever the highlight is on.", windows: "Narrator" },
    Term { term: "Bazzite", means: "Another gaming system based on Linux, like SteamOS. Some tune-up settings come from it.", windows: "" },
    Term { term: "CachyOS", means: "A Linux system tuned for speed. Some tune-up settings come from it.", windows: "" },
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

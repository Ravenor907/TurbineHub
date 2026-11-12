//! Bring it over from Windows: what's on the player's Windows drive (Steam games to copy instead of downloading again,
//! Windows apps they use) and what each Windows app becomes on SteamOS. The Windows drive is only ever read: it is
//! mounted read only by the app, and nothing here writes to it.

use crate::vdf;
use std::fs;
use std::path::{Path, PathBuf};

/// A Steam game installed on the Windows drive.
#[derive(Debug, Clone, PartialEq)]
pub struct WinGame {
    pub app_id: String,
    pub name: String,
    /// The game's folder (steamapps/common/<installdir>).
    pub dir: PathBuf,
    pub manifest: PathBuf,
    pub bytes: u64,
}

/// Steam libraries on a Windows partition: the default one under Program Files and any "SteamLibrary" folder at the
/// top of the drive (the name Steam suggests). Each is a steamapps folder.
pub fn steam_libraries(mount: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for c in ["Program Files (x86)/Steam/steamapps", "Program Files/Steam/steamapps", "SteamLibrary/steamapps", "Steam/steamapps", "Games/SteamLibrary/steamapps"] {
        let p = mount.join(c);
        if p.is_dir() && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// The installed games in those libraries, sorted by name. Only fully installed ones (StateFlags 4).
pub fn steam_games(mount: &Path) -> Vec<WinGame> {
    let mut games = Vec::new();
    for lib in steam_libraries(mount) {
        let Ok(dir) = fs::read_dir(&lib) else { continue };
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(e.path()) else { continue };
            let Ok(doc) = vdf::parse(&text) else { continue };
            let s = |k: &str| doc.str_at(&["AppState", k]).unwrap_or("").to_string();
            let (app_id, install_dir) = (s("appid"), s("installdir"));
            let installed = s("StateFlags").parse::<u32>().map(|f| f & 4 != 0).unwrap_or(false);
            if app_id.is_empty() || !app_id.bytes().all(|b| b.is_ascii_digit()) || install_dir.is_empty() || install_dir.contains(['/', '\\']) || install_dir == ".." || !installed {
                continue;
            }
            let dir = lib.join("common").join(&install_dir);
            if dir.is_dir() {
                games.push(WinGame { name: s("name"), bytes: s("SizeOnDisk").parse().unwrap_or(0), app_id, dir, manifest: e.path() });
            }
        }
    }
    games.sort_by_key(|g| g.name.to_lowercase());
    games.dedup_by(|a, b| a.app_id == b.app_id);
    games
}

/// Where a game is put in a SteamOS library: its folder under steamapps/common and its manifest next to it. Steam
/// sees the game at its next start and checks the files instead of downloading them.
pub fn copy_plan(g: &WinGame, steamapps: &Path) -> Vec<(PathBuf, PathBuf)> {
    let name = g.dir.file_name().map(PathBuf::from).unwrap_or_default();
    vec![(g.dir.clone(), steamapps.join("common").join(name)), (g.manifest.clone(), steamapps.join(format!("appmanifest_{}.acf", g.app_id)))]
}

/// What a Windows app becomes on SteamOS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Becomes {
    /// The same or an equivalent app from Flathub.
    Flatpak(&'static str),
    /// A library tile added to Steam (stores and launchers).
    Tile(&'static str),
    /// Nothing to install: SteamOS or Steam already does it, or a Turbine Hub switch does.
    BuiltIn,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppMap {
    pub id: &'static str,
    /// Its name on Windows.
    pub windows: &'static str,
    /// What it becomes, in words.
    pub steamos: &'static str,
    pub note: &'static str,
    pub becomes: Becomes,
    /// Where it lives on a Windows drive (relative to the drive, or to a user's folder when it starts with "~/").
    pub found_at: &'static [&'static str],
}

/// The Windows apps Turbine Hub knows, in the order shown.
pub const APPS: &[AppMap] = &[
    AppMap { id: "discord", windows: "Discord", steamos: "Discord", note: "Same app, same account", becomes: Becomes::Flatpak("com.discordapp.Discord"), found_at: &["~/AppData/Local/Discord"] },
    AppMap { id: "afterburner", windows: "MSI Afterburner", steamos: "MangoHud and MangoJuice", note: "FPS and temps on screen", becomes: Becomes::Flatpak("io.github.radiolamp.mangojuice"), found_at: &["Program Files (x86)/MSI Afterburner"] },
    AppMap { id: "ds4windows", windows: "DS4Windows", steamos: "Steam Input", note: "Already part of Steam", becomes: Becomes::BuiltIn, found_at: &["~/AppData/Roaming/DS4Windows", "Program Files/DS4Windows"] },
    AppMap { id: "xbox", windows: "Xbox app", steamos: "Xbox Cloud", note: "Game Pass in your library", becomes: Becomes::Tile("xcloud"), found_at: &["XboxGames"] },
    AppMap { id: "gamebar", windows: "Xbox Game Bar", steamos: "Steam recording", note: "Record from Steam's overlay", becomes: Becomes::BuiltIn, found_at: &[] },
    AppMap { id: "adrenalin", windows: "AMD Software: Adrenalin", steamos: "Game switches", note: "FSR 4 and frame generation per game", becomes: Becomes::BuiltIn, found_at: &["Program Files/AMD/CNext"] },
    AppMap { id: "nvidiaapp", windows: "NVIDIA app", steamos: "Game switches", note: "Upscaling and frame generation per game", becomes: Becomes::BuiltIn, found_at: &["Program Files/NVIDIA Corporation/NVIDIA app"] },
    AppMap { id: "obs", windows: "OBS Studio", steamos: "OBS Studio", note: "Same app", becomes: Becomes::Flatpak("com.obsproject.Studio"), found_at: &["Program Files/obs-studio"] },
    AppMap { id: "spotify", windows: "Spotify", steamos: "Spotify", note: "Same app", becomes: Becomes::Flatpak("com.spotify.Client"), found_at: &["~/AppData/Roaming/Spotify"] },
    AppMap { id: "chrome", windows: "Chrome", steamos: "Chrome", note: "Same browser, sign in to sync", becomes: Becomes::Flatpak("com.google.Chrome"), found_at: &["Program Files/Google/Chrome", "Program Files (x86)/Google/Chrome"] },
    AppMap { id: "firefox", windows: "Firefox", steamos: "Firefox", note: "Same browser, sign in to sync", becomes: Becomes::Flatpak("org.mozilla.firefox"), found_at: &["Program Files/Mozilla Firefox"] },
    AppMap { id: "edge", windows: "Microsoft Edge", steamos: "Microsoft Edge", note: "Same browser, sign in to sync", becomes: Becomes::Flatpak("com.microsoft.Edge"), found_at: &[] },
    AppMap { id: "epic", windows: "Epic Games Launcher", steamos: "Heroic Games Launcher", note: "Your Epic games in your library", becomes: Becomes::Flatpak("com.heroicgameslauncher.hgl"), found_at: &["Program Files (x86)/Epic Games", "Program Files/Epic Games"] },
    AppMap { id: "gog", windows: "GOG Galaxy", steamos: "Heroic Games Launcher", note: "Your GOG games in your library", becomes: Becomes::Flatpak("com.heroicgameslauncher.hgl"), found_at: &["Program Files (x86)/GOG Galaxy"] },
    AppMap { id: "battlenet", windows: "Battle.net", steamos: "Battle.net", note: "Added as a library tile", becomes: Becomes::Tile("battlenet"), found_at: &["Program Files (x86)/Battle.net"] },
    AppMap { id: "ea", windows: "EA app", steamos: "EA app", note: "Added as a library tile", becomes: Becomes::Tile("ea"), found_at: &["Program Files/Electronic Arts/EA Desktop"] },
    AppMap { id: "ubisoft", windows: "Ubisoft Connect", steamos: "Ubisoft Connect", note: "Added as a library tile", becomes: Becomes::Tile("ubisoft"), found_at: &["Program Files (x86)/Ubisoft/Ubisoft Game Launcher"] },
    AppMap { id: "moonlight", windows: "Moonlight", steamos: "Moonlight", note: "Same app", becomes: Becomes::Flatpak("com.moonlight_stream.Moonlight"), found_at: &["Program Files/Moonlight Game Streaming"] },
    AppMap { id: "playnite", windows: "Playnite", steamos: "Steam library", note: "Every store's games in one library", becomes: Becomes::BuiltIn, found_at: &["~/AppData/Local/Playnite"] },
];

/// The user folders on a Windows drive (Users/<name>, without the built-in ones).
pub fn windows_users(mount: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(mount.join("Users"))
        .map(|d| d.flatten().filter(|e| e.path().is_dir()).filter(|e| !["Public", "Default", "Default User", "All Users"].contains(&e.file_name().to_string_lossy().as_ref())).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// The apps from `APPS` found on this Windows drive.
pub fn apps_found(mount: &Path) -> Vec<&'static str> {
    let users = windows_users(mount);
    APPS.iter()
        .filter(|a| {
            a.found_at.iter().any(|p| match p.strip_prefix("~/") {
                Some(rest) => users.iter().any(|u| u.join(rest).exists()),
                None => mount.join(p).exists(),
            })
        })
        .map(|a| a.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_has_words_and_a_valid_target() {
        let mut ids = std::collections::HashSet::new();
        for a in APPS {
            assert!(ids.insert(a.id), "{} twice", a.id);
            assert!(!a.windows.is_empty() && !a.steamos.is_empty() && !a.note.is_empty());
            if let Becomes::Flatpak(id) = a.becomes {
                assert!(id.split('.').count() >= 3, "{id}");
            }
        }
    }
}

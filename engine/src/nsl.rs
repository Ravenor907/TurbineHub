//! NonSteamLaunchers (github.com/moraroy/NonSteamLaunchers-On-Steam-Deck): a free project that installs Windows game
//! launchers (Battle.net, EA app, Ubisoft Connect and many more) through Proton and adds them and their games to the
//! Steam library. Turbine Hub never installs it through its helper and needs no password for it: it fetches the
//! project's own script from a release tag and starts it on the host, where it shows its own windows (zenity), so
//! it runs in Desktop Mode.
//!
//! What the project's files at release v4.2.94 do, as read for this module:
//! - `NonSteamLaunchers.desktop` (the release asset players normally run) is one line:
//!   `curl -Ls https://raw.githubusercontent.com/moraroy/NonSteamLaunchers-On-Steam-Deck/main/NonSteamLaunchers.sh
//!   | nohup /bin/bash`. Turbine Hub runs the same script, but from the release tag instead of `main`.
//! - `NonSteamLaunchers.sh` carries its own version (`version=v4.2.94`), logs to
//!   `~/Downloads/NonSteamLaunchers-install.log`, installs GE-Proton and UMU, puts every launcher into one set of
//!   Windows files, `steamapps/compatdata/NonSteamLaunchers` (or one per launcher with "separate app IDs", e.g.
//!   `compatdata/Battle.netLauncher`), adds the shortcuts to Steam, and sets up a game scanner as a user service:
//!   `~/.config/systemd/user/NSLGameScanner.py` and `nslgamescanner.service`. The scanner itself is fetched from the
//!   project's `main` branch by the script; that part is the project's own. The first run restarts Steam once
//!   (after it creates `.cef-enable-remote-debugging` in Steam's folder).
//! - Removing: its window has Uninstall (one launcher) and Start Fresh (everything it added: the Windows files of its
//!   launchers, their shader cache, the scanner and its service; game saves in `~/NSLGameSaves` stay).
//! - `NSLPlugin.desktop` runs `NSLPluginInstaller.sh` the same way. That installer asks for the SteamOS password in
//!   its own zenity window (`sudo -S`, because Decky's plugin folder belongs to root), refuses to run without Decky
//!   Loader (`~/homebrew/plugins`), and copies the plugin from github.com/moraroy/NonSteamLaunchersDecky (its `main`
//!   branch) into `~/homebrew/plugins/NonSteamLaunchers`, whose package.json is named "nonsteamlaunchers".
//!
//! The project is no longer in Decky's plugin store (its own script says so), so that is not a way to get it.

use crate::json;
use std::path::{Path, PathBuf};

/// The project on GitHub, as owner/name.
pub const REPO: &str = "moraroy/NonSteamLaunchers-On-Steam-Deck";
/// The project's page, for the words on screen.
pub const PAGE: &str = "github.com/moraroy/NonSteamLaunchers-On-Steam-Deck";
/// Where its Decky plugin comes from (the installer fetches it itself).
pub const PLUGIN_PAGE: &str = "github.com/moraroy/NonSteamLaunchersDecky";
/// The script that shows its window and does the work.
pub const SCRIPT: &str = "NonSteamLaunchers.sh";
/// The script that installs its Decky plugin.
pub const PLUGIN_SCRIPT: &str = "NSLPluginInstaller.sh";
/// The plugin's name in its package.json, whatever its folder among Decky's plugins is called.
pub const PLUGIN_PACKAGE: &str = "nonsteamlaunchers";
/// The one set of Windows files all launchers share (under Steam's compatdata folder).
pub const SHARED: &str = "NonSteamLaunchers";
/// The game scanner the script sets up, under the home folder.
pub const SCANNER: &str = ".config/systemd/user/NSLGameScanner.py";
/// Turbine Hub's note of the release it last started, in its own folder.
const RECORD: &str = "nsl-version";

/// A launcher NonSteamLaunchers can install.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Launcher {
    pub id: &'static str,
    /// Its name in Turbine Hub.
    pub name: &'static str,
    /// Its name in NonSteamLaunchers' list, the one to tick.
    pub tick: &'static str,
    /// Its own Windows files when the player picked "separate app IDs".
    pub own: &'static str,
    /// What is there once it is installed, inside the Windows files' drive_c (the script's `_path1`/`_path2`).
    pub file: &'static str,
    /// Heroic Games Launcher does it better: native, with a controller-ready window.
    pub heroic: bool,
}

const fn l(id: &'static str, name: &'static str, tick: &'static str, own: &'static str, file: &'static str, heroic: bool) -> Launcher {
    Launcher { id, name, tick, own, file, heroic }
}

/// The launchers the Store lists, in the order shown. Names and places from NonSteamLaunchers.sh
/// (`CheckInstallations` and the `*_path1` lines). EA and itch.io keep a version folder in their path, so their
/// folder is looked for.
pub const LAUNCHERS: &[Launcher] = &[
    l("battlenet", "Battle.net", "Battle.net", "Battle.netLauncher", "Program Files (x86)/Battle.net/Battle.net Launcher.exe", false),
    l("ea", "EA app", "EA App", "TheEAappLauncher", "Program Files/Electronic Arts/EA Desktop", false),
    l("ubisoft", "Ubisoft Connect", "Ubisoft Connect", "UplayLauncher", "Program Files (x86)/Ubisoft/Ubisoft Game Launcher/upc.exe", false),
    l("rockstar", "Rockstar Games Launcher", "Rockstar Games Launcher", "RockstarGamesLauncher", "Program Files/Rockstar Games/Launcher/Launcher.exe", false),
    l("itchio", "itch.io", "itch.io", "itchioLauncher", "users/steamuser/AppData/Local/itch", false),
    l("legacy", "Legacy Games", "Legacy Games", "LegacyGamesLauncher", "Program Files/Legacy Games/Legacy Games Launcher/Legacy Games Launcher.exe", false),
    l("humble", "Humble Games", "Humble Games Collection", "HumbleGamesLauncher", "Program Files/Humble App/Humble App.exe", false),
    l("epic", "Epic Games Launcher", "Epic Games", "EpicGamesLauncher", "Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe", true),
    l("gog", "GOG Galaxy", "GOG Galaxy", "GogGalaxyLauncher", "Program Files (x86)/GOG Galaxy/GalaxyClient.exe", true),
    l("amazon", "Amazon Games", "Amazon Games", "AmazonGamesLauncher", "users/steamuser/AppData/Local/Amazon Games/App/Amazon Games.exe", true),
];

pub fn launcher(id: &str) -> Option<&'static Launcher> {
    LAUNCHERS.iter().find(|x| x.id == id)
}

fn compatdata(home: &Path) -> PathBuf {
    // The script only ever uses this path (`compatdata_dir` in NonSteamLaunchers.sh).
    home.join(".local/share/Steam/steamapps/compatdata")
}

/// Is this launcher installed by NonSteamLaunchers, in the shared Windows files or its own?
pub fn launcher_here(home: &Path, x: &Launcher) -> bool {
    [SHARED, x.own].iter().any(|d| compatdata(home).join(d).join("pfx/drive_c").join(x.file).exists())
}

/// What is known about NonSteamLaunchers on this machine. Read only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// It ran here at least once: its shared Windows files, a launcher of it, or its game scanner are there.
    pub installed: bool,
    /// The release Turbine Hub last started, when it was Turbine Hub.
    pub version: Option<String>,
    /// The launchers it installed, by id.
    pub launchers: Vec<&'static str>,
    /// Its Decky plugin's version, when the plugin is there ("" when its version isn't written down).
    pub plugin: Option<String>,
}

pub fn state(home: &Path, folder: &Path) -> State {
    let launchers: Vec<&'static str> = LAUNCHERS.iter().filter(|x| launcher_here(home, x)).map(|x| x.id).collect();
    let installed = !launchers.is_empty() || compatdata(home).join(SHARED).join("pfx").is_dir() || home.join(SCANNER).is_file();
    State { installed, version: if installed { recorded(folder) } else { None }, launchers, plugin: plugin_version(home) }
}

/// The plugin's version, from the package.json of the Decky plugin named "nonsteamlaunchers".
pub fn plugin_version(home: &Path) -> Option<String> {
    let dir = std::fs::read_dir(home.join("homebrew/plugins")).ok()?;
    dir.flatten().find_map(|e| {
        let j = std::fs::read_to_string(e.path().join("package.json")).ok().and_then(|t| json::parse(&t).ok())?;
        (j.str_at("name") == Some(PLUGIN_PACKAGE)).then(|| j.str_at("version").unwrap_or("").to_string())
    })
}

/// A release tag as the project writes them ("v4.2.94"): it goes into an address and a file name, so nothing else
/// passes.
pub fn tag_ok(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else { return false };
    !rest.is_empty() && rest.len() <= 24 && rest.bytes().all(|b| b.is_ascii_digit() || b == b'.') && !rest.starts_with('.') && !rest.contains("..")
}

/// The script's address at one release tag (never `main`), on GitHub's raw file host.
pub fn script_url(tag: &str, plugin: bool) -> String {
    format!("https://raw.githubusercontent.com/{REPO}/{tag}/{}", if plugin { PLUGIN_SCRIPT } else { SCRIPT })
}

/// Is this the script that was asked for? A shell script, and the main script names its own version, which must be
/// the tag's: a page of an error or another file never runs.
pub fn script_ok(text: &str, tag: &str, plugin: bool) -> bool {
    if !text.starts_with("#!") {
        return false;
    }
    if plugin {
        text.contains("NonSteamLaunchersDecky") && text.contains("homebrew/plugins")
    } else {
        text.lines().any(|line| line.trim() == format!("version={tag}"))
    }
}

/// Where the fetched script is kept: Turbine Hub's cache folder, which the host sees at the same path.
pub fn script_file(cache: &Path, tag: &str, plugin: bool) -> PathBuf {
    let name = if plugin { PLUGIN_SCRIPT } else { SCRIPT };
    cache.join("nsl").join(format!("{tag}-{name}"))
}

/// The command that starts a fetched script on the host, as its desktop file does (bash, as the player).
pub fn run_line(script: &Path) -> Vec<String> {
    vec!["flatpak-spawn".into(), "--host".into(), "bash".into(), script.display().to_string()]
}

/// The release Turbine Hub last started.
pub fn recorded(folder: &Path) -> Option<String> {
    std::fs::read_to_string(folder.join(RECORD)).ok().map(|t| t.trim().to_string()).filter(|t| tag_ok(t))
}

pub fn remember(folder: &Path, tag: &str) -> std::io::Result<()> {
    crate::write_atomic(&folder.join(RECORD), format!("{tag}\n").as_bytes())
}

/// Is `newest` a later release than `have`? Tags compare number by number ("v4.2.100" is after "v4.2.94").
pub fn newer(newest: &str, have: &str) -> bool {
    let nums = |t: &str| t.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    nums(newest) > nums(have)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-nsl-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn put(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "MZ").unwrap();
    }

    #[test]
    fn finds_it_and_its_launchers_in_either_kind_of_windows_files() {
        let h = home("find");
        let folder = h.join("config/turbinehub");
        assert_eq!(state(&h, &folder), State::default());
        let cd = h.join(".local/share/Steam/steamapps/compatdata");
        // Battle.net in the shared Windows files, EA app (a folder) in its own.
        put(&cd.join("NonSteamLaunchers/pfx/drive_c/Program Files (x86)/Battle.net/Battle.net Launcher.exe"));
        std::fs::create_dir_all(cd.join("TheEAappLauncher/pfx/drive_c/Program Files/Electronic Arts/EA Desktop/13.1")).unwrap();
        let st = state(&h, &folder);
        assert!(st.installed);
        assert_eq!(st.launchers, vec!["battlenet", "ea"]);
        assert_eq!((st.version, st.plugin), (None, None));
        remember(&folder, "v4.2.94").unwrap();
        assert_eq!(state(&h, &folder).version.as_deref(), Some("v4.2.94"));
        // The shared Windows files alone (nothing ticked yet) count as installed; so does the scanner.
        let h2 = home("bare");
        std::fs::create_dir_all(h2.join(".local/share/Steam/steamapps/compatdata/NonSteamLaunchers/pfx")).unwrap();
        assert!(state(&h2, &folder).installed && state(&h2, &folder).launchers.is_empty());
        let h3 = home("scanner");
        put(&h3.join(SCANNER));
        assert!(state(&h3, &folder).installed);
        // Its Decky plugin is found by its package name, whatever its folder.
        let p = h3.join("homebrew/plugins/nsl/package.json");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, r#"{"name":"nonsteamlaunchers","version":"1.3.90"}"#).unwrap();
        assert_eq!(state(&h3, &folder).plugin.as_deref(), Some("1.3.90"));
        for d in [h, h2, h3] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn only_a_release_tag_and_its_own_script_are_used() {
        for good in ["v4.2.94", "v10.0"] {
            assert!(tag_ok(good), "{good}");
        }
        for bad in ["", "v", "4.2.94", "main", "v4.2.94/../x", "v4..2", "v.4", "v4.2.94\n"] {
            assert!(!tag_ok(bad), "{bad:?}");
        }
        let url = script_url("v4.2.94", false);
        assert_eq!(url, "https://raw.githubusercontent.com/moraroy/NonSteamLaunchers-On-Steam-Deck/v4.2.94/NonSteamLaunchers.sh");
        assert!(crate::net::allowed_url(&url) && crate::net::allowed_url(&script_url("v4.2.94", true)));
        let script = "#!/usr/bin/env bash\nset -o pipefail\n# Version number (major.minor)\nversion=v4.2.94\n";
        assert!(script_ok(script, "v4.2.94", false));
        assert!(!script_ok(script, "v4.2.95", false));
        assert!(!script_ok("<!DOCTYPE html>\nversion=v4.2.94\n", "v4.2.94", false));
        assert!(script_ok("#!/bin/bash\nREPO_URL=\"https://github.com/moraroy/NonSteamLaunchersDecky/archive/refs/heads/main.zip\"\nLOCAL_DIR=\"${logged_in_home}/homebrew/plugins/NonSteamLaunchers\"\n", "v4.2.94", true));
        assert!(!script_ok(script, "v4.2.94", true));
        let f = script_file(Path::new("/c"), "v4.2.94", false);
        assert_eq!(f, Path::new("/c/nsl/v4.2.94-NonSteamLaunchers.sh"));
        assert_eq!(run_line(&f), vec!["flatpak-spawn", "--host", "bash", "/c/nsl/v4.2.94-NonSteamLaunchers.sh"]);
        assert!(newer("v4.2.100", "v4.2.94") && !newer("v4.2.94", "v4.2.94") && !newer("v4.1.99", "v4.2.0"));
    }
}

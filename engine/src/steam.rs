//! Finds Steam games: Steam's own folder, every library listed in libraryfolders.vdf (internal, SD cards,
//! other drives), each game's appmanifest, its Proton prefix and the art Steam already keeps on the device.

use crate::games::{Game, Runtime, Scan, Store};
use crate::vdf::{self, Vdf};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Steam's own folders on SteamOS, in the order Steam itself uses them.
pub fn steam_roots(home: &Path) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for rel in [".local/share/Steam", ".steam/steam", ".steam/root"] {
        let p = home.join(rel);
        if !p.join("steamapps").is_dir() {
            continue;
        }
        let real = fs::canonicalize(&p).unwrap_or(p);
        if seen.insert(real.clone()) {
            out.push(real);
        }
    }
    out
}

/// Steam's tools and runtimes show up as apps; they are never games.
fn is_tool(app_id: &str, name: &str) -> bool {
    const IDS: &[&str] = &["228980", "1070560", "1391110", "1628350", "1493710", "2180100", "1826330", "1580130"];
    const PREFIXES: &[&str] = &["Proton", "Steam Linux Runtime", "Steamworks Common", "SteamVR", "Steam Audio"];
    IDS.contains(&app_id) || PREFIXES.iter().any(|p| name.starts_with(p))
}

fn read_vdf(path: &Path, scan: &mut Scan) -> Option<Vdf> {
    let text = match fs::read(path) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            // Often a whole SD card's games hide behind one unreadable file, so say so.
            scan.problems.push(format!("Couldn't open {} ({e}).", path.display()));
            return None;
        }
    };
    match vdf::parse(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            scan.problems.push(format!("Couldn't read {} ({e}).", path.display()));
            None
        }
    }
}

/// Library folders listed by Steam, with Steam's own folder first. Missing ones (an SD card that's out) are skipped.
/// The start of the scan problem for a Steam library Turbine Hub isn't allowed to read (the app offers to allow it).
pub const UNSEEN_LIBRARY: &str = "Can't see the Steam library at ";
pub const UNSEEN_WHY: &str = "Turbine Hub isn't allowed to read that folder";

/// Is this folder outside the ones Turbine Hub's Flatpak may read (Steam's own folder, removable drives, ~/Games)? A
/// library there that isn't visible may be there all the same; one under /run/media is a drive that isn't in.
pub fn outside_allowed(p: &Path) -> bool {
    let home = crate::home_dir();
    let allowed = [home.join(".local/share/Steam"), home.join(".steam"), home.join("Games"), PathBuf::from("/run/media")];
    !allowed.iter().any(|a| p.starts_with(a))
}

pub fn libraries(root: &Path, scan: &mut Scan) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    let file = root.join("steamapps/libraryfolders.vdf");
    if let Some(v) = read_vdf(&file, scan) {
        let list = v.get("libraryfolders").or_else(|| v.get("LibraryFolders"));
        for (_, entry) in list.map(Vdf::entries).unwrap_or(&[]) {
            let path = match entry {
                Vdf::Obj(_) => entry.str_at(&["path"]),
                Vdf::Str(s) => Some(s.as_str()), // older format: "1" "/path"
            };
            if let Some(p) = path {
                let p = PathBuf::from(p);
                if p.join("steamapps").is_dir() {
                    let real = fs::canonicalize(&p).unwrap_or(p);
                    if !out.contains(&real) {
                        out.push(real);
                    }
                } else if outside_allowed(&p) {
                    // A library in a folder the Flatpak may not read is never silent.
                    scan.problems.push(format!("{UNSEEN_LIBRARY}{} ({UNSEEN_WHY})", p.display()));
                }
            }
        }
    }
    out
}

/// Turbine Hub's own folder for art it downloaded from Steam's store: <XDG_CACHE_HOME>/turbine-mod-manager/art (TURBINE_ART
/// overrides it; tests). None when no cache folder is known.
pub fn art_cache() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("TURBINE_ART").filter(|v| !v.is_empty()) {
        return Some(v.into());
    }
    let base = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("turbinehub-mod-manager").join("art"))
}

/// A Steam game's portrait cover, looked for in this order: the player's own grid art in Steam
/// (userdata/<account>/config/grid/<appid>p.png or .jpg), Steam's library cache (library_600x900, then
/// library_capsule, the name newer Steam uses), then a copy Turbine Hub downloaded once from Steam's store
/// (`art_cache`, see `cdn_art`).
pub fn find_cover(root: &Path, app_id: &str, art_cache: Option<&Path>) -> Option<PathBuf> {
    grid_art(root, app_id, "p")
        .or_else(|| find_art(root, app_id, "library_600x900"))
        .or_else(|| find_art(root, app_id, "library_capsule"))
        .or_else(|| art_cache.and_then(|c| cached_cdn_art(c, app_id, "cover")))
}

/// A Steam game's wide picture: the player's own wide grid art (<appid>.png or .jpg), Steam's library cache header,
/// then Turbine Hub's downloaded copy.
pub fn find_header(root: &Path, app_id: &str, art_cache: Option<&Path>) -> Option<PathBuf> {
    grid_art(root, app_id, "")
        .or_else(|| find_art(root, app_id, "header"))
        .or_else(|| art_cache.and_then(|c| cached_cdn_art(c, app_id, "header")))
}

/// The player's custom art in Steam's grid folders (any account), <appid><suffix>.png or .jpg.
pub fn grid_art(root: &Path, app_id: &str, suffix: &str) -> Option<PathBuf> {
    let mut accounts: Vec<PathBuf> = fs::read_dir(root.join("userdata")).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    accounts.sort();
    accounts.iter().find_map(|a| {
        ["png", "jpg"].iter().map(|ext| a.join("config/grid").join(format!("{app_id}{suffix}.{ext}"))).find(|p| p.is_file())
    })
}

/// Where Steam's store keeps a game's art: (what it is for, address), tried in order. The cover first as
/// library_600x900, then library_capsule; the wide picture as header.
pub fn cdn_art_urls(app_id: &str) -> Vec<(&'static str, String)> {
    if app_id.is_empty() || !app_id.bytes().all(|b| b.is_ascii_digit()) {
        return Vec::new();
    }
    let base = format!("https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/{app_id}");
    vec![
        ("cover", format!("{base}/library_600x900.jpg")),
        ("cover", format!("{base}/library_capsule.jpg")),
        ("header", format!("{base}/header.jpg")),
    ]
}

/// The file a downloaded picture is kept in: <cache>/<appid>_<cover|header>.jpg.
pub fn cdn_art_file(art_cache: &Path, app_id: &str, what: &str) -> PathBuf {
    art_cache.join(format!("{app_id}_{what}.jpg"))
}

fn cached_cdn_art(art_cache: &Path, app_id: &str, what: &str) -> Option<PathBuf> {
    let p = cdn_art_file(art_cache, app_id, what);
    p.is_file().then_some(p)
}

/// Does this look like a picture (JPEG or PNG), and not an error page?
pub fn is_picture(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF]) || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

/// How long a game whose art couldn't be downloaded is left alone before Turbine Hub tries once more.
pub const CDN_RETRY_SECS: u64 = 7 * 24 * 3600;

/// Should Turbine Hub try Steam's store for this game's art now? Not when it tried in the last week (the file
/// <cache>/<appid>.tried holds when).
pub fn cdn_art_due(art_cache: &Path, app_id: &str, now: u64) -> bool {
    match fs::read_to_string(art_cache.join(format!("{app_id}.tried"))).ok().and_then(|t| t.trim().parse::<u64>().ok()) {
        Some(at) => now.saturating_sub(at) >= CDN_RETRY_SECS,
        None => true,
    }
}

/// Downloads a game's missing art from Steam's store once (the cover, then the wide picture, each from the first
/// address that answers with a picture), into `art_cache`. Writes when it tried, so a failure isn't retried for a
/// week. Answers what it saved, and a line for the log.
pub fn cdn_art(client: &crate::net::Client, art_cache: &Path, app_id: &str, want_cover: bool, want_header: bool, now: u64) -> (Vec<PathBuf>, String) {
    let mut saved = Vec::new();
    let mut said = Vec::new();
    let _ = fs::create_dir_all(art_cache);
    let _ = fs::write(art_cache.join(format!("{app_id}.tried")), now.to_string());
    for what in ["cover", "header"] {
        if (what == "cover" && !want_cover) || (what == "header" && !want_header) {
            continue;
        }
        let mut got = false;
        for (_, url) in cdn_art_urls(app_id).into_iter().filter(|(k, _)| *k == what) {
            match client.download(&url, &crate::net::Expect::up_to(8 << 20), None) {
                Ok(d) => {
                    let head = fs::read(&d.path).map(|b| b.into_iter().take(8).collect::<Vec<u8>>()).unwrap_or_default();
                    if !is_picture(&head) {
                        let _ = fs::remove_file(&d.path);
                        said.push(format!("{url}: not a picture"));
                        continue;
                    }
                    let to = cdn_art_file(art_cache, app_id, what);
                    if fs::rename(&d.path, &to).is_ok() || (fs::copy(&d.path, &to).is_ok() && fs::remove_file(&d.path).is_ok()) {
                        said.push(format!("{what} from {url}"));
                        saved.push(to);
                        got = true;
                        break;
                    }
                }
                Err(e) => said.push(format!("{url}: {e}")),
            }
        }
        if !got {
            said.push(format!("no {what}"));
        }
    }
    (saved, said.join("; "))
}

/// Art Steam keeps in appcache/librarycache. Older Steam used flat files ("489830_library_600x900.jpg"); newer Steam
/// keeps a folder per game, sometimes with a hashed subfolder. Both are checked.
pub fn find_art(root: &Path, app_id: &str, kind: &str) -> Option<PathBuf> {
    let cache = root.join("appcache/librarycache");
    for ext in ["jpg", "png"] {
        let flat = cache.join(format!("{app_id}_{kind}.{ext}"));
        if flat.is_file() {
            return Some(flat);
        }
    }
    let dir = cache.join(app_id);
    find_named(&dir, kind, 2)
}

fn find_named(dir: &Path, stem: &str, depth: u32) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if p.file_stem().and_then(|s| s.to_str()) == Some(stem)
            && matches!(p.extension().and_then(|s| s.to_str()), Some("jpg" | "png"))
        {
            return Some(p);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.sort();
    subdirs.iter().find_map(|d| find_named(d, stem, depth - 1))
}

/// Windows or native, from what's on disk: a Proton prefix means it already ran under Proton; otherwise .exe files
/// at the top of its folder mean a Windows build.
pub fn runtime_of(install_dir: &Path, prefix: Option<&Path>) -> Runtime {
    if prefix.is_some() {
        return Runtime::Windows;
    }
    let Ok(entries) = fs::read_dir(install_dir) else { return Runtime::Unknown };
    let mut any = false;
    for e in entries.flatten() {
        any = true;
        let name = e.file_name();
        let name = name.to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".exe") {
            return Runtime::Windows;
        }
    }
    if any {
        Runtime::Native
    } else {
        Runtime::Unknown
    }
}

/// The game's folder for a manifest's installdir: one plain folder name inside steamapps/common, nothing else.
/// The folder itself may be a link (a player moved the game elsewhere), but never to the common folder, the root of
/// the drive or any folder above common.
pub fn game_folder(common: &Path, installdir: &str) -> Option<PathBuf> {
    let name = installdir.trim_end_matches('/');
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return None;
    }
    let dir = common.join(name);
    let Ok(real_common) = fs::canonicalize(common) else {
        // No common folder: the game's folder isn't there either, and the caller skips it.
        return Some(dir);
    };
    match fs::canonicalize(&dir) {
        // Not there (yet): the caller skips it.
        Err(_) => Some(dir),
        Ok(real) => {
            let is_link = fs::symlink_metadata(&dir).map(|m| m.file_type().is_symlink()).unwrap_or(false);
            let fine = if is_link {
                !real_common.starts_with(&real)
            } else {
                real.parent() == Some(real_common.as_path())
            };
            fine.then_some(dir)
        }
    }
}

/// Every installed Steam game, across all libraries.
pub fn scan(home: &Path, scan: &mut Scan) {
    let mut seen = HashSet::new();
    for root in steam_roots(home) {
        for lib in libraries(&root, scan) {
            let steamapps = lib.join("steamapps");
            let Ok(entries) = fs::read_dir(&steamapps) else { continue };
            let mut manifests: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    n.starts_with("appmanifest_") && n.ends_with(".acf")
                })
                .collect();
            manifests.sort();
            for m in manifests {
                let Some(v) = read_vdf(&m, scan) else { continue };
                let Some(app) = v.get("AppState") else { continue };
                let (Some(app_id), Some(name), Some(dir)) =
                    (app.str_at(&["appid"]), app.str_at(&["name"]), app.str_at(&["installdir"]))
                else {
                    continue;
                };
                // The app ID is used in folder names (compatdata, librarycache): digits only.
                if app_id.is_empty() || !app_id.bytes().all(|b| b.is_ascii_digit()) {
                    scan.problems.push(format!("Skipped {}: its app ID isn't a number.", m.display()));
                    continue;
                }
                if is_tool(app_id, name) || !seen.insert(app_id.to_string()) {
                    continue;
                }
                // StateFlags 4 = fully installed; anything else is downloading, updating or broken.
                let flags: u32 = app.str_at(&["StateFlags"]).and_then(|s| s.parse().ok()).unwrap_or(4);
                if flags & 4 == 0 {
                    continue;
                }
                let common = steamapps.join("common");
                let Some(install_dir) = game_folder(&common, dir) else {
                    scan.problems.push(format!("Skipped {name}: {} names a folder outside steamapps/common.", m.display()));
                    continue;
                };
                if !install_dir.is_dir() {
                    continue;
                }
                // A linked game folder may live elsewhere, but never at the root of the drive, the home folder or above.
                if !crate::heroic::safe_install_path(&install_dir, home) {
                    scan.problems.push(format!("Skipped {name}: its folder links to {}, which isn't a game folder.", install_dir.display()));
                    continue;
                }
                let pfx = steamapps.join("compatdata").join(app_id).join("pfx");
                let prefix = pfx.is_dir().then_some(pfx);
                let runtime = runtime_of(&install_dir, prefix.as_deref());
                scan.games.push(Game {
                    store: Store::Steam,
                    store_id: app_id.to_string(),
                    name: name.to_string(),
                    install_dir,
                    library: lib.clone(),
                    build: app.str_at(&["buildid"]).map(str::to_string),
                    runtime,
                    cover: find_cover(&root, app_id, art_cache().as_deref()),
                    header: find_header(&root, app_id, art_cache().as_deref()),
                    prefix,
                });
            }
        }
    }
}

/// The game's Steam Launch Options as the player set them (read only), from each Steam user's localconfig.vdf:
/// Some("") when the game has none set, None when no localconfig.vdf mentions the game (or it can't be read).
/// Several users: the first one with the game listed.
pub fn launch_options(home: &Path, app_id: &str) -> Option<String> {
    for root in steam_roots(home) {
        let Ok(rd) = fs::read_dir(root.join("userdata")) else { continue };
        let mut users: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        users.sort();
        for u in users {
            let Ok(text) = fs::read_to_string(u.join("config").join("localconfig.vdf")) else { continue };
            if let Some(o) = launch_options_in(&text, app_id) {
                return Some(o);
            }
        }
    }
    None
}

/// `launch_options` for one localconfig.vdf's text.
pub fn launch_options_in(text: &str, app_id: &str) -> Option<String> {
    let v = crate::vdf::parse(text).ok()?;
    let app = v.path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps", app_id])?;
    Some(app.str_at(&["LaunchOptions"]).unwrap_or("").to_string())
}

// ---- An update waiting in Steam ----

/// Steam's app state bits (appmanifest StateFlags) that mean an update is waiting or under way: update required,
/// running, paused, started, downloading, staging, committing.
pub const UPDATE_FLAGS: u32 = 2 | 256 | 512 | 1024 | 1_048_576 | 2_097_152 | 4_194_304;

/// An update for a game that Steam knows about but hasn't finished installing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingUpdate {
    /// The build installed now and the one Steam is updating to (when the manifest names it).
    pub build: Option<String>,
    pub target: Option<String>,
    /// Bytes Steam still has to download (0 when unknown or queued without a size yet).
    pub bytes_left: u64,
    /// Steam's last update result when it isn't 0 (a failed or stopped update).
    pub result: Option<String>,
}

/// Reads a waiting update from an appmanifest's text: the update bits in StateFlags, a TargetBuildID other than
/// the build installed, bytes still to download, or a failed update (UpdateResult not 0). None when the game is up
/// to date.
pub fn pending_update_in(text: &str) -> Option<PendingUpdate> {
    let v = crate::vdf::parse(text).ok()?;
    let app = v.get("AppState")?;
    let num = |k: &str| app.str_at(&[k]).and_then(|s| s.trim().parse::<u64>().ok());
    let flags = num("StateFlags").unwrap_or(4) as u32;
    let build = app.str_at(&["buildid"]).map(str::to_string).filter(|b| b != "0");
    let target = app.str_at(&["TargetBuildID"]).map(str::to_string).filter(|t| !t.is_empty() && t != "0");
    let to_get = num("BytesToDownload").unwrap_or(0);
    let got = num("BytesDownloaded").unwrap_or(0);
    let bytes_left = to_get.saturating_sub(got);
    let result = app.str_at(&["UpdateResult"]).map(str::to_string).filter(|r| !r.is_empty() && r != "0");
    let newer_target = target.is_some() && target != build;
    if flags & UPDATE_FLAGS != 0 || newer_target || bytes_left > 0 || result.is_some() {
        Some(PendingUpdate { build, target, bytes_left, result })
    } else {
        None
    }
}

/// A waiting update for an installed Steam game (its appmanifest in its library).
pub fn pending_update(game: &Game) -> Option<PendingUpdate> {
    if game.store != Store::Steam || !game.store_id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let f = game.library.join("steamapps").join(format!("appmanifest_{}.acf", game.store_id));
    pending_update_in(&fs::read_to_string(f).ok()?)
}

/// The words for the game update warning when an update is waiting in Steam.
pub fn pending_update_words(name: &str, p: &PendingUpdate) -> String {
    let mut t = format!("An update for {name} is waiting in Steam: mods may break after it installs.");
    if p.bytes_left > 0 {
        t.push_str(&format!(" Steam still has {} MB to download.", p.bytes_left.div_ceil(1 << 20)));
    }
    if p.result.is_some() {
        t.push_str(" Steam's last try to update it didn't finish.");
    }
    t
}

/// The Steam account signed in most recently (its userdata folder name, the 32-bit account id), from
/// config/loginusers.vdf ("MostRecent" "1"). None when it can't be told.
pub fn signed_in_account(home: &Path) -> Option<String> {
    for root in steam_roots(home).into_iter().chain([home.join(".local/share/Steam"), home.join(".steam/steam")]) {
        let Ok(text) = std::fs::read_to_string(root.join("config/loginusers.vdf")) else { continue };
        let Ok(v) = crate::vdf::parse(&text) else { continue };
        let users = v.get("users").or_else(|| v.get("Users"));
        for (id, u) in users.map(|u| u.entries()).unwrap_or(&[]) {
            let recent = u.str_at(&["MostRecent"]).or_else(|| u.str_at(&["mostrecent"])) == Some("1");
            if let (true, Ok(id64)) = (recent, id.parse::<u64>()) {
                return id64.checked_sub(76_561_197_960_265_728).map(|a| a.to_string());
            }
        }
    }
    None
}

/// The signed-in account's file among userdata/<account>/... files, when that account is known and has one.
pub fn signed_in_file(home: &Path, files: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    let acc = signed_in_account(home)?;
    files.iter().find(|p| p.components().any(|c| c.as_os_str() == acc.as_str())).cloned()
}

/// Puts the signed-in account's file first in a list of userdata/<account>/... files (the rest keep their order).
pub fn signed_in_first(home: &Path, files: &mut [std::path::PathBuf]) {
    let Some(acc) = signed_in_account(home) else { return };
    let mine = |p: &std::path::PathBuf| p.components().any(|c| c.as_os_str() == acc.as_str());
    files.sort_by_key(|p| !mine(p));
}

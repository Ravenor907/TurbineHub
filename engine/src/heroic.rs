//! Finds Heroic games (Epic, GOG and games added to Heroic by hand), for both the Flatpak and the native Heroic.

use crate::games::{Game, Runtime, Scan, Store};
use crate::json::{self, Json};
use crate::steam::runtime_of;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Heroic's config folders that exist: the Flatpak's first (the usual one on SteamOS), then a native install.
pub fn heroic_roots(home: &Path) -> Vec<PathBuf> {
    [".var/app/com.heroicgameslauncher.hgl/config/heroic", ".config/heroic"]
        .iter()
        .map(|r| home.join(r))
        .filter(|p| p.is_dir())
        .collect()
}

fn read_json(path: &Path, scan: &mut Scan) -> Option<Json> {
    let text = match fs::read(path) {
        Ok(b) => match String::from_utf8(b) {
            Ok(t) => t,
            Err(_) => {
                scan.problems.push(format!("Couldn't read {} (it isn't valid text).", path.display()));
                return None;
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            scan.problems.push(format!("Couldn't open {} ({e}).", path.display()));
            return None;
        }
    };
    match json::parse(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            scan.problems.push(format!("Couldn't read {} ({e}).", path.display()));
            None
        }
    }
}

fn runtime_from_platform(platform: Option<&str>, install_dir: &Path) -> Runtime {
    match platform.map(str::to_ascii_lowercase).as_deref() {
        Some("linux") => Runtime::Native,
        Some("windows" | "win32" | "win64") => Runtime::Windows,
        _ => runtime_of(install_dir, None),
    }
}

/// Is `dir` usable as a game's folder? It must be a full path, and never the root of the drive, the home folder or a
/// folder above it (Turbine Hub would put mod files straight into it).
pub fn safe_install_path(dir: &Path, home: &Path) -> bool {
    if !dir.is_absolute() || dir.parent().is_none() {
        return false;
    }
    let too_high = |d: &Path, h: &Path| h.starts_with(d);
    if too_high(dir, home) {
        return false;
    }
    match (fs::canonicalize(dir), fs::canonicalize(home)) {
        (Ok(d), Ok(h)) => d.parent().is_some() && !too_high(&d, &h),
        (Ok(d), Err(_)) => d.parent().is_some(),
        _ => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn push(scan: &mut Scan, home: &Path, store: Store, id: &str, name: &str, dir: &str, build: Option<&str>, platform: Option<&str>) {
    if id.is_empty() || dir.is_empty() {
        return;
    }
    let install_dir = PathBuf::from(dir);
    if !safe_install_path(&install_dir, home) {
        let shown = if name.is_empty() { id } else { name };
        scan.problems.push(format!("Skipped {shown}: Heroic lists its folder as {dir}, which isn't a game folder."));
        return;
    }
    if !install_dir.is_dir() {
        return;
    }
    if scan.games.iter().any(|g| g.store == store && g.store_id == id) {
        return;
    }
    let name = if name.is_empty() {
        install_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.to_string())
    } else {
        name.to_string()
    };
    let runtime = runtime_from_platform(platform, &install_dir);
    scan.games.push(Game {
        store,
        store_id: id.to_string(),
        name,
        library: install_dir.parent().map(Path::to_path_buf).unwrap_or_default(),
        install_dir,
        build: build.map(str::to_string),
        runtime,
        cover: None,
        header: None,
        prefix: None,
    });
}

/// Looks up a GOG game's title in Heroic's library caches (installed.json doesn't carry it).
fn gog_titles(root: &Path, scan: &mut Scan) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for rel in ["store_cache/gog_library.json", "gog_store/library.json"] {
        let Some(v) = read_json(&root.join(rel), scan) else { continue };
        let list = v.get("games").map(Json::as_array).unwrap_or(&[]);
        for g in list {
            if let (Some(id), Some(t)) = (g.str_at("app_name"), g.str_at("title")) {
                out.push((id.to_string(), t.to_string()));
            }
        }
    }
    out
}

pub fn scan(home: &Path, scan: &mut Scan) {
    for root in heroic_roots(home) {
        // Epic, through Heroic's copy of Legendary.
        if let Some(v) = read_json(&root.join("legendaryConfig/legendary/installed.json"), scan) {
            for (id, g) in v.entries() {
                if g.get("is_dlc").and_then(Json::as_bool) == Some(true) {
                    continue;
                }
                push(
                    scan,
                    home,
                    Store::Epic,
                    id,
                    g.str_at("title").unwrap_or(""),
                    g.str_at("install_path").unwrap_or(""),
                    g.str_at("version"),
                    g.str_at("platform"),
                );
            }
        }
        // GOG.
        let titles = gog_titles(&root, scan);
        if let Some(v) = read_json(&root.join("gog_store/installed.json"), scan) {
            for g in v.get("installed").map(Json::as_array).unwrap_or(&[]) {
                if g.get("is_dlc").and_then(Json::as_bool) == Some(true) {
                    continue;
                }
                let id = g.str_at("appName").unwrap_or("");
                let title = titles.iter().find(|(i, _)| i == id).map(|(_, t)| t.as_str()).unwrap_or("");
                push(
                    scan,
                    home,
                    Store::Gog,
                    id,
                    title,
                    g.str_at("install_path").unwrap_or(""),
                    g.str_at("version"),
                    g.str_at("platform"),
                );
            }
        }
        // Games added to Heroic by hand.
        if let Some(v) = read_json(&root.join("sideload_apps/library.json"), scan) {
            for g in v.get("games").map(Json::as_array).unwrap_or(&[]) {
                if g.get("is_installed").and_then(Json::as_bool) == Some(false) {
                    continue;
                }
                let platform = g.get("install").and_then(|i| i.str_at("platform"));
                push(
                    scan,
                    home,
                    Store::Sideload,
                    g.str_at("app_name").unwrap_or(""),
                    g.str_at("title").unwrap_or(""),
                    g.str_at("folder_name").unwrap_or(""),
                    None,
                    platform,
                );
            }
        }
        fill_art(&root, scan);
        fill_prefixes(&root, home, scan);
    }
}

// ---- Wine and Proton prefixes ----
//
// Heroic keeps each game's settings in <config>/heroic/GamesConfig/<app name>.json, under the app name:
// "winePrefix" (the prefix folder) and "wineVersion" {"type": "wine" | "proton" | "toolkit" | ...}. A Proton
// runner keeps Wine's registry in <winePrefix>/pfx/user.reg (Proton's own layout), any other in
// <winePrefix>/user.reg. Without a game file, Heroic's default is <defaultWinePrefix>/<title> from config.json's
// "defaultSettings" (~/Games/Heroic/Prefixes/default when unset). Game::prefix is the folder that holds user.reg, as
// for Steam games, so the same DLL overrides work for Heroic games (proton.rs).

fn expand_home(p: &str, home: &Path) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(p),
    }
}

/// The folder holding user.reg for a Heroic prefix and its runner type.
pub fn registry_dir(wine_prefix: &Path, wine_type: &str) -> PathBuf {
    let pfx = wine_prefix.join("pfx");
    let (first, second) = if wine_type.eq_ignore_ascii_case("proton") { (pfx, wine_prefix.to_path_buf()) } else { (wine_prefix.to_path_buf(), pfx) };
    if !first.join("user.reg").is_file() && second.join("user.reg").is_file() {
        second
    } else {
        first
    }
}

/// A Heroic game's prefix (the folder that holds, or will hold, user.reg), from its GamesConfig file or Heroic's
/// default folder. None for a native game or when Heroic has no prefix for it.
pub fn prefix_for(root: &Path, home: &Path, app_name: &str, title: &str) -> Option<PathBuf> {
    if app_name.is_empty() || app_name.contains('/') || app_name.contains("..") {
        return None;
    }
    let mut scratch = Scan::default();
    if let Some(v) = read_json(&root.join("GamesConfig").join(format!("{app_name}.json")), &mut scratch) && let Some(cfg) = v.get(app_name) && let Some(wp) = cfg.str_at("winePrefix").filter(|p| !p.is_empty()) {
        let wine_type = cfg.get("wineVersion").and_then(|w| w.str_at("type")).unwrap_or("wine");
        return Some(registry_dir(&expand_home(wp, home), wine_type));
    }
    let defaults = read_json(&root.join("config.json"), &mut scratch);
    let d = defaults.as_ref().and_then(|v| v.get("defaultSettings"));
    let base = d.and_then(|d| d.str_at("defaultWinePrefix")).map(|p| expand_home(p, home)).unwrap_or_else(|| home.join("Games/Heroic/Prefixes/default"));
    let wine_type = d.and_then(|d| d.get("wineVersion")).and_then(|w| w.str_at("type")).unwrap_or("wine");
    let dir = base.join(title);
    (!title.is_empty() && !title.contains('/') && dir.is_dir()).then(|| registry_dir(&dir, wine_type))
}

fn fill_prefixes(root: &Path, home: &Path, scan: &mut Scan) {
    for g in scan.games.iter_mut() {
        if g.prefix.is_some() || g.store == Store::Steam || g.runtime != Runtime::Windows {
            continue;
        }
        g.prefix = prefix_for(root, home, &g.store_id, &g.name);
    }
}

// ---- Cover art from Heroic's image cache ----
//
// Heroic shows store art through its own "imagecache://" protocol and keeps each image in <config>/heroic/images-cache
// under the SHA-256 (hex) of the image's URL: HeroicGamesLauncher src/backend/images_cache.ts:8 (the folder) and :27
// (the digest of the URL, which :25 decodes from the imagecache:// address built in
// src/frontend/components/UI/CachedImage/index.tsx:47). The library grid asks for art_square
// (src/frontend/screens/Library/components/GameCard/index.tsx:129-130 and :486), and for Epic games adds
// "?h=400&resize=1&w=300" to it first (GameCard/constants.ts:9-10); art_cover is the wide picture. The URLs come
// from the library caches: store_cache/legendary_library.json under "library"
// (src/backend/storeManagers/legendary/electronStores.ts:8-11, library.ts:184), store_cache/gog_library.json under
// "games" (gog/electronStores.ts:23, gog/library.ts:581) and sideload_apps/library.json under "games".
// Checked against Heroic's main branch at commit 4737fdb3.

/// The suffix Heroic adds to Epic art URLs on library cards.
pub const EPIC_CARD_SUFFIX: &str = "?h=400&resize=1&w=300";

/// The file Heroic keeps a cached image in.
pub fn image_cache_file(root: &Path, url: &str) -> PathBuf {
    root.join("images-cache").join(crate::sha256::digest_hex(url.as_bytes()))
}

fn quiet_json(path: &Path) -> Option<Json> {
    let text = fs::read_to_string(path).ok()?;
    json::parse(&text).ok()
}

/// (store, app name, art_square, art_cover) for every game in Heroic's library caches under `root`.
pub fn library_art(root: &Path) -> Vec<(Store, String, String, String)> {
    let mut out = Vec::new();
    let lists = [
        (Store::Epic, "store_cache/legendary_library.json", "library"),
        (Store::Gog, "store_cache/gog_library.json", "games"),
        (Store::Sideload, "sideload_apps/library.json", "games"),
    ];
    for (store, rel, key) in lists {
        let Some(v) = quiet_json(&root.join(rel)) else { continue };
        for g in v.get(key).map(Json::as_array).unwrap_or(&[]) {
            let Some(id) = g.str_at("app_name") else { continue };
            let square = g.str_at("art_square").unwrap_or("").to_string();
            let cover = g.str_at("art_cover").unwrap_or("").to_string();
            out.push((store, id.to_string(), square, cover));
        }
    }
    out
}

/// The cached tall and wide pictures for one game, when Heroic has them on disk.
pub fn cached_art(root: &Path, store: Store, square: &str, cover: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    let found = |url: &str| -> Option<PathBuf> {
        if !url.starts_with("http") {
            return None;
        }
        let p = image_cache_file(root, url);
        fs::metadata(&p).ok().filter(|m| m.is_file() && m.len() > 0).map(|_| p)
    };
    let mut tall = Vec::new();
    if store == Store::Epic && !square.is_empty() {
        tall.push(format!("{square}{EPIC_CARD_SUFFIX}"));
    }
    tall.push(square.to_string());
    let tall = tall.iter().find_map(|u| found(u));
    let wide = found(cover).or_else(|| if store == Store::Epic && !cover.is_empty() { found(&format!("{cover}{EPIC_CARD_SUFFIX}")) } else { None });
    (tall, wide)
}

/// Fills cover and header for Heroic games found so far from `root`'s image cache.
fn fill_art(root: &Path, scan: &mut Scan) {
    if !root.join("images-cache").is_dir() {
        return;
    }
    let art = library_art(root);
    for g in scan.games.iter_mut().filter(|g| g.store != Store::Steam && (g.cover.is_none() || g.header.is_none())) {
        let Some((_, _, square, cover)) = art.iter().find(|(s, id, _, _)| *s == g.store && *id == g.store_id) else { continue };
        let (tall, wide) = cached_art(root, g.store, square, cover);
        if g.cover.is_none() {
            g.cover = tall;
        }
        if g.header.is_none() {
            g.header = wide;
        }
    }
}

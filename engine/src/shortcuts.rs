//! The player's non-Steam games (Steam's shortcuts.vdf), read only. In Game Mode gamescope shows only what Steam
//! itself starts, so Turbine Hub opens the player's browser through Steam: it looks for a Chrome-family browser the
//! player added to Steam as a non-Steam game and starts it with steam://rungameid/<id>. The browser helper then
//! opens the Nexus page in it.
//!
//! The file lives at <Steam>/userdata/<account>/config/shortcuts.vdf (binary VDF, see vdf::parse_binary). Each
//! shortcut keeps its 32-bit appid; very old files have none, and then Steam's own rule gives it:
//! crc32(exe + name) with the top bit set. The id Steam's rungameid link takes is (appid << 32) | 0x02000000.
//! Turbine Hub writes this file only to add or remove its own tiles, while Steam is closed (see `add_shortcut`).

use crate::vdf::{self, Vdf};
use std::path::{Path, PathBuf};

/// One non-Steam game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub appid: u32,
    pub name: String,
    pub exe: String,
    pub launch_options: String,
}

/// Chrome-family browsers Turbine Hub can find among shortcuts, in the order it prefers them, each with the words
/// (lower case) that mark it in the program path or launch options. Chrome-family because the browser helper is a
/// Chrome extension.
pub const BROWSERS: &[(&str, &[&str])] = &[
    ("Google Chrome", &["com.google.chrome", "google-chrome", "/opt/google/chrome/"]),
    ("Brave", &["com.brave.browser", "brave-browser", "/usr/bin/brave"]),
    ("Chromium", &["org.chromium.chromium", "/usr/bin/chromium", "chromium-browser"]),
    ("Microsoft Edge", &["com.microsoft.edge", "microsoft-edge"]),
    ("Vivaldi", &["com.vivaldi.vivaldi", "vivaldi-stable", "/usr/bin/vivaldi"]),
];

/// Steam's appid for a shortcut that has none stored: crc32 of the program path (as stored, quotes and all) followed
/// by the name, with the top bit set.
pub fn legacy_appid(exe: &str, name: &str) -> u32 {
    let mut s = Vec::with_capacity(exe.len() + name.len());
    s.extend_from_slice(exe.as_bytes());
    s.extend_from_slice(name.as_bytes());
    crate::zip::crc32(&s) | 0x8000_0000
}

/// The 64-bit id steam://rungameid/ takes for a shortcut.
pub fn game_id(appid: u32) -> u64 {
    (u64::from(appid) << 32) | 0x0200_0000
}

/// steam://rungameid/<id> for a shortcut.
pub fn rungameid_url(appid: u32) -> String {
    format!("steam://rungameid/{}", game_id(appid))
}

fn field<'a>(v: &'a Vdf, key: &str) -> &'a str {
    v.get(key).and_then(Vdf::as_str).unwrap_or("")
}

/// The shortcuts in a shortcuts.vdf file's bytes. A broken file gives an error; a shortcut with an unreadable appid
/// gets Steam's computed one.
pub fn parse(bytes: &[u8]) -> Result<Vec<Shortcut>, vdf::VdfError> {
    let root = vdf::parse_binary(bytes)?;
    let list = root.get("shortcuts").map(Vdf::entries).unwrap_or(&[]);
    let mut out = Vec::new();
    for (_, s) in list {
        if !matches!(s, Vdf::Obj(_)) {
            continue;
        }
        let name = field(s, "AppName").to_string();
        let exe = field(s, "Exe").to_string();
        let launch_options = field(s, "LaunchOptions").to_string();
        let appid = field(s, "appid").parse::<u32>().ok().filter(|a| *a != 0).unwrap_or_else(|| legacy_appid(&exe, &name));
        out.push(Shortcut { appid, name, exe, launch_options });
    }
    Ok(out)
}

/// Which Chrome-family browser a shortcut starts, by its program path or launch options (never by its name alone, a
/// player can call anything "Chrome").
pub fn browser_of(s: &Shortcut) -> Option<&'static str> {
    let hay = format!("{} {}", s.exe, s.launch_options).to_lowercase();
    BROWSERS.iter().find(|(_, marks)| marks.iter().any(|m| hay.contains(m))).map(|(n, _)| *n)
}

/// The preferred browser among some shortcuts (BROWSERS order, then file order).
pub fn pick_browser(list: &[Shortcut]) -> Option<(&'static str, &Shortcut)> {
    BROWSERS.iter().find_map(|(name, _)| list.iter().find(|s| browser_of(s) == Some(*name)).map(|s| (*name, s)))
}

/// A browser found among the player's shortcuts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserShortcut {
    /// "Google Chrome", "Brave" and so on.
    pub browser: String,
    /// The name the player sees in Steam.
    pub name: String,
    pub appid: u32,
    /// The file it was read from.
    pub file: PathBuf,
}

impl BrowserShortcut {
    pub fn url(&self) -> String {
        rungameid_url(self.appid)
    }

    /// {"found":true,"browser","name","appid","gameid","url"} for the app and the activity log.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"found\":true,\"browser\":{},\"name\":{},\"appid\":{},\"gameid\":\"{}\",\"url\":{}}}",
            crate::json::quote(&self.browser),
            crate::json::quote(&self.name),
            self.appid,
            game_id(self.appid),
            crate::json::quote(&self.url())
        )
    }
}

/// Every shortcuts.vdf under Steam's folders in this home, the most recently changed first (that is the account
/// signed in now).
pub fn shortcut_files(home: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = crate::steam::steam_roots(home);
    // A Steam folder without steamapps (a fresh install) still has userdata.
    for rel in [".local/share/Steam", ".steam/steam"] {
        let p = home.join(rel);
        let real = std::fs::canonicalize(&p).unwrap_or(p);
        if real.join("userdata").is_dir() && !roots.contains(&real) {
            roots.push(real);
        }
    }
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(root.join("userdata")) else { continue };
        for e in rd.flatten() {
            let f = e.path().join("config").join("shortcuts.vdf");
            if let Ok(m) = std::fs::metadata(&f) && m.is_file() && !files.iter().any(|(_, p)| *p == f) {
                files.push((m.modified().unwrap_or(std::time::UNIX_EPOCH), f));
            }
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    let mut list: Vec<PathBuf> = files.into_iter().map(|(_, p)| p).collect();
    crate::steam::signed_in_first(home, &mut list);
    list
}

/// The player's Chrome-family browser among their non-Steam games, if they added one. Read only.
pub fn find_browser(home: &Path) -> Option<BrowserShortcut> {
    for f in shortcut_files(home) {
        // shortcuts.vdf is small; anything huge is not one.
        let Ok(bytes) = std::fs::read(&f) else { continue };
        if bytes.len() > 16 << 20 {
            continue;
        }
        let Ok(list) = parse(&bytes) else { continue };
        if let Some((browser, s)) = pick_browser(&list) {
            return Some(BrowserShortcut { browser: browser.into(), name: s.name.clone(), appid: s.appid, file: f });
        }
    }
    None
}

/// Words that mark Turbine Hub's own shortcut (the Flatpak's id, or its program), lower case.
pub const SELF_MARKS: &[&str] = &["io.github.ravenor907.turbinehub", "/app/bin/turbinehub"];

/// Is this shortcut Turbine Hub itself (by its program path or launch options, never by its name alone)?
pub fn is_self(s: &Shortcut) -> bool {
    let hay = format!("{} {}", s.exe, s.launch_options).to_lowercase();
    SELF_MARKS.iter().any(|m| hay.contains(m))
}

/// Turbine Hub's own entry among the player's non-Steam games, if it was added to Steam: after signing in with the
/// player's own browser in Game Mode, steam://rungameid/<its id> brings Turbine Hub back to the front. Read only.
pub fn find_self(home: &Path) -> Option<(u32, String)> {
    for f in shortcut_files(home) {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        if bytes.len() > 16 << 20 {
            continue;
        }
        let Ok(list) = parse(&bytes) else { continue };
        if let Some(s) = list.iter().find(|s| is_self(s) && !s.launch_options.contains("--play-coop")) {
            return Some((s.appid, s.name.clone()));
        }
    }
    None
}

/// The steam://rungameid link for a game id Steam put in the environment (SteamGameId, set for a game or
/// non-Steam shortcut Steam started), when it is a plausible one.
pub fn rungameid_from_env(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty() && v.len() <= 20 && v.bytes().all(|b| b.is_ascii_digit()) && v != "0").then(|| format!("steam://rungameid/{v}"))
}

// ---- Turbine Hub's own tiles (the co-op tile) ----
//
// Steam keeps shortcuts.vdf in memory while it runs and writes it back over any change made meanwhile, and it reads
// the file again only when it starts. There is no documented way to add a non-Steam game to a running Steam (the
// client's own script API is for plugins inside Steam), and the tools that do this (Steam ROM Manager, BoilR,
// SteamTinkerLaunch) write the file while Steam is closed, then Steam is started again. Turbine Hub does the same:
// it changes the file only when the app says Steam is closed, keeps a backup first, checks right before replacing
// it that nobody changed it since it was read, and never adds a second copy of its tile. Everything else in the
// file is written back byte for byte (vdf::BinDoc).

/// A shortcut Turbine Hub adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewShortcut {
    pub name: String,
    /// The program as Steam stores it (in quotes).
    pub exe: String,
    pub start_dir: String,
    pub launch_options: String,
    /// The Flatpak's id when the program is `flatpak run <id>` (Steam's own "Add a non-Steam game" fills it the same
    /// way); empty otherwise.
    pub flatpak_id: String,
    /// The icon file Steam shows for it (the co-op tile's own icon in the grid folder); empty for none.
    pub icon: String,
}

impl NewShortcut {
    /// The appid Steam ROM Manager, BoilR and Steam's own older rule give it: crc32(exe + name) with the top bit
    /// set. Grid art is named by it.
    pub fn appid(&self) -> u32 {
        legacy_appid(&self.exe, &self.name)
    }
}

/// The words in a co-op tile's launch options that mark it (one per game).
pub fn coop_marker(game_id: &str) -> String {
    format!("--play-coop {game_id}")
}

/// The co-op tile for a game ("steam:1245620"): Turbine Hub's Flatpak with `--play-coop <game id>`. Turbine Hub then gets
/// the game ready for Seamless Co-op for the next Play only and starts it through Steam, so Steam's own launch,
/// playtime and overlay are used. `--from-tile`: Turbine Hub opens the game's link itself (no Steam menu plugin reads its
/// answer), or opens its own window on the game when something needs the player first.
pub fn coop_tile(game_name: &str, game_id: &str, flatpak_id: &str) -> NewShortcut {
    NewShortcut {
        name: format!("{game_name} Co-op"),
        exe: "\"/usr/bin/flatpak\"".into(),
        start_dir: "\"/usr/bin/\"".into(),
        launch_options: format!("run --branch=stable --arch=x86_64 {flatpak_id} {} --from-tile", coop_marker(game_id)),
        flatpak_id: flatpak_id.into(),
        icon: String::new(),
    }
}

/// The game id after `--play-coop` in Turbine Hub's own arguments ("steam:<digits>"), when it is a plausible one.
pub fn play_coop_arg(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a == "--play-coop")?;
    let id = args.get(i + 1)?;
    let digits = id.strip_prefix("steam:")?;
    (!digits.is_empty() && digits.len() <= 10 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| id.clone())
}

/// What a change to the shortcuts did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileChange {
    /// Added with this appid; Steam shows it after it starts again.
    Added(u32),
    /// Already there (this appid): nothing written.
    Already(u32),
    /// Removed (this appid); Steam drops it after it starts again.
    Removed(u32),
    /// Not there: nothing written.
    NotThere,
}

/// The shortcuts.vdf for the account signed in last (the most recently changed one), or, when that account has no
/// non-Steam games yet, where its file goes.
pub fn tile_file(home: &Path) -> Option<PathBuf> {
    // Another account's file never stands in for the signed-in account's (it may have none yet).
    if let Some(f) = shortcut_files(home).into_iter().next() && (crate::steam::signed_in_account(home).is_none() || crate::steam::signed_in_file(home, std::slice::from_ref(&f)).is_some()) {
        return Some(f);
    }
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for root in crate::steam::steam_roots(home).into_iter().chain([home.join(".local/share/Steam"), home.join(".steam/steam")]) {
        let Ok(rd) = std::fs::read_dir(root.join("userdata")) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let cfg = e.path().join("config");
            if name != "0" && name.bytes().all(|b| b.is_ascii_digit()) && cfg.is_dir() {
                let t = std::fs::metadata(&cfg).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                dirs.push((t, cfg.join("shortcuts.vdf")));
            }
        }
    }
    dirs.sort_by_key(|d| std::cmp::Reverse(d.0));
    let mut list: Vec<PathBuf> = dirs.into_iter().map(|(_, p)| p).collect();
    crate::steam::signed_in_first(home, &mut list);
    list.into_iter().next()
}

/// The backup Turbine Hub writes before each change (the file as it was just before).
pub fn backup_file(file: &Path) -> PathBuf {
    file.with_file_name("shortcuts.vdf.turbine-backup")
}

/// The file as it was before Turbine Hub's first change, kept for good.
pub fn original_file(file: &Path) -> PathBuf {
    file.with_file_name("shortcuts.vdf.turbine-original")
}

const STEAM_OPEN: &str = "Close Steam first: Steam writes its list of non-Steam games back over any change while it runs";

fn entry_text(e: &vdf::BinItem, key: &str) -> String {
    e.children().iter().find(|i| i.key_is(key)).and_then(vdf::BinItem::as_text).unwrap_or_default()
}

fn entry_appid(e: &vdf::BinItem) -> u32 {
    e.child("appid").and_then(vdf::BinItem::as_int).filter(|a| *a != 0).unwrap_or_else(|| legacy_appid(&entry_text(e, "Exe"), &entry_text(e, "AppName")))
}

/// Is this entry Turbine Hub's tile with this marker (Turbine Hub's program and the marker in its launch options)?
fn is_tile(e: &vdf::BinItem, marker: &str) -> bool {
    let s = Shortcut { appid: 0, name: String::new(), exe: entry_text(e, "Exe"), launch_options: entry_text(e, "LaunchOptions") };
    is_self(&s) && s.launch_options.contains(marker)
}

/// The file's bytes and parsed form (an empty list when there is no file yet).
fn load(file: &Path) -> Result<(Option<Vec<u8>>, vdf::BinDoc), String> {
    match std::fs::read(file) {
        Ok(b) if b.len() > 16 << 20 => Err("Steam's list of non-Steam games is too big to be one".into()),
        Ok(b) => {
            let doc = vdf::parse_binary_doc(&b).map_err(|e| format!("Steam's list of non-Steam games can't be read ({e})"))?;
            if !doc.items.iter().any(|i| i.key_is("shortcuts") && matches!(i.value, vdf::BinValue::Obj(..))) {
                return Err("Steam's list of non-Steam games has no list in it".into());
            }
            Ok((Some(b), doc))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok((None, vdf::BinDoc { items: vec![vdf::BinItem::obj("shortcuts", Vec::new())], end: Some(0x08), tail: Vec::new() }))
        }
        Err(e) => Err(format!("Steam's list of non-Steam games can't be read ({e})")),
    }
}

fn list_mut(doc: &mut vdf::BinDoc) -> &mut Vec<vdf::BinItem> {
    let root = doc.items.iter_mut().find(|i| i.key_is("shortcuts")).and_then(vdf::BinItem::children_mut);
    root.expect("checked in load")
}

/// Backs the file up, then replaces it, unless it changed since it was read.
fn save(file: &Path, before: Option<&[u8]>, doc: &vdf::BinDoc) -> Result<(), String> {
    let dir = file.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if let Some(b) = before {
        let orig = original_file(file);
        if !orig.exists() {
            std::fs::write(&orig, b).map_err(|e| format!("Couldn't keep a copy of Steam's list first ({e})"))?;
        }
        std::fs::write(backup_file(file), b).map_err(|e| format!("Couldn't keep a copy of Steam's list first ({e})"))?;
    }
    let tmp = file.with_file_name("shortcuts.vdf.turbine-tmp");
    std::fs::write(&tmp, vdf::write_binary_doc(doc)).map_err(|e| e.to_string())?;
    let now = std::fs::read(file).ok();
    if now.as_deref() != before {
        let _ = std::fs::remove_file(&tmp);
        return Err("Steam's list of non-Steam games changed meanwhile (is Steam running?): nothing written".into());
    }
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())
}

/// Adds the shortcut unless one with the same marker (Turbine Hub's tile) or the same appid is there. Only while Steam
/// is closed (`steam_running` from the app's own check); a backup first.
pub fn add_shortcut(file: &Path, s: &NewShortcut, marker: &str, steam_running: bool) -> Result<TileChange, String> {
    let (before, mut doc) = load(file)?;
    let appid = s.appid();
    let list = list_mut(&mut doc);
    if let Some(e) = list.iter().find(|e| is_tile(e, marker) || entry_appid(e) == appid) {
        return Ok(TileChange::Already(entry_appid(e)));
    }
    if steam_running {
        return Err(STEAM_OPEN.into());
    }
    let next = list.iter().filter_map(|e| std::str::from_utf8(&e.key).ok()?.parse::<u32>().ok()).max().map_or(0, |m| m + 1).max(list.len() as u32);
    use vdf::BinItem as I;
    list.push(I::obj(
        &next.to_string(),
        vec![
            I::int("appid", appid),
            I::text("AppName", &s.name),
            I::text("Exe", &s.exe),
            I::text("StartDir", &s.start_dir),
            I::text("icon", &s.icon),
            I::text("ShortcutPath", ""),
            I::text("LaunchOptions", &s.launch_options),
            I::int("IsHidden", 0),
            I::int("AllowDesktopConfig", 1),
            I::int("AllowOverlay", 1),
            I::int("OpenVR", 0),
            I::int("Devkit", 0),
            I::text("DevkitGameID", ""),
            I::int("DevkitOverrideAppID", 0),
            I::int("LastPlayTime", 0),
            I::text("FlatpakAppID", &s.flatpak_id),
            I::obj("tags", Vec::new()),
        ],
    ));
    save(file, before.as_deref(), &doc)?;
    Ok(TileChange::Added(appid))
}

/// Removes Turbine Hub's tile with this marker (and only that), renumbering the rest as Steam does. Only while Steam is
/// closed; a backup first.
pub fn remove_shortcut(file: &Path, marker: &str, steam_running: bool) -> Result<TileChange, String> {
    let (before, mut doc) = load(file)?;
    let list = list_mut(&mut doc);
    let Some(at) = list.iter().position(|e| is_tile(e, marker)) else { return Ok(TileChange::NotThere) };
    if steam_running {
        return Err(STEAM_OPEN.into());
    }
    let appid = entry_appid(&list[at]);
    list.remove(at);
    for (n, e) in list.iter_mut().enumerate() {
        if std::str::from_utf8(&e.key).ok().is_some_and(|k| k.parse::<u32>().is_ok()) {
            e.key = n.to_string().into_bytes();
        }
    }
    save(file, before.as_deref(), &doc)?;
    Ok(TileChange::Removed(appid))
}

/// Adds the co-op tile with its art: the art (capsule, wide, hero, logo, icon from `art_dir`, see
/// steam_art::COOP_PARTS) goes into the grid folder by the tile's appid first, never over the player's own, and the
/// tile's icon is the icon there. `record`: Turbine Hub's art record.
pub fn add_tile_with_art(file: &Path, tile: &NewShortcut, marker: &str, art_dir: &Path, record: &Path, steam_running: bool) -> Result<TileChange, String> {
    let mut t = tile.clone();
    let appid = t.appid();
    let grid = crate::steam_art::grid_dir(file);
    // The tile first, then its art, so a tile that couldn't be added leaves no art behind. Its icon
    // is where the art goes (known before it is written).
    if let (true, Some(name)) = (art_dir.join("icon.png").is_file(), crate::steam_art::grid_name(appid, "icon")) {
        t.icon = grid.join(name).to_string_lossy().to_string();
    }
    let r = add_shortcut(file, &t, marker, steam_running)?;
    if !steam_running && matches!(r, TileChange::Added(_) | TileChange::Already(_)) {
        crate::steam_art::add_art_opts(art_dir, &grid, appid, crate::steam_art::COOP_PARTS, record, true).map_err(|e| format!("Couldn't add the tile's art ({e})"))?;
    }
    Ok(r)
}

/// Removes the co-op tile and Turbine Hub's own art for it.
pub fn remove_tile_with_art(file: &Path, marker: &str, record: &Path, steam_running: bool) -> Result<TileChange, String> {
    let r = remove_shortcut(file, marker, steam_running)?;
    if let TileChange::Removed(appid) = r {
        crate::steam_art::remove_art(&crate::steam_art::grid_dir(file), appid, record).map_err(|e| format!("Couldn't remove the tile's art ({e})"))?;
    }
    Ok(r)
}

/// Turbine Hub's own shortcut with the shortcuts.vdf it is in (read only): for its library art.
pub fn find_self_file(home: &Path) -> Option<(PathBuf, u32)> {
    for f in shortcut_files(home) {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        if bytes.len() > 16 << 20 {
            continue;
        }
        let Ok(list) = parse(&bytes) else { continue };
        if let Some(s) = list.iter().find(|s| is_self(s) && !s.launch_options.contains("--play-coop")) {
            return Some((f, s.appid));
        }
    }
    None
}

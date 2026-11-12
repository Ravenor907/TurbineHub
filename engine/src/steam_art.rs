//! Steam library art for Turbine Hub's own non-Steam shortcut and for the co-op tile. Steam shows a non-Steam
//! game's art from userdata/<account>/config/grid/, named by the shortcut's appid: <appid>p.png (the 600x900
//! capsule), <appid>.png (the 920x430 wide one), <appid>_hero.png, <appid>_logo.png and <appid>_icon.png.
//!
//! Turbine Hub writes a file there only when none is there, or when the one there is one Turbine Hub wrote itself (its
//! SHA-256 is in Turbine Hub's record), so art the player picked is never replaced. Remove takes out only files whose
//! contents are still exactly what Turbine Hub wrote. Turbine Hub's own shortcut is only read (shortcuts::find_self_file).

use crate::json::{self, quote, Json};
use crate::sha256;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The art pieces: (the source file's name without .png, what follows the appid in the grid folder).
pub const PARTS: &[(&str, &str)] = &[("capsule", "p"), ("wide", ""), ("hero", "_hero"), ("logo", "_logo"), ("icon", "_icon")];

/// The pieces for Turbine Hub's own shortcut (its icon stays the player's choice in Steam).
pub const TURBINE_PARTS: &[&str] = &["capsule", "wide", "hero", "logo"];

/// The pieces for the co-op tile (its icon is set in the shortcut itself).
pub const COOP_PARTS: &[&str] = &["capsule", "wide", "hero", "logo", "icon"];

/// The grid file name for a piece, or None for an unknown piece.
pub fn grid_name(appid: u32, part: &str) -> Option<String> {
    PARTS.iter().find(|(p, _)| *p == part).map(|(_, suffix)| format!("{appid}{suffix}.png"))
}

/// The grid folder next to a shortcuts.vdf.
pub fn grid_dir(shortcuts_file: &Path) -> PathBuf {
    shortcuts_file.with_file_name("grid")
}

/// Turbine Hub's record of the art files it wrote: (full path, SHA-256 of what it wrote).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtRecord {
    pub files: Vec<(String, String)>,
}

impl ArtRecord {
    pub fn load(path: &Path) -> ArtRecord {
        let Ok(text) = fs::read_to_string(path) else { return ArtRecord::default() };
        let Ok(v) = json::parse(&text) else { return ArtRecord::default() };
        let files = v
            .get("files")
            .map(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|f| Some((f.str_at("path")?.to_string(), f.str_at("sha256")?.to_string())))
            .collect();
        ArtRecord { files }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let files: Vec<String> = self.files.iter().map(|(p, h)| format!("{{\"path\":{},\"sha256\":{}}}", quote(p), quote(h))).collect();
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, format!("{{\"files\":[{}]}}\n", files.join(",")))?;
        fs::rename(tmp, path)
    }

    fn hash_of(&self, file: &Path) -> Option<&str> {
        let key = file.to_string_lossy();
        self.files.iter().find(|(p, _)| *p == key).map(|(_, h)| h.as_str())
    }

    fn set(&mut self, file: &Path, hash: String) {
        let key = file.to_string_lossy().to_string();
        self.files.retain(|(p, _)| *p != key);
        self.files.push((key, hash));
    }

    fn forget(&mut self, file: &Path) {
        let key = file.to_string_lossy();
        self.files.retain(|(p, _)| *p != key);
    }
}

/// What happened to one grid file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtOutcome {
    /// Written (there was none, or it was Turbine Hub's own older one).
    Written,
    /// Already Turbine Hub's current art: nothing written.
    Unchanged,
    /// The player's own art is there: kept, nothing written.
    PlayersOwn,
    /// Turbine Hub has no such piece in the source folder.
    NoSource,
    /// Turbine Hub's file was there and the player cleared it in Steam: not written again until the
    /// player asks (Settings, Add).
    Cleared,
}

/// The player's own picture for this piece in another format Steam reads (.jpg, .jpeg, .webp; Turbine Hub only writes
/// .png): it is the player's, whatever the .png says.
fn players_other_format(grid: &Path, png_name: &str) -> bool {
    let stem = png_name.trim_end_matches(".png");
    ["jpg", "jpeg", "webp", "JPG", "PNG"].iter().any(|ext| grid.join(format!("{stem}.{ext}")).exists())
}

const CLEARED: &str = "cleared";

/// Is this grid file Turbine Hub's own (what Turbine Hub wrote, unchanged since)?
fn is_ours(record: &ArtRecord, file: &Path) -> bool {
    let Some(want) = record.hash_of(file) else { return false };
    sha256::file_hex(file).is_ok_and(|now| sha256::same_digest(&now, want))
}

/// Writes the pieces from `src` (<piece>.png) into `grid` for `appid`, never over the player's own art. `record`
/// is Turbine Hub's record file. Answers each grid file with what happened. `again`: the player asked for Turbine Hub's art (Settings, Add, or a new tile), so pieces the player had cleared in
/// Steam are written again. Without it (every start) they stay cleared. Only files Turbine Hub wrote (in its record) are
/// ever treated as Turbine Hub's (an identical file of the player's is never adopted).
pub fn add_art_opts(src: &Path, grid: &Path, appid: u32, parts: &[&str], record: &Path, again: bool) -> io::Result<Vec<(PathBuf, ArtOutcome)>> {
    let mut rec = ArtRecord::load(record);
    let mut out = Vec::new();
    for part in parts {
        let Some(name) = grid_name(appid, part) else { continue };
        let dest = grid.join(&name);
        let from = src.join(format!("{part}.png"));
        let Ok(bytes) = fs::read(&from) else {
            out.push((dest, ArtOutcome::NoSource));
            continue;
        };
        let hash = sha256::digest_hex(&bytes);
        let other = players_other_format(grid, &name);
        let outcome = if dest.exists() {
            let now = sha256::file_hex(&dest).unwrap_or_default();
            if is_ours(&rec, &dest) && other {
                // The player set a picture of their own (another format) beside Turbine Hub's: Turbine Hub's steps aside.
                let _ = fs::remove_file(&dest);
                rec.forget(&dest);
                ArtOutcome::PlayersOwn
            } else if is_ours(&rec, &dest) && sha256::same_digest(&now, &hash) {
                ArtOutcome::Unchanged
            } else if is_ours(&rec, &dest) {
                crate::write_atomic(&dest, &bytes)?;
                rec.set(&dest, hash);
                ArtOutcome::Written
            } else {
                ArtOutcome::PlayersOwn
            }
        } else if other {
            ArtOutcome::PlayersOwn
        } else if !again && rec.hash_of(&dest).is_some() {
            // Turbine Hub wrote it and it's gone: the player cleared it in Steam.
            rec.set(&dest, CLEARED.to_string());
            ArtOutcome::Cleared
        } else {
            crate::write_atomic(&dest, &bytes)?;
            rec.set(&dest, hash);
            ArtOutcome::Written
        };
        out.push((dest, outcome));
    }
    rec.save(record)?;
    Ok(out)
}

/// One line that accounts for every piece. Zero counts are left out.
pub fn summary(done: &[(PathBuf, ArtOutcome)]) -> String {
    let count = |o: ArtOutcome| done.iter().filter(|(_, x)| *x == o).count();
    let parts = [
        (count(ArtOutcome::Written), "written"),
        (count(ArtOutcome::Unchanged), "already in place"),
        (count(ArtOutcome::PlayersOwn), "of your own kept"),
        (count(ArtOutcome::Cleared), "cleared in Steam and left out (Settings, Steam library art, Add puts them back)"),
        (count(ArtOutcome::NoSource), "missing from Turbine Hub's own files"),
    ];
    let said: Vec<String> = parts.iter().filter(|(n, _)| *n > 0).map(|(n, w)| format!("{n} {w}")).collect();
    if said.is_empty() {
        "no pictures to place".to_string()
    } else {
        said.join(", ")
    }
}

/// Did every piece come out badly (none written, in place or the player's own)? Then the player should hear why.
pub fn nothing_shown(done: &[(PathBuf, ArtOutcome)]) -> bool {
    !done.iter().any(|(_, o)| matches!(o, ArtOutcome::Written | ArtOutcome::Unchanged | ArtOutcome::PlayersOwn))
}

/// Removes Turbine Hub's own art for `appid` from `grid` (only files still exactly as Turbine Hub wrote them). Answers the
/// files removed.
pub fn remove_art(grid: &Path, appid: u32, record: &Path) -> io::Result<Vec<PathBuf>> {
    let mut rec = ArtRecord::load(record);
    let mut removed = Vec::new();
    for (part, _) in PARTS {
        let Some(name) = grid_name(appid, part) else { continue };
        let f = grid.join(name);
        if is_ours(&rec, &f) {
            fs::remove_file(&f)?;
            removed.push(f.clone());
        }
        rec.forget(&f);
    }
    rec.save(record)?;
    Ok(removed)
}

/// Where the Flatpak keeps the art sets (packaging/steam-art/<set>/*.png in the repository).
pub fn installed_set(set: &str) -> PathBuf {
    Path::new("/app/share/turbine-mod-manager/steam-art").join(set)
}

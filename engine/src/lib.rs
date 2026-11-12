//! Turbine Hub's engine: machine detection, setup steps, checks and the change ledger. It has no interface of its
//! own: the app, the background agent and the `--json` command line call into it.

pub mod agent;
pub mod bring;
pub mod checkup;
pub mod cli;
pub mod compat_tool;
pub mod fake;
pub mod games;
pub mod glossary;
pub mod hardware;
pub mod helper;
pub mod heroic;
pub mod history;
pub mod inflate;
pub mod json;
pub mod library;
pub mod lights;
pub mod modules;
pub mod net;
pub mod report;
pub mod saves;
pub mod selfupdate;
pub mod sensors;
pub mod settings;
pub mod sha256;
pub mod shortcuts;
pub mod snapshots;
pub mod sources;
pub mod steam;
pub mod steam_art;
pub mod switches;
pub mod tuneup;
pub mod ui;
pub mod vdf;
pub mod zip;

pub use games::{Game, Runtime, Scan, Store};

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes a file whole or not at all: a hidden file beside it first (synced), then renamed over it, then the folder
/// synced so the rename itself lasts through a power cut (matters on SD cards). Makes the folder when it is missing.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, bytes, None)
}

/// `write_atomic`, and the new file takes the permissions of `perms_from` when that file is there.
pub(crate) fn write_atomic_with(path: &Path, bytes: &[u8], perms_from: Option<&Path>) -> io::Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.turbinehub-new"));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        if let Some(meta) = perms_from.and_then(|p| fs::metadata(p).ok()) {
            let _ = f.set_permissions(meta.permissions());
        }
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Some(p) = path.parent()
        && let Ok(d) = File::open(p)
    {
        let _ = d.sync_all();
    }
    Ok(())
}

/// The player's home folder. TURBINEHUB_HOME points tests and screenshot runs at a fake one.
pub fn home_dir() -> PathBuf {
    std::env::var_os("TURBINEHUB_HOME").or_else(|| std::env::var_os("HOME")).map(Into::into).unwrap_or_else(|| "/home/deck".into())
}

/// Seconds since 1970 now.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Every installed Steam and Heroic game under this home folder, sorted by name.
pub fn scan_games(home: &Path) -> Scan {
    let mut scan = Scan::default();
    steam::scan(home, &mut scan);
    heroic::scan(home, &mut scan);
    scan.games.sort_by_key(|g| g.name.to_lowercase());
    scan
}

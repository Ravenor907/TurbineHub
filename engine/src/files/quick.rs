//! Quick access: the folders the player pinned and the files opened lately, kept in one small file in Turbine Hub's
//! own folder.

use super::fs::clean;
use super::places::Places;
use crate::json::{self, Json, quote};
use std::io;
use std::path::{Path, PathBuf};

/// The file in Turbine Hub's folder.
pub const FILE: &str = "files.json";
/// How many opened files are remembered.
pub const RECENT_MAX: usize = 20;
/// How many folders can be pinned.
pub const PINS_MAX: usize = 40;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Quick {
    /// Pinned folders (real paths), in the player's order.
    pub pins: Vec<PathBuf>,
    /// The files opened through Turbine Hub, newest first.
    pub recent: Vec<PathBuf>,
}

impl Quick {
    /// What a new player starts with: Desktop, Downloads, Documents, Pictures, Music and Videos, those that exist.
    pub fn standard(places: &Places) -> Quick {
        Quick { pins: places.dirs.iter().map(|d| d.path.clone()).collect(), recent: Vec::new() }
    }

    /// The saved list, or the standard one when nothing was saved yet or the file can't be read.
    pub fn load(folder: &Path, places: &Places) -> Quick {
        std::fs::read_to_string(folder.join(FILE)).ok().and_then(|t| Quick::from_json(&t)).unwrap_or_else(|| Quick::standard(places))
    }

    pub fn save(&self, folder: &Path) -> io::Result<()> {
        crate::write_atomic(&folder.join(FILE), self.to_json().as_bytes())
    }

    pub fn to_json(&self) -> String {
        let list = |v: &[PathBuf]| v.iter().map(|p| quote(&p.to_string_lossy())).collect::<Vec<_>>().join(",");
        format!("{{\"pins\":[{}],\"recent\":[{}]}}\n", list(&self.pins), list(&self.recent))
    }

    pub fn from_json(text: &str) -> Option<Quick> {
        let j = json::parse(text).ok()?;
        let list = |key: &str, max: usize| -> Vec<PathBuf> {
            let mut out: Vec<PathBuf> = Vec::new();
            for p in j.get(key).map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).filter(|p| p.starts_with('/')).map(|p| clean(Path::new(p))) {
                if !out.contains(&p) && out.len() < max {
                    out.push(p);
                }
            }
            out
        };
        j.get("pins")?;
        Some(Quick { pins: list("pins", PINS_MAX), recent: list("recent", RECENT_MAX) })
    }

    pub fn is_pinned(&self, folder: &Path) -> bool {
        self.pins.contains(&clean(folder))
    }

    /// Pins a folder at the end. False when it was pinned already or the list is full.
    pub fn pin(&mut self, folder: &Path) -> bool {
        let p = clean(folder);
        if self.pins.contains(&p) || self.pins.len() >= PINS_MAX {
            return false;
        }
        self.pins.push(p);
        true
    }

    pub fn unpin(&mut self, folder: &Path) -> bool {
        let before = self.pins.len();
        self.pins.retain(|p| *p != clean(folder));
        self.pins.len() != before
    }

    /// Moves the pin at `from` to stand at `to`.
    pub fn move_pin(&mut self, from: usize, to: usize) -> bool {
        if from >= self.pins.len() || to >= self.pins.len() || from == to {
            return false;
        }
        let p = self.pins.remove(from);
        self.pins.insert(to, p);
        true
    }

    /// A file was opened: it goes to the top of Recent.
    pub fn opened(&mut self, file: &Path) {
        let p = clean(file);
        self.recent.retain(|r| *r != p);
        self.recent.insert(0, p);
        self.recent.truncate(RECENT_MAX);
    }

    /// Takes a file off Recent (it was deleted, or the player asked).
    pub fn forget(&mut self, file: &Path) {
        self.recent.retain(|r| r != file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_keep_their_order_and_recent_is_capped() {
        let mut q = Quick::default();
        assert!(q.pin(Path::new("/home/deck/Desktop")));
        assert!(q.pin(Path::new("/home/deck/Downloads/")));
        assert!(!q.pin(Path::new("/home/deck/Downloads")));
        assert!(q.pin(Path::new("/run/media/deck/SD/My Mods")));
        assert!(q.is_pinned(Path::new("/home/deck/Downloads")));
        assert!(q.move_pin(2, 0));
        assert!(!q.move_pin(0, 9));
        assert_eq!(q.pins, [PathBuf::from("/run/media/deck/SD/My Mods"), "/home/deck/Desktop".into(), "/home/deck/Downloads".into()]);
        assert!(q.unpin(Path::new("/home/deck/Desktop")));
        assert!(!q.unpin(Path::new("/home/deck/Desktop")));
        for i in 0..30 {
            q.opened(Path::new(&format!("/home/deck/Documents/{i}.txt")));
        }
        q.opened(Path::new("/home/deck/Documents/25.txt"));
        assert_eq!(q.recent.len(), RECENT_MAX);
        assert_eq!(q.recent[0], PathBuf::from("/home/deck/Documents/25.txt"));
        assert_eq!(q.recent.iter().filter(|p| p.ends_with("25.txt")).count(), 1);
        q.forget(Path::new("/home/deck/Documents/25.txt"));
        assert_eq!(q.recent[0], PathBuf::from("/home/deck/Documents/29.txt"));
    }

    #[test]
    fn the_list_is_saved_and_read_back() {
        let dir = std::env::temp_dir().join(format!("turbinehub-quick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let places = Places::default();
        assert_eq!(Quick::load(&dir, &places), Quick::default());
        let mut q = Quick::default();
        q.pin(Path::new("/home/deck/Größe \"x\""));
        q.opened(Path::new("/home/deck/a\\b.txt"));
        q.save(&dir).unwrap();
        assert_eq!(Quick::load(&dir, &places), q);
        std::fs::write(dir.join(FILE), "{broken").unwrap();
        assert_eq!(Quick::load(&dir, &places), Quick::default());
        // Nothing but full paths is taken from the file.
        assert_eq!(Quick::from_json("{\"pins\":[\"relative\",7,\"/a/../b\"],\"recent\":[]}").unwrap().pins, [PathBuf::from("/b")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

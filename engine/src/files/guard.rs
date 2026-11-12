//! What may never be changed from the explorer, with the reason in plain words. Every operation asks here first.

use super::fs::{Fs, clean};
use super::places::{Places, name_of};
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

pub const SYSTEM_WHY: &str = "This is part of SteamOS. It is locked so updates always work, and Turbine Hub never unlocks it.";
pub const OWN_WHY: &str = "Turbine Hub keeps its own files here. Changing them could break it.";
pub const WINDOWS_DRIVE_WHY: &str = "This is a Windows drive. Turbine Hub only reads it, so Windows finds it just as it left it.";
pub const DRIVE_WHY: &str = "This is a whole drive (or holds one). It can't be renamed, moved or deleted.";
pub const HOME_WHY: &str = "This folder holds everything of yours. It can't be renamed, moved or deleted.";
pub const STEAM_WHY: &str = "Steam needs this folder to find your games. It can't be renamed, moved or deleted.";
pub const SETTINGS_WHY: &str = "Every app keeps its settings in here. It can't be renamed, moved or deleted.";
pub const BIN_WHY: &str = "Things in the Recycle Bin can only be restored or emptied.";
pub const NOT_ALLOWED: &str = "You aren't allowed to change things in this folder.";

/// The system's folders: looked at, never changed.
const SYSTEM: &[&str] = &["/usr", "/etc", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/boot", "/efi", "/proc", "/sys", "/dev"];

/// The path with every link in its folders followed, and its own name kept: what a rename or a delete would really
/// touch (a link itself, never what it points to).
pub fn true_path(fs: &dyn Fs, p: &Path) -> PathBuf {
    let p = clean(p);
    match (p.parent(), p.file_name()) {
        (Some(dir), Some(name)) => fs.canonical(dir).map(|d| d.join(name)).unwrap_or(p),
        _ => p,
    }
}

/// Is this a name a file or folder can have?
pub fn valid_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        Err("Type a name first.".into())
    } else if name == "." || name == ".." {
        Err("That name is taken by the system. Pick another.".into())
    } else if name.contains('/') || name.contains('\0') {
        Err("A name can't have a slash (/) in it.".into())
    } else if name.len() > 255 {
        Err("That name is too long. Keep it under 255 letters.".into())
    } else {
        Ok(())
    }
}

/// A problem the system reported, in plain words.
pub fn io_words(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::PermissionDenied => "you aren't allowed to change this".into(),
        io::ErrorKind::ReadOnlyFilesystem => "this drive is read only".into(),
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => "the drive is full".into(),
        io::ErrorKind::NotFound => "it isn't there any more".into(),
        io::ErrorKind::AlreadyExists => "there is something with that name already".into(),
        io::ErrorKind::DirectoryNotEmpty => "the folder isn't empty".into(),
        io::ErrorKind::Interrupted => "stopped".into(),
        _ => e.to_string(),
    }
}

/// A name for `name` that is free in `dir`: the name itself, else "name (2).ext", "name (3).ext"...
pub fn free_name(fs: &dyn Fs, dir: &Path, name: &OsStr) -> PathBuf {
    let first = dir.join(name);
    if fs.meta(&first).is_err() {
        return first;
    }
    let text = name.to_string_lossy();
    // The ending is what follows the last dot, unless the name starts with that dot.
    let (stem, ext) = match text.rfind('.') {
        Some(i) if i > 0 => (&text[..i], &text[i..]),
        _ => (&text[..], ""),
    };
    (2..100_000).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| fs.meta(p).is_err()).unwrap_or(first)
}

impl Places {
    /// Why nothing at this path can be changed, when it is in a place that is only ever looked at. By the text of
    /// the path alone: quick enough for every line of a list.
    pub fn locked(&self, real: &Path) -> Option<&'static str> {
        if SYSTEM.iter().any(|s| real.starts_with(s)) {
            Some(SYSTEM_WHY)
        } else if self.who.own.iter().any(|o| real.starts_with(o)) {
            Some(OWN_WHY)
        } else if self.drive_of(real).is_some_and(|d| d.read_only) {
            Some(WINDOWS_DRIVE_WHY)
        } else {
            None
        }
    }

    fn in_bin(&self, real: &Path) -> bool {
        self.trash_folders().iter().any(|t| real.starts_with(t))
    }

    /// The folders that hold everything together: they stay where they are and as they are named.
    fn fixed(&self, real: &Path) -> Option<&'static str> {
        let home = &self.who.home;
        if home.starts_with(real) {
            return Some(if real == home { HOME_WHY } else { DRIVE_WHY });
        }
        if self.drives.iter().any(|d| d.mount.starts_with(real)) {
            return Some(DRIVE_WHY);
        }
        let steam_link = real.starts_with(home.join(".steam")) && real.components().count() <= home.components().count() + 2;
        let account = self.steam.iter().any(|s| s.ends_with("userdata") && real.parent() == Some(s));
        if steam_link || account || self.steam.iter().any(|s| s == real) {
            return Some(STEAM_WHY);
        }
        if [".config", ".local", ".local/share", ".var", ".var/app", ".local/share/flatpak"].iter().any(|k| home.join(k) == real) {
            return Some(SETTINGS_WHY);
        }
        None
    }

    /// May this one item be renamed, moved away or deleted? Looks at the path as typed and at where its folders
    /// really lead, so a link into a locked place opens nothing.
    pub fn may_change(&self, fs: &dyn Fs, path: &Path) -> Result<(), String> {
        let typed = clean(path);
        let real = true_path(fs, &typed);
        for p in [&typed, &real] {
            if let Some(why) = self.locked(p).or_else(|| self.fixed(p)) {
                return Err(why.into());
            }
            if self.in_bin(p) {
                return Err(BIN_WHY.into());
            }
        }
        let meta = fs.meta(&real).map_err(|_| format!("{} isn't there any more.", name_of(&real)))?;
        let parent = real.parent().ok_or(DRIVE_WHY)?;
        let folder = fs.meta(parent).map_err(|_| NOT_ALLOWED.to_string())?;
        // Another drive begins here, even if nothing listed it.
        if folder.dev != meta.dev && meta.is_dir() {
            return Err(DRIVE_WHY.into());
        }
        if !self.who.may_write(&folder) {
            return Err(NOT_ALLOWED.into());
        }
        // In a folder like /tmp only the owner may remove a name.
        if folder.mode & 0o1000 != 0 && self.who.uid != 0 && meta.uid != self.who.uid && folder.uid != self.who.uid {
            return Err(NOT_ALLOWED.into());
        }
        Ok(())
    }

    /// May something be put into this folder?
    pub fn may_add_to(&self, fs: &dyn Fs, dir: &Path) -> Result<(), String> {
        let typed = clean(dir);
        let check = |p: &Path| match self.locked(p) {
            Some(why) => Err(why.to_string()),
            None if self.in_bin(p) => Err(BIN_WHY.to_string()),
            None => Ok(()),
        };
        check(&typed)?;
        let real = fs.canonical(&typed).map_err(|_| format!("{} isn't there any more.", name_of(&typed)))?;
        check(&real)?;
        match fs.meta(&real) {
            Ok(m) if !m.is_dir() => Err(format!("{} is a file, not a folder.", name_of(&real))),
            Ok(m) if self.who.may_write(&m) => Ok(()),
            _ => Err(NOT_ALLOWED.into()),
        }
    }
}

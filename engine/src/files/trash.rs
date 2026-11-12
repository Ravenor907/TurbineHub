//! The Recycle Bin, kept the way the freedesktop Trash specification (1.0) says, so Dolphin and every other file
//! manager see the same bin: `files/` holds what was deleted, `info/<name>.trashinfo` says where it came from and
//! when, and `directorysizes` remembers how big each deleted folder is. The home folder's drive uses
//! ~/.local/share/Trash; every other drive has its own bin at its top (`.Trash/<uid>` when the drive's owner set one
//! up, else `.Trash-<uid>`), so deleting never copies a file from one drive to another.

use super::When;
use super::fs::{Fs, NodeKind, measure, remove_tree};
use super::guard::{DRIVE_WHY, free_name, io_words, true_path};
use super::ops::move_tree;
use super::places::{Places, name_of};
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// One thing in a Recycle Bin.
#[derive(Debug, Clone, PartialEq)]
pub struct Trashed {
    /// The bin it is in (the folder that holds `files` and `info`).
    pub trash: PathBuf,
    /// Its name inside the bin.
    pub name: String,
    /// Where it is now: <trash>/files/<name>.
    pub at: PathBuf,
    /// Where it was deleted from.
    pub original: PathBuf,
    /// When, as the bin wrote it: 2026-11-12T09:31:05.
    pub deleted: String,
}

/// A path the way a .trashinfo file holds it: every byte but letters, digits, "-", "_", ".", "~" and "/" as %XX.
pub fn escape(path: &Path) -> String {
    let mut out = String::new();
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The bytes behind %XX text. A "%" that starts no number stays as it is.
pub fn unescape(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit()).then(|| std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())).flatten();
        match hex {
            Some(v) => {
                out.push(v);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    out
}

/// The top of the drive a bin belongs to, for the bins of other drives (their paths are written from there).
fn top_of(places: &Places, trash: &Path) -> Option<PathBuf> {
    if *trash == places.home_trash() {
        None
    } else if trash.parent().is_some_and(|p| p.ends_with(".Trash")) {
        trash.parent()?.parent().map(Path::to_path_buf)
    } else {
        trash.parent().map(Path::to_path_buf)
    }
}

fn real_folder(fs: &dyn Fs, p: &Path) -> bool {
    fs.meta(p).is_ok_and(|m| m.kind == NodeKind::Folder)
}

/// A bin is used only when it and its two folders are real folders: a link standing in for one is never followed.
fn usable(fs: &dyn Fs, trash: &Path) -> bool {
    real_folder(fs, trash) && real_folder(fs, &trash.join("files")) && real_folder(fs, &trash.join("info"))
}

fn make_bin(fs: &dyn Fs, trash: &Path) -> Result<(), String> {
    let fail = |e: io::Error| format!("The Recycle Bin of this drive can't be set up ({}).", io_words(&e));
    if fs.meta(trash).is_err() {
        fs.make_dirs(trash).map_err(fail)?;
        let _ = fs.set_mode(trash, 0o700);
    }
    // Nothing is made inside a bin that is a link to somewhere else.
    if !real_folder(fs, trash) {
        return Err("The Recycle Bin of this drive isn't a plain folder, so Turbine Hub won't use it.".into());
    }
    for sub in ["files", "info"] {
        if fs.meta(&trash.join(sub)).is_err() {
            fs.make_dir(&trash.join(sub)).map_err(fail)?;
        }
    }
    if usable(fs, trash) { Ok(()) } else { Err("The Recycle Bin of this drive isn't a plain folder, so Turbine Hub won't use it.".into()) }
}

/// The bin a deleted item goes to: the home folder's when it is on that drive, else the bin at the top of its own
/// drive. Sets the bin up when it isn't there yet.
pub fn bin_for(fs: &dyn Fs, places: &Places, path: &Path) -> Result<PathBuf, String> {
    let path = true_path(fs, path);
    let gone = |_| format!("{} isn't there any more.", name_of(&path));
    let meta = fs.meta(&path).map_err(gone)?;
    let home = places.home_trash();
    let home_dev = fs.meta(&home).or_else(|_| fs.follow(&places.who.home)).map(|m| m.dev).ok();
    let folder = path.parent().ok_or(DRIVE_WHY)?;
    let folder_dev = fs.meta(folder).map_err(gone)?.dev;
    if meta.is_dir() && meta.dev != folder_dev {
        return Err(DRIVE_WHY.into());
    }
    if home_dev == Some(folder_dev) {
        make_bin(fs, &home)?;
        return Ok(home);
    }
    // The top of the drive: the highest folder still on it.
    let mut top = folder.to_path_buf();
    while let Some(up) = top.parent() {
        if fs.meta(up).is_ok_and(|m| m.dev == folder_dev) {
            top = up.to_path_buf();
        } else {
            break;
        }
    }
    let uid = places.who.uid.to_string();
    // A shared ".Trash" counts only as a real folder with the sticky bit, as the specification asks.
    let shared = top.join(".Trash");
    let trash = if fs.meta(&shared).is_ok_and(|m| m.kind == NodeKind::Folder && m.mode & 0o1000 != 0) { shared.join(&uid) } else { top.join(format!(".Trash-{uid}")) };
    make_bin(fs, &trash)?;
    Ok(trash)
}

fn numbered(name: &OsStr, n: usize) -> OsString {
    if n == 1 {
        return name.to_os_string();
    }
    let bytes = name.as_bytes();
    let dot = bytes.iter().rposition(|&b| b == b'.').filter(|&i| i > 0).unwrap_or(bytes.len());
    let mut out = bytes[..dot].to_vec();
    out.extend(format!(" ({n})").as_bytes());
    out.extend(&bytes[dot..]);
    OsString::from_vec(out)
}

fn info_file(trash: &Path, name: &OsStr) -> PathBuf {
    let mut n = name.to_os_string();
    n.push(".trashinfo");
    trash.join("info").join(n)
}

fn sizes_file(trash: &Path) -> PathBuf {
    trash.join("directorysizes")
}

/// Rewrites `directorysizes` without `name`, and with a new line for it when `add` gives its size and date.
fn note_size(fs: &dyn Fs, trash: &Path, name: &OsStr, add: Option<(u64, u64)>) {
    let file = sizes_file(trash);
    let key = escape(Path::new(name));
    let old = fs.head(&file, 4 << 20).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut lines: Vec<String> = old.lines().filter(|l| l.splitn(3, ' ').nth(2) != Some(key.as_str())).map(String::from).collect();
    if let Some((bytes, date)) = add {
        lines.push(format!("{bytes} {date} {key}"));
    }
    if lines.is_empty() {
        let _ = fs.remove_file(&file);
    } else if lines.join("\n") != old.trim_end() {
        let _ = fs.write(&file, (lines.join("\n") + "\n").as_bytes());
    }
}

/// Moves one item to the Recycle Bin of its drive. The item itself moves: a link goes as a link.
pub fn put(fs: &dyn Fs, places: &Places, path: &Path, when: When) -> Result<Trashed, String> {
    places.may_change(fs, path)?;
    let path = true_path(fs, path);
    let trash = bin_for(fs, places, &path)?;
    let name = path.file_name().ok_or(DRIVE_WHY)?;
    let told = match top_of(places, &trash) {
        Some(top) => path.strip_prefix(&top).map(Path::to_path_buf).unwrap_or_else(|_| path.clone()),
        None => path.clone(),
    };
    let deleted = when.stamp(when.secs);
    let info = format!("[Trash Info]\nPath={}\nDeletionDate={deleted}\n", escape(&told));
    let is_dir = fs.meta(&path).is_ok_and(|m| m.is_dir());
    let size = if is_dir { measure(fs, &path).1 } else { 0 };
    for n in 1..10_000 {
        let kept = numbered(name, n);
        let note = info_file(&trash, &kept);
        // The note comes first and is made only if its name is free: that is what keeps two deletes apart.
        match fs.create_new(&note, info.as_bytes()) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{} can't go to the Recycle Bin ({}).", name_of(&path), io_words(&e))),
        }
        let at = trash.join("files").join(&kept);
        if fs.meta(&at).is_ok() {
            let _ = fs.remove_file(&note);
            continue;
        }
        if let Err(e) = move_tree(fs, &path, &at, &mut |_| true) {
            let _ = fs.remove_file(&note);
            return Err(format!("{} can't go to the Recycle Bin ({}).", name_of(&path), io_words(&e)));
        }
        if is_dir {
            let date = fs.meta(&note).map(|m| m.modified).unwrap_or(when.secs);
            note_size(fs, &trash, &kept, Some((size, date)));
        }
        return Ok(Trashed { trash, name: kept.to_string_lossy().into_owned(), at, original: path, deleted });
    }
    Err("The Recycle Bin is full of things with this name. Empty it first.".into())
}

/// The bins that are there: the home folder's and those of the other drives.
pub fn bins(fs: &dyn Fs, places: &Places) -> Vec<PathBuf> {
    places.trash_folders().into_iter().filter(|t| usable(fs, t)).collect()
}

/// Everything in every Recycle Bin, newest first.
pub fn items(fs: &dyn Fs, places: &Places) -> Vec<Trashed> {
    let mut out = Vec::new();
    for trash in bins(fs, places) {
        let top = top_of(places, &trash);
        for note in fs.names(&trash.join("info")).unwrap_or_default() {
            let bytes = note.as_bytes();
            let Some(kept) = bytes.strip_suffix(b".trashinfo").map(|b| OsString::from_vec(b.to_vec())) else { continue };
            let at = trash.join("files").join(&kept);
            if fs.meta(&at).is_err() {
                continue;
            }
            let text = fs.head(&trash.join("info").join(&note), 64 * 1024).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            let value = |key: &str| text.lines().find_map(|l| l.trim().strip_prefix(key)).map(str::to_string);
            let Some(told) = value("Path=") else { continue };
            let told = PathBuf::from(OsString::from_vec(unescape(&told)));
            // The home bin holds full paths; a drive's bin holds paths from the top of its drive, and a note that
            // points anywhere else (a full path, or ".." out of the drive) is not believed.
            let original = match (&top, told.is_absolute()) {
                (None, true) => super::fs::clean(&told),
                (Some(top), false) if !told.components().any(|c| c == std::path::Component::ParentDir) => super::fs::clean(&top.join(told)),
                _ => continue,
            };
            out.push(Trashed { trash: trash.clone(), name: kept.to_string_lossy().into_owned(), at, original, deleted: value("DeletionDate=").unwrap_or_default() });
        }
    }
    out.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Puts one item back where it was deleted from (its folders are made again when they are gone). When something
/// has taken its place: an error, or with `keep_both` it comes back under a free name. Answers where it is now.
pub fn restore(fs: &dyn Fs, places: &Places, item: &Trashed, keep_both: bool) -> Result<PathBuf, String> {
    let shown = name_of(&item.original);
    if !usable(fs, &item.trash) || !bins(fs, places).contains(&item.trash) || fs.meta(&item.at).is_err() || item.at.parent() != Some(&item.trash.join("files")) {
        return Err(format!("{shown} isn't in the Recycle Bin any more."));
    }
    let folder = item.original.parent().ok_or(DRIVE_WHY)?.to_path_buf();
    if let Some(why) = places.locked(&folder) {
        return Err(why.into());
    }
    fs.make_dirs(&folder).map_err(|e| format!("The folder {shown} was in can't be made again ({}).", io_words(&e)))?;
    places.may_add_to(fs, &folder)?;
    let name = item.original.file_name().ok_or(DRIVE_WHY)?;
    let to = if fs.meta(&item.original).is_ok() {
        if !keep_both {
            return Err(format!("There is already something called {shown} in {}.", folder.display()));
        }
        free_name(fs, &folder, name)
    } else {
        item.original.clone()
    };
    move_tree(fs, &item.at, &to, &mut |_| true).map_err(|e| format!("{shown} can't be put back ({}).", io_words(&e)))?;
    let kept = item.at.file_name().unwrap_or(name);
    let _ = fs.remove_file(&info_file(&item.trash, kept));
    note_size(fs, &item.trash, kept, None);
    Ok(to)
}

/// Erases everything in every Recycle Bin for good: the one thing here that can't be undone. Links are removed as
/// links. Answers how many things went and what couldn't be removed.
pub fn empty(fs: &dyn Fs, places: &Places) -> (u64, Vec<String>) {
    let mut gone = 0;
    let mut problems = Vec::new();
    for trash in bins(fs, places) {
        let (n, p) = empty_bin(fs, places, &trash);
        gone += n;
        problems.extend(p);
    }
    (gone, problems)
}

/// Erases everything in one Recycle Bin for good (the bin of one drive). Only ever one of this machine's bins that
/// is a plain folder: anything else is left alone and nothing is counted.
pub fn empty_bin(fs: &dyn Fs, places: &Places, trash: &Path) -> (u64, Vec<String>) {
    let mut gone = 0;
    let mut problems = Vec::new();
    if !bins(fs, places).iter().any(|b| b == trash) {
        return (gone, problems);
    }
    let files = trash.join("files");
    for name in fs.names(&files).unwrap_or_default() {
        match remove_tree(fs, &files.join(&name)) {
            Ok(()) => {
                gone += 1;
                let _ = fs.remove_file(&info_file(trash, &name));
            }
            Err(e) => problems.push(format!("{} couldn't be erased ({}).", name.to_string_lossy(), io_words(&e))),
        }
    }
    // Notes whose file is gone say nothing any more.
    for note in fs.names(&trash.join("info")).unwrap_or_default() {
        let kept = note.as_bytes().strip_suffix(b".trashinfo").map(|b| OsString::from_vec(b.to_vec()));
        if kept.is_none_or(|k| fs.meta(&files.join(k)).is_err()) {
            let _ = fs.remove_file(&trash.join("info").join(&note));
        }
    }
    if fs.names(&files).is_ok_and(|n| n.is_empty()) {
        let _ = fs.remove_file(&sizes_file(trash));
    }
    (gone, problems)
}

/// How many things are in the bins.
pub fn count(fs: &dyn Fs, places: &Places) -> usize {
    bins(fs, places).iter().map(|t| fs.names(&t.join("files")).map(|n| n.len()).unwrap_or(0)).sum()
}

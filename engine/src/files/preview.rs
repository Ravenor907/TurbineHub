//! What can be shown inside the app (text and common pictures; everything else opens with its own app), and the
//! properties of one item.

use super::fs::{Fs, NodeKind, Space};
use super::list::{mode_text, type_by_bytes, type_by_name};
use super::places::{Places, name_of};
use super::View;
use std::path::{Path, PathBuf};

/// The most text shown from one file.
pub const TEXT_MAX: usize = 256 * 1024;
/// The biggest picture loaded inside the app.
pub const PICTURE_MAX: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Preview {
    /// The text, how it was encoded, and whether the file goes on past what is shown.
    Text { text: String, encoding: &'static str, cut: bool },
    /// A picture, with the path the app loads it from.
    Picture { load: PathBuf },
    /// Not shown inside the app: it opens with its own app (in Desktop Mode), and `why` says so in plain words.
    Outside { why: String },
}

/// Text from bytes: UTF-8 (with or without its mark), UTF-16 with its mark, else the Western European bytes old
/// Windows files use. None for bytes that aren't text.
pub fn decode(bytes: &[u8]) -> Option<(String, &'static str)> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Some((String::from_utf8_lossy(rest).into_owned(), "UTF-8"));
    }
    for (mark, name, little) in [([0xFF, 0xFE], "UTF-16 LE", true), ([0xFE, 0xFF], "UTF-16 BE", false)] {
        if let Some(rest) = bytes.strip_prefix(&mark) {
            let units: Vec<u16> = rest.chunks_exact(2).map(|c| if little { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
            return Some((String::from_utf16_lossy(&units), name));
        }
    }
    if bytes.contains(&0) {
        return None;
    }
    match std::str::from_utf8(bytes) {
        Ok(t) => Some((t.to_string(), "UTF-8")),
        // A letter cut in half at the very end is still UTF-8.
        Err(e) if e.error_len().is_none() => Some((String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned(), "UTF-8")),
        Err(_) => {
            let odd = bytes.iter().filter(|b| b.is_ascii_control() && !b.is_ascii_whitespace()).count();
            (odd * 50 <= bytes.len()).then(|| (bytes.iter().map(|&b| b as char).collect(), "Western (Latin-1)"))
        }
    }
}

/// Can this file be shown inside the app, and what is there to show?
pub fn preview(fs: &dyn Fs, path: &Path) -> Preview {
    let outside = |why: &str| Preview::Outside { why: why.into() };
    // Where the file really is, so a link is read where it leads.
    let Ok(real) = fs.canonical(path) else { return outside("This file isn't there any more.") };
    let Ok(meta) = fs.meta(&real) else { return outside("This file isn't there any more.") };
    if meta.kind != NodeKind::File {
        return outside("This isn't a file.");
    }
    let Ok(head) = fs.head(&real, TEXT_MAX + 4) else { return outside("You aren't allowed to read this file.") };
    let name = name_of(path);
    let by_name = type_by_name(&name);
    let by_bytes = type_by_bytes(&head[..head.len().min(512)]);
    let svg = name.to_lowercase().ends_with(".svg") && by_bytes == "Text";
    if by_bytes == "Picture" || svg {
        if meta.size > PICTURE_MAX {
            return outside("This picture is too big to show here. It opens with its own app.");
        }
        return match fs.disk(&real) {
            Some(load) => Preview::Picture { load },
            None => outside("Turbine Hub can't reach this file."),
        };
    }
    let texty = matches!(by_name, Some("Text" | "Settings file" | "Script" | "Shortcut" | "Web page" | "Windows shortcut")) || by_name.is_none();
    if texty && matches!(by_bytes, "Text" | "Script" | "Empty file" | "File") {
        let cut = head.len() > TEXT_MAX;
        if let Some((text, encoding)) = decode(&head[..head.len().min(TEXT_MAX)]) {
            return Preview::Text { text, encoding, cut };
        }
    }
    outside("This kind of file opens with its own app.")
}

/// The command that opens a file with its own app on the host (Desktop Mode): through the Flatpak's door when the
/// app runs inside one.
pub fn open_line(path: &Path, in_sandbox: bool) -> Vec<String> {
    let mut line: Vec<String> = if in_sandbox { vec!["flatpak-spawn".into(), "--host".into()] } else { Vec::new() };
    line.extend(["xdg-open".to_string(), path.to_string_lossy().into_owned()]);
    line
}

/// Everything the Properties dialog shows for one item.
#[derive(Debug, Clone, PartialEq)]
pub struct Properties {
    /// The name on the disk.
    pub name: String,
    pub real_path: PathBuf,
    pub windows_path: String,
    /// A plain word for what it is.
    pub type_word: String,
    pub is_dir: bool,
    /// Its bytes; for a folder, everything counted in it.
    pub size: u64,
    /// Files and folders inside a folder.
    pub files: u64,
    pub folders: u64,
    /// False when a folder holds more than was counted (size, files and folders are then "at least").
    pub counted_all: bool,
    pub created: Option<u64>,
    pub modified: u64,
    /// The player can't change it.
    pub read_only: bool,
    /// Why, when it is in a locked place.
    pub locked: Option<String>,
    /// What a link says, as written.
    pub link: Option<String>,
    /// The drive it is on, as this view names it, and the room there.
    pub drive: String,
    pub space: Option<Space>,
    /// "drwxr-xr-x".
    pub mode: String,
    pub uid: u32,
    pub gid: u32,
}

/// The properties of one item. A folder is counted through, up to `count_max` names (links are counted as
/// themselves and never followed).
pub fn properties(fs: &dyn Fs, places: &Places, path: &Path, view: View, count_max: usize) -> Result<Properties, String> {
    let own = fs.meta(path).map_err(|_| format!("{} isn't there any more.", name_of(path)))?;
    let meta = fs.follow(path).unwrap_or(own);
    let name = name_of(path);
    let mut p = Properties {
        real_path: path.to_path_buf(),
        windows_path: places.windows_path(path),
        type_word: String::new(),
        is_dir: meta.is_dir(),
        size: if meta.kind == NodeKind::File { meta.size } else { 0 },
        files: 0,
        folders: 0,
        counted_all: true,
        created: own.created,
        modified: own.modified,
        read_only: places.locked(path).is_some() || !places.who.may_write(&meta),
        locked: places.locked(path).map(String::from),
        link: (own.kind == NodeKind::Link).then(|| fs.link_target(path).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default()),
        drive: places.drive_title(path, view),
        space: fs.space(if places.drive_of(path).is_some() { path } else { &places.who.home }),
        mode: mode_text(own.kind, own.mode),
        uid: own.uid,
        gid: own.gid,
        name,
    };
    p.type_word = match (own.kind, meta.kind) {
        (NodeKind::Link, NodeKind::Folder) => "Link to a folder".into(),
        (NodeKind::Link, _) => "Link".into(),
        (_, NodeKind::Folder) => "File folder".into(),
        (_, NodeKind::File) => type_by_name(&p.name).map(String::from).unwrap_or_else(|| fs.canonical(path).and_then(|at| fs.head(&at, 512)).map(|b| type_by_bytes(&b).to_string()).unwrap_or_else(|_| "File".into())),
        _ => "System file".into(),
    };
    if own.kind == NodeKind::Folder {
        let mut todo = vec![path.to_path_buf()];
        let mut seen = 0usize;
        'walk: while let Some(dir) = todo.pop() {
            for n in fs.names(&dir).unwrap_or_default() {
                if seen >= count_max {
                    p.counted_all = false;
                    break 'walk;
                }
                seen += 1;
                let child = dir.join(n);
                match fs.meta(&child) {
                    Ok(m) if m.kind == NodeKind::Folder => {
                        p.folders += 1;
                        todo.push(child);
                    }
                    Ok(m) => {
                        p.files += 1;
                        p.size += if m.kind == NodeKind::File { m.size } else { 0 };
                    }
                    Err(_) => {}
                }
            }
        }
    }
    Ok(p)
}

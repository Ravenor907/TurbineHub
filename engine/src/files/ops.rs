//! What the explorer can do to files: new folder, new text file, new shortcut, rename, copy, move, delete (to the
//! Recycle Bin), restore and empty. Each is a `Plan` first, which says in plain words what would happen and refuses
//! what may not; `run` does it and answers with the `Undo` that takes it back. Emptying the Recycle Bin is the only
//! thing that erases for good. Everything runs as the player: no password is ever involved.

use super::fs::{Fs, NodeKind, measure, remove_tree};
use super::guard::{free_name, io_words, true_path, valid_name};
use super::places::{Places, name_of};
use super::trash::{self, Trashed};
use super::{When, size_words};
use std::io;
use std::path::{Path, PathBuf};

/// What to do when the name is taken where something is going.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    /// Leave that one out.
    #[default]
    Skip,
    /// The one already there goes to the Recycle Bin first.
    Replace,
    /// The new one gets a name of its own: "name (2)".
    KeepBoth,
}

/// The player's answers for the names that are taken: one for all, and others for single items (by the path of what
/// is being copied or moved).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answers {
    pub all: Choice,
    pub each: Vec<(PathBuf, Choice)>,
}

impl Answers {
    pub fn all(choice: Choice) -> Answers {
        Answers { all: choice, each: Vec::new() }
    }
    fn of(&self, item: &Path) -> Choice {
        self.each.iter().find(|(p, _)| p == item).map(|(_, c)| *c).unwrap_or(self.all)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKind {
    New,
    Rename,
    Copy,
    Move,
    Delete,
    Restore,
    Empty,
}

/// One thing to do, with real paths.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    NewFolder { at: PathBuf, name: String },
    /// An empty text file (".txt" is added to a name without an ending).
    NewText { at: PathBuf, name: String },
    /// A shortcut (a .desktop file) to a folder, a file or a web address. The target may be typed in either style.
    NewShortcut { at: PathBuf, name: String, target: String },
    Rename { path: PathBuf, name: String },
    Copy { items: Vec<PathBuf>, into: PathBuf },
    Move { items: Vec<PathBuf>, into: PathBuf },
    /// To the Recycle Bin.
    Delete { items: Vec<PathBuf> },
    /// Out of the Recycle Bin, by where each item is in it (`Trashed::at`).
    Restore { items: Vec<PathBuf> },
    /// Erases everything in the Recycle Bin for good.
    EmptyBin,
}

impl Op {
    pub fn kind(&self) -> OpKind {
        match self {
            Op::NewFolder { .. } | Op::NewText { .. } | Op::NewShortcut { .. } => OpKind::New,
            Op::Rename { .. } => OpKind::Rename,
            Op::Copy { .. } => OpKind::Copy,
            Op::Move { .. } => OpKind::Move,
            Op::Delete { .. } => OpKind::Delete,
            Op::Restore { .. } => OpKind::Restore,
            Op::EmptyBin => OpKind::Empty,
        }
    }
}

/// What would happen, for the dialog to show before anything is touched.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub op: Op,
    pub title: String,
    /// What it does, one plain sentence per line.
    pub lines: Vec<String>,
    /// It erases for good: ask, with Cancel highlighted.
    pub dangerous: bool,
    pub files: u64,
    pub bytes: u64,
    /// The items whose name is taken where they are going: ask what to do with them.
    pub conflicts: Vec<PathBuf>,
    /// A move that is only a rename: done at once however big.
    pub same_drive: bool,
    /// What a new item will really be called (with its ending).
    pub makes: Option<PathBuf>,
}

/// How far a copy or move is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    pub files_done: u64,
    pub files: u64,
    pub bytes_done: u64,
    pub bytes: u64,
    /// The file being worked on.
    pub name: String,
}

/// How it went.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Done {
    /// One plain sentence for the note.
    pub words: String,
    /// What takes it back, when something can.
    pub undo: Option<Undo>,
    /// What is new on the disk (to put the highlight on).
    pub made: Vec<PathBuf>,
    /// What was left out, each with the reason.
    pub skipped: Vec<(PathBuf, String)>,
}

/// What takes one finished operation back.
#[derive(Debug, Clone, PartialEq)]
pub enum Undo {
    /// A new folder, text file or shortcut.
    Made { path: PathBuf, folder: bool },
    Renamed { from: PathBuf, to: PathBuf },
    /// The copies made, and what they replaced (now in the Recycle Bin).
    Copied { made: Vec<PathBuf>, replaced: Vec<Trashed> },
    /// (where it was, where it is), and what was replaced.
    Moved { moved: Vec<(PathBuf, PathBuf)>, replaced: Vec<Trashed> },
    Deleted { items: Vec<Trashed> },
    Restored { paths: Vec<PathBuf> },
}

impl Undo {
    pub fn kind(&self) -> OpKind {
        match self {
            Undo::Made { .. } => OpKind::New,
            Undo::Renamed { .. } => OpKind::Rename,
            Undo::Copied { .. } => OpKind::Copy,
            Undo::Moved { .. } => OpKind::Move,
            Undo::Deleted { .. } => OpKind::Delete,
            Undo::Restored { .. } => OpKind::Restore,
        }
    }
    /// What the Undo button says it takes back.
    pub fn words(&self) -> String {
        let count = |n: usize| if n == 1 { "1 item".to_string() } else { format!("{n} items") };
        match self {
            Undo::Made { path, .. } => format!("Undo making {}", name_of(path)),
            Undo::Renamed { from, to } => format!("Undo renaming {} to {}", name_of(from), name_of(to)),
            Undo::Copied { made, .. } => format!("Undo copying {}", count(made.len())),
            Undo::Moved { moved, .. } => format!("Undo moving {}", count(moved.len())),
            Undo::Deleted { items } => format!("Undo deleting {}", count(items.len())),
            Undo::Restored { paths } => format!("Undo restoring {}", count(paths.len())),
        }
    }
}

/// The last operation of each kind that can be taken back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    /// Oldest first, one per kind.
    kept: Vec<Undo>,
}

impl History {
    /// Keeps what a finished operation handed back, in place of the last one of its kind.
    pub fn remember(&mut self, done: &Done) {
        if let Some(u) = &done.undo {
            self.kept.retain(|k| k.kind() != u.kind());
            self.kept.push(u.clone());
        }
    }
    /// The newest of all: what Ctrl+Z takes back.
    pub fn newest(&self) -> Option<&Undo> {
        self.kept.last()
    }
    pub fn last(&self, kind: OpKind) -> Option<&Undo> {
        self.kept.iter().find(|u| u.kind() == kind)
    }
    /// Takes one out to undo it.
    pub fn take(&mut self, kind: OpKind) -> Option<Undo> {
        let i = self.kept.iter().position(|u| u.kind() == kind)?;
        Some(self.kept.remove(i))
    }
    pub fn take_newest(&mut self) -> Option<Undo> {
        self.kept.pop()
    }
}

/// What a copy reports as it goes.
pub enum Step<'a> {
    Bytes(u64),
    /// One file, link or folder is done.
    File(&'a Path),
}

fn stopped() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "stopped")
}

/// Copies a file, a link (as a link) or a whole folder to a name that is free. Never follows a link and never
/// writes over anything. `step` stops it by answering false.
pub fn copy_tree(fs: &dyn Fs, from: &Path, to: &Path, step: &mut dyn FnMut(Step) -> bool) -> io::Result<()> {
    let m = fs.meta(from)?;
    match m.kind {
        NodeKind::Link => fs.make_link(&fs.link_target(from)?, to)?,
        NodeKind::File => {
            fs.copy_file(from, to, &mut |n| step(Step::Bytes(n)))?;
        }
        NodeKind::Folder => {
            fs.make_dir(to)?;
            for name in fs.names(from)? {
                copy_tree(fs, &from.join(&name), &to.join(&name), step)?;
            }
            let _ = fs.set_mode(to, m.mode);
            let _ = fs.set_modified(to, m.modified);
        }
        // Pipes, sockets and devices mean something only where they are.
        NodeKind::Other => {}
    }
    if step(Step::File(from)) { Ok(()) } else { Err(stopped()) }
}

/// Moves to a name that is free: a rename on the same drive, else a copy and then the removal of the original.
/// A copy that fails is taken away again and the original stays.
pub fn move_tree(fs: &dyn Fs, from: &Path, to: &Path, step: &mut dyn FnMut(Step) -> bool) -> io::Result<()> {
    match fs.rename(from, to) {
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {}
        other => return other,
    }
    if let Err(e) = copy_tree(fs, from, to, step) {
        let _ = remove_tree(fs, to);
        return Err(e);
    }
    remove_tree(fs, from)
}

fn count(n: usize) -> String {
    if n == 1 { "1 item".into() } else { format!("{n} items") }
}

fn taken(name: &str) -> String {
    format!("There is already something called {name} here.")
}

/// The file a shortcut is: a link in the .desktop format every desktop understands.
fn shortcut_text(fs: &dyn Fs, places: &Places, name: &str, target: &str) -> Result<String, String> {
    let target = target.trim();
    if target.is_empty() || target.contains(['\n', '\r']) {
        return Err("Type what the shortcut opens: a folder, a file or a web address.".into());
    }
    let web = ["http://", "https://", "steam://"].iter().any(|p| target.starts_with(p));
    let (url, icon) = if web {
        (target.to_string(), "applications-internet")
    } else {
        let path = places.real_path(fs, target).ok_or("Type what the shortcut opens: a folder, a file or a web address.")?;
        let meta = fs.follow(&path).map_err(|_| format!("Can't find {target}."))?;
        (format!("file://{}", trash::escape(&path)), if meta.is_dir() { "folder" } else { "text-x-generic" })
    };
    Ok(format!("[Desktop Entry]\nType=Link\nName={}\nURL={url}\nIcon={icon}\n", name.replace(['\n', '\r'], " ")))
}

fn with_ending(name: &str, ending: &str) -> String {
    if name.to_lowercase().ends_with(ending) || (ending == ".txt" && name.rfind('.').is_some_and(|i| i > 0)) { name.to_string() } else { format!("{name}{ending}") }
}

/// Looks at what an operation would do and says it in plain words, or refuses with the reason. Changes nothing.
pub fn plan(fs: &dyn Fs, places: &Places, op: Op) -> Result<Plan, String> {
    let mut p = Plan { op: op.clone(), title: String::new(), lines: Vec::new(), dangerous: false, files: 0, bytes: 0, conflicts: Vec::new(), same_drive: true, makes: None };
    match &op {
        Op::NewFolder { at, name } | Op::NewText { at, name } | Op::NewShortcut { at, name, .. } => {
            valid_name(name)?;
            let (what, full) = match &op {
                Op::NewFolder { .. } => ("folder", name.clone()),
                Op::NewText { .. } => ("text file", with_ending(name, ".txt")),
                _ => ("shortcut", with_ending(name, ".desktop")),
            };
            valid_name(&full)?;
            places.may_add_to(fs, at)?;
            if let Op::NewShortcut { target, .. } = &op {
                shortcut_text(fs, places, name, target)?;
            }
            let to = fs.canonical(at).unwrap_or_else(|_| super::fs::clean(at)).join(&full);
            if fs.meta(&to).is_ok() {
                return Err(taken(&full));
            }
            p.title = format!("New {what}");
            p.lines.push(format!("Makes the {what} {full} in {}.", name_of(at)));
            p.makes = Some(to);
        }
        Op::Rename { path, name } => {
            valid_name(name)?;
            places.may_change(fs, path)?;
            let real = true_path(fs, path);
            if name_of(&real) == *name {
                return Err("That is its name already.".into());
            }
            let to = real.with_file_name(name);
            if fs.meta(&to).is_ok() {
                return Err(taken(name));
            }
            p.title = "Rename".into();
            p.lines.push(format!("Renames {} to {name}.", name_of(&real)));
            p.makes = Some(to);
        }
        Op::Copy { items, into } | Op::Move { items, into } => {
            let moving = matches!(op, Op::Move { .. });
            if items.is_empty() {
                return Err("Pick something first.".into());
            }
            places.may_add_to(fs, into)?;
            let into = fs.canonical(into).map_err(|e| io_words(&e))?;
            let into_dev = fs.meta(&into).map(|m| m.dev).ok();
            for item in items {
                let real = true_path(fs, item);
                let meta = fs.meta(&real).map_err(|_| format!("{} isn't there any more.", name_of(&real)))?;
                if moving {
                    places.may_change(fs, item)?;
                    if real.parent() == Some(&into) {
                        return Err(format!("{} is in this folder already.", name_of(&real)));
                    }
                }
                if meta.is_dir() && into.starts_with(&real) {
                    return Err(format!("A folder can't be put inside itself ({}).", name_of(&real)));
                }
                let (files, bytes) = measure(fs, &real);
                p.files += files;
                p.bytes += bytes;
                p.same_drive &= Some(meta.dev) == into_dev;
                if real.file_name().is_some_and(|n| fs.meta(&into.join(n)).is_ok()) {
                    p.conflicts.push(item.clone());
                }
            }
            if (!moving || !p.same_drive) && let Some(room) = fs.space(&into) && room.free < p.bytes {
                return Err(format!("Not enough room: this needs {} and {} has {} free.", size_words(p.bytes), places.drive_title(&into, super::View::Windows), size_words(room.free)));
            }
            p.title = format!("{} {}", if moving { "Move" } else { "Copy" }, count(items.len()));
            p.lines.push(format!("{} {} ({}) to {}.", if moving { "Moves" } else { "Copies" }, count(items.len()), size_words(p.bytes), name_of(&into)));
            if !p.conflicts.is_empty() {
                p.lines.push(format!("{} there already: skip, replace (the old one goes to the Recycle Bin) or keep both.", if p.conflicts.len() == 1 { "1 name is".to_string() } else { format!("{} names are", p.conflicts.len()) }));
            }
        }
        Op::Delete { items } => {
            if items.is_empty() {
                return Err("Pick something first.".into());
            }
            for item in items {
                places.may_change(fs, item)?;
            }
            p.title = format!("Delete {}", count(items.len()));
            p.lines.push(format!("{} to the Recycle Bin. You can get {} back from there.", if items.len() == 1 { format!("{} goes", name_of(&items[0])) } else { format!("{} go", count(items.len())) }, if items.len() == 1 { "it" } else { "them" }));
        }
        Op::Restore { items } => {
            let all = trash::items(fs, places);
            if items.is_empty() {
                return Err("Pick something first.".into());
            }
            for item in items {
                let t = all.iter().find(|t| t.at == *item).ok_or_else(|| format!("{} isn't in the Recycle Bin any more.", name_of(item)))?;
                if let Some(why) = t.original.parent().and_then(|f| places.locked(f)) {
                    return Err(why.into());
                }
                if fs.meta(&t.original).is_ok() {
                    p.conflicts.push(item.clone());
                }
            }
            p.title = format!("Restore {}", count(items.len()));
            p.lines.push(format!("Puts {} back where {} deleted from.", count(items.len()), if items.len() == 1 { "it was" } else { "they were" }));
        }
        Op::EmptyBin => {
            let n = trash::count(fs, places);
            if n == 0 {
                return Err("The Recycle Bin is empty already.".into());
            }
            p.files = n as u64;
            p.dangerous = true;
            p.title = "Empty the Recycle Bin".into();
            p.lines.push(format!("Erases {} for good. This can't be undone.", count(n)));
        }
    }
    Ok(p)
}

/// Does what the plan said, after looking again (the disk may have changed since). `tick` hears how far a copy or
/// move is and stops it by answering false. Nothing is ever written over: a name that is taken follows `answers`.
pub fn run(fs: &dyn Fs, places: &Places, planned: &Plan, answers: &Answers, when: When, tick: &mut dyn FnMut(&Progress) -> bool) -> Result<Done, String> {
    let p = plan(fs, places, planned.op.clone())?;
    let failed = |what: &str, e: io::Error| format!("{what} ({}).", io_words(&e));
    match &p.op {
        Op::NewFolder { name, .. } | Op::NewText { name, .. } | Op::NewShortcut { name, .. } => {
            let to = p.makes.clone().ok_or("Nothing to make.")?;
            let folder = matches!(p.op, Op::NewFolder { .. });
            match &p.op {
                Op::NewFolder { .. } => fs.make_dir(&to),
                Op::NewText { .. } => fs.create_new(&to, b""),
                Op::NewShortcut { target, .. } => fs.create_new(&to, shortcut_text(fs, places, name, target)?.as_bytes()),
                _ => Ok(()),
            }
            .map_err(|e| failed(&format!("{} couldn't be made", name_of(&to)), e))?;
            Ok(Done { words: format!("{} is made.", name_of(&to)), undo: Some(Undo::Made { path: to.clone(), folder }), made: vec![to], skipped: Vec::new() })
        }
        Op::Rename { path, .. } => {
            let (from, to) = (true_path(fs, path), p.makes.clone().ok_or("Nothing to rename.")?);
            fs.rename(&from, &to).map_err(|e| failed(&format!("{} couldn't be renamed", name_of(&from)), e))?;
            Ok(Done { words: format!("{} is now called {}.", name_of(&from), name_of(&to)), undo: Some(Undo::Renamed { from, to: to.clone() }), made: vec![to], skipped: Vec::new() })
        }
        Op::Copy { items, into } | Op::Move { items, into } => {
            let moving = matches!(p.op, Op::Move { .. });
            let into = fs.canonical(into).map_err(|e| io_words(&e))?;
            let mut progress = Progress { files: p.files, bytes: p.bytes, ..Progress::default() };
            let mut done = Done::default();
            let (mut pairs, mut replaced) = (Vec::new(), Vec::new());
            let mut halted = false;
            for item in items {
                let from = true_path(fs, item);
                let Some(name) = from.file_name() else { continue };
                let mut to = into.join(name);
                if fs.meta(&to).is_ok() {
                    // A copy next to itself can only be a second one.
                    let choice = if to == from { Choice::KeepBoth } else { answers.of(item) };
                    match choice {
                        Choice::Skip => {
                            done.skipped.push((item.clone(), "There is something with this name already.".into()));
                            continue;
                        }
                        Choice::KeepBoth => to = free_name(fs, &into, name),
                        Choice::Replace if from.starts_with(&to) => {
                            done.skipped.push((item.clone(), "It is inside the folder it would replace.".into()));
                            continue;
                        }
                        Choice::Replace => match trash::put(fs, places, &to, when) {
                            Ok(t) => replaced.push(t),
                            Err(why) => {
                                done.skipped.push((item.clone(), why));
                                continue;
                            }
                        },
                    }
                }
                progress.name = name.to_string_lossy().into_owned();
                let mut step = |s: Step| {
                    match s {
                        Step::Bytes(n) => progress.bytes_done += n,
                        Step::File(_) => progress.files_done += 1,
                    }
                    tick(&progress)
                };
                let went = if moving {
                    move_tree(fs, &from, &to, &mut step)
                } else {
                    copy_tree(fs, &from, &to, &mut step).inspect_err(|_| {
                        let _ = remove_tree(fs, &to);
                    })
                };
                match went {
                    Ok(()) => {
                        if moving {
                            pairs.push((from, to.clone()));
                        }
                        done.made.push(to);
                    }
                    Err(e) => {
                        halted = e.kind() == io::ErrorKind::Interrupted;
                        done.skipped.push((item.clone(), io_words(&e)));
                        if halted {
                            break;
                        }
                    }
                }
            }
            let verb = if moving { "moved" } else { "copied" };
            done.words = match (done.made.len(), done.skipped.len(), halted) {
                (n, _, true) => format!("Stopped: {} {verb}.", count(n)),
                (n, 0, _) => format!("{} {verb} to {}.", count(n), name_of(&into)),
                (n, s, _) => format!("{} {verb} to {}, {} left out.", count(n), name_of(&into), s),
            };
            if !done.made.is_empty() || !replaced.is_empty() {
                done.undo = Some(if moving { Undo::Moved { moved: pairs, replaced } } else { Undo::Copied { made: done.made.clone(), replaced } });
            }
            Ok(done)
        }
        Op::Delete { items } => {
            let mut done = Done::default();
            let mut gone = Vec::new();
            for item in items {
                match trash::put(fs, places, item, when) {
                    Ok(t) => gone.push(t),
                    Err(why) => done.skipped.push((item.clone(), why)),
                }
            }
            if gone.is_empty() {
                return Err(done.skipped.first().map(|(_, why)| why.clone()).unwrap_or_else(|| "Nothing was deleted.".into()));
            }
            done.words = format!("{} moved to the Recycle Bin.", if gone.len() == 1 { name_of(&gone[0].original) } else { count(gone.len()) });
            done.undo = Some(Undo::Deleted { items: gone });
            Ok(done)
        }
        Op::Restore { items } => {
            let all = trash::items(fs, places);
            let mut done = Done::default();
            for item in items {
                let Some(t) = all.iter().find(|t| t.at == *item) else { continue };
                let keep_both = match answers.of(item) {
                    Choice::KeepBoth => true,
                    Choice::Skip => false,
                    // The one in the way goes to the Recycle Bin itself, so nothing is lost.
                    Choice::Replace => fs.meta(&t.original).is_ok() && trash::put(fs, places, &t.original, when).is_err(),
                };
                match trash::restore(fs, places, t, keep_both) {
                    Ok(to) => done.made.push(to),
                    Err(why) => done.skipped.push((item.clone(), why)),
                }
            }
            if done.made.is_empty() {
                return Err(done.skipped.first().map(|(_, why)| why.clone()).unwrap_or_else(|| "Nothing was restored.".into()));
            }
            done.words = format!("{} put back.", count(done.made.len()));
            done.undo = Some(Undo::Restored { paths: done.made.clone() });
            Ok(done)
        }
        Op::EmptyBin => {
            let (gone, problems) = trash::empty(fs, places);
            let words = if problems.is_empty() { format!("Recycle Bin emptied: {} erased.", count(gone as usize)) } else { format!("{} erased, {} couldn't be: {}", count(gone as usize), problems.len(), problems[0]) };
            Ok(Done { words, undo: None, made: Vec::new(), skipped: Vec::new() })
        }
    }
}

/// Takes one finished operation back. Nothing is erased by an undo: what has to go, goes to the Recycle Bin.
pub fn undo(fs: &dyn Fs, places: &Places, what: &Undo, when: When) -> Result<Done, String> {
    let mut done = Done::default();
    let bring_back = |replaced: &[Trashed], done: &mut Done| {
        for t in replaced {
            if let Err(why) = trash::restore(fs, places, t, false) {
                done.skipped.push((t.original.clone(), why));
            }
        }
    };
    match what {
        Undo::Made { path, folder: true } => {
            places.may_change(fs, path)?;
            fs.remove_dir(path).map_err(|e| match e.kind() {
                io::ErrorKind::DirectoryNotEmpty => format!("{} has things in it now, so it stays. Delete it if you want it gone.", name_of(path)),
                _ => format!("{} couldn't be taken away ({}).", name_of(path), io_words(&e)),
            })?;
            done.words = format!("{} is gone again.", name_of(path));
        }
        Undo::Made { path, folder: false } => {
            trash::put(fs, places, path, when)?;
            done.words = format!("{} moved to the Recycle Bin.", name_of(path));
        }
        Undo::Renamed { from, to } => {
            places.may_change(fs, to)?;
            fs.rename(to, from).map_err(|e| format!("{} couldn't get its old name back ({}).", name_of(to), io_words(&e)))?;
            done.words = format!("{} is called {} again.", name_of(to), name_of(from));
            done.made.push(from.clone());
        }
        Undo::Copied { made, replaced } => {
            let mut gone = 0;
            for path in made.iter().filter(|p| fs.meta(p).is_ok()) {
                match trash::put(fs, places, path, when) {
                    Ok(_) => gone += 1,
                    Err(why) => done.skipped.push((path.clone(), why)),
                }
            }
            bring_back(replaced, &mut done);
            done.words = format!("{} moved to the Recycle Bin.", if gone == 1 { "The copy".to_string() } else { format!("{gone} copies") });
        }
        Undo::Moved { moved, replaced } => {
            for (from, to) in moved {
                let back = (|| -> Result<(), String> {
                    if fs.meta(from).is_ok() {
                        return Err(taken(&name_of(from)));
                    }
                    places.may_change(fs, to)?;
                    places.may_add_to(fs, from.parent().unwrap_or(Path::new("/")))?;
                    move_tree(fs, to, from, &mut |_| true).map_err(|e| io_words(&e))
                })();
                match back {
                    Ok(()) => done.made.push(from.clone()),
                    Err(why) => done.skipped.push((to.clone(), why)),
                }
            }
            bring_back(replaced, &mut done);
            done.words = format!("{} moved back.", count(done.made.len()));
        }
        Undo::Deleted { items } => {
            for t in items {
                match trash::restore(fs, places, t, false) {
                    Ok(to) => done.made.push(to),
                    Err(why) => done.skipped.push((t.original.clone(), why)),
                }
            }
            done.words = format!("{} put back.", count(done.made.len()));
            if !done.made.is_empty() {
                done.undo = Some(Undo::Restored { paths: done.made.clone() });
            }
        }
        Undo::Restored { paths } => {
            let mut gone = Vec::new();
            for path in paths {
                match trash::put(fs, places, path, when) {
                    Ok(t) => gone.push(t),
                    Err(why) => done.skipped.push((path.clone(), why)),
                }
            }
            done.words = format!("{} moved to the Recycle Bin again.", count(gone.len()));
            if !gone.is_empty() {
                done.undo = Some(Undo::Deleted { items: gone });
            }
        }
    }
    if done.made.is_empty() && done.words.starts_with("0 items") {
        return Err(done.skipped.first().map(|(_, why)| why.clone()).unwrap_or_else(|| "Nothing could be taken back.".into()));
    }
    Ok(done)
}

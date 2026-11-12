//! Every operation, every refusal and the Recycle Bin, in a temp folder.

use super::guard::*;
use super::ops::{Step, copy_tree};
use super::places::DriveAt;
use super::testing::{NOW, TwoDrives, machine, places};
use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const DOCS: &str = "/home/deck/Documents";
const CARD: &str = "/run/media/deck/sdcard";

fn quiet(_: &Progress) -> bool {
    true
}

/// Plans and runs in one go, the way a press with no question to ask does.
fn go(fs: &dyn Fs, p: &Places, op: Op, answers: Choice) -> Result<Done, String> {
    let planned = plan(fs, p, op)?;
    run(fs, p, &planned, &Answers::all(answers), NOW, &mut quiet)
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

#[test]
fn new_folder_text_file_and_shortcut() {
    let (t, fs, p) = machine("new");
    let folder = Op::NewFolder { at: DOCS.into(), name: "Mods to try".into() };
    let planned = plan(&fs, &p, folder.clone()).unwrap();
    assert_eq!((planned.title.as_str(), planned.lines.as_slice(), planned.dangerous), ("New folder", &["Makes the folder Mods to try in Documents.".to_string()][..], false));
    // A plan changes nothing.
    assert!(!t.has("/home/deck/Documents/Mods to try"));
    let done = run(&fs, &p, &planned, &Answers::default(), NOW, &mut quiet).unwrap();
    assert!(t.at("/home/deck/Documents/Mods to try").is_dir());
    assert_eq!((done.words.as_str(), done.made.as_slice()), ("Mods to try is made.", &[PathBuf::from("/home/deck/Documents/Mods to try")][..]));
    assert_eq!(plan(&fs, &p, folder).unwrap_err(), "There is already something called Mods to try here.");
    // Undo takes an empty new folder away; one that got something in it stays.
    let undo_it = done.undo.unwrap();
    assert_eq!(undo_it.words(), "Undo making Mods to try");
    t.write("/home/deck/Documents/Mods to try/a.txt", b"");
    assert!(undo(&fs, &p, &undo_it, NOW).unwrap_err().contains("has things in it now"));
    std::fs::remove_file(t.at("/home/deck/Documents/Mods to try/a.txt")).unwrap();
    assert_eq!(undo(&fs, &p, &undo_it, NOW).unwrap().words, "Mods to try is gone again.");
    assert!(!t.has("/home/deck/Documents/Mods to try"));

    // A text file gets its ending; undo sends it to the Recycle Bin.
    let done = go(&fs, &p, Op::NewText { at: DOCS.into(), name: "Shopping list".into() }, Choice::Skip).unwrap();
    assert_eq!(t.read("/home/deck/Documents/Shopping list.txt"), "");
    assert_eq!(go(&fs, &p, Op::NewText { at: DOCS.into(), name: "notes.md".into() }, Choice::Skip).unwrap().made, paths(&["/home/deck/Documents/notes.md"]));
    undo(&fs, &p, &done.undo.unwrap(), NOW).unwrap();
    assert!(!t.has("/home/deck/Documents/Shopping list.txt") && t.has("/home/deck/.local/share/Trash/files/Shopping list.txt"));

    // A shortcut to a folder typed the Windows way, and one to a web page.
    go(&fs, &p, Op::NewShortcut { at: "/home/deck/Desktop".into(), name: "My downloads".into(), target: "C:\\Users\\deck\\Downloads".into() }, Choice::Skip).unwrap();
    assert_eq!(t.read("/home/deck/Desktop/My downloads.desktop"), "[Desktop Entry]\nType=Link\nName=My downloads\nURL=file:///home/deck/Downloads\nIcon=folder\n");
    go(&fs, &p, Op::NewShortcut { at: "/home/deck/Desktop".into(), name: "ProtonDB".into(), target: "https://www.protondb.com".into() }, Choice::Skip).unwrap();
    assert!(t.read("/home/deck/Desktop/ProtonDB.desktop").contains("URL=https://www.protondb.com\n"));
    go(&fs, &p, Op::NewShortcut { at: "/home/deck/Desktop".into(), name: "Setup".into(), target: "/home/deck/Documents/My setup.json".into() }, Choice::Skip).unwrap();
    assert!(t.read("/home/deck/Desktop/Setup.desktop").contains("URL=file:///home/deck/Documents/My%20setup.json\nIcon=text-x-generic\n"));
    let shortcut = |target: &str| plan(&fs, &p, Op::NewShortcut { at: DOCS.into(), name: "x".into(), target: target.into() });
    assert!(shortcut("").unwrap_err().contains("what the shortcut opens"));
    assert!(shortcut("C:\\Users\\deck\\Nope").unwrap_err().contains("Can't find"));
    assert!(shortcut("a\nExec=rm").is_err());

    // Names that can't be.
    for (name, why) in [("", "Type a name first."), ("   ", "Type a name first."), ("..", "That name is taken by the system. Pick another."), ("a/b", "A name can't have a slash (/) in it."), (&"x".repeat(256), "That name is too long. Keep it under 255 letters.")] {
        assert_eq!(plan(&fs, &p, Op::NewFolder { at: DOCS.into(), name: name.into() }).unwrap_err(), why, "{name:?}");
    }
    // Odd but fine.
    for name in ["Ünïcode ✓", "back\\slash", " spaces ", "C:", "100%"] {
        go(&fs, &p, Op::NewFolder { at: DOCS.into(), name: name.into() }, Choice::Skip).unwrap();
        assert!(t.at(DOCS).join(name).is_dir(), "{name}");
    }
    // Where nothing may be made.
    assert_eq!(plan(&fs, &p, Op::NewFolder { at: "/etc".into(), name: "x".into() }).unwrap_err(), SYSTEM_WHY);
    assert_eq!(plan(&fs, &p, Op::NewFolder { at: "/home/deck/.local/share/Trash/files".into(), name: "x".into() }).unwrap_err(), BIN_WHY);
    assert!(plan(&fs, &p, Op::NewFolder { at: "/home/deck/Nope".into(), name: "x".into() }).unwrap_err().contains("isn't there"));
    assert!(plan(&fs, &p, Op::NewFolder { at: "/home/deck/Documents/My setup.json".into(), name: "x".into() }).unwrap_err().contains("is a file"));
}

#[test]
fn rename_and_its_undo() {
    let (t, fs, p) = machine("rename");
    let from = "/home/deck/Documents/Controller layout notes.txt";
    let done = go(&fs, &p, Op::Rename { path: from.into(), name: "Layout - no, plain.txt".into() }, Choice::Skip).unwrap();
    assert!(!t.has(from) && t.has("/home/deck/Documents/Layout - no, plain.txt"));
    assert_eq!(done.words, "Controller layout notes.txt is now called Layout - no, plain.txt.");
    let back = undo(&fs, &p, &done.undo.unwrap(), NOW).unwrap();
    assert!(t.has(from) && back.words.contains("again"));
    // Never over something else, and not to its own name.
    assert_eq!(plan(&fs, &p, Op::Rename { path: from.into(), name: "My setup.json".into() }).unwrap_err(), "There is already something called My setup.json here.");
    assert_eq!(plan(&fs, &p, Op::Rename { path: from.into(), name: "Controller layout notes.txt".into() }).unwrap_err(), "That is its name already.");
    assert!(plan(&fs, &p, Op::Rename { path: from.into(), name: "a/b".into() }).is_err());
    assert!(plan(&fs, &p, Op::Rename { path: "/home/deck/Documents/gone".into(), name: "x".into() }).unwrap_err().contains("isn't there any more"));
    // A link is renamed itself; what it points to keeps its name.
    t.link("My setup.json", "/home/deck/Documents/link");
    go(&fs, &p, Op::Rename { path: "/home/deck/Documents/link".into(), name: "link 2".into() }, Choice::Skip).unwrap();
    assert!(t.at("/home/deck/Documents/link 2").is_symlink() && t.has("/home/deck/Documents/My setup.json"));
    // An undo that would land on a new file says so and leaves both alone.
    let done = go(&fs, &p, Op::Rename { path: from.into(), name: "b.txt".into() }, Choice::Skip).unwrap();
    t.write(from, b"new");
    assert!(undo(&fs, &p, &done.undo.unwrap(), NOW).is_err());
    assert_eq!(t.read(from), "new");
}

#[test]
fn copy_with_progress_conflicts_and_undo() {
    let (t, fs, p) = machine("copy");
    // A folder with a file, a deeper folder, a link and a read-only folder.
    t.write("/home/deck/Documents/Pack/readme.txt", b"hello");
    t.write("/home/deck/Documents/Pack/data/big.bin", &vec![7u8; 3_000_000]);
    t.link("readme.txt", "/home/deck/Documents/Pack/link");
    t.link("/etc/os-release", "/home/deck/Documents/Pack/system link");
    t.write("/home/deck/Documents/Pack/locked/x", b"x");
    std::fs::set_permissions(t.at("/home/deck/Documents/Pack/locked"), std::fs::Permissions::from_mode(0o555)).unwrap();
    std::fs::set_permissions(t.at("/home/deck/Documents/Pack/readme.txt"), std::fs::Permissions::from_mode(0o640)).unwrap();
    fs.set_modified(Path::new("/home/deck/Documents/Pack/readme.txt"), 1_700_000_000).unwrap();
    let op = Op::Copy { items: paths(&["/home/deck/Documents/Pack", "/home/deck/Documents/My setup.json"]), into: "/home/deck/Downloads".into() };
    let planned = plan(&fs, &p, op.clone()).unwrap();
    let setup = std::fs::metadata(t.at("/home/deck/Documents/My setup.json")).unwrap().len();
    assert_eq!((planned.files, planned.bytes, planned.conflicts.len(), planned.same_drive), (9, 3_000_006 + setup, 0, true));
    assert_eq!(planned.lines, ["Copies 2 items (2.9 MB) to Downloads."]);
    let mut seen = Vec::new();
    let done = run(&fs, &p, &planned, &Answers::default(), NOW, &mut |pr: &Progress| {
        seen.push(pr.clone());
        true
    })
    .unwrap();
    assert_eq!(done.words, "2 items copied to Downloads.");
    let last = seen.last().unwrap();
    assert_eq!((last.files_done, last.files, last.bytes_done, last.bytes), (9, 9, planned.bytes, planned.bytes));
    assert!(seen.len() > 8 && seen.windows(2).all(|w| w[0].bytes_done <= w[1].bytes_done) && seen[0].name == "Pack");
    // Everything arrived as it was: bytes, the link as a link, permissions and dates. The original is untouched.
    let copy = t.at("/home/deck/Downloads/Pack");
    assert_eq!(std::fs::read(copy.join("data/big.bin")).unwrap().len(), 3_000_000);
    assert_eq!(std::fs::read_link(copy.join("link")).unwrap(), PathBuf::from("readme.txt"));
    assert_eq!(std::fs::read_link(copy.join("system link")).unwrap(), PathBuf::from("/etc/os-release"));
    assert_eq!(std::fs::metadata(copy.join("locked")).unwrap().permissions().mode() & 0o777, 0o555);
    assert_eq!(std::fs::metadata(copy.join("readme.txt")).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(fs.meta(Path::new("/home/deck/Downloads/Pack/readme.txt")).unwrap().modified, 1_700_000_000);
    assert!(t.has("/home/deck/Documents/Pack/readme.txt") && t.has("/home/deck/Downloads/My setup.json"));

    // The same again: both names are taken now. Skip leaves everything, keep both makes "(2)", replace sends the
    // old ones to the Recycle Bin first.
    let planned = plan(&fs, &p, op.clone()).unwrap();
    assert_eq!(planned.conflicts, paths(&["/home/deck/Documents/Pack", "/home/deck/Documents/My setup.json"]));
    assert!(planned.lines[1].contains("2 names are there already"));
    let skipped = run(&fs, &p, &planned, &Answers::all(Choice::Skip), NOW, &mut quiet).unwrap();
    assert_eq!((skipped.made.len(), skipped.skipped.len(), skipped.undo, skipped.words.as_str()), (0, 2, None, "0 items copied to Downloads, 2 left out."));
    let answers = Answers { all: Choice::KeepBoth, each: vec![("/home/deck/Documents/My setup.json".into(), Choice::Replace)] };
    t.write("/home/deck/Downloads/My setup.json", b"an older one");
    let mixed = run(&fs, &p, &planned, &answers, NOW, &mut quiet).unwrap();
    assert_eq!(mixed.made, paths(&["/home/deck/Downloads/Pack (2)", "/home/deck/Downloads/My setup.json"]));
    assert!(t.read("/home/deck/Downloads/My setup.json").contains("guided"));
    assert_eq!(t.read("/home/deck/.local/share/Trash/files/My setup.json"), "an older one");
    // Undo: the copies go to the Recycle Bin (nothing is erased) and what was replaced comes back.
    let back = undo(&fs, &p, &mixed.undo.unwrap(), NOW).unwrap();
    assert_eq!(back.words, "2 copies moved to the Recycle Bin.");
    assert!(!t.has("/home/deck/Downloads/Pack (2)") && t.has("/home/deck/Downloads/Pack"));
    assert_eq!(t.read("/home/deck/Downloads/My setup.json"), "an older one");
    assert!(t.has("/home/deck/.local/share/Trash/files/Pack (2)/data/big.bin"));

    // A copy next to itself is a second one, whatever the answer.
    let twin = go(&fs, &p, Op::Copy { items: paths(&["/home/deck/Documents/My setup.json"]), into: DOCS.into() }, Choice::Replace).unwrap();
    assert_eq!(twin.made, paths(&["/home/deck/Documents/My setup (2).json"]));
    let again = go(&fs, &p, Op::Copy { items: paths(&["/home/deck/Documents/My setup.json"]), into: DOCS.into() }, Choice::Skip).unwrap();
    assert_eq!(again.made, paths(&["/home/deck/Documents/My setup (3).json"]));
    // A folder never goes into itself, by any name.
    t.link("Pack/data", "/home/deck/Documents/shortcut into pack");
    for into in ["/home/deck/Documents/Pack", "/home/deck/Documents/Pack/data", "/home/deck/Documents/shortcut into pack"] {
        assert_eq!(plan(&fs, &p, Op::Copy { items: paths(&["/home/deck/Documents/Pack"]), into: into.into() }).unwrap_err(), "A folder can't be put inside itself (Pack).", "{into}");
    }
    assert_eq!(plan(&fs, &p, Op::Copy { items: Vec::new(), into: DOCS.into() }).unwrap_err(), "Pick something first.");
    assert!(plan(&fs, &p, Op::Copy { items: paths(&["/home/deck/Documents/gone"]), into: DOCS.into() }).unwrap_err().contains("isn't there any more"));
    // Copying out of a locked place is fine; into one never is.
    assert!(go(&fs, &p, Op::Copy { items: paths(&["/etc/os-release"]), into: DOCS.into() }, Choice::Skip).is_ok());
    assert_eq!(plan(&fs, &p, Op::Copy { items: paths(&["/home/deck/Documents/My setup.json"]), into: "/etc".into() }).unwrap_err(), SYSTEM_WHY);
}

#[test]
fn a_copy_can_be_stopped_and_checks_the_room_first() {
    let (t, mut fs, p) = machine("stop");
    t.write("/home/deck/Documents/Pack/a.bin", &vec![1u8; 2_500_000]);
    t.write("/home/deck/Documents/Pack/b.bin", &vec![2u8; 2_500_000]);
    let op = Op::Copy { items: paths(&["/home/deck/Documents/Pack", "/home/deck/Documents/My setup.json"]), into: "/home/deck/Downloads".into() };
    let planned = plan(&fs, &p, op.clone()).unwrap();
    // Stopped part-way: what was half copied is taken away again, the originals are whole.
    let done = run(&fs, &p, &planned, &Answers::default(), NOW, &mut |pr: &Progress| pr.bytes_done < 3_000_000).unwrap();
    assert_eq!((done.words.as_str(), done.made.len(), done.undo), ("Stopped: 0 items copied.", 0, None));
    assert!(!t.has("/home/deck/Downloads/Pack") && !t.has("/home/deck/Downloads/My setup.json"));
    assert_eq!(std::fs::read(t.at("/home/deck/Documents/Pack/b.bin")).unwrap().len(), 2_500_000);
    // Not enough room: refused before a byte is written, in words with both numbers.
    fs.room = Some(Space { free: 1_000_000, total: 512_000_000_000 });
    assert_eq!(plan(&fs, &p, op).unwrap_err(), "Not enough room: this needs 4.8 MB and SteamOS (C:) has 977 KB free.");
    assert!(!t.has("/home/deck/Downloads/Pack"));
    // A move within the drive needs none.
    assert!(plan(&fs, &p, Op::Move { items: paths(&["/home/deck/Documents/Pack"]), into: "/home/deck/Downloads".into() }).is_ok());
    // The copy itself never writes over a file that turned up meanwhile.
    t.write("/home/deck/Downloads/a.bin", b"mine");
    assert!(copy_tree(&fs, Path::new("/home/deck/Documents/Pack/a.bin"), Path::new("/home/deck/Downloads/a.bin"), &mut |_: Step| true).is_err());
    assert_eq!(t.read("/home/deck/Downloads/a.bin"), "mine");
}

#[test]
fn move_within_a_drive_and_across_drives() {
    let (t, disk, _) = machine("move");
    let fs = TwoDrives { disk, drive: CARD.into() };
    let p = places(&t, &fs);
    t.write("/home/deck/Documents/Pack/readme.txt", b"hello");
    t.write("/home/deck/Documents/Pack/deep/er/x.bin", &[9u8; 5000]);
    t.link("readme.txt", "/home/deck/Documents/Pack/link");
    // Within the drive: a rename, done at once.
    let op = Op::Move { items: paths(&["/home/deck/Documents/Pack"]), into: "/home/deck/Downloads".into() };
    let planned = plan(&fs, &p, op).unwrap();
    assert!(planned.same_drive && planned.lines == ["Moves 1 item (5 KB) to Downloads."]);
    let done = run(&fs, &p, &planned, &Answers::default(), NOW, &mut quiet).unwrap();
    assert!(!t.has("/home/deck/Documents/Pack") && t.has("/home/deck/Downloads/Pack/deep/er/x.bin"));
    assert_eq!(done.words, "1 item moved to Downloads.");
    assert_eq!(undo(&fs, &p, &done.undo.unwrap(), NOW).unwrap().words, "1 item moved back.");
    assert!(t.has("/home/deck/Documents/Pack/link") && !t.has("/home/deck/Downloads/Pack"));

    // To the card: copied, then the original removed. Links arrive as links.
    let op = Op::Move { items: paths(&["/home/deck/Documents/Pack", "/home/deck/Documents/My setup.json"]), into: format!("{CARD}/ROMs").into() };
    let planned = plan(&fs, &p, op).unwrap();
    assert!(!planned.same_drive);
    let mut ticks = 0;
    let done = run(&fs, &p, &planned, &Answers::default(), NOW, &mut |_: &Progress| {
        ticks += 1;
        true
    })
    .unwrap();
    assert!(ticks >= 5 && !t.has("/home/deck/Documents/Pack") && !t.has("/home/deck/Documents/My setup.json"));
    assert_eq!(std::fs::read(t.at(&format!("{CARD}/ROMs/Pack/deep/er/x.bin"))).unwrap(), [9u8; 5000]);
    assert!(t.at(&format!("{CARD}/ROMs/Pack/link")).is_symlink());
    // And back again with undo, across the drives the other way.
    let back = undo(&fs, &p, &done.undo.unwrap(), NOW).unwrap();
    assert_eq!((back.made.len(), back.skipped.len()), (2, 0));
    assert!(t.has("/home/deck/Documents/Pack/deep/er/x.bin") && !t.has(&format!("{CARD}/ROMs/Pack")) && t.has("/home/deck/Documents/My setup.json"));

    // Refused: into itself, to where it already is, a name taken (unless answered).
    assert_eq!(plan(&fs, &p, Op::Move { items: paths(&["/home/deck/Documents/Pack"]), into: "/home/deck/Documents/Pack/deep".into() }).unwrap_err(), "A folder can't be put inside itself (Pack).");
    assert_eq!(plan(&fs, &p, Op::Move { items: paths(&["/home/deck/Documents/Pack"]), into: DOCS.into() }).unwrap_err(), "Pack is in this folder already.");
    t.write("/home/deck/Downloads/Pack/other.txt", b"the one already there");
    let op = Op::Move { items: paths(&["/home/deck/Documents/Pack"]), into: "/home/deck/Downloads".into() };
    assert_eq!(go(&fs, &p, op.clone(), Choice::Skip).unwrap().skipped.len(), 1);
    assert!(t.has("/home/deck/Documents/Pack"));
    let replaced = go(&fs, &p, op, Choice::Replace).unwrap();
    assert!(t.has("/home/deck/Downloads/Pack/readme.txt") && !t.has("/home/deck/Downloads/Pack/other.txt"));
    assert_eq!(t.read("/home/deck/.local/share/Trash/files/Pack/other.txt"), "the one already there");
    undo(&fs, &p, &replaced.undo.unwrap(), NOW).unwrap();
    assert!(t.has("/home/deck/Documents/Pack/readme.txt") && t.has("/home/deck/Downloads/Pack/other.txt") && !t.has("/home/deck/Downloads/Pack/readme.txt"));
    // A folder never replaces the folder it is in.
    t.write("/home/deck/Documents/Pack/Pack/inner.txt", b"");
    let inner = go(&fs, &p, Op::Move { items: paths(&["/home/deck/Documents/Pack/Pack"]), into: DOCS.into() }, Choice::Replace).unwrap();
    assert_eq!(inner.skipped[0].1, "It is inside the folder it would replace.");
    assert!(t.has("/home/deck/Documents/Pack/Pack/inner.txt"));
    // A folder on the card the player may not write in takes nothing.
    std::fs::set_permissions(t.at(&format!("{CARD}/Movies")), std::fs::Permissions::from_mode(0o555)).unwrap();
    if p.who.uid != 0 {
        assert_eq!(plan(&fs, &p, Op::Move { items: paths(&["/home/deck/Documents/Pack"]), into: format!("{CARD}/Movies").into() }).unwrap_err(), NOT_ALLOWED);
    }
}

#[test]
fn delete_goes_to_the_recycle_bin_as_the_specification_says() {
    let (t, fs, p) = machine("trash");
    let bin = "/home/deck/.local/share/Trash";
    let file = "/home/deck/Documents/Ünï code & 100%.txt";
    t.write(file, b"keep me");
    let planned = plan(&fs, &p, Op::Delete { items: paths(&[file]) }).unwrap();
    assert_eq!((planned.title.as_str(), planned.dangerous), ("Delete 1 item", false));
    assert_eq!(planned.lines, ["Ünï code & 100%.txt goes to the Recycle Bin. You can get it back from there."]);
    let done = run(&fs, &p, &planned, &Answers::default(), NOW, &mut quiet).unwrap();
    assert_eq!(done.words, "Ünï code & 100%.txt moved to the Recycle Bin.");
    assert!(!t.has(file));
    assert_eq!(t.read(&format!("{bin}/files/Ünï code & 100%.txt")), "keep me");
    // The note: the exact three lines, the path escaped byte by byte, the time on the player's clock.
    assert_eq!(t.read(&format!("{bin}/info/Ünï code & 100%.txt.trashinfo")), "[Trash Info]\nPath=/home/deck/Documents/%C3%9Cn%C3%AF%20code%20%26%20100%25.txt\nDeletionDate=2026-11-12T10:31:05\n");
    // A second file of the same name gets a name of its own in the bin; both go back to where they were.
    t.write(file, b"second");
    let second = go(&fs, &p, Op::Delete { items: paths(&[file]) }, Choice::Skip).unwrap();
    assert_eq!(t.read(&format!("{bin}/files/Ünï code & 100% (2).txt")), "second");
    assert!(t.read(&format!("{bin}/info/Ünï code & 100% (2).txt.trashinfo")).contains("Path=/home/deck/Documents/%C3%9Cn%C3%AF%20code%20%26%20100%25.txt\n"));
    let listed = trash::items(&fs, &p);
    assert_eq!(listed.iter().filter(|i| i.original == Path::new(file)).count(), 2);
    assert_eq!((listed[0].deleted.as_str(), listed[0].original.as_path()), ("2026-11-12T10:31:05", Path::new(file)));
    // Undo puts the second back; the first then finds its place taken and can come back beside it.
    let back = undo(&fs, &p, &second.undo.unwrap(), NOW).unwrap();
    assert_eq!((back.words.as_str(), t.read(file).as_str()), ("1 item put back.", "second"));
    assert!(!t.has(&format!("{bin}/info/Ünï code & 100% (2).txt.trashinfo")));
    let first = Op::Restore { items: paths(&[&format!("{bin}/files/Ünï code & 100%.txt")]) };
    assert_eq!(plan(&fs, &p, first.clone()).unwrap().conflicts.len(), 1);
    assert!(go(&fs, &p, first.clone(), Choice::Skip).unwrap_err().contains("There is already something called"));
    assert_eq!(go(&fs, &p, first, Choice::KeepBoth).unwrap().made, paths(&["/home/deck/Documents/Ünï code & 100% (2).txt"]));
    // Undoing a restore deletes again, and that can be undone too.
    let again = undo(&fs, &p, &back.undo.unwrap(), NOW).unwrap();
    assert!(!t.has(file) && matches!(again.undo, Some(Undo::Deleted { .. })));

    // A folder: moved whole, and its size is written down for other file managers.
    t.write("/home/deck/Documents/Old mods/a/b.bin", &[1u8; 700]);
    t.write("/home/deck/Documents/Old mods/c.txt", b"123");
    let gone = go(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/Old mods", "/home/deck/Documents/My setup.json"]) }, Choice::Skip).unwrap();
    assert_eq!(gone.words, "2 items moved to the Recycle Bin.");
    assert!(t.has(&format!("{bin}/files/Old mods/a/b.bin")));
    let sizes = t.read(&format!("{bin}/directorysizes"));
    let line: Vec<&str> = sizes.trim().split(' ').collect();
    assert_eq!((line.len(), line[0], line[2]), (3, "703", "Old%20mods"));
    assert!(line[1].parse::<u64>().unwrap() > 1_700_000_000);
    // Restore makes the folders it was in again when they are gone.
    t.write("/home/deck/Documents/Deep/er/note.txt", b"n");
    go(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/Deep/er/note.txt"]) }, Choice::Skip).unwrap();
    std::fs::remove_dir_all(t.at("/home/deck/Documents/Deep")).unwrap();
    go(&fs, &p, Op::Restore { items: paths(&[&format!("{bin}/files/note.txt")]) }, Choice::Skip).unwrap();
    assert_eq!(t.read("/home/deck/Documents/Deep/er/note.txt"), "n");
    undo(&fs, &p, &gone.undo.unwrap(), NOW).unwrap();
    assert!(t.has("/home/deck/Documents/Old mods/a/b.bin") && !t.has(&format!("{bin}/directorysizes")));

    // A link is deleted as a link: what it points to stays.
    t.link("/home/deck/Pictures", "/home/deck/Documents/pics");
    go(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/pics"]) }, Choice::Skip).unwrap();
    assert!(t.at(&format!("{bin}/files/pics")).is_symlink() && t.has("/home/deck/Pictures/Screenshots/Night City.png"));
    // What is in the bin can't be deleted, renamed or moved from there, only restored or emptied.
    let inside = format!("{bin}/files/pics");
    assert_eq!(plan(&fs, &p, Op::Delete { items: paths(&[&inside]) }).unwrap_err(), BIN_WHY);
    assert_eq!(plan(&fs, &p, Op::Rename { path: inside.clone().into(), name: "x".into() }).unwrap_err(), BIN_WHY);
    assert_eq!(plan(&fs, &p, Op::Move { items: paths(&[&inside]), into: DOCS.into() }).unwrap_err(), BIN_WHY);
    assert!(plan(&fs, &p, Op::Restore { items: paths(&["/home/deck/Documents/My setup.json"]) }).unwrap_err().contains("isn't in the Recycle Bin"));
    assert_eq!(plan(&fs, &p, Op::Delete { items: Vec::new() }).unwrap_err(), "Pick something first.");
    // Escaping goes both ways, for any bytes.
    assert_eq!(trash::escape(Path::new("/a b/ü~-_.%")), "/a%20b/%C3%BC~-_.%25");
    assert_eq!(trash::unescape("/a%20b/%C3%BC%zz%2"), "/a b/ü%zz%2".as_bytes());
}

#[test]
fn another_drive_has_its_own_bin() {
    let (t, disk, _) = machine("drivebin");
    let fs = TwoDrives { disk, drive: CARD.into() };
    let p = places(&t, &fs);
    let uid = p.who.uid;
    let own = format!("{CARD}/.Trash-{uid}");
    // Deleted on the card: stays on the card, with its path written from the top of the card.
    let done = go(&fs, &p, Op::Delete { items: paths(&[&format!("{CARD}/Movies/Holiday 2025.mp4")]) }, Choice::Skip).unwrap();
    assert!(t.has(&format!("{own}/files/Holiday 2025.mp4")) && !t.has("/home/deck/.local/share/Trash/files/Holiday 2025.mp4"));
    assert_eq!(t.read(&format!("{own}/info/Holiday 2025.mp4.trashinfo")), "[Trash Info]\nPath=Movies/Holiday%202025.mp4\nDeletionDate=2026-11-12T10:31:05\n");
    let item = trash::items(&fs, &p).into_iter().find(|i| i.name == "Holiday 2025.mp4").unwrap();
    assert_eq!((item.original.as_path(), item.trash.as_path()), (Path::new("/run/media/deck/sdcard/Movies/Holiday 2025.mp4"), Path::new(&own)));
    undo(&fs, &p, &done.undo.unwrap(), NOW).unwrap();
    assert!(t.has(&format!("{CARD}/Movies/Holiday 2025.mp4")));
    // A file from the home drive still goes to the home bin.
    go(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/My setup.json"]) }, Choice::Skip).unwrap();
    assert!(t.has("/home/deck/.local/share/Trash/files/My setup.json"));

    // A shared ".Trash" set up by the drive's owner (a real folder with the sticky bit) is used, per user.
    t.dir(&format!("{CARD}/.Trash"));
    std::fs::set_permissions(t.at(&format!("{CARD}/.Trash")), std::fs::Permissions::from_mode(0o1777)).unwrap();
    go(&fs, &p, Op::Delete { items: paths(&[&format!("{CARD}/ROMs/readme.txt")]) }, Choice::Skip).unwrap();
    assert_eq!(t.read(&format!("{CARD}/.Trash/{uid}/info/readme.txt.trashinfo")).lines().nth(1), Some("Path=ROMs/readme.txt"));
    assert!(trash::items(&fs, &p).iter().any(|i| i.original == Path::new("/run/media/deck/sdcard/ROMs/readme.txt")));
    // A note in a drive's bin that points out of the drive is not believed: nothing is "restored" to a place it
    // never was.
    t.write(&format!("{CARD}/.Trash/{uid}/files/evil"), b"x");
    t.write(&format!("{CARD}/.Trash/{uid}/info/evil.trashinfo"), b"[Trash Info]\nPath=../../../../home/deck/.bashrc\nDeletionDate=2026-11-12T10:00:00\n");
    t.write(&format!("{CARD}/.Trash/{uid}/files/evil2"), b"x");
    t.write(&format!("{CARD}/.Trash/{uid}/info/evil2.trashinfo"), b"[Trash Info]\nPath=/home/deck/.profile\nDeletionDate=2026-11-12T10:00:00\n");
    assert!(!trash::items(&fs, &p).iter().any(|i| i.name.starts_with("evil")));
    assert!(plan(&fs, &p, Op::Restore { items: paths(&[&format!("{CARD}/.Trash/{uid}/files/evil")]) }).is_err());
    // One without the sticky bit is not trusted: the per-user folder is used.
    std::fs::set_permissions(t.at(&format!("{CARD}/.Trash")), std::fs::Permissions::from_mode(0o777)).unwrap();
    t.write(&format!("{CARD}/ROMs/a.rom"), b"a");
    go(&fs, &p, Op::Delete { items: paths(&[&format!("{CARD}/ROMs/a.rom")]) }, Choice::Skip).unwrap();
    assert!(t.has(&format!("{own}/files/a.rom")));

    // A bin that is a link to somewhere else is never used, to put in or to empty.
    std::fs::remove_dir_all(t.at(&own)).unwrap();
    t.link("../../../../home/deck/Documents", &own);
    t.write(&format!("{CARD}/ROMs/b.rom"), b"b");
    assert!(go(&fs, &p, Op::Delete { items: paths(&[&format!("{CARD}/ROMs/b.rom")]) }, Choice::Skip).unwrap_err().contains("isn't a plain folder"));
    assert!(t.has(&format!("{CARD}/ROMs/b.rom")) && !t.has("/home/deck/Documents/files"));
    // The top of a drive nobody listed is still the top of a drive.
    t.write("/mnt/usb/stuff/a", b"");
    let usb = TwoDrives { disk: fs.disk.clone(), drive: "/mnt/usb".into() };
    assert_eq!(plan(&usb, &p, Op::Delete { items: paths(&["/mnt/usb"]) }).unwrap_err(), DRIVE_WHY);
    assert_eq!(plan(&usb, &p, Op::Delete { items: paths(&["/mnt/usb/stuff"]) }).map(|p| p.title), Ok("Delete 1 item".to_string()));
}

#[test]
fn emptying_the_bin_is_the_only_thing_that_erases() {
    let (t, fs, p) = machine("empty");
    let bin = "/home/deck/.local/share/Trash";
    t.write("/home/deck/Documents/Folder/x", b"x");
    t.link("/home/deck/Pictures", "/home/deck/Documents/pics");
    go(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/Folder", "/home/deck/Documents/pics"]) }, Choice::Skip).unwrap();
    assert_eq!(trash::count(&fs, &p), 5);
    let planned = plan(&fs, &p, Op::EmptyBin).unwrap();
    assert!(planned.dangerous);
    assert_eq!((planned.title.as_str(), planned.lines.as_slice()), ("Empty the Recycle Bin", &["Erases 5 items for good. This can't be undone.".to_string()][..]));
    let done = run(&fs, &p, &planned, &Answers::default(), NOW, &mut quiet).unwrap();
    assert_eq!((done.words.as_str(), done.undo), ("Recycle Bin emptied: 5 items erased.", None));
    // Both bins are empty, the folders themselves stay, and the link's target was never touched.
    for dir in [format!("{bin}/files"), format!("{bin}/info"), format!("{CARD}/.Trash-{}/files", p.who.uid), format!("{CARD}/.Trash-{}/info", p.who.uid)] {
        assert_eq!(std::fs::read_dir(t.at(&dir)).unwrap().count(), 0, "{dir}");
    }
    assert!(!t.has(&format!("{bin}/directorysizes")) && t.has("/home/deck/Pictures/Screenshots/Night City.png"));
    assert_eq!(plan(&fs, &p, Op::EmptyBin).unwrap_err(), "The Recycle Bin is empty already.");
    assert!(list(&fs, &p, &Place::Trash, View::Windows, &Options::default()).entries.is_empty());

    // A bin whose "files" is a link somewhere else is left alone entirely.
    std::fs::remove_dir(t.at(&format!("{bin}/files"))).unwrap();
    t.link("../../../Documents", &format!("{bin}/files"));
    assert_eq!(trash::empty(&fs, &p), (0, Vec::new()));
    assert!(t.has("/home/deck/Documents/My setup.json") && trash::items(&fs, &p).is_empty());
    assert!(go(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/My setup.json"]) }, Choice::Skip).unwrap_err().contains("isn't a plain folder"));
}

#[test]
fn what_holds_the_machine_together_is_never_touched() {
    let (t, fs, mut p) = machine("refuse");
    t.write("/usr/bin/ls", b"\x7fELF");
    t.write("/home/deck/.var/app/io.github.ravenor907.TurbineHub/config/turbinehub/settings.json", b"{}");
    t.write("/home/deck/.local/share/Steam/userdata/1001/760/remote/shot.jpg", b"jpg");
    t.link("../.local/share/Steam", "/home/deck/.steam/steam");
    let steam = "/home/deck/.local/share/Steam";
    let refused: &[(&str, &str)] = &[
        ("/", DRIVE_WHY),
        ("/home", DRIVE_WHY),
        ("/run/media", DRIVE_WHY),
        ("/run/media/deck", DRIVE_WHY),
        (CARD, DRIVE_WHY),
        ("/home/deck", HOME_WHY),
        ("/home/deck/.config", SETTINGS_WHY),
        ("/home/deck/.local/share", SETTINGS_WHY),
        ("/home/deck/.var/app", SETTINGS_WHY),
        (steam, STEAM_WHY),
        ("/home/deck/.local/share/Steam/steamapps", STEAM_WHY),
        ("/home/deck/.local/share/Steam/steamapps/common", STEAM_WHY),
        ("/home/deck/.local/share/Steam/steamapps/compatdata", STEAM_WHY),
        ("/home/deck/.local/share/Steam/userdata", STEAM_WHY),
        ("/home/deck/.local/share/Steam/userdata/1001", STEAM_WHY),
        ("/run/media/deck/sdcard/steamapps", STEAM_WHY),
        ("/run/media/deck/sdcard/steamapps/common", STEAM_WHY),
        ("/home/deck/.steam", STEAM_WHY),
        ("/home/deck/.steam/steam", STEAM_WHY),
        // Reached through Steam's own link: still Steam's folder.
        ("/home/deck/.steam/steam/steamapps", STEAM_WHY),
        ("/usr", SYSTEM_WHY),
        ("/usr/bin/ls", SYSTEM_WHY),
        ("/etc/os-release", SYSTEM_WHY),
        ("/home/deck/.var/app/io.github.ravenor907.TurbineHub", OWN_WHY),
        ("/home/deck/.var/app/io.github.ravenor907.TurbineHub/config/turbinehub/settings.json", OWN_WHY),
        ("/home/deck/.local/share/Trash", BIN_WHY),
        ("/home/deck/.local/share/Trash/files", BIN_WHY),
    ];
    for (path, why) in refused {
        assert_eq!(p.may_change(&fs, Path::new(path)), Err(why.to_string()), "{path}");
        for op in [Op::Delete { items: paths(&[path]) }, Op::Rename { path: path.into(), name: "renamed".into() }, Op::Move { items: paths(&[path]), into: "/home/deck/Documents".into() }] {
            assert_eq!(plan(&fs, &p, op.clone()).unwrap_err(), *why, "{op:?}");
        }
        assert!(t.has(path));
    }
    // Nothing goes into the locked places either.
    for into in ["/usr/bin", "/etc", "/home/deck/.var/app/io.github.ravenor907.TurbineHub/config", "/home/deck/.local/share/Trash/files"] {
        for op in [Op::Copy { items: paths(&["/home/deck/Documents/My setup.json"]), into: into.into() }, Op::Move { items: paths(&["/home/deck/Documents/My setup.json"]), into: into.into() }, Op::NewFolder { at: into.into(), name: "x".into() }, Op::NewText { at: into.into(), name: "x".into() }] {
            assert!([SYSTEM_WHY, OWN_WHY, BIN_WHY].contains(&plan(&fs, &p, op.clone()).unwrap_err().as_str()), "{op:?}");
        }
    }
    // What lives inside those folders is the player's to manage: a game's folder, its pretend Windows, a screenshot.
    for path in ["/home/deck/.local/share/Steam/steamapps/common/Baldurs Gate 3", "/home/deck/.local/share/Steam/steamapps/compatdata/1086940", "/home/deck/.local/share/Steam/userdata/1001/760/remote/shot.jpg", "/home/deck/.config/unity3d", "/home/deck/Documents", "/run/media/deck/sdcard/ROMs"] {
        assert_eq!(p.may_change(&fs, Path::new(path)), Ok(()), "{path}");
    }

    // Links that lead into a locked place open nothing.
    t.link("/usr", "/home/deck/Documents/system");
    t.link("../.local/share/Steam", "/home/deck/Documents/steam");
    t.link("/home/deck", "/home/deck/Documents/me");
    assert_eq!(plan(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/system/bin/ls"]) }).unwrap_err(), SYSTEM_WHY);
    assert_eq!(plan(&fs, &p, Op::Rename { path: "/home/deck/Documents/system/bin".into(), name: "x".into() }).unwrap_err(), SYSTEM_WHY);
    assert_eq!(plan(&fs, &p, Op::Copy { items: paths(&["/home/deck/Documents/My setup.json"]), into: "/home/deck/Documents/system/bin".into() }).unwrap_err(), SYSTEM_WHY);
    assert_eq!(plan(&fs, &p, Op::NewFolder { at: "/home/deck/Documents/system".into(), name: "x".into() }).unwrap_err(), SYSTEM_WHY);
    assert_eq!(plan(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/steam/steamapps"]) }).unwrap_err(), STEAM_WHY);
    assert_eq!(plan(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/me/.config"]) }).unwrap_err(), SETTINGS_WHY);
    assert_eq!(plan(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/../../deck"]) }).unwrap_err(), HOME_WHY);
    assert_eq!(plan(&fs, &p, Op::Move { items: paths(&["/home/deck/Documents/me/Documents"]), into: "/home/deck/Documents/me/Documents/system".into() }).unwrap_err(), SYSTEM_WHY);
    // The links themselves are the player's: deleting one removes the link and nothing behind it.
    go(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/system", "/home/deck/Documents/steam", "/home/deck/Documents/me"]) }, Choice::Skip).unwrap();
    assert!(t.has("/usr/bin/ls") && t.has("/home/deck/.local/share/Steam/steamapps") && t.has("/home/deck/Documents/My setup.json"));
    assert!(!t.has("/home/deck/Documents/system") && t.at("/home/deck/.local/share/Trash/files/system").is_symlink());

    // A folder the player may not write in.
    t.write("/home/deck/Documents/Readonly/a.txt", b"a");
    std::fs::set_permissions(t.at("/home/deck/Documents/Readonly"), std::fs::Permissions::from_mode(0o555)).unwrap();
    if p.who.uid != 0 {
        assert_eq!(plan(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/Readonly/a.txt"]) }).unwrap_err(), NOT_ALLOWED);
        assert_eq!(plan(&fs, &p, Op::Rename { path: "/home/deck/Documents/Readonly/a.txt".into(), name: "b".into() }).unwrap_err(), NOT_ALLOWED);
        assert_eq!(plan(&fs, &p, Op::NewFolder { at: "/home/deck/Documents/Readonly".into(), name: "x".into() }).unwrap_err(), NOT_ALLOWED);
        assert_eq!(plan(&fs, &p, Op::Copy { items: paths(&["/home/deck/Documents/My setup.json"]), into: "/home/deck/Documents/Readonly".into() }).unwrap_err(), NOT_ALLOWED);
    }

    // A Windows drive is only ever read.
    t.write("/run/media/deck/Windows/Users/Sam/save.dat", b"s");
    p.drives.push(DriveAt { letter: 'E', label: "Windows".into(), mount: "/run/media/deck/Windows".into(), fs: "ntfs".into(), removable: false, device: "sda3".into(), read_only: true });
    assert_eq!(plan(&fs, &p, Op::Delete { items: paths(&["/run/media/deck/Windows/Users/Sam/save.dat"]) }).unwrap_err(), WINDOWS_DRIVE_WHY);
    assert_eq!(plan(&fs, &p, Op::NewFolder { at: "/run/media/deck/Windows/Users".into(), name: "x".into() }).unwrap_err(), WINDOWS_DRIVE_WHY);
    assert_eq!(plan(&fs, &p, Op::Move { items: paths(&["/home/deck/Documents/My setup.json"]), into: "/run/media/deck/Windows".into() }).unwrap_err(), WINDOWS_DRIVE_WHY);
    // Copying a save off it is what it is there for.
    assert!(go(&fs, &p, Op::Copy { items: paths(&["/run/media/deck/Windows/Users/Sam/save.dat"]), into: DOCS.into() }, Choice::Skip).is_ok());
    let l = list(&fs, &p, &Place::Folder("/run/media/deck/Windows/Users".into()), View::Windows, &Options::default());
    assert!(l.locked.as_deref() == Some(WINDOWS_DRIVE_WHY) && l.entries[0].locked && !l.entries[0].writable);
}

#[test]
fn the_last_operation_of_each_kind_can_be_undone() {
    let (t, fs, p) = machine("history");
    let mut history = History::default();
    assert!(history.newest().is_none());
    let made = go(&fs, &p, Op::NewFolder { at: DOCS.into(), name: "One".into() }, Choice::Skip).unwrap();
    history.remember(&made);
    let renamed = go(&fs, &p, Op::Rename { path: "/home/deck/Documents/My setup.json".into(), name: "setup.json".into() }, Choice::Skip).unwrap();
    history.remember(&renamed);
    history.remember(&go(&fs, &p, Op::NewFolder { at: DOCS.into(), name: "Two".into() }, Choice::Skip).unwrap());
    history.remember(&go(&fs, &p, Op::Delete { items: paths(&["/home/deck/Documents/Controller layout notes.txt"]) }, Choice::Skip).unwrap());
    // Nothing to take back: nothing is remembered.
    history.remember(&Done::default());
    assert_eq!(history.newest().unwrap().words(), "Undo deleting 1 item");
    assert_eq!(history.last(OpKind::New).unwrap().words(), "Undo making Two");
    assert_eq!(history.last(OpKind::Rename).unwrap().words(), "Undo renaming My setup.json to setup.json");
    assert!(history.last(OpKind::Copy).is_none() && history.last(OpKind::Empty).is_none());
    // Any kind can be undone, not only the newest.
    undo(&fs, &p, &history.take(OpKind::Rename).unwrap(), NOW).unwrap();
    assert!(t.has("/home/deck/Documents/My setup.json") && history.last(OpKind::Rename).is_none());
    undo(&fs, &p, &history.take_newest().unwrap(), NOW).unwrap();
    assert!(t.has("/home/deck/Documents/Controller layout notes.txt"));
    undo(&fs, &p, &history.take_newest().unwrap(), NOW).unwrap();
    assert!(!t.has("/home/deck/Documents/Two") && t.has("/home/deck/Documents/One") && history.newest().is_none());
}

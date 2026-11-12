//! Listing, sorting, grouping, searching and previews, on the made-up machine.

use super::list::{date_group, mode_text, size_group, type_by_bytes, type_by_name};
use super::testing::{NOW, Temp, machine, places};
use super::*;
use std::path::{Path, PathBuf};

fn folder(p: &str) -> Place {
    Place::Folder(p.into())
}

fn names(l: &Listing) -> Vec<&str> {
    l.entries.iter().map(|e| e.name.as_str()).collect()
}

fn entry<'a>(l: &'a Listing, name: &str) -> &'a Entry {
    l.entries.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no {name} in {:?}", names(l)))
}

fn win() -> Options {
    Options { now: NOW, ..Options::default() }
}

fn all() -> Options {
    Options { hidden: Some(true), ..win() }
}

#[test]
fn this_pc_and_home_are_the_start_pages() {
    let (_t, fs, mut p) = machine("pc");
    let l = list(&fs, &p, &Place::ThisPc, View::Windows, &win());
    assert_eq!(names(&l), ["Desktop", "Downloads", "Documents", "Pictures", "Music", "Videos", "SteamOS (C:)", "SD card (D:)", "Recycle Bin"]);
    assert_eq!(l.groups, [("Folders".to_string(), 6), ("Devices and drives".to_string(), 3)]);
    assert_eq!(l.title, "This PC");
    let c = entry(&l, "SteamOS (C:)");
    assert_eq!((c.kind, c.opens.clone(), c.space), (Kind::Drive, Some(folder("/")), Some(Space { free: 229_780_000_000, total: 512_000_000_000 })));
    let d = entry(&l, "SD card (D:)");
    assert_eq!((d.kind, d.path.as_path(), d.detail.as_str()), (Kind::Drive, Path::new("/run/media/deck/sdcard"), "/run/media/deck/sdcard"));
    // Three things are in the bins: two on the home drive, one on the card.
    let bin = entry(&l, "Recycle Bin");
    assert_eq!((bin.kind, bin.opens.clone(), bin.size), (Kind::Bin, Some(Place::Trash), Some(3)));
    let linux = list(&fs, &p, &Place::ThisPc, View::Linux, &win());
    assert_eq!(&names(&linux)[6..], ["/", "/run/media/deck/sdcard", "Trash"]);
    assert_eq!(linux.groups[1].0, "Mounted");

    // Home: the pinned folders, then the files opened lately.
    p.quick.unpin(Path::new("/home/deck/Music"));
    p.quick.pin(Path::new("/run/media/deck/sdcard/ROMs"));
    p.quick.opened(Path::new("/home/deck/Documents/My setup.json"));
    p.quick.opened(Path::new("/home/deck/Downloads/wallpaper-jade.png"));
    let home = list(&fs, &p, &Place::Home, View::Windows, &win());
    assert_eq!(names(&home), ["Desktop", "Downloads", "Documents", "Pictures", "Videos", "ROMs", "wallpaper-jade.png", "My setup.json"]);
    assert_eq!(home.groups, [("Quick access".to_string(), 6), ("Recent".to_string(), 2)]);
    assert_eq!(entry(&home, "ROMs").detail, "D:\\ROMs");
    assert_eq!((entry(&home, "My setup.json").detail.as_str(), entry(&home, "My setup.json").type_word.as_str()), ("C:\\Users\\deck\\Documents", "Settings file"));
    // The Linux view has no Home page: it is the home folder.
    assert_eq!(list(&fs, &p, &Place::Home, View::Linux, &win()).place, folder("/home/deck"));
}

#[test]
fn the_c_drive_and_the_home_folder_read_like_windows() {
    let (t, fs, p) = machine("c");
    t.write("/usr/bin/ls", b"\x7fELF");
    t.write("/usr/share/fonts/a.ttf", b"x");
    let c = list(&fs, &p, &folder("/"), View::Windows, &win());
    assert_eq!(names(&c), ["Games", "Program Files", "Users", "Windows"]);
    assert_eq!(c.entries.iter().map(|e| e.opens.clone().unwrap()).collect::<Vec<_>>(), [Place::Games, Place::Programs, folder("/home"), Place::Windows]);
    assert!(c.hidden > 0 && entry(&c, "Windows").locked && entry(&c, "Windows").icon() == "locked");
    assert_eq!((c.title.as_str(), c.shown_path.as_str()), ("SteamOS (C:)", "C:\\"));
    // The rest of the top of the disk is there for those who ask.
    let c_all = list(&fs, &p, &folder("/"), View::Windows, &all());
    assert!(names(&c_all).contains(&"run") && names(&c_all).contains(&"var") && !names(&c_all).contains(&"usr") && !names(&c_all).contains(&"home"));
    // The Linux view shows the disk as it is.
    let root = list(&fs, &p, &folder("/"), View::Linux, &win());
    assert!(["etc", "home", "proc", "run", "sys", "usr", "var"].iter().all(|n| names(&root).contains(n)), "{:?}", names(&root));
    assert!(entry(&root, "usr").locked && !entry(&root, "usr").mode.is_empty());

    let home = list(&fs, &p, &folder("/home/deck"), View::Windows, &win());
    assert_eq!(names(&home), ["AppData", "Desktop", "Documents", "Downloads", "Music", "Pictures", "Videos"]);
    assert_eq!(entry(&home, "AppData").opens, Some(Place::AppData));
    assert_eq!(home.hidden, 3);
    let home_linux = list(&fs, &p, &folder("/home/deck"), View::Linux, &win());
    assert_eq!(names(&home_linux), [".config", ".local", ".var", "Desktop", "Documents", "Downloads", "Music", "Pictures", "Videos"]);
    let config = entry(&home_linux, ".config");
    assert!(config.hidden && config.writable && config.mode.starts_with("drwx") && !config.owner.is_empty());
    assert_eq!(list(&fs, &p, &folder("/home/deck"), View::Linux, &Options { hidden: Some(false), ..win() }).entries.len(), 6);

    let appdata = list(&fs, &p, &Place::AppData, View::Windows, &win());
    assert_eq!(names(&appdata), ["Apps from the Store", "Local", "Roaming"]);
    assert_eq!((entry(&appdata, "Roaming").path.as_path(), entry(&appdata, "Roaming").detail.as_str()), (Path::new("/home/deck/.config"), "/home/deck/.config"));
    assert!(appdata.note.unwrap().contains("AppData"));
    assert_eq!(names(&list(&fs, &p, &Place::AppData, View::Linux, &win())), [".config", ".local/share", ".var/app"]);
    let store = list(&fs, &p, &folder("/home/deck/.var/app"), View::Windows, &win());
    assert_eq!(names(&store), ["Discord", "Firefox", "Heroic Games Launcher", "ProtonPlus"]);
    assert_eq!(entry(&store, "Discord").real_name, "com.discordapp.Discord");

    // C:\Windows: /usr and /etc, locked, with the reason.
    let windows = list(&fs, &p, &folder("/usr"), View::Windows, &win());
    assert_eq!((windows.place.clone(), names(&windows)), (Place::Windows, vec!["bin", "etc", "share"]));
    assert!(windows.locked.as_deref().unwrap().contains("part of SteamOS") && windows.entries.iter().all(|e| e.locked && !e.writable));
    assert_eq!(entry(&windows, "etc").path, PathBuf::from("/etc"));
    assert_eq!(windows.real, [PathBuf::from("/usr"), PathBuf::from("/etc")]);
    let inside = list(&fs, &p, &folder("/usr/bin"), View::Windows, &win());
    assert!(inside.locked.is_some() && entry(&inside, "ls").icon() == "locked");
    assert_eq!(names(&list(&fs, &p, &Place::Windows, View::Linux, &win())), ["etc", "usr"]);
}

#[test]
fn games_apps_and_the_bin_are_listed_by_name() {
    let (t, fs, p) = machine("games");
    t.write("/home/deck/.local/share/Steam/steamapps/common/Proton 11.0/proton", b"");
    let games = list(&fs, &p, &Place::Games, View::Windows, &win());
    assert_eq!(names(&games), ["Baldur's Gate 3", "Cyberpunk 2077", "Valheim"]);
    let bg3 = entry(&games, "Baldur's Gate 3");
    assert_eq!((bg3.kind, bg3.real_name.as_str(), bg3.opens.clone()), (Kind::Game, "Baldurs Gate 3", Some(Place::Game("steam:1086940".into()))));
    assert_eq!(games.hidden, 1);
    assert_eq!(entry(&list(&fs, &p, &Place::Games, View::Windows, &all()), "Proton 11.0").type_word, "Steam tool");
    // The real folder behind it is the same place in the Windows view.
    assert_eq!(list(&fs, &p, &folder("/run/media/deck/sdcard/steamapps/common"), View::Windows, &win()).place, Place::Games);
    let linux = list(&fs, &p, &Place::Games, View::Linux, &win());
    assert_eq!(names(&linux), ["Baldurs Gate 3", "Cyberpunk 2077", "Proton 11.0", "Valheim"]);
    assert_eq!(entry(&linux, "Valheim").opens, Some(folder("/run/media/deck/sdcard/steamapps/common/Valheim")));

    let game = list(&fs, &p, &Place::Game("steam:1086940".into()), View::Windows, &win());
    assert_eq!(names(&game), ["Game files", "Its Windows C: drive"]);
    assert_eq!(entry(&game, "Its Windows C: drive").opens, Some(folder("/home/deck/.local/share/Steam/steamapps/compatdata/1086940/pfx/drive_c")));
    assert!(game.note.unwrap().contains("Windows C: drive"));
    assert_eq!(names(&list(&fs, &p, &Place::Game("steam:892970".into()), View::Windows, &win())), ["Game files", "Saves"]);
    assert_eq!(names(&list(&fs, &p, &Place::Game("steam:1086940".into()), View::Linux, &win())), ["Baldurs Gate 3", "drive_c"]);
    assert!(list(&fs, &p, &Place::Game("steam:1".into()), View::Windows, &win()).problem.is_some());

    let apps = list(&fs, &p, &Place::Programs, View::Windows, &win());
    assert_eq!(names(&apps), ["Discord", "Firefox", "Heroic Games Launcher", "ProtonPlus"]);
    let discord = entry(&apps, "Discord");
    assert_eq!((discord.kind, discord.detail.as_str(), discord.type_word.as_str()), (Kind::App, "com.discordapp.Discord", "App"));
    assert_eq!(discord.opens, Some(folder("/var/lib/flatpak/app/com.discordapp.Discord/current/active/files")));
    assert_eq!(entry(&apps, "ProtonPlus").type_word, "App (only for you)");
    assert_eq!(names(&list(&fs, &p, discord.opens.as_ref().unwrap(), View::Windows, &win())), ["bin"]);
    assert_eq!(names(&list(&fs, &p, &Place::Programs, View::Linux, &win())), ["com.discordapp.Discord", "com.heroicgameslauncher.hgl", "com.vysp3r.ProtonPlus", "org.mozilla.firefox"]);

    let bin = list(&fs, &p, &Place::Trash, View::Windows, &win());
    assert_eq!(names(&bin), ["notes (copy).txt", "old-rom.zip", "old-screenshot.png"]);
    let rom = entry(&bin, "old-rom.zip");
    assert_eq!((rom.detail.as_str(), rom.deleted.as_deref(), rom.type_word.as_str(), rom.opens.clone()), ("D:\\ROMs", Some("2026-11-01T12:00:00"), "Zip archive", None));
    assert_eq!(rom.path, PathBuf::from(format!("/run/media/deck/sdcard/.Trash-{}/files/old-rom.zip", p.who.uid)));
    assert_eq!(entry(&bin, "notes (copy).txt").detail, "C:\\Users\\deck\\Documents");
    assert_eq!(entry(&list(&fs, &p, &Place::Trash, View::Linux, &win()), "old-screenshot.png").detail, "/home/deck/Pictures/Screenshots");
}

#[test]
fn files_get_plain_types_sizes_and_dates_and_sort_every_way() {
    let (t, fs, p) = machine("sort");
    let dl = "/home/deck/Downloads";
    let day = 86_400;
    for (name, ago) in [("TurbineHub.flatpak", 600), ("NonSteamLaunchers-install.log", 5000), ("bg3-mod-fixer.zip", 3 * day), ("wallpaper-jade.png", 4 * day)] {
        fs.set_modified(&Path::new(dl).join(name), NOW.secs - ago).unwrap();
    }
    t.dir("/home/deck/Downloads/Mods");
    let l = list(&fs, &p, &folder(dl), View::Windows, &win());
    assert_eq!(names(&l), ["Mods", "bg3-mod-fixer.zip", "NonSteamLaunchers-install.log", "TurbineHub.flatpak", "wallpaper-jade.png"]);
    let types: Vec<&str> = l.entries.iter().map(|e| e.type_word.as_str()).collect();
    assert_eq!(types, ["File folder", "Zip archive", "Text", "App installer", "Picture"]);
    let zip = entry(&l, "bg3-mod-fixer.zip");
    assert_eq!((zip.size, zip.modified, zip.kind, zip.is_dir, zip.writable, zip.locked, zip.hidden), (Some(5004), Some(NOW.secs - 3 * day), Kind::File, false, true, false, false));
    assert_eq!((entry(&l, "Mods").size, entry(&l, "Mods").is_dir, entry(&l, "Mods").opens.clone()), (None, true, Some(folder("/home/deck/Downloads/Mods"))));
    assert_eq!(list(&fs, &p, &folder(dl), View::Linux, &win()).entries[0].type_word, "Folder");

    let sorted = |sort, descending, folders_first| {
        let l = list(&fs, &p, &folder(dl), View::Windows, &Options { sort, descending, folders_first, ..win() });
        l.entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
    };
    assert_eq!(sorted(Sort::Name, true, true), ["Mods", "wallpaper-jade.png", "TurbineHub.flatpak", "NonSteamLaunchers-install.log", "bg3-mod-fixer.zip"]);
    assert_eq!(sorted(Sort::Size, true, true), ["Mods", "wallpaper-jade.png", "bg3-mod-fixer.zip", "TurbineHub.flatpak", "NonSteamLaunchers-install.log"]);
    assert_eq!(sorted(Sort::Size, false, true)[1], "NonSteamLaunchers-install.log");
    assert_eq!(sorted(Sort::Date, true, true)[1..], ["TurbineHub.flatpak", "NonSteamLaunchers-install.log", "bg3-mod-fixer.zip", "wallpaper-jade.png"]);
    assert_eq!(sorted(Sort::Type, false, true), ["Mods", "TurbineHub.flatpak", "wallpaper-jade.png", "NonSteamLaunchers-install.log", "bg3-mod-fixer.zip"]);
    assert_eq!(sorted(Sort::Name, false, false), ["bg3-mod-fixer.zip", "Mods", "NonSteamLaunchers-install.log", "TurbineHub.flatpak", "wallpaper-jade.png"]);

    // Search in the folder: any part of the name, upper or lower case.
    let found = list(&fs, &p, &folder(dl), View::Windows, &Options { search: " MOD ".into(), ..win() });
    assert_eq!(names(&found), ["Mods", "bg3-mod-fixer.zip"]);

    // Numbers in names count as numbers.
    for n in ["shot 10.png", "shot 9.png", "Shot 1.png", "shot 09b.png"] {
        t.write(&format!("/home/deck/Pictures/Screenshots/{n}"), b"");
    }
    let shots = list(&fs, &p, &folder("/home/deck/Pictures/Screenshots"), View::Windows, &win());
    assert_eq!(names(&shots), ["Baldurs Gate 3 camp.png", "Night City.png", "Shot 1.png", "shot 9.png", "shot 09b.png", "shot 10.png"]);

    // The whole thing as JSON for the interface.
    let json = crate::json::parse(&l.to_json()).unwrap();
    assert_eq!(json.str_at("place"), Some(dl));
    assert_eq!(json.str_at("shownPath"), Some("C:\\Users\\deck\\Downloads"));
    let first = &json.get("entries").unwrap().as_array()[1];
    assert_eq!((first.str_at("name"), first.str_at("type"), first.get("size").and_then(crate::json::Json::as_f64)), (Some("bg3-mod-fixer.zip"), Some("Zip archive"), Some(5004.0)));
}

#[test]
fn lists_group_the_way_explorer_does() {
    let (t, fs, p) = machine("group");
    let dir = "/home/deck/Documents/Dates";
    let day = 86_400;
    // NOW is Thursday 12 Nov 2026, 10:31 in the morning on the player's clock.
    let made = [("today.txt", 3600, "Today"), ("yesterday.txt", day, "Yesterday"), ("monday.txt", 3 * day, "Earlier this week"), ("last-week.txt", 6 * day, "Last week"), ("month.txt", 11 * day, "Earlier this month"), ("october.txt", 20 * day, "Last month"), ("spring.txt", 200 * day, "Earlier this year"), ("old.txt", 800 * day, "A long time ago")];
    for (name, ago, _) in made {
        t.write(&format!("{dir}/{name}"), b"x");
        fs.set_modified(&Path::new(dir).join(name), NOW.secs - ago).unwrap();
    }
    let by_date = Options { group: Group::Date, sort: Sort::Date, descending: true, ..win() };
    let l = list(&fs, &p, &folder(dir), View::Windows, &by_date);
    assert_eq!(names(&l), made.map(|(n, _, _)| n));
    assert_eq!(l.groups, made.map(|(_, _, g)| (g.to_string(), 1)));
    assert_eq!(entry(&l, "monday.txt").group_label, "Earlier this week");
    // Oldest first turns the groups around too; sorting by name keeps the newest group on top.
    let up = list(&fs, &p, &folder(dir), View::Windows, &Options { descending: false, ..by_date.clone() });
    assert_eq!((up.groups[0].0.as_str(), up.entries[0].name.as_str()), ("A long time ago", "old.txt"));
    assert_eq!(list(&fs, &p, &folder(dir), View::Windows, &Options { sort: Sort::Name, descending: false, ..by_date }).groups[0].0, "Today");
    // The clock's own midnight decides: a minute past 23:00 UTC yesterday is today, one hour east.
    assert_eq!(date_group(NOW, NOW.secs - 10 * 3600 - 1800).1, "Today");
    assert_eq!(date_group(When::utc(NOW.secs), NOW.secs - 10 * 3600 - 1800).1, "Yesterday");
    assert_eq!(date_group(NOW, NOW.secs + 500).1, "Today");

    let dl = folder("/home/deck/Downloads");
    let by_type = list(&fs, &p, &dl, View::Windows, &Options { group: Group::Type, ..win() });
    assert_eq!(by_type.groups, [("App installer".to_string(), 1), ("Picture".to_string(), 1), ("Text".to_string(), 1), ("Zip archive".to_string(), 1)]);
    t.dir("/home/deck/Downloads/Mods");
    t.write("/home/deck/Downloads/empty", b"");
    let by_size = list(&fs, &p, &dl, View::Windows, &Options { group: Group::Size, sort: Sort::Size, ..win() });
    assert_eq!(by_size.groups, [("Folders".to_string(), 1), ("Empty (0 KB)".to_string(), 1), ("Tiny (up to 16 KB)".to_string(), 3), ("Small (16 KB to 1 MB)".to_string(), 1)]);
    assert_eq!(size_group(5 << 30).1, "Gigantic (over 4 GB)");
    let by_name = list(&fs, &p, &dl, View::Windows, &Options { group: Group::Name, ..win() });
    assert_eq!(by_name.groups, [("A to H".to_string(), 2), ("I to P".to_string(), 2), ("Q to Z".to_string(), 2)]);
    assert!(list(&fs, &p, &dl, View::Windows, &win()).groups.is_empty());
}

#[test]
fn links_odd_files_and_folders_that_cant_be_read_never_break_a_list() {
    let (t, fs, p) = machine("links");
    let dir = "/home/deck/Documents";
    t.link("../Pictures", &format!("{dir}/my pictures"));
    t.link("My setup.json", &format!("{dir}/setup link"));
    t.link("/nowhere/at/all", &format!("{dir}/dead"));
    t.link("/usr/share", &format!("{dir}/system"));
    t.write("/usr/share/doc/readme", b"hello");
    t.write(&format!("{dir}/no ending"), b"\x89PNG\r\n\x1a\nrest");
    t.write(&format!("{dir}/run-me"), b"#!/bin/sh\necho hi\n");
    t.write(&format!("{dir}/data"), &[0, 159, 146, 150, 0, 1]);
    t.write(&format!("{dir}/backup~"), b"old");
    t.write(&format!("{dir}/.hidden"), b"Controller layout notes.txt\n");
    // A name that isn't text at all still gets its line, and keeps its real path.
    let odd = <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"bytes-\xff\xfe.txt");
    std::fs::write(t.at(dir).join(odd), b"x").unwrap();
    let l = list(&fs, &p, &folder(dir), View::Linux, &all());
    assert_eq!(entry(&l, "bytes-\u{fffd}\u{fffd}.txt").path, Path::new(dir).join(odd));
    let pics = entry(&l, "my pictures");
    assert_eq!((pics.kind, pics.link.as_deref(), pics.is_dir, pics.type_word.as_str(), pics.broken), (Kind::Link, Some("../Pictures"), true, "Link to a folder", false));
    assert!(pics.mode.starts_with('l'));
    assert_eq!(pics.opens, Some(folder("/home/deck/Documents/my pictures")));
    assert_eq!((entry(&l, "setup link").type_word.as_str(), entry(&l, "setup link").size.is_some()), ("Link to a settings file", true));
    let dead = entry(&l, "dead");
    assert_eq!((dead.broken, dead.type_word.as_str(), dead.link.as_deref(), dead.opens.clone()), (true, "Broken link", Some("/nowhere/at/all"), None));
    assert_eq!((entry(&l, "no ending").type_word.as_str(), entry(&l, "run-me").type_word.as_str(), entry(&l, "data").type_word.as_str()), ("Picture", "Script", "File"));
    assert!(entry(&l, "backup~").hidden && entry(&l, ".hidden").hidden && entry(&l, "Controller layout notes.txt").hidden && !entry(&l, "data").hidden);
    assert!(l.entries.len() >= 11);
    // A link to a full path is followed in the machine's own paths: the listing shows what is really in /usr/share.
    let system = list(&fs, &p, &folder("/home/deck/Documents/system"), View::Linux, &all());
    assert_eq!(names(&system), ["doc"]);
    assert_eq!(system.entries[0].path, PathBuf::from("/home/deck/Documents/system/doc"));

    // A folder nobody may look into: listed as a line, and opening it says why instead of failing.
    t.write(&format!("{dir}/secret/x"), b"");
    std::process::Command::new("chmod").arg("000").arg(t.at(&format!("{dir}/secret"))).status().unwrap();
    let again = list(&fs, &p, &folder(dir), View::Windows, &win());
    assert!(entry(&again, "secret").is_dir);
    let closed = list(&fs, &p, &folder(&format!("{dir}/secret")), View::Windows, &win());
    if p.who.uid != 0 {
        assert!(!entry(&again, "secret").writable);
        assert_eq!((closed.entries.len(), closed.problem.as_deref()), (0, Some("You aren't allowed to look in this folder.")));
    }
    assert_eq!(list(&fs, &p, &folder("/home/deck/Nope"), View::Windows, &win()).problem.as_deref(), Some("This folder isn't there (any more)."));
    assert_eq!(list(&fs, &p, &folder("/home/deck/Documents/My setup.json"), View::Windows, &win()).problem.as_deref(), Some("This is a file, not a folder."));
}

#[test]
fn words_for_types_and_permissions() {
    for (name, word) in [("a.PNG", "Picture"), ("clip.mkv", "Video"), ("song.flac", "Music"), ("notes.txt", "Text"), ("mods.zip", "Zip archive"), ("x.tar.gz", "Archive"), ("Hub.flatpak", "App installer"), ("setup.exe", "Windows program"), ("d3d11.dll", "Windows program part"), ("Game.desktop", "Shortcut"), ("Game.lnk", "Windows shortcut"), ("config.ini", "Settings file"), ("manual.pdf", "PDF document")] {
        assert_eq!(type_by_name(name), Some(word), "{name}");
    }
    assert_eq!((type_by_name("README"), type_by_name(".bashrc"), type_by_name("odd.zzz")), (None, None, None));
    for (bytes, word) in [(&b"\x89PNG\r\n\x1a\n"[..], "Picture"), (b"\xff\xd8\xff\xe0", "Picture"), (b"GIF89a", "Picture"), (b"RIFF\0\0\0\0WEBPVP8 ", "Picture"), (b"RIFF\0\0\0\0WAVEfmt ", "Music"), (b"\0\0\0\x18ftypmp42", "Video"), (b"%PDF-1.7", "PDF document"), (b"PK\x03\x04\x14", "Zip archive"), (b"\x1f\x8b\x08", "Archive"), (b"\x7fELF\x02", "Program"), (b"MZ\x90\0", "Windows program"), (b"#!/bin/bash\n", "Script"), (b"plain words\n", "Text"), (b"", "Empty file"), (b"\0\x01\x02\x03", "File")] {
        assert_eq!(type_by_bytes(bytes), word, "{bytes:?}");
    }
    assert_eq!(mode_text(NodeKind::Folder, 0o755), "drwxr-xr-x");
    assert_eq!(mode_text(NodeKind::File, 0o640), "-rw-r-----");
    assert_eq!(mode_text(NodeKind::Folder, 0o1777), "drwxrwxrwt");
    assert_eq!(mode_text(NodeKind::File, 0o4755), "-rwsr-xr-x");
    assert_eq!(mode_text(NodeKind::Link, 0o777), "lrwxrwxrwx");
}

#[test]
fn a_folder_of_twenty_thousand_files_lists_fast() {
    let t = Temp::new("many");
    let fs = Disk::under(&t.0);
    t.dir("/home/deck/Big");
    for i in 0..20_000 {
        // Some with an ending, some without: those would need their first bytes read.
        let name = if i % 4 == 0 { format!("save {i}") } else { format!("file{i}.dat") };
        std::fs::write(t.at("/home/deck/Big").join(name), b"").unwrap();
    }
    let p = places(&t, &fs);
    let started = std::time::Instant::now();
    let l = list(&fs, &p, &folder("/home/deck/Big"), View::Linux, &Options { sort: Sort::Name, ..win() });
    let by_date = list(&fs, &p, &folder("/home/deck/Big"), View::Windows, &Options { sort: Sort::Date, group: Group::Date, descending: true, ..win() });
    let took = started.elapsed();
    assert_eq!((l.entries.len(), by_date.entries.len()), (20_000, 20_000));
    assert_eq!((l.entries[0].name.as_str(), l.entries[1].name.as_str(), l.entries[2].name.as_str()), ("file1.dat", "file2.dat", "file3.dat"));
    assert_eq!(l.entries[19_999].name, "save 19996");
    assert_eq!(by_date.groups, [("Today".to_string(), 20_000)]);
    // Generous for a slow builder; on a handheld both lists together take a fraction of a second.
    assert!(took < std::time::Duration::from_secs(8), "listing 20,000 entries twice took {took:?}");
    // And the search below it stops at its limits.
    let found = search(&fs, &p, &folder("/home/deck"), View::Linux, "file", Limits { depth: 3, hits: 50, looked: 100_000 });
    assert_eq!((found.entries.len(), found.more), (50, true));
    assert!(search(&fs, &p, &folder("/home/deck"), View::Linux, "save 19996", Limits { depth: 3, hits: 50, looked: 500 }).more);
}

#[test]
fn search_below_a_folder_stays_inside_it() {
    let (t, fs, p) = machine("search");
    t.write("/home/deck/Documents/Guides/Deep/Deeper/readme deep.txt", b"");
    t.write("/home/deck/Documents/Guides/README first.md", b"");
    // Links out of the folder, and around in a circle: never walked.
    t.link("/run/media/deck/sdcard/ROMs", "/home/deck/Documents/roms");
    t.link("..", "/home/deck/Documents/Guides/loop");
    let hits = |place: &Place, needle: &str, limits: Limits| {
        let f = search(&fs, &p, place, View::Windows, needle, limits);
        (f.entries.iter().map(|e| e.path.to_string_lossy().into_owned()).collect::<Vec<_>>(), f.more)
    };
    let docs = folder("/home/deck/Documents");
    // Nearest first; the card's readme.txt is behind a link and stays out.
    assert_eq!(hits(&docs, "ReadMe", Limits::default()), (vec!["/home/deck/Documents/Guides/README first.md".to_string(), "/home/deck/Documents/Guides/Deep/Deeper/readme deep.txt".to_string()], false));
    assert_eq!(hits(&docs, "readme", Limits { depth: 2, ..Limits::default() }), (vec!["/home/deck/Documents/Guides/README first.md".to_string()], true));
    assert_eq!(hits(&docs, "readme", Limits { hits: 1, ..Limits::default() }).0.len(), 1);
    assert_eq!(hits(&docs, "  ", Limits::default()), (Vec::new(), false));
    // The link itself can be found by its name; where a hit is, is said in the view's words.
    let f = search(&fs, &p, &docs, View::Windows, "roms", Limits::default());
    assert_eq!((f.entries[0].kind, f.entries[0].detail.as_str()), (Kind::Link, "C:\\Users\\deck\\Documents"));
    // From the top of the disk: never into /proc, /sys or /dev (the made-up machine has files there).
    assert!(t.has("/proc/sys/vm/swappiness") && t.has("/sys/class/dmi/id/product_name"));
    let top = Limits { depth: 12, hits: 500, looked: 200_000 };
    assert_eq!(hits(&folder("/"), "swappiness", top).0, Vec::<String>::new());
    assert_eq!(hits(&folder("/"), "product_name", top).0, Vec::<String>::new());
    assert_eq!(hits(&folder("/proc"), "swappiness", top).0, Vec::<String>::new());
    assert_eq!(hits(&folder("/"), "os-release", top).0, ["/etc/os-release"]);
    // A made-up place searches every folder behind it: all the game libraries at once.
    assert_eq!(hits(&Place::Games, "valheim.x86", top).0, ["/run/media/deck/sdcard/steamapps/common/Valheim/valheim.x86_64"]);
}

#[test]
fn text_and_pictures_preview_inside_the_app() {
    let (t, fs, p) = machine("preview");
    let text = |path: &str| match preview(&fs, Path::new(path)) {
        Preview::Text { text, encoding, cut } => (text, encoding, cut),
        other => panic!("{path}: {other:?}"),
    };
    assert_eq!(text("/home/deck/Documents/Controller layout notes.txt"), ("Back paddles: L4 jump, R4 dodge.\nGyro only while aiming.\n".to_string(), "UTF-8", false));
    assert_eq!(text("/home/deck/Documents/My setup.json").1, "UTF-8");
    assert_eq!(text("/home/deck/Desktop/Turbine Hub.desktop").0.lines().next(), Some("[Desktop Entry]"));
    t.write("/home/deck/Documents/bom.txt", b"\xef\xbb\xbfGr\xc3\xbc\xc3\x9fe");
    t.write("/home/deck/Documents/wide.txt", &[0xFF, 0xFE, b'H', 0, b'i', 0, 0xE9, 0]);
    t.write("/home/deck/Documents/wide-be.txt", &[0xFE, 0xFF, 0, b'H', 0, b'i']);
    t.write("/home/deck/Documents/old-windows.ini", b"name=Caf\xe9\r\n");
    t.write("/home/deck/Documents/empty.txt", b"");
    t.write("/home/deck/Documents/LICENSE", b"Permission is hereby granted\n");
    assert_eq!(text("/home/deck/Documents/bom.txt"), ("Grüße".to_string(), "UTF-8", false));
    assert_eq!(text("/home/deck/Documents/wide.txt"), ("Hié".to_string(), "UTF-16 LE", false));
    assert_eq!(text("/home/deck/Documents/wide-be.txt"), ("Hi".to_string(), "UTF-16 BE", false));
    assert_eq!(text("/home/deck/Documents/old-windows.ini"), ("name=Café\r\n".to_string(), "Western (Latin-1)", false));
    assert_eq!(text("/home/deck/Documents/empty.txt"), (String::new(), "UTF-8", false));
    assert!(!text("/home/deck/Documents/LICENSE").2);
    // A long file shows its beginning, never half a letter.
    t.write("/home/deck/Documents/long.log", "é".repeat(preview::TEXT_MAX).as_bytes());
    let (long, _, cut) = text("/home/deck/Documents/long.log");
    assert!(cut && long.len() == preview::TEXT_MAX && long.chars().all(|c| c == 'é'));
    // Pictures are handed over as the path the app loads, by what the file is and not by its name.
    assert_eq!(preview(&fs, Path::new("/home/deck/Downloads/wallpaper-jade.png")), Preview::Picture { load: t.at("/home/deck/Downloads/wallpaper-jade.png") });
    std::fs::copy(t.at("/home/deck/Downloads/wallpaper-jade.png"), t.at("/home/deck/Downloads/picture.txt")).unwrap();
    assert!(matches!(preview(&fs, Path::new("/home/deck/Downloads/picture.txt")), Preview::Picture { .. }));
    t.write("/home/deck/Pictures/drawing.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    assert!(matches!(preview(&fs, Path::new("/home/deck/Pictures/drawing.svg")), Preview::Picture { .. }));
    t.link("../Documents/Controller layout notes.txt", "/home/deck/Desktop/notes link");
    assert!(text("/home/deck/Desktop/notes link").0.starts_with("Back paddles"));
    // Everything else opens with its own app.
    for path in ["/home/deck/Downloads/bg3-mod-fixer.zip", "/home/deck/Downloads/TurbineHub.flatpak", "/home/deck/Downloads", "/home/deck/Nope.txt", "/home/deck/.local/share/Steam/steamapps/common/Baldurs Gate 3/game.exe"] {
        assert!(matches!(preview(&fs, Path::new(path)), Preview::Outside { .. }), "{path}");
    }
    t.write("/home/deck/Documents/fake.txt", &[0u8, 1, 2, 3, 0, 0, 9, 9]);
    assert!(matches!(preview(&fs, Path::new("/home/deck/Documents/fake.txt")), Preview::Outside { .. }));
    assert_eq!(open_line(Path::new("/home/deck/a b.pdf"), true), ["flatpak-spawn", "--host", "xdg-open", "/home/deck/a b.pdf"]);
    assert_eq!(open_line(Path::new("/home/deck/a b.pdf"), false), ["xdg-open", "/home/deck/a b.pdf"]);

    // Properties.
    let zip = properties(&fs, &p, Path::new("/home/deck/Downloads/bg3-mod-fixer.zip"), View::Windows, 1000).unwrap();
    assert_eq!((zip.name.as_str(), zip.windows_path.as_str(), zip.type_word.as_str(), zip.size, zip.is_dir, zip.read_only), ("bg3-mod-fixer.zip", "C:\\Users\\deck\\Downloads\\bg3-mod-fixer.zip", "Zip archive", 5004, false, false));
    assert_eq!((zip.drive.as_str(), zip.space.map(|s| s.total), zip.mode.len(), zip.link), ("SteamOS (C:)", Some(512_000_000_000), 10, None));
    assert!(zip.modified > 0);
    let game = properties(&fs, &p, Path::new("/home/deck/.local/share/Steam/steamapps/common/Baldurs Gate 3"), View::Windows, 1000).unwrap();
    assert_eq!((game.type_word.as_str(), game.files, game.folders, game.size, game.counted_all), ("File folder", 2, 1, 4, true));
    let capped = properties(&fs, &p, Path::new("/home/deck"), View::Linux, 5).unwrap();
    assert!(!capped.counted_all && capped.files + capped.folders == 5);
    let card = properties(&fs, &p, Path::new("/run/media/deck/sdcard/ROMs/readme.txt"), View::Windows, 10).unwrap();
    assert_eq!((card.drive.as_str(), card.type_word.as_str()), ("SD card (D:)", "Text"));
    assert_eq!(properties(&fs, &p, Path::new("/run/media/deck/sdcard/ROMs"), View::Linux, 10).unwrap().drive, "/run/media/deck/sdcard");
    let link = properties(&fs, &p, Path::new("/home/deck/Desktop/notes link"), View::Windows, 10).unwrap();
    assert_eq!((link.link.as_deref(), link.type_word.as_str(), link.files), (Some("../Documents/Controller layout notes.txt"), "Link", 0));
    let system = properties(&fs, &p, Path::new("/etc/os-release"), View::Windows, 10).unwrap();
    assert!(system.read_only && system.locked.unwrap().contains("part of SteamOS") && system.windows_path == "C:\\Windows\\etc\\os-release");
    assert!(properties(&fs, &p, Path::new("/home/deck/Nope"), View::Windows, 10).is_err());
}

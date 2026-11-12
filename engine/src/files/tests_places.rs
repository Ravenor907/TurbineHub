//! The places and the two-way mapping, on the made-up machine.

use super::testing::{Temp, machine, places};
use super::*;
use std::path::{Path, PathBuf};

const BG3: &str = "/home/deck/.local/share/Steam/steamapps/common/Baldurs Gate 3";
const BG3_C: &str = "/home/deck/.local/share/Steam/steamapps/compatdata/1086940/pfx/drive_c";

fn folder(p: &str) -> Place {
    Place::Folder(p.into())
}

#[test]
fn the_machine_is_read_into_places() {
    let (_t, _fs, p) = machine("read");
    assert_eq!(p.os, "SteamOS");
    assert_eq!(p.who.user, "deck");
    assert_eq!(p.dirs.iter().map(|d| d.name).collect::<Vec<_>>(), ["Desktop", "Downloads", "Documents", "Pictures", "Music", "Videos"]);
    assert!(p.dirs.iter().all(|d| d.mapped && d.path.starts_with("/home/deck")));
    assert_eq!(p.drives.len(), 1);
    let d = &p.drives[0];
    assert_eq!((d.title().as_str(), d.mount.as_path(), d.fs.as_str(), d.read_only), ("SD card (D:)", Path::new("/run/media/deck/sdcard"), "ext4", false));
    assert_eq!(p.games.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["Baldur's Gate 3", "Cyberpunk 2077", "Valheim"]);
    let bg3 = p.game("steam:1086940").unwrap();
    assert_eq!((bg3.files.as_path(), bg3.windows.as_deref(), bg3.saves.as_deref()), (Path::new(BG3), Some(Path::new(BG3_C)), None));
    let valheim = p.game("steam:892970").unwrap();
    assert_eq!(valheim.files, PathBuf::from("/run/media/deck/sdcard/steamapps/common/Valheim"));
    assert_eq!((valheim.windows.as_deref(), valheim.saves.as_deref()), (None, Some(Path::new("/home/deck/.config/unity3d/IronGate/Valheim"))));
    assert_eq!(p.apps.iter().map(|a| (a.name.as_str(), a.system)).collect::<Vec<_>>(), [("Discord", true), ("Heroic Games Launcher", true), ("Firefox", true), ("ProtonPlus", false)]);
    assert_eq!(p.apps[0].files, PathBuf::from("/var/lib/flatpak/app/com.discordapp.Discord/current/active/files"));
    assert_eq!(p.libraries, [PathBuf::from("/home/deck/.local/share/Steam/steamapps/common"), "/run/media/deck/sdcard/steamapps/common".into()]);
    // New players start with their six folders pinned.
    assert_eq!(p.quick.pins.len(), 6);
}

/// (real path, how the Windows view writes it).
const PATHS: &[(&str, &str)] = &[
    ("/", "C:\\"),
    ("/home", "C:\\Users"),
    ("/home/deck", "C:\\Users\\deck"),
    ("/home/deck/Downloads", "C:\\Users\\deck\\Downloads"),
    ("/home/deck/Downloads/bg3-mod-fixer.zip", "C:\\Users\\deck\\Downloads\\bg3-mod-fixer.zip"),
    ("/home/deck/.config/unity3d", "C:\\Users\\deck\\AppData\\Roaming\\unity3d"),
    ("/home/deck/.local/share", "C:\\Users\\deck\\AppData\\Local"),
    ("/home/deck/.var/app", "C:\\Users\\deck\\AppData\\Apps from the Store"),
    ("/home/deck/.var/app/com.discordapp.Discord/config", "C:\\Users\\deck\\AppData\\Apps from the Store\\Discord\\config"),
    ("/home/deck/.local/share/Steam/steamapps/common", "C:\\Games"),
    ("/home/deck/.local/share/Steam/steamapps/common/Baldurs Gate 3", "C:\\Games\\Baldur's Gate 3\\Game files"),
    ("/home/deck/.local/share/Steam/steamapps/common/Baldurs Gate 3/bin/bg3.exe", "C:\\Games\\Baldur's Gate 3\\Game files\\bin\\bg3.exe"),
    ("/home/deck/.local/share/Steam/steamapps/compatdata/1086940/pfx/drive_c/users/steamuser", "C:\\Games\\Baldur's Gate 3\\Its Windows C: drive\\users\\steamuser"),
    ("/home/deck/.local/share/Steam/steamapps/compatdata", "C:\\Users\\deck\\AppData\\Local\\Steam\\steamapps\\compatdata"),
    ("/run/media/deck/sdcard/steamapps/common/Valheim/valheim_Data", "C:\\Games\\Valheim\\Game files\\valheim_Data"),
    ("/home/deck/.config/unity3d/IronGate/Valheim/worlds_local", "C:\\Games\\Valheim\\Saves\\worlds_local"),
    ("/var/lib/flatpak/app", "C:\\Program Files"),
    ("/var/lib/flatpak/app/org.mozilla.firefox/current/active/files/bin", "C:\\Program Files\\Firefox\\bin"),
    ("/home/deck/.local/share/flatpak/app/com.vysp3r.ProtonPlus/current/active/files", "C:\\Program Files\\ProtonPlus"),
    ("/var/lib/flatpak/app/org.mozilla.firefox/x86_64", "C:\\Program Files\\org.mozilla.firefox\\x86_64"),
    ("/usr", "C:\\Windows"),
    ("/usr/bin/ls", "C:\\Windows\\bin\\ls"),
    ("/etc", "C:\\Windows\\etc"),
    ("/etc/os-release", "C:\\Windows\\etc\\os-release"),
    ("/run/media/deck/sdcard", "D:\\"),
    ("/run/media/deck/sdcard/ROMs/readme.txt", "D:\\ROMs\\readme.txt"),
    ("/home/deck/.local/share/Trash/files", "Recycle Bin"),
    ("/home/deck/.local/share/Trash/files/old-screenshot.png", "Recycle Bin\\old-screenshot.png"),
    ("/proc/sys/vm", "C:\\proc\\sys\\vm"),
];

#[test]
fn every_real_path_has_a_windows_path_and_comes_back_from_it() {
    let (t, fs, p) = machine("map");
    t.write("/usr/bin/ls", b"\x7fELF");
    for (real, shown) in PATHS {
        assert_eq!(p.windows_path(Path::new(real)), *shown, "{real}");
        assert_eq!(p.real_path(&fs, shown), Some(PathBuf::from(real)), "{shown}");
        // A real path typed as it is stays itself.
        assert_eq!(p.resolve(&fs, real), Ok(folder(real)), "{real}");
    }
    // A real folder whose name the Windows view uses for something else keeps its plain path.
    assert_eq!(p.windows_path(Path::new("/usr/etc/x")), "C:\\usr\\etc\\x");
    assert_eq!(p.real_path(&fs, "C:\\usr\\etc\\x"), Some("/usr/etc/x".into()));
    // Trailing slashes, doubled slashes and dots change nothing.
    assert_eq!(p.windows_path(Path::new("/home/deck//Downloads/../Documents/./")), "C:\\Users\\deck\\Documents");
    assert_eq!(p.copy_as_path(Path::new("/home/deck/Downloads/")), ("\"C:\\Users\\deck\\Downloads\"".to_string(), "/home/deck/Downloads".to_string()));
}

#[test]
fn typed_paths_work_in_both_styles() {
    let (t, fs, p) = machine("typed");
    t.write("/usr/bin/ls", b"\x7fELF");
    let ok = |typed: &str| p.resolve(&fs, typed).unwrap_or_else(|e| panic!("{typed}: {e}"));
    // Upper and lower case, either slash, quotes, spaces around.
    assert_eq!(ok("c:\\users\\DECK\\downloads"), folder("/home/deck/Downloads"));
    assert_eq!(ok("C:/Users/deck/Documents/"), folder("/home/deck/Documents"));
    assert_eq!(ok("  \"C:\\Users\\deck\\Documents\\My setup.json\" "), folder("/home/deck/Documents/My setup.json"));
    // ".." the Windows way: by the text, and never above the drive.
    assert_eq!(ok("C:\\Users\\deck\\Downloads\\..\\Pictures\\.\\Screenshots"), folder("/home/deck/Pictures/Screenshots"));
    assert_eq!(ok("C:\\.."), folder("/"));
    assert_eq!(ok("C:\\Games\\Baldur's Gate 3\\Game files\\.."), Place::Game("steam:1086940".into()));
    assert_eq!(ok("C:\\Games\\Baldur's Gate 3\\.."), Place::Games);
    assert_eq!(ok("C:\\Users\\deck\\AppData\\Roaming\\.."), Place::AppData);
    assert_eq!(ok("C:"), folder("/"));
    // The made-up places.
    assert_eq!(ok("C:\\Games"), Place::Games);
    assert_eq!(ok("c:\\games\\baldur's gate 3"), Place::Game("steam:1086940".into()));
    assert_eq!(ok("C:\\Games\\Baldur's Gate 3\\bin"), folder(&format!("{BG3}/bin")));
    assert_eq!(ok("C:\\Program Files"), Place::Programs);
    assert_eq!(ok("C:\\Program Files\\Discord\\bin"), folder("/var/lib/flatpak/app/com.discordapp.Discord/current/active/files/bin"));
    assert_eq!(ok("C:\\Windows"), Place::Windows);
    assert_eq!(ok("C:\\Windows\\BIN\\ls"), folder("/usr/bin/ls"));
    assert_eq!(ok("C:\\Users\\deck\\AppData"), Place::AppData);
    assert_eq!(ok("Recycle Bin"), Place::Trash);
    assert_eq!(ok("This PC"), Place::ThisPc);
    assert_eq!(ok("This PC\\SD card (D:)\\ROMs"), folder("/run/media/deck/sdcard/ROMs"));
    assert_eq!(ok("d:\\roms"), folder("/run/media/deck/sdcard/ROMs"));
    // What people paste from Windows guides and from Linux ones.
    assert_eq!(ok("%APPDATA%\\unity3d\\IronGate"), folder("/home/deck/.config/unity3d/IronGate"));
    assert_eq!(ok("%LocalAppData%"), folder("/home/deck/.local/share"));
    assert_eq!(ok("%USERPROFILE%\\Desktop"), folder("/home/deck/Desktop"));
    assert_eq!(ok("~"), folder("/home/deck"));
    assert_eq!(ok("~/Music/"), folder("/home/deck/Music"));
    assert_eq!(ok("file:///home/deck/Documents/My%20setup.json"), folder("/home/deck/Documents/My setup.json"));
    assert_eq!(ok("/home/deck/../deck/./Videos//"), folder("/home/deck/Videos"));
    // A path that isn't there yet still has a meaning (a new name).
    assert_eq!(ok("C:\\Users\\deck\\Documents\\New folder\\a.txt"), folder("/home/deck/Documents/New folder/a.txt"));
    // What is no path says how to type one.
    assert!(p.resolve(&fs, "E:\\").unwrap_err().contains("no drive E:"));
    assert!(p.resolve(&fs, "Documents\\x").unwrap_err().contains("full path"));
    assert!(p.resolve(&fs, "   ").is_err());

    // The address bar: a folder opens, a file opens its folder with the file picked, nothing says so.
    assert_eq!(p.go(&fs, "C:\\Users\\deck\\Downloads", View::Windows), Ok((folder("/home/deck/Downloads"), None)));
    assert_eq!(p.go(&fs, "/usr", View::Windows), Ok((Place::Windows, None)));
    assert_eq!(p.go(&fs, "/usr", View::Linux), Ok((folder("/usr"), None)));
    assert_eq!(p.go(&fs, "C:\\Users\\deck\\Documents\\My setup.json", View::Windows), Ok((folder("/home/deck/Documents"), Some("/home/deck/Documents/My setup.json".into()))));
    assert!(p.go(&fs, "C:\\Users\\deck\\Nope", View::Windows).unwrap_err().contains("Can't find"));
}

#[test]
fn odd_names_spaces_unicode_and_links_keep_their_meaning() {
    let (t, fs, _) = machine("odd");
    for name in ["we ird\\name", "Ünïcode ✓ ordner", " leading and trailing ", "100% real", "semi;colon:name", "tab\tname", "C:"] {
        t.write(&format!("/home/deck/Documents/{name}/inside.txt"), b"x");
    }
    // Links: one to the games, one to a folder with spaces.
    t.link(".local/share/Steam/steamapps/common", "/home/deck/games");
    t.link("/home/deck/Documents/Ünïcode ✓ ordner", "/home/deck/Desktop/shortcut to it");
    let p = places(&t, &fs);
    for name in ["we ird\\name", "Ünïcode ✓ ordner", " leading and trailing ", "100% real", "semi;colon:name", "tab\tname", "C:"] {
        let real = PathBuf::from(format!("/home/deck/Documents/{name}/inside.txt"));
        let shown = p.windows_path(&real);
        assert_eq!(shown, format!("C:\\Users\\deck\\Documents\\{name}\\inside.txt"));
        assert_eq!(p.real_path(&fs, &shown), Some(real), "{shown}");
    }
    // A link keeps the name it was reached by, both ways: it is shown where it is, not where it leads.
    let through = Path::new("/home/deck/games/Baldurs Gate 3/bin");
    assert_eq!(p.windows_path(through), "C:\\Users\\deck\\games\\Baldurs Gate 3\\bin");
    assert_eq!(p.real_path(&fs, "C:\\Users\\deck\\games\\Baldurs Gate 3\\bin"), Some(through.to_path_buf()));
    assert_eq!(p.real_path(&fs, "C:\\Users\\deck\\Desktop\\shortcut to it\\inside.txt"), Some("/home/deck/Desktop/shortcut to it/inside.txt".into()));
    // Where it really leads is one question away, with a link that names a full path too.
    assert_eq!(fs.canonical(through).unwrap(), PathBuf::from(format!("{BG3}/bin")));
    assert_eq!(fs.canonical(Path::new("/home/deck/Desktop/shortcut to it/inside.txt")).unwrap(), PathBuf::from("/home/deck/Documents/Ünïcode ✓ ordner/inside.txt"));
    // Upper and lower case only help when one name fits.
    t.write("/home/deck/Documents/Notes/a", b"");
    t.write("/home/deck/Documents/NOTES/b", b"");
    assert_eq!(p.real_path(&fs, "C:\\Users\\deck\\Documents\\notes"), Some("/home/deck/Documents/notes".into()));
    assert_eq!(p.real_path(&fs, "C:\\Users\\deck\\Documents\\NOTES\\b"), Some("/home/deck/Documents/NOTES/b".into()));
}

#[test]
fn the_players_folders_follow_the_systems_own_list() {
    let t = Temp::new("xdg");
    let fs = Disk::under(&t.0);
    for d in ["Bureau", "Téléchargements", "Documents", "Pictures", "Videos", "Music"] {
        t.dir(&format!("/home/deck/{d}"));
    }
    t.dir("/mnt/big/Films");
    // No list: the standard names that are there.
    assert_eq!(places::user_dirs(&fs, Path::new("/home/deck")).iter().map(|d| d.name).collect::<Vec<_>>(), ["Documents", "Pictures", "Music", "Videos"]);
    t.write("/home/deck/.config/user-dirs.dirs", "# made by xdg-user-dirs\nXDG_DESKTOP_DIR=\"$HOME/Bureau\"\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\nXDG_MUSIC_DIR=\"$HOME/\"\nXDG_VIDEOS_DIR=\"/mnt/big/Films\"\nXDG_PICTURES_DIR=\"$HOME/Nowhere\"\n".as_bytes());
    let p = places(&t, &fs);
    let got: Vec<(&str, &str, bool)> = p.dirs.iter().map(|d| (d.name, d.path.to_str().unwrap(), d.mapped)).collect();
    // Music is switched off, Pictures isn't there, Videos is elsewhere (shown, but under its own path).
    assert_eq!(got, [("Desktop", "/home/deck/Bureau", true), ("Downloads", "/home/deck/Téléchargements", true), ("Documents", "/home/deck/Documents", true), ("Videos", "/mnt/big/Films", false)]);
    assert_eq!(p.windows_path(Path::new("/home/deck/Téléchargements/a.zip")), "C:\\Users\\deck\\Downloads\\a.zip");
    assert_eq!(p.real_path(&fs, "C:\\Users\\deck\\Downloads\\a.zip"), Some("/home/deck/Téléchargements/a.zip".into()));
    assert_eq!(p.real_path(&fs, "C:\\Users\\deck\\Téléchargements"), Some("/home/deck/Téléchargements".into()));
    assert_eq!(p.windows_path(Path::new("/mnt/big/Films")), "C:\\mnt\\big\\Films");
    // A folder that really is called Downloads keeps that name; the listed one then goes by its own.
    t.dir("/home/deck/Downloads");
    let p = places(&t, &fs);
    assert!(!p.dirs.iter().find(|d| d.name == "Downloads").unwrap().mapped);
    assert_eq!(p.windows_path(Path::new("/home/deck/Téléchargements")), "C:\\Users\\deck\\Téléchargements");
    assert_eq!(p.real_path(&fs, "C:\\Users\\deck\\Downloads"), Some("/home/deck/Downloads".into()));
}

#[test]
fn up_and_the_address_bar_follow_the_view() {
    let (_t, fs, p) = machine("crumbs");
    let titles = |place: &Place, view| p.crumbs(&fs, place, view).into_iter().map(|(t, _)| t).collect::<Vec<_>>();
    assert_eq!(titles(&folder("/home/deck/Downloads"), View::Windows), ["This PC", "SteamOS (C:)", "Users", "deck", "Downloads"]);
    assert_eq!(titles(&folder("/home/deck/Downloads"), View::Linux), ["/", "home", "deck", "Downloads"]);
    assert_eq!(titles(&folder(&format!("{BG3}/bin")), View::Windows), ["This PC", "SteamOS (C:)", "Games", "Baldur's Gate 3", "Game files", "bin"]);
    assert_eq!(titles(&folder(&format!("{BG3_C}/users")), View::Windows), ["This PC", "SteamOS (C:)", "Games", "Baldur's Gate 3", "Its Windows C: drive", "users"]);
    assert_eq!(titles(&folder("/run/media/deck/sdcard/ROMs"), View::Windows), ["This PC", "SD card (D:)", "ROMs"]);
    assert_eq!(titles(&folder("/etc"), View::Windows), ["This PC", "SteamOS (C:)", "Windows", "etc"]);
    assert_eq!(titles(&folder("/home/deck/.config"), View::Windows), ["This PC", "SteamOS (C:)", "Users", "deck", "AppData", "Roaming"]);
    assert_eq!(titles(&folder("/var/lib/flatpak/app/org.mozilla.firefox/current/active/files/bin"), View::Windows), ["This PC", "SteamOS (C:)", "Program Files", "Firefox", "bin"]);
    assert_eq!(titles(&Place::Trash, View::Windows), ["Recycle Bin"]);
    assert_eq!(titles(&Place::ThisPc, View::Linux), ["Computer"]);
    // Every piece leads somewhere: the third piece of Downloads is C:\Users.
    assert_eq!(p.crumbs(&fs, &folder("/home/deck/Downloads"), View::Windows)[2].1, folder("/home"));
    assert_eq!(p.up(&fs, &folder("/run/media/deck/sdcard"), View::Windows), Some(Place::ThisPc));
    assert_eq!(p.up(&fs, &folder("/"), View::Linux), Some(Place::ThisPc));
    assert_eq!(p.up(&fs, &folder(BG3), View::Windows), Some(Place::Game("steam:1086940".into())));
    assert_eq!(p.up(&fs, &folder(BG3), View::Linux), Some(folder("/home/deck/.local/share/Steam/steamapps/common")));
    assert_eq!(p.up(&fs, &folder("/home/deck/.local/share/Steam/steamapps/common/Proton 11.0"), View::Windows), Some(Place::Games));
    assert_eq!(p.up(&fs, &Place::Home, View::Windows), None);
    assert_eq!(p.shown_path(&Place::Game("steam:1086940".into()), View::Windows), "C:\\Games\\Baldur's Gate 3");
    assert_eq!(p.shown_path(&Place::Games, View::Linux), "/home/deck/.local/share/Steam/steamapps/common");
    assert_eq!(p.shown_path(&folder("/home/deck"), View::Windows), "C:\\Users\\deck");

    // The navigation pane of each view.
    let nav = |view| p.nav(view).into_iter().map(|n| n.label).collect::<Vec<_>>();
    assert_eq!(nav(View::Windows), ["Home", "Desktop", "Downloads", "Documents", "Pictures", "Music", "Videos", "This PC", "SteamOS (C:)", "SD card (D:)", "Games", "Apps", "Recycle Bin"]);
    assert_eq!(nav(View::Linux), ["Home  /home/deck", "Root  /", "/run/media/deck/sdcard", "Steam games", "Flatpak apps", "App data", "Trash"]);
    let w = p.nav(View::Windows);
    assert!(w[1].pinned && w[1].starts_group && w[8].depth == 1 && w[7].starts_group && !w[0].pinned);
    assert!(p.note(&Place::Windows, View::Windows).unwrap().contains("locked"));
    assert!(p.note(&Place::Game("steam:892970".into()), View::Windows).unwrap().contains("without Windows"));
    assert_eq!(p.note(&folder("/home/deck"), View::Windows), None);
}

#[test]
fn the_desktop_and_the_tv_pc_have_the_same_files() {
    use crate::fake::{self, Preset};
    for preset in [Preset::Desktop, Preset::TvPc] {
        let t = Temp::new("presets");
        fake::machine(&t.0, preset);
        fake::files(&t.0, &t.at(fake::FILES_HOME));
        let fs = Disk::under(&t.0);
        let p = places(&t, &fs);
        // No card reader here: the folder under /run/media is a drive all the same, named by its folder.
        assert_eq!(p.drives.iter().map(|d| d.title()).collect::<Vec<_>>(), ["sdcard (D:)"]);
        assert_eq!((p.games.len(), p.apps.len(), p.dirs.len(), trash::items(&fs, &p).len()), (3, 4, 6, 3));
        let pc = list(&fs, &p, &Place::ThisPc, View::Windows, &Options::default());
        assert_eq!(pc.entries.len(), 9);
        // The made-up pictures are real pictures.
        let png = std::fs::read(t.at("/home/deck/Pictures/wallpaper-jade.png")).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n") && png.ends_with(&[0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82]) && png.len() > 17_000);
    }
}

#[test]
fn drives_get_letters_and_plain_names() {
    use crate::hardware::{Drive, DriveRole, Partition};
    let t = Temp::new("drives");
    let fs = Disk::under(&t.0);
    for d in ["/home/deck", "/run/media/deck/My Games", "/run/media/deck/1234-ABCD", "/mnt/windows", "/mnt/data", "/var/lib/docker", "/efi"] {
        t.dir(d);
    }
    // A second name for a card, as SteamOS keeps one: not a second drive.
    t.link("deck/1234-ABCD", "/run/media/mmcblk0p1");
    let part = |name: &str, fs: &str, label: &str, mount: &str| Partition { name: name.into(), fs: fs.into(), label: label.into(), part_name: String::new(), mount: Some(mount.into()) };
    let drive = |name: &str, removable: bool, role, partitions| Drive { name: name.into(), model: String::new(), bytes: 1, removable, partitions, role };
    let hardware = [
        drive("nvme0n1", false, DriveRole::System, vec![part("nvme0n1p1", "vfat", "esp", "/efi"), part("nvme0n1p4", "btrfs", "rootfs", "/"), part("nvme0n1p8", "ext4", "home", "/home"), part("nvme0n1p7", "ext4", "var", "/var/lib/docker")]),
        drive("mmcblk0", true, DriveRole::Data, vec![part("mmcblk0p1", "exfat", "", "/run/media/deck/1234-ABCD")]),
        drive("sda", false, DriveRole::Windows, vec![part("sda3", "ntfs", "", "/mnt/windows")]),
        drive("sdb", false, DriveRole::Data, vec![part("sdb1", "ext4", "Big/Data", "/mnt/data"), part("sdb2", "ext4", "gone", "/mnt/not-there")]),
    ];
    let who = Who::read(&fs, Path::new("/home/deck"));
    let got: Vec<(String, String, bool)> = places::drives(&fs, &who, &hardware).into_iter().map(|d| (d.title(), d.mount.to_string_lossy().into_owned(), d.read_only)).collect();
    assert_eq!(
        got,
        [
            ("SD card (D:)".to_string(), "/run/media/deck/1234-ABCD".to_string(), false),
            ("My Games (E:)".into(), "/run/media/deck/My Games".into(), false),
            ("Big Data (F:)".into(), "/mnt/data".into(), false),
            ("Windows drive (G:)".into(), "/mnt/windows".into(), true),
        ]
    );
}

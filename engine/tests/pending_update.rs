//! The game update warning also sees an update that is still waiting in Steam.

use std::fs;
use turbinehub_engine::steam::{self, PendingUpdate};

fn manifest(flags: u32, build: &str, target: &str, to_get: u64, got: u64, result: &str) -> String {
    format!(
        "\"AppState\"\n{{\n\t\"appid\"\t\t\"1245620\"\n\t\"name\"\t\t\"ELDEN RING\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"ELDEN RING\"\n\t\"buildid\"\t\t\"{build}\"\n\t\"UpdateResult\"\t\t\"{result}\"\n\t\"BytesToDownload\"\t\t\"{to_get}\"\n\t\"BytesDownloaded\"\t\t\"{got}\"\n\t\"TargetBuildID\"\t\t\"{target}\"\n}}\n"
    )
}

#[test]
fn reads_waiting_updates() {
    // Up to date.
    assert_eq!(steam::pending_update_in(&manifest(4, "25827938", "25827938", 0, 0, "0")), None);
    assert_eq!(steam::pending_update_in(&manifest(4, "25827938", "0", 0, 0, "0")), None);
    // Update required (queued), with a new target build.
    let p = steam::pending_update_in(&manifest(6, "25827938", "26000001", 0, 0, "0")).unwrap();
    assert_eq!(p, PendingUpdate { build: Some("25827938".into()), target: Some("26000001".into()), bytes_left: 0, result: None });
    // Downloading: bytes left.
    let p = steam::pending_update_in(&manifest(4 | 1024 | 1_048_576, "25827938", "26000001", 300 << 20, 100 << 20, "0")).unwrap();
    assert_eq!(p.bytes_left, 200 << 20);
    assert!(steam::pending_update_words("ELDEN RING", &p).contains("200 MB to download"));
    // Only the target build differs (Steam hasn't set the bits yet).
    assert!(steam::pending_update_in(&manifest(4, "1", "2", 0, 0, "0")).is_some());
    // A failed update.
    let p = steam::pending_update_in(&manifest(4, "1", "1", 0, 0, "12")).unwrap();
    assert!(steam::pending_update_words("ELDEN RING", &p).contains("didn't finish"));
    assert_eq!(
        steam::pending_update_words("ELDEN RING", &steam::pending_update_in(&manifest(6, "1", "2", 0, 0, "0")).unwrap()),
        "An update for ELDEN RING is waiting in Steam: mods may break after it installs."
    );
    assert_eq!(steam::pending_update_in("not a manifest"), None);
}

#[test]
fn a_game_with_a_waiting_update_is_still_listed_and_found() {
    let d = std::env::temp_dir().join(format!("turbine-pending-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let lib = d.join(".local/share/Steam");
    fs::create_dir_all(lib.join("steamapps/common/ELDEN RING/Game")).unwrap();
    fs::write(lib.join("steamapps/libraryfolders.vdf"), format!("\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n", lib.display())).unwrap();
    fs::write(lib.join("steamapps/appmanifest_1245620.acf"), manifest(6, "25827938", "26000001", 0, 0, "0")).unwrap();
    let mut scan = turbinehub_engine::games::Scan::default();
    steam::scan(&d, &mut scan);
    let g = scan.games.iter().find(|g| g.store_id == "1245620").expect("listed while an update waits");
    assert_eq!(steam::pending_update(g).unwrap().target.as_deref(), Some("26000001"));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_steam_library_turbine_cant_read_is_named() {
    use turbinehub_engine::steam;
    let root = std::env::temp_dir().join(format!("turbine-unseen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("steamapps")).unwrap();
    std::fs::write(
        root.join("steamapps/libraryfolders.vdf"),
        "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/mnt/games-drive/SteamLibrary\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/run/media/deck/SD/SteamLibrary\"\n\t}\n}\n",
    )
    .unwrap();
    let mut scan = turbinehub_engine::games::Scan::default();
    let libs = steam::libraries(&root, &mut scan);
    assert_eq!(libs.len(), 1, "only the root is readable");
    // A drive outside the allowed folders is named, with why; a removable drive that isn't in is not.
    assert_eq!(scan.problems.len(), 1, "{:?}", scan.problems);
    assert!(scan.problems[0].starts_with(steam::UNSEEN_LIBRARY) && scan.problems[0].contains("/mnt/games-drive/SteamLibrary"), "{:?}", scan.problems);
    assert!(scan.problems[0].contains(steam::UNSEEN_WHY));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unreadable_steam_libraries_are_listed_and_only_plain_folders_can_be_allowed() {
    use std::path::{Path, PathBuf};
    use turbinehub_engine::steam;
    let home = std::env::temp_dir().join(format!("turbinehub-unseen-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let steam_dir = home.join(".local/share/Steam");
    std::fs::create_dir_all(steam_dir.join("steamapps")).unwrap();
    assert!(steam::unseen_libraries(&home).is_empty());
    std::fs::write(
        steam_dir.join("steamapps/libraryfolders.vdf"),
        format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n\"1\"\n{{\n\"path\" \"/mnt/games-drive/SteamLibrary\"\n}}\n\"2\"\n{{\n\"path\" \"/run/media/deck/SD/SteamLibrary\"\n}}\n\"3\"\n{{\n\"path\" \"/mnt/games-drive/SteamLibrary\"\n}}\n}}\n", steam_dir.display()),
    )
    .unwrap();
    // The drive outside the allowed folders, once; an SD card that isn't in is not a permission problem.
    assert_eq!(steam::unseen_libraries(&home), vec![PathBuf::from("/mnt/games-drive/SteamLibrary")]);
    let _ = std::fs::remove_dir_all(&home);

    let id = "io.github.ravenor907.TurbineHub";
    let line = steam::allow_line(Path::new("/mnt/games-drive/SteamLibrary"), id).unwrap();
    assert_eq!(line, ["flatpak-spawn", "--host", "flatpak", "override", "--user", "--filesystem=/mnt/games-drive/SteamLibrary", id]);
    assert_eq!(steam::allowed_by(&line, id), Some(PathBuf::from("/mnt/games-drive/SteamLibrary")));
    // Another app's override (the Store's, for Chrome) is not Turbine Hub's folder.
    let chrome: Vec<String> = ["flatpak-spawn", "--host", "flatpak", "override", "--user", "--filesystem=/run/udev:ro", "com.google.Chrome"].iter().map(|x| x.to_string()).collect();
    assert_eq!(steam::allowed_by(&chrome, id), None);
    assert_eq!(steam::allowed_by(&line[..5], id), None);
    // A folder with spaces is one argument, no shell in between.
    assert!(steam::allow_line(Path::new("/mnt/My Games/SteamLibrary"), id).unwrap().contains(&"--filesystem=/mnt/My Games/SteamLibrary".to_string()));
    // Never a keyword, an option, a mode, a way out of the folder, the whole system or the whole home folder.
    for bad in ["host", "home", "relative/path", "/", "/mnt", "/mnt/x:ro", "/mnt/x=y/z", "/mnt/a;b/c", "/mnt/a\nb/c", "/mnt/x/../../etc", "/mnt/x/..", "/mnt/a,b/c", "/mnt/a\\b/c", "--filesystem=host", ""] {
        assert!(steam::allow_line(Path::new(bad), id).is_none(), "{bad:?}");
    }
    assert!(steam::allow_line(&turbinehub_engine::home_dir(), id).is_none());
}

#[test]
fn downloaded_art_is_kept_in_turbine_hubs_own_cache_folder() {
    use turbinehub_engine::steam;
    assert_eq!(steam::ART_CACHE, "turbinehub/art");
    // Under the cache folder everything else Turbine Hub fetched lives in, whatever the environment says it is.
    if std::env::var_os("TURBINEHUB_ART").is_none() {
        let dir = steam::art_cache().expect("a cache folder");
        assert!(dir.ends_with("turbinehub/art"), "{}", dir.display());
        assert!(!dir.to_string_lossy().contains("mod-manager"));
    }
}

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

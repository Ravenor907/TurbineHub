//! Builds a fake SteamOS home folder (internal library, SD card library, Heroic Flatpak) and checks what's found.

use std::fs;
use std::path::{Path, PathBuf};
use turbinehub_engine::{scan_games, Runtime, Store};

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn manifest(app_id: &str, name: &str, dir: &str, flags: u32) -> String {
    format!(
        "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"{dir}\"\n\t\"buildid\"\t\t\"1234567\"\n}}\n"
    )
}

fn fake_home() -> Home {
    let h = std::env::temp_dir().join(format!("turbine-scan-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = fs::remove_dir_all(&h);
    let steam = h.join(".local/share/Steam");
    let sd = h.join("media/SD Card");
    let sapps = steam.join("steamapps");
    let dapps = sd.join("steamapps");

    write(
        &sapps.join("libraryfolders.vdf"),
        &format!(
            "\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n\"1\"\n{{\n\"path\" \"{}\"\n}}\n\"2\"\n{{\n\"path\" \"/run/media/deck/missing card\"\n}}\n}}\n",
            steam.display(),
            sd.display()
        ),
    );
    // Internal: Skyrim (Windows, has a prefix and flat art), Proton itself (a tool), a game still downloading.
    write(&sapps.join("appmanifest_489830.acf"), &manifest("489830", "The Elder Scrolls V: Skyrim Special Edition", "Skyrim Special Edition", 4));
    write(&sapps.join("common/Skyrim Special Edition/SkyrimSE.exe"), "");
    fs::create_dir_all(sapps.join("compatdata/489830/pfx")).unwrap();
    write(&steam.join("appcache/librarycache/489830_library_600x900.jpg"), "x");
    write(&sapps.join("appmanifest_1493710.acf"), &manifest("1493710", "Proton Experimental", "Proton - Experimental", 4));
    write(&sapps.join("common/Proton - Experimental/proton"), "");
    write(&sapps.join("appmanifest_100.acf"), &manifest("100", "Half Downloaded", "Half", 1026));
    write(&sapps.join("common/Half/game.exe"), "");
    // SD card: Valheim native, with new-style art in a hashed subfolder.
    write(&dapps.join("appmanifest_892970.acf"), &manifest("892970", "Valheim", "Valheim", 4));
    write(&dapps.join("common/Valheim/valheim.x86_64"), "");
    write(&steam.join("appcache/librarycache/892970/abc123/library_600x900.jpg"), "x");
    write(&steam.join("appcache/librarycache/892970/header.jpg"), "x");
    // A broken manifest must not stop the scan.
    write(&dapps.join("appmanifest_999.acf"), "\"AppState\" {");

    // Heroic (Flatpak): one Epic game, one GOG game, one DLC, one sideloaded game, one missing folder.
    let heroic = h.join(".var/app/com.heroicgameslauncher.hgl/config/heroic");
    let cp = h.join("Games/Heroic/Cyberpunk 2077");
    let sv = h.join("Games/Heroic/Stardew Valley");
    let side = h.join("Games/Other/Tiny Game");
    write(&cp.join("bin/x64/Cyberpunk2077.exe"), "");
    write(&sv.join("StardewValley"), "");
    write(&side.join("tiny.exe"), "");
    write(
        &heroic.join("legendaryConfig/legendary/installed.json"),
        &format!(
            r#"{{"Ginger": {{"title": "Cyberpunk 2077", "install_path": "{}", "platform": "Windows", "version": "2.3", "is_dlc": false}},
                "GingerDLC": {{"title": "Phantom Liberty", "install_path": "{}", "platform": "Windows", "is_dlc": true}},
                "Gone": {{"title": "Removed Game", "install_path": "/nowhere", "platform": "Windows"}}}}"#,
            cp.display(),
            cp.display()
        ),
    );
    write(
        &heroic.join("gog_store/installed.json"),
        &format!(r#"{{"installed": [{{"appName": "1453375253", "install_path": "{}", "platform": "linux", "version": "1.6.15"}}]}}"#, sv.display()),
    );
    write(&heroic.join("store_cache/gog_library.json"), r#"{"games": [{"app_name": "1453375253", "title": "Stardew Valley"}]}"#);
    write(
        &heroic.join("sideload_apps/library.json"),
        &format!(r#"{{"games": [{{"app_name": "x1", "title": "Tiny Game", "folder_name": "{}", "is_installed": true, "install": {{"platform": "Windows"}}}}]}}"#, side.display()),
    );
    Home(h)
}

#[test]
fn finds_steam_and_heroic_games() {
    let home = fake_home();
    let scan = scan_games(&home.0);
    let names: Vec<&str> = scan.games.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["Cyberpunk 2077", "Stardew Valley", "The Elder Scrolls V: Skyrim Special Edition", "Tiny Game", "Valheim"]);

    let sky = scan.games.iter().find(|g| g.store_id == "489830").unwrap();
    assert_eq!(sky.store, Store::Steam);
    assert_eq!(sky.id(), "steam:489830");
    assert_eq!(sky.runtime, Runtime::Windows);
    assert!(sky.prefix.is_some());
    assert_eq!(sky.build.as_deref(), Some("1234567"));
    assert!(sky.cover.as_ref().unwrap().ends_with("489830_library_600x900.jpg"));

    let val = scan.games.iter().find(|g| g.store_id == "892970").unwrap();
    assert_eq!(val.runtime, Runtime::Native);
    assert!(val.library.ends_with("SD Card"));
    assert!(val.cover.as_ref().unwrap().ends_with("abc123/library_600x900.jpg"));
    assert!(val.header.as_ref().unwrap().ends_with("892970/header.jpg"));

    let cp = scan.games.iter().find(|g| g.store == Store::Epic).unwrap();
    assert_eq!((cp.store_id.as_str(), cp.runtime, cp.build.as_deref()), ("Ginger", Runtime::Windows, Some("2.3")));
    let sv = scan.games.iter().find(|g| g.store == Store::Gog).unwrap();
    assert_eq!((sv.name.as_str(), sv.runtime), ("Stardew Valley", Runtime::Native));
    let tiny = scan.games.iter().find(|g| g.store == Store::Sideload).unwrap();
    assert_eq!(tiny.runtime, Runtime::Windows);

    // The broken manifest is reported in words, the rest still found.
    assert_eq!(scan.problems.len(), 1, "{:?}", scan.problems);
    assert!(scan.problems[0].contains("appmanifest_999.acf"));
}

#[test]
fn empty_home_finds_nothing_quietly() {
    let h = std::env::temp_dir().join(format!("turbine-empty-{}", std::process::id()));
    fs::create_dir_all(&h).unwrap();
    let scan = scan_games(&h);
    assert!(scan.games.is_empty() && scan.problems.is_empty());
    let _ = fs::remove_dir_all(&h);
}

#[test]
fn games_json_is_valid() {
    let home = fake_home();
    let scan = scan_games(&home.0);
    let v = turbinehub_engine::json::parse(&scan.games_json()).unwrap();
    let list = v.as_array();
    assert_eq!(list.len(), 5);
    let val = list.iter().find(|g| g.str_at("storeId") == Some("892970")).unwrap();
    assert_eq!(val.str_at("drive"), Some("Internal storage")); // the fake SD card isn't under /run/media
    assert_eq!(val.str_at("runtime"), Some("native"));
    assert!(val.str_at("cover").is_some());
}

#[test]
fn launch_options_read_from_localconfig() {
    let text = "\"UserLocalConfigStore\"\n{\n\"Software\"\n{\n\"Valve\"\n{\n\"Steam\"\n{\n\"Apps\"\n{\n\"489830\"\n{\n\"LaunchOptions\" \"WINEDLLOVERRIDES=\\\"x=n,b\\\" %command%\"\n}\n\"413150\"\n{\n\"LastPlayed\" \"1\"\n}\n}\n}\n}\n}\n}\n";
    assert_eq!(turbinehub_engine::steam::launch_options_in(text, "489830").as_deref(), Some("WINEDLLOVERRIDES=\"x=n,b\" %command%"));
    assert_eq!(turbinehub_engine::steam::launch_options_in(text, "413150").as_deref(), Some(""), "listed, none set");
    assert_eq!(turbinehub_engine::steam::launch_options_in(text, "1"), None, "not listed");
}

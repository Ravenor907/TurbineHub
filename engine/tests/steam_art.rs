//! Steam library art. Grid files are named by the shortcut's appid; the player's own art is never replaced;
//! Remove takes out only what Turbine Hub wrote.

mod support;

use std::fs;
use std::path::PathBuf;
use support::scratch as tmp;
use turbinehub_engine::steam_art::{self, ArtOutcome};
use turbinehub_engine::{shortcuts, vdf};

#[test]
fn grid_names_from_the_appid() {
    assert_eq!(steam_art::grid_name(3_871_208_806, "capsule").unwrap(), "3871208806p.png");
    assert_eq!(steam_art::grid_name(3_871_208_806, "wide").unwrap(), "3871208806.png");
    assert_eq!(steam_art::grid_name(3_871_208_806, "hero").unwrap(), "3871208806_hero.png");
    assert_eq!(steam_art::grid_name(3_871_208_806, "logo").unwrap(), "3871208806_logo.png");
    assert_eq!(steam_art::grid_name(3_871_208_806, "icon").unwrap(), "3871208806_icon.png");
    assert_eq!(steam_art::grid_name(1, "banner"), None);
    assert_eq!(steam_art::TURBINE_PARTS, ["capsule", "wide", "hero", "logo"]);
}

fn src(d: &std::path::Path, tag: &str) -> PathBuf {
    let s = d.join(format!("src-{tag}"));
    fs::create_dir_all(&s).unwrap();
    for p in ["capsule", "wide", "hero", "logo"] {
        fs::write(s.join(format!("{p}.png")), format!("{p} {tag}")).unwrap();
    }
    s
}

#[test]
fn never_over_the_players_art_and_remove_only_turbines() {
    let d = tmp("own");
    let grid = d.join("grid");
    let record = d.join("data/steam-art.json");
    fs::create_dir_all(&grid).unwrap();
    // The player already picked a hero picture.
    fs::write(grid.join("77_hero.png"), "mine").unwrap();
    fs::write(grid.join("77_icon.png"), "mine too").unwrap();
    fs::write(grid.join("78p.png"), "another game's").unwrap();
    let v1 = src(&d, "v1");
    let got = steam_art::add_art_opts(&v1, &grid, 77, steam_art::TURBINE_PARTS, &record, false).unwrap();
    let outcome = |n: &str| got.iter().find(|(p, _)| p.ends_with(n)).map(|(_, o)| o.clone()).unwrap();
    assert_eq!(outcome("77p.png"), ArtOutcome::Written);
    assert_eq!(outcome("77.png"), ArtOutcome::Written);
    assert_eq!(outcome("77_logo.png"), ArtOutcome::Written);
    assert_eq!(outcome("77_hero.png"), ArtOutcome::PlayersOwn);
    assert_eq!(fs::read_to_string(grid.join("77_hero.png")).unwrap(), "mine");
    // Again: nothing to write.
    let got = steam_art::add_art_opts(&v1, &grid, 77, steam_art::TURBINE_PARTS, &record, false).unwrap();
    assert!(got.iter().all(|(p, o)| *o == ArtOutcome::Unchanged || p.ends_with("77_hero.png")), "{got:?}");
    // After an update Turbine Hub's own files are renewed; the player replaced the wide one meanwhile: kept.
    fs::write(grid.join("77.png"), "player's wide").unwrap();
    let v2 = src(&d, "v2");
    let got = steam_art::add_art_opts(&v2, &grid, 77, steam_art::TURBINE_PARTS, &record, false).unwrap();
    assert_eq!(got.iter().find(|(p, _)| p.ends_with("77p.png")).unwrap().1, ArtOutcome::Written);
    assert_eq!(fs::read_to_string(grid.join("77p.png")).unwrap(), "capsule v2");
    assert_eq!(got.iter().find(|(p, _)| p.ends_with("77.png")).unwrap().1, ArtOutcome::PlayersOwn);
    assert_eq!(fs::read_to_string(grid.join("77.png")).unwrap(), "player's wide");
    // Remove: only Turbine Hub's own files.
    let removed = steam_art::remove_art(&grid, 77, &record).unwrap();
    let names: Vec<String> = removed.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
    assert_eq!(names, ["77p.png", "77_logo.png"]);
    for (n, want) in [("77_hero.png", "mine"), ("77_icon.png", "mine too"), ("77.png", "player's wide"), ("78p.png", "another game's")] {
        assert_eq!(fs::read_to_string(grid.join(n)).unwrap(), want, "{n}");
    }
    assert!(steam_art::remove_art(&grid, 77, &record).unwrap().is_empty());
    // A missing source piece is skipped.
    let empty = d.join("none");
    let got = steam_art::add_art_opts(&empty, &grid, 5, steam_art::TURBINE_PARTS, &record, false).unwrap();
    assert!(got.iter().all(|(_, o)| *o == ArtOutcome::NoSource));
    let _ = fs::remove_dir_all(&d);
}

/// Turbine Hub's own shortcut is found in shortcuts.vdf (read only) by its Flatpak run line.
#[test]
fn finds_turbines_own_shortcut() {
    let d = tmp("find");
    let file = d.join(".local/share/Steam/userdata/42/config/shortcuts.vdf");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut doc = vdf::BinDoc { items: vec![vdf::BinItem::obj("shortcuts", vec![])], end: Some(8), tail: vec![] };
    let own = vdf::BinItem::obj(
        "0",
        vec![
            vdf::BinItem::int("appid", 0x9000_0042),
            vdf::BinItem::text("AppName", "Turbine Hub"),
            vdf::BinItem::text("Exe", "\"/usr/bin/flatpak\""),
            vdf::BinItem::text("LaunchOptions", "run --branch=stable --arch=x86_64 --command=turbine-mod-manager io.github.ravenor907.TurbineHub"),
        ],
    );
    if let vdf::BinValue::Obj(list, _) = &mut doc.items[0].value {
        list.push(own);
    }
    let bytes = vdf::write_binary_doc(&doc);
    fs::write(&file, &bytes).unwrap();
    // The co-op tile next to it doesn't count as Turbine Hub itself.
    let tile = shortcuts::coop_tile("ELDEN RING", "steam:1245620", "io.github.ravenor907.TurbineHub");
    shortcuts::add_shortcut(&file, &tile, &shortcuts::coop_marker("steam:1245620"), false).unwrap();
    let (f, appid) = shortcuts::find_self_file(&d).unwrap();
    assert_eq!(f, file);
    assert_eq!(appid, 0x9000_0042);
    assert_eq!(shortcuts::find_self(&d).unwrap().0, 0x9000_0042);
    assert_eq!(steam_art::grid_dir(&f), file.with_file_name("grid"));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_players_other_formats_and_cleared_art_are_respected() {
    let d = std::env::temp_dir().join(format!("turbine-art-m5-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let grid = d.join("grid");
    let record = d.join("data/steam-art.json");
    fs::create_dir_all(&grid).unwrap();
    let v1 = src(&d, "v1");
    // The player's capsule is a JPEG: Turbine Hub writes no .png beside it.
    fs::write(grid.join("77p.jpg"), "my jpeg").unwrap();
    let got = steam_art::add_art_opts(&v1, &grid, 77, steam_art::TURBINE_PARTS, &record, false).unwrap();
    let outcome = |g: &Vec<(std::path::PathBuf, ArtOutcome)>, n: &str| g.iter().find(|(p, _)| p.ends_with(n)).map(|(_, o)| o.clone()).unwrap();
    assert_eq!(outcome(&got, "77p.png"), ArtOutcome::PlayersOwn);
    assert!(!grid.join("77p.png").exists());
    // A JPEG set later beside Turbine Hub's own .png: Turbine Hub's steps aside.
    fs::write(grid.join("77_hero.webp"), "my hero").unwrap();
    let got = steam_art::add_art_opts(&v1, &grid, 77, steam_art::TURBINE_PARTS, &record, false).unwrap();
    assert_eq!(outcome(&got, "77_hero.png"), ArtOutcome::PlayersOwn);
    assert!(!grid.join("77_hero.png").exists());
    // The player clears Turbine Hub's wide picture in Steam: the next start doesn't bring it back...
    assert!(grid.join("77.png").exists());
    fs::remove_file(grid.join("77.png")).unwrap();
    let got = steam_art::add_art_opts(&v1, &grid, 77, steam_art::TURBINE_PARTS, &record, false).unwrap();
    assert_eq!(outcome(&got, "77.png"), ArtOutcome::Cleared);
    let got = steam_art::add_art_opts(&v1, &grid, 77, steam_art::TURBINE_PARTS, &record, false).unwrap();
    assert_eq!(outcome(&got, "77.png"), ArtOutcome::Cleared);
    assert!(!grid.join("77.png").exists());
    // ...until the player asks for Turbine Hub's art again.
    let got = steam_art::add_art_opts(&v1, &grid, 77, steam_art::TURBINE_PARTS, &record, true).unwrap();
    assert_eq!(outcome(&got, "77.png"), ArtOutcome::Written);
    // A file of the player's that happens to equal Turbine Hub's is never claimed: Remove leaves it.
    let other = d.join("grid2");
    fs::create_dir_all(&other).unwrap();
    fs::copy(v1.join("logo.png"), other.join("9_logo.png")).unwrap();
    let rec2 = d.join("data/rec2.json");
    let got = steam_art::add_art_opts(&v1, &other, 9, steam_art::TURBINE_PARTS, &rec2, false).unwrap();
    assert_eq!(outcome(&got, "9_logo.png"), ArtOutcome::PlayersOwn);
    steam_art::remove_art(&other, 9, &rec2).unwrap();
    assert!(other.join("9_logo.png").exists());
    let _ = fs::remove_dir_all(&d);
}

/// The log line accounts for every piece, so it tells whether the art is in place, cleared or missing (not "0 written,
/// 0 of your own kept" at every start).
#[test]
fn the_summary_accounts_for_every_piece() {
    let d = tmp("summary");
    let grid = d.join("grid");
    let record = d.join("data/steam-art.json");
    fs::create_dir_all(&grid).unwrap();
    let v1 = src(&d, "v1");
    let first = steam_art::add_art_opts(&v1, &grid, 5, steam_art::TURBINE_PARTS, &record, false).unwrap();
    assert_eq!(steam_art::summary(&first), "4 written");
    // The second start: the same art is there, and it doesn't read as nothing done.
    let second = steam_art::add_art_opts(&v1, &grid, 5, steam_art::TURBINE_PARTS, &record, false).unwrap();
    assert_eq!(steam_art::summary(&second), "4 already in place");
    assert!(!steam_art::nothing_shown(&second));
    // One cleared in Steam, one the player's own.
    fs::remove_file(grid.join("5.png")).unwrap();
    fs::write(grid.join("5p.jpg"), "mine").unwrap();
    let third = steam_art::add_art_opts(&v1, &grid, 5, steam_art::TURBINE_PARTS, &record, false).unwrap();
    let s = steam_art::summary(&third);
    assert!(s.starts_with("2 already in place, 1 of your own kept, 1 cleared in Steam"), "{s}");
    // Turbine Hub's own pictures missing (a broken install): said, and counted as nothing shown.
    let none = steam_art::add_art_opts(&d.join("no-such-set"), &d.join("grid2"), 6, steam_art::TURBINE_PARTS, &d.join("data/r2.json"), false).unwrap();
    assert_eq!(steam_art::summary(&none), "4 missing from Turbine Hub's own files");
    assert!(steam_art::nothing_shown(&none));
    assert_eq!(steam_art::summary(&[]), "no pictures to place");
    let _ = fs::remove_dir_all(&d);
}
